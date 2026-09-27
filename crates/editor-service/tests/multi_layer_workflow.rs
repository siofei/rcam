//! S4-B1 Multi-Gerber Workspace: real service-only workflow (no GUI).
//!
//! Covers the mandatory automated gates of the S4-B1 task book: layer model,
//! aperture namespace isolation, atomic batch import, empty layer, tiered layer
//! deletion with Undo/Redo, workspace-only view style, category classification
//! and locks, per-layer export, Save/Export semantics and the headless workflow.
use editor_core::workspace::{DisplayClass, LayerDisplayMode};
use editor_core::{ApertureShape, Exposure, SemanticGeometry};
use editor_service::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

const A: &str = "G04 A*
%FSLAX26Y26*%
%MOMM*%
%ADD10C,1.0*%
%ADD11C,0.2*%
%ADD12C,4.0*%
%ADD13C,2.0*%
D10*
X0Y0D03*
X5000000Y0D03*
D11*
X0Y0D02*
G01*
X5000000Y0D01*
D12*
X10000000Y10000000D03*
%LPC*%
D13*
X10000000Y10000000D03*
%LPD*%
M02*
";

const B: &str = "%FSLAX26Y26*%
%MOMM*%
%ADD10R,2.0X1.0*%
%ADD11O,2.0X1.0*%
%ADD12P,3.0X6*%
%AMBOX*21,1,1.5,0.5,0,0,0*%
%ADD13BOX*%
%ADD14C,3.0*%
D10*
X1000000Y1000000D03*
D11*
X3000000Y1000000D03*
D12*
X5000000Y1000000D03*
D13*
X7000000Y1000000D03*
D14*
X10000000Y10000000D03*
M02*
";

const C: &str = "%FSLAX26Y26*%
%MOMM*%
%ADD10C,3.0*%
D10*
X20000000Y0D03*
G36*
X0Y5000000D02*
G01*
X4000000Y5000000D01*
X4000000Y9000000D01*
X0Y9000000D01*
X0Y5000000D01*
G37*
M02*
";

struct W {
    svc: ApplicationService,
    dir: PathBuf,
    doc: String,
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rcam-b1-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(dir.join("in")).unwrap();
    std::fs::create_dir_all(dir.join("in2")).unwrap();
    std::fs::create_dir_all(dir.join("out")).unwrap();
    dir
}

impl W {
    /// A brand new empty Workspace with the three fixtures on disk.
    fn new(tag: &str) -> Self {
        let dir = temp_dir(tag);
        for (name, text) in [("A", A), ("B", B), ("C", C)] {
            std::fs::write(dir.join(format!("in/{name}.gbr")), text).unwrap();
        }
        std::fs::write(dir.join("in2/A.gbr"), C).unwrap();
        let canonical = dir.canonicalize().unwrap();
        let mut svc = ApplicationService::with_file_access(FileAccessPolicy::new(
            canonical.clone(),
            [canonical.clone()],
            [canonical.clone()],
        ));
        let doc = svc.document_new().unwrap().document_id;
        Self {
            svc,
            dir: canonical,
            doc,
        }
    }

    fn p(&self, rel: &str) -> String {
        self.dir.join(rel).to_string_lossy().into_owned()
    }
    fn info(&self) -> DocumentInfo {
        self.svc.document_get(&self.doc).unwrap()
    }
    fn rev(&self) -> String {
        self.info().revision
    }
    fn wrev(&self) -> String {
        self.info().workspace_revision
    }
    fn layers(&self) -> Vec<LayerInfo> {
        self.svc.layers_list(&self.doc).unwrap()
    }
    fn import(&mut self, rels: &[&str]) -> Result<ImportLayersResult, ServiceError> {
        let paths = rels.iter().map(|r| self.p(r)).collect();
        let rev = self.rev();
        self.svc
            .import_gerber_layers(&self.doc, &rev, ImportGerberLayersParams { paths })
    }
    fn import_ok(&mut self, rels: &[&str]) -> Vec<String> {
        self.import(rels)
            .unwrap()
            .layers
            .into_iter()
            .map(|l| l.layer_id)
            .collect()
    }
    fn update(&mut self, patch: LayerUpdateParams) -> DocumentInfo {
        let rev = self.rev();
        self.svc.layer_update(&self.doc, &rev, patch).unwrap()
    }
    fn patch(&self, layer: &str) -> LayerUpdateParams {
        LayerUpdateParams {
            layer_id: layer.into(),
            expected_workspace_revision: self.wrev(),
            ..Default::default()
        }
    }
    fn objects(&self, layer: &str) -> Vec<editor_core::SemanticObject> {
        self.svc
            .objects_query(
                &self.doc,
                QueryParams {
                    layer_id: layer.into(),
                    geometry_type: None,
                    region_mm: None,
                    relation: None,
                    limit: Some(1000),
                    cursor: None,
                },
            )
            .unwrap()
            .objects
            .into_iter()
            .map(|o| o.object)
            .collect()
    }
    fn remove(&mut self, layer: &str, allow: bool) -> Result<RemoveLayerResult, ServiceError> {
        let rev = self.rev();
        self.svc.remove_layer(
            &self.doc,
            &rev,
            RemoveLayerParams {
                layer_id: layer.into(),
                allow_non_empty: allow,
            },
        )
    }
    fn undo(&mut self) -> EditResult {
        let rev = self.rev();
        self.svc.history_undo(&self.doc, &rev).unwrap()
    }
    fn redo(&mut self) -> EditResult {
        let rev = self.rev();
        self.svc.history_redo(&self.doc, &rev).unwrap()
    }
    fn export(&mut self, layer: &str, name: &str) -> Result<ExportResult, ServiceError> {
        let rev = self.rev();
        self.svc.export_layer(
            &self.doc,
            &rev,
            ExportParams {
                layer_id: layer.into(),
                path: self.p(&format!("out/{name}")),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
                compatibility_precision_override_mm: None,
            },
        )
    }
    fn move_object(&mut self, layer: &str, id: &str, dx: f64) -> Result<EditResult, ServiceError> {
        let rev = self.rev();
        self.svc.objects_move(
            &self.doc,
            &rev,
            MoveParams {
                layer_id: layer.into(),
                object_ids: vec![id.into()],
                dx_mm: dx,
                dy_mm: 0.,
            },
        )
    }
    fn reopen(&mut self, name: &str) -> (String, Vec<editor_core::SemanticObject>) {
        let path = self.p(&format!("out/{name}"));
        let info = self.svc.open(&path).unwrap();
        let layer = info.layer_ids[0].clone();
        let objects = self
            .svc
            .objects_query(
                &info.document_id,
                QueryParams {
                    layer_id: layer,
                    geometry_type: None,
                    region_mm: None,
                    relation: None,
                    limit: Some(1000),
                    cursor: None,
                },
            )
            .unwrap()
            .objects
            .into_iter()
            .map(|o| o.object)
            .collect();
        (info.document_id, objects)
    }
    fn shapes(&self) -> Vec<(String, ApertureShape)> {
        self.svc
            .render_snapshot(&self.doc)
            .unwrap()
            .apertures
            .into_iter()
            .map(|a| (a.id, a.shape))
            .collect()
    }
}

fn circle_diameter(shape: &ApertureShape) -> f64 {
    match shape {
        ApertureShape::Circle { diameter_mm, .. } => *diameter_mm,
        other => panic!("not a circle: {other:?}"),
    }
}

fn sha_of(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

fn json_call(
    svc: &mut ApplicationService,
    op: &str,
    doc: Option<&str>,
    rev: Option<&str>,
    params: Value,
) -> Value {
    let mut request = json!({"api_version":1,"request_id":op,"op":op,"params":params});
    if let Some(doc) = doc {
        request["document_id"] = json!(doc);
    }
    if let Some(rev) = rev {
        request["expected_revision"] = json!(rev);
    }
    svc.execute_json(&request.to_string())
}

// ---------------------------------------------------------------- model

#[test]
fn new_workspace_is_empty_clean_and_openable_without_source() {
    let w = W::new("new");
    let info = w.info();
    assert!(info.layer_ids.is_empty() && info.display_order.is_empty());
    assert_eq!(info.active_layer_id, None);
    assert!(!info.dirty);
    assert_eq!(
        (info.revision.as_str(), info.workspace_revision.as_str()),
        ("0", "0")
    );
    assert!(info.source_path.is_empty() && info.source_sha256.is_empty());
    assert!(w.layers().is_empty());
    assert_eq!(w.svc.render_snapshot(&w.doc).unwrap().layers.len(), 0);
}

#[test]
fn batch_import_gives_unique_ids_source_identity_provenance_and_top_of_panel_order() {
    let mut w = W::new("batch");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]);
    assert_eq!(ids.len(), 3);
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 3, "unique LayerIds");
    let rows = w.layers();
    // File-picker order is the panel order, top first; the first file is active.
    assert_eq!(
        rows.iter().map(|r| r.layer_id.clone()).collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        rows.iter()
            .map(|r| r.display_name.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "C"]
    );
    assert!(rows[0].is_active && !rows[1].is_active && !rows[2].is_active);
    assert_eq!(w.info().active_layer_id.as_deref(), Some(ids[0].as_str()));
    let sources: std::collections::HashSet<_> =
        rows.iter().map(|r| r.source_id.clone().unwrap()).collect();
    assert_eq!(sources.len(), 3, "each file is its own source group");
    for row in &rows {
        let provenance = row.provenance.as_ref().unwrap();
        assert!(provenance.original_file_name.ends_with(".gbr"));
        assert_eq!(provenance.imported_sha256.len(), 64);
        assert!(provenance.imported_at.ends_with('Z'));
    }
    let info = w.info();
    assert!(info.dirty, "adding layers changes the Workspace content");
    assert_eq!((info.undo_entries, info.revision.as_str()), (1, "1"));
    // A second batch goes on top of the first one, keeping its own order.
    w.import_ok(&["in/C.gbr"]);
    let order = w.info().display_order;
    assert_eq!(order.len(), 4);
    assert_eq!(&order[1..], &ids[..]);
    assert_eq!(w.info().undo_entries, 2);
}

#[test]
fn duplicate_basename_and_same_file_twice_are_two_independent_layers() {
    let mut w = W::new("dup");
    let ids = w.import_ok(&["in/A.gbr", "in2/A.gbr", "in/A.gbr"]);
    let rows = w.layers();
    assert_eq!(
        rows.iter()
            .map(|r| r.display_name.as_str())
            .collect::<Vec<_>>(),
        ["A", "A (2)", "A (3)"]
    );
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
    // Same disk file twice: identical provenance name/SHA, different identity.
    assert_eq!(
        rows[0].provenance.as_ref().unwrap().imported_sha256,
        rows[2].provenance.as_ref().unwrap().imported_sha256
    );
    assert_ne!(rows[0].source_id, rows[2].source_id);
    let a0 = w.objects(&ids[0]);
    let a2 = w.objects(&ids[2]);
    assert_eq!(a0.len(), a2.len());
    assert!(
        a0.iter()
            .all(|o| a2.iter().all(|p| p.object_id != o.object_id)),
        "object ids are namespaced per source"
    );
}

#[test]
fn add_batch_is_all_or_none_and_leaves_no_trace() {
    let mut w = W::new("atomic");
    let first = w.import_ok(&["in/A.gbr"]);
    std::fs::write(
        w.dir.join("in/bad.gbr"),
        "%FSLAX26Y26*%\n%MOMM*%\nNOT GERBER\n",
    )
    .unwrap();
    let before = w.info();
    let layers_before = w.layers();
    let snapshot_before = w.svc.render_snapshot(&w.doc).unwrap();
    let error = w
        .import(&["in/B.gbr", "in/bad.gbr", "in/C.gbr"])
        .unwrap_err();
    assert_eq!(error.details["import_index"], 1, "{error:?}");
    assert_eq!(error.details["import_atomic"], true);
    assert_eq!(w.info(), before, "no revision, history or dirty change");
    assert_eq!(w.layers(), layers_before);
    assert_eq!(w.svc.render_snapshot(&w.doc).unwrap(), snapshot_before);
    // Missing file, path outside the granted roots, empty batch: same guarantee.
    assert!(w.import(&["in/B.gbr", "in/missing.gbr"]).is_err());
    assert!(w.import(&[]).is_err());
    let outside = std::env::temp_dir().join(format!("rcam-outside-{}.gbr", std::process::id()));
    std::fs::write(&outside, B).unwrap();
    let mut policy = ApplicationService::with_file_access(FileAccessPolicy::new(
        w.dir.clone(),
        [w.dir.clone()],
        [w.dir.clone()],
    ));
    let doc = policy.document_new().unwrap().document_id;
    let denied = policy.import_gerber_layers(
        &doc,
        "0",
        ImportGerberLayersParams {
            paths: vec![w.p("in/B.gbr"), outside.to_string_lossy().into_owned()],
        },
    );
    assert_eq!(denied.unwrap_err().code, "PERMISSION_DENIED");
    assert!(policy.layers_list(&doc).unwrap().is_empty());
    assert_eq!(first.len(), 1);
    // The next successful import is unaffected by the failed ones (ids never reused).
    let next = w.import_ok(&["in/B.gbr"]);
    assert_ne!(next[0], first[0]);
}

#[test]
fn stale_revision_is_rejected_before_any_file_is_read() {
    let mut w = W::new("stale");
    w.import_ok(&["in/A.gbr"]);
    let error = w
        .svc
        .import_gerber_layers(
            &w.doc,
            "0",
            ImportGerberLayersParams {
                paths: vec![w.p("in/B.gbr")],
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "REVISION_CONFLICT");
    assert_eq!(w.layers().len(), 1);
}

#[test]
fn import_add_is_one_undo_and_redo_restores_identity_style_order_and_active() {
    let mut w = W::new("undoadd");
    let base = w.import_ok(&["in/A.gbr"]);
    w.update(LayerUpdateParams {
        base_color: Some("#123456".into()),
        display_mode: Some(LayerDisplayMode::Outline),
        ..w.patch(&base[0])
    });
    let before = (w.layers(), w.info().display_order, w.info().active_layer_id);
    let batch = w.import_ok(&["in/B.gbr", "in/C.gbr"]);
    let after_layers = w.layers();
    let after_info = w.info();
    assert_eq!(
        after_info.active_layer_id.as_deref(),
        Some(batch[0].as_str())
    );
    let undone = w.undo();
    assert_eq!(undone.changed_layer_ids, batch);
    assert!(undone.changed_object_ids.is_empty());
    let info = w.info();
    // Undo restores the previous active layer and leaves everything else alone.
    assert_eq!(
        (w.layers(), info.display_order, info.active_layer_id),
        before
    );
    assert_eq!(info.undo_entries, 1);
    assert_eq!(info.redo_entries, 1);
    assert!(
        w.svc
            .render_snapshot(&w.doc)
            .unwrap()
            .apertures
            .iter()
            .all(|a| a.id.starts_with("src-1::") || !a.id.contains("::"))
    );
    w.redo();
    assert_eq!(
        w.layers(),
        after_layers,
        "same LayerIds, colours, order, styles"
    );
    assert_eq!(w.info().active_layer_id, after_info.active_layer_id);
    assert_eq!(w.info().layer_ids, after_info.layer_ids);
}

#[test]
fn reorder_is_workspace_only_and_solo_never_touches_visibility() {
    let mut w = W::new("order");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]);
    let before = w.info();
    let snapshot_layers = w.svc.render_snapshot(&w.doc).unwrap();
    let reordered = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
    let rev = w.rev();
    let info = w
        .svc
        .layers_reorder(
            &w.doc,
            &rev,
            ReorderLayersParams {
                expected_workspace_revision: w.wrev(),
                layer_ids: reordered.clone(),
            },
        )
        .unwrap();
    assert_eq!(info.display_order, reordered);
    assert_eq!(
        info.revision, before.revision,
        "workspace-only: no manufacturing revision"
    );
    assert_ne!(info.workspace_revision, before.workspace_revision);
    assert_eq!(
        (info.undo_entries, info.dirty),
        (before.undo_entries, before.dirty)
    );
    // Manufacturing order inside the document (and each layer's exposure order) is intact.
    assert_eq!(info.layer_ids, before.layer_ids);
    // Composite order is bottom-first: the last panel row is drawn first.
    let snapshot = w.svc.render_snapshot(&w.doc).unwrap();
    assert_eq!(
        snapshot
            .layers
            .iter()
            .map(|l| l.id.clone())
            .collect::<Vec<_>>(),
        [ids[1].clone(), ids[0].clone(), ids[2].clone()]
    );
    for layer in &snapshot.layers {
        let original = snapshot_layers
            .layers
            .iter()
            .find(|l| l.id == layer.id)
            .unwrap();
        assert_eq!(
            layer, original,
            "exposure order of every layer is untouched"
        );
    }
    // Not a permutation → rejected, nothing changes.
    let rev = w.rev();
    let bad = w.svc.layers_reorder(
        &w.doc,
        &rev,
        ReorderLayersParams {
            expected_workspace_revision: w.wrev(),
            layer_ids: vec![ids[0].clone(), ids[0].clone(), ids[1].clone()],
        },
    );
    assert!(bad.is_err());
    assert_eq!(w.info().display_order, reordered);
    // Solo: overrides effective visibility only.
    let rev = w.rev();
    w.svc
        .layers_set_solo(
            &w.doc,
            &rev,
            SetSoloLayerParams {
                expected_workspace_revision: w.wrev(),
                layer_id: Some(ids[1].clone()),
            },
        )
        .unwrap();
    for row in w.layers() {
        assert!(row.visible, "user visibility untouched");
        assert_eq!(row.effective_visible, row.layer_id == ids[1]);
    }
    let visible = w.svc.render_snapshot(&w.doc).unwrap();
    assert_eq!(visible.styles.iter().filter(|s| s.visible).count(), 1);
    let rev = w.rev();
    w.svc
        .layers_set_solo(
            &w.doc,
            &rev,
            SetSoloLayerParams {
                expected_workspace_revision: w.wrev(),
                layer_id: None,
            },
        )
        .unwrap();
    assert!(w.layers().iter().all(|r| r.effective_visible));
}

#[test]
fn workspace_only_state_changes_never_touch_manufacturing_state_history_or_bytes() {
    let mut w = W::new("wsonly");
    let ids = w.import_ok(&["in/A.gbr"]);
    let layer = &ids[0];
    let baseline = w.info();
    let snapshot = w.svc.render_snapshot(&w.doc).unwrap();
    let a = w.export(layer, "baseline.gbr").unwrap();
    for step in 0..2 {
        let patch = if step == 0 {
            LayerUpdateParams {
                base_color: Some("#ff8800".into()),
                ..w.patch(layer)
            }
        } else {
            LayerUpdateParams {
                visible: Some(false),
                locked: Some(true),
                selectable: Some(false),
                display_name: Some("钢网 # 顶层".into()),
                ..w.patch(layer)
            }
        };
        let info = w.update(patch);
        assert_eq!(info.revision, baseline.revision);
        assert_eq!(
            (info.undo_entries, info.redo_entries),
            (baseline.undo_entries, baseline.redo_entries)
        );
        assert_eq!(info.dirty, baseline.dirty);
        assert_ne!(info.workspace_revision, baseline.workspace_revision);
    }
    for mode in [
        LayerDisplayMode::Outline,
        LayerDisplayMode::ZeroWidth,
        LayerDisplayMode::Filled,
    ] {
        w.update(LayerUpdateParams {
            display_mode: Some(mode),
            ..w.patch(layer)
        });
    }
    w.update(LayerUpdateParams {
        color_mode: Some(ColorMode::CategoryColor),
        classes: vec![
            ClassStyleUpdate {
                class: Some(DisplayClass::FlashCircle),
                visible: Some(false),
                ..Default::default()
            },
            ClassStyleUpdate {
                class: Some(DisplayClass::Stroke),
                locked: Some(true),
                selectable: Some(false),
                color_override: Some("#00ff00".into()),
                ..Default::default()
            },
        ],
        ..w.patch(layer)
    });
    // The manufacturing scene is byte-identical; only the style block changed.
    let after = w.svc.render_snapshot(&w.doc).unwrap();
    assert_eq!(after.layers, snapshot.layers);
    assert_eq!(after.apertures, snapshot.apertures);
    assert_eq!(after.revision, snapshot.revision);
    // Writer bytes do not depend on any view setting.
    w.update(LayerUpdateParams {
        visible: Some(true),
        locked: Some(false),
        ..w.patch(layer)
    });
    let b = w.export(layer, "styled.gbr").unwrap();
    assert_eq!(a.sha256, b.sha256, "export hash invariant under view style");
    assert_eq!(
        sha_of(&w.dir.join("out/baseline.gbr")),
        sha_of(&w.dir.join("out/styled.gbr"))
    );
    assert_eq!(w.info().revision, baseline.revision);
    assert!(w.info().undo_entries == 1);
}

// ------------------------------------------------------------ apertures

#[test]
fn same_dcode_in_three_files_never_shares_an_aperture() {
    let mut w = W::new("apertures");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]);
    let shapes = w.shapes();
    // A/B/C each define D10 (circle 1.0 / rectangle 2x1 / circle 3.0).
    let d10: Vec<_> = shapes
        .iter()
        .filter(|(id, _)| id.ends_with("aperture-10"))
        .collect();
    assert_eq!(d10.len(), 3, "three private D10 apertures: {shapes:?}");
    assert_eq!(
        d10.iter()
            .map(|(id, _)| id.clone())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    let flash_of = |layer: &str, nth: usize| -> String {
        w.objects(layer)
            .iter()
            .filter_map(|o| match &o.geometry {
                SemanticGeometry::Flash { aperture_id, .. } => Some(aperture_id.clone()),
                _ => None,
            })
            .nth(nth)
            .unwrap()
    };
    let shape_of = |id: &str| shapes.iter().find(|(a, _)| a == id).unwrap().1.clone();
    let a_first = flash_of(&ids[0], 0);
    let b_first = flash_of(&ids[1], 0);
    let c_first = flash_of(&ids[2], 0);
    assert_eq!(circle_diameter(&shape_of(&a_first)), 1.0);
    assert!(
        matches!(shape_of(&b_first), ApertureShape::Rectangle { width_mm, height_mm, .. } if width_mm == 2.0 && height_mm == 1.0)
    );
    assert_eq!(circle_diameter(&shape_of(&c_first)), 3.0);
    // Editing A's flash (copy-on-write size change) never changes B or C.
    let a_flash = w.objects(&ids[0])[0].object_id.clone();
    let rev = w.rev();
    w.svc
        .objects_set_properties(
            &w.doc,
            &rev,
            SetPropertiesParams {
                layer_id: ids[0].clone(),
                object_ids: vec![a_flash],
                width_mm: 1.5,
                height_mm: None,
            },
        )
        .unwrap();
    let shapes_after = w.shapes();
    let shape_after = |id: &str| {
        shapes_after
            .iter()
            .find(|(a, _)| a == id)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(circle_diameter(&shape_after(&c_first)), 3.0, "C untouched");
    assert!(matches!(
        shape_after(&b_first),
        ApertureShape::Rectangle { .. }
    ));
    // Each layer exports only its own apertures, with freshly allocated DCodes.
    let exports: Vec<_> = ["A", "B", "C"]
        .iter()
        .zip(&ids)
        .map(|(name, id)| {
            w.export(id, &format!("{name}.gbr")).unwrap();
            std::fs::read_to_string(w.dir.join(format!("out/{name}.gbr"))).unwrap()
        })
        .collect();
    assert!(
        exports[0].contains("%ADD10C,1.5*%")
            || exports[0].contains("%ADD10C,1.500000*%")
            || exports[0].contains("C,1.5"),
        "{}",
        exports[0]
    );
    assert!(
        exports[1].contains("R,2") && !exports[1].contains("C,4"),
        "{}",
        exports[1]
    );
    assert!(
        exports[2].contains("C,3") && !exports[2].contains("R,2"),
        "{}",
        exports[2]
    );
    for text in &exports {
        assert_eq!(
            text.matches("%ADD10").count(),
            1,
            "DCodes restart per output: {text}"
        );
    }
}

#[test]
fn clear_exposure_is_isolated_per_layer() {
    let mut w = W::new("clear");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr"]);
    let snapshot = w.svc.render_snapshot(&w.doc).unwrap();
    // Layer A punches a Clear hole around (10,10); layer B has Dark there.
    let doc = editor_core::SemanticDocument {
        id: "probe".into(),
        unit: "mm".into(),
        format: editor_core::SemanticFormat {
            integer: 3,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: snapshot.layers.clone(),
        apertures: snapshot.apertures.clone(),
        source: Default::default(),
        block_definitions: Vec::new(),
    };
    doc.validate().unwrap();
    let centre = editor_core::MmPoint::new(10., 10.);
    assert_eq!(
        doc.layer_coverage_at(&ids[0], centre),
        Some(false),
        "A: cleared hole"
    );
    assert_eq!(
        doc.layer_coverage_at(&ids[1], centre),
        Some(true),
        "B: Dark stays"
    );
    assert!(
        snapshot
            .layers
            .iter()
            .find(|l| l.id == ids[0])
            .unwrap()
            .objects
            .iter()
            .any(|o| o.exposure == Exposure::Clear)
    );
    assert!(
        snapshot
            .layers
            .iter()
            .find(|l| l.id == ids[1])
            .unwrap()
            .objects
            .iter()
            .all(|o| o.exposure == Exposure::Dark)
    );
}

// ---------------------------------------------------------- add / delete

#[test]
fn create_empty_layer_is_active_top_auto_coloured_and_one_undo() {
    let mut w = W::new("empty");
    let rev = w.rev();
    let created = w
        .svc
        .create_empty_layer(&w.doc, &rev, CreateEmptyLayerParams::default())
        .unwrap();
    let info = w.info();
    assert_eq!(info.layer_ids, std::slice::from_ref(&created.layer_id));
    assert_eq!(
        info.active_layer_id.as_deref(),
        Some(created.layer_id.as_str())
    );
    assert_eq!((info.revision.as_str(), info.undo_entries), ("1", 1));
    assert!(info.dirty, "structure change dirties the Workspace");
    let row = &w.layers()[0];
    assert_eq!(row.object_count, 0);
    assert!(row.visible && row.selectable && !row.locked);
    assert_eq!(row.display_mode, LayerDisplayMode::Filled);
    assert_eq!(row.color_mode, ColorMode::LayerColor);
    assert_eq!(row.classes.len(), DisplayClass::ALL.len());
    assert!(
        row.classes
            .iter()
            .all(|c| c.visible && c.selectable && !c.locked && c.color_override.is_none())
    );
    assert!(row.source_id.is_none() && row.provenance.is_none());
    assert_eq!(row.base_color, auto_layer_color(0));
    // A second empty layer is on top, active and coloured differently.
    let rev = w.rev();
    let second = w
        .svc
        .create_empty_layer(
            &w.doc,
            &rev,
            CreateEmptyLayerParams {
                display_name: Some("文字 #层".into()),
            },
        )
        .unwrap();
    let rows = w.layers();
    assert_eq!(rows[0].layer_id, second.layer_id);
    assert_eq!(rows[0].display_name, "文字 #层");
    assert_ne!(rows[0].base_color, rows[1].base_color);
    assert!(rows[0].is_active && !rows[1].is_active);
    // Undo: layer gone, previous active restored; Redo: the very same layer and style.
    let style = rows[0].clone();
    w.undo();
    assert_eq!(w.layers().len(), 1);
    assert_eq!(
        w.info().active_layer_id.as_deref(),
        Some(created.layer_id.as_str())
    );
    w.redo();
    assert_eq!(w.layers()[0], style);
    // Bad names are rejected without effect.
    let rev = w.rev();
    let before = w.info();
    assert!(
        w.svc
            .create_empty_layer(
                &w.doc,
                &rev,
                CreateEmptyLayerParams {
                    display_name: Some("  ".into())
                }
            )
            .is_err()
    );
    assert_eq!(w.info(), before);
}

#[test]
fn colours_are_deterministic_and_ten_layers_are_all_distinct() {
    let mut w = W::new("palette");
    for _ in 0..12 {
        let rev = w.rev();
        w.svc
            .create_empty_layer(&w.doc, &rev, CreateEmptyLayerParams::default())
            .unwrap();
    }
    let rows = w.layers();
    let colours: std::collections::HashSet<_> =
        rows.iter().map(|r| r.base_color.to_hex()).collect();
    assert_eq!(colours.len(), 12);
    // Bottom row got palette entry 0, and so on upwards.
    for (index, row) in rows.iter().rev().enumerate() {
        assert_eq!(row.base_color, auto_layer_color(index));
    }
    // Manual colour change, then restore the deterministic palette.
    let id = rows[0].layer_id.clone();
    w.update(LayerUpdateParams {
        base_color: Some("#010203".into()),
        ..w.patch(&id)
    });
    assert_eq!(w.layers()[0].base_color, Color::rgb(1, 2, 3));
    let rev = w.rev();
    w.svc
        .layers_reset_colors(
            &w.doc,
            &rev,
            ResetLayerColorsParams {
                expected_workspace_revision: w.wrev(),
            },
        )
        .unwrap();
    assert_eq!(w.layers(), rows);
    // Invalid colour is a typed error.
    let rev = w.rev();
    let bad = w.svc.layer_update(
        &w.doc,
        &rev,
        LayerUpdateParams {
            base_color: Some("red".into()),
            ..w.patch(&id)
        },
    );
    assert_eq!(bad.unwrap_err().code, "INVALID_ARGUMENT");
}

#[test]
fn empty_layer_deletes_directly_and_undoes() {
    let mut w = W::new("delempty");
    let rev = w.rev();
    let created = w
        .svc
        .create_empty_layer(&w.doc, &rev, CreateEmptyLayerParams::default())
        .unwrap();
    let summary = w
        .svc
        .layer_summary(
            &w.doc,
            LayerSummaryParams {
                layer_id: created.layer_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(summary.risk, DeleteRisk::Empty);
    assert!(summary.summary.is_empty());
    let removed = w.remove(&created.layer_id, false).unwrap();
    assert_eq!(removed.risk, DeleteRisk::Empty);
    assert!(w.layers().is_empty());
    assert_eq!(w.info().active_layer_id, None);
    assert_eq!(w.info().undo_entries, 2, "create + remove");
    w.undo();
    assert_eq!(w.layers()[0].layer_id, created.layer_id);
    assert_eq!(
        w.info().active_layer_id.as_deref(),
        Some(created.layer_id.as_str())
    );
}

#[test]
fn a_gerber_that_parses_to_zero_objects_is_still_an_empty_layer() {
    let mut w = W::new("zero");
    std::fs::write(
        w.dir.join("in/none.gbr"),
        "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1.0*%\nM02*\n",
    )
    .unwrap();
    let ids = w.import_ok(&["in/none.gbr"]);
    let summary = w
        .svc
        .layer_summary(
            &w.doc,
            LayerSummaryParams {
                layer_id: ids[0].clone(),
            },
        )
        .unwrap();
    assert!(
        summary.summary.has_import_provenance,
        "provenance does not make it non-empty"
    );
    assert_eq!(summary.risk, DeleteRisk::Empty);
    w.remove(&ids[0], false).unwrap();
}

#[test]
fn non_empty_delete_needs_explicit_intent_and_reports_the_summary() {
    let mut w = W::new("delnon");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr"]);
    let before = w.info();
    let error = w.remove(&ids[0], false).unwrap_err();
    assert_eq!(error.code, "CONFIRMATION_REQUIRED");
    assert_eq!(error.details["field"], "params.allow_non_empty");
    assert_eq!(error.details["summary"]["object_count"], 5);
    assert_eq!(error.details["risk"], "non_empty_clean");
    assert_eq!(w.info(), before, "rejected delete changes nothing");
    // JSON entry: an automation client cannot delete a non-empty layer by accident.
    let reply = json_call(
        &mut w.svc,
        "document.remove_layer",
        Some(&w.doc.clone()),
        Some(&before.revision),
        json!({"layer_id": ids[0]}),
    );
    assert_eq!(reply["status"], "confirmation_required", "{reply}");
    assert_eq!(w.layers().len(), 2);
    let reply = json_call(
        &mut w.svc,
        "document.remove_layer",
        Some(&w.doc.clone()),
        Some(&before.revision),
        json!({"layer_id": ids[0], "allow_non_empty": true}),
    );
    assert_eq!(reply["status"], "completed", "{reply}");
    assert_eq!(w.layers().len(), 1);
    // The imported file on disk is untouched by everything above.
    assert_eq!(std::fs::read_to_string(w.dir.join("in/A.gbr")).unwrap(), A);
}

#[test]
fn dirty_and_generated_layers_report_modified_and_generated_counts() {
    let mut w = W::new("dirtydel");
    let ids = w.import_ok(&["in/A.gbr"]);
    let clean = w
        .svc
        .layer_summary(
            &w.doc,
            LayerSummaryParams {
                layer_id: ids[0].clone(),
            },
        )
        .unwrap();
    assert_eq!(clean.risk, DeleteRisk::NonEmptyClean);
    assert_eq!(clean.summary.imported_object_count, 5);
    assert_eq!(clean.summary.generated_object_count, 0);
    assert!(!clean.summary.manufacturing_dirty);
    // Move one object: modified. Move it back: clean again (fingerprint based).
    let first = w.objects(&ids[0])[0].object_id.clone();
    w.move_object(&ids[0], &first, 1.0).unwrap();
    let modified = w
        .svc
        .layer_summary(
            &w.doc,
            LayerSummaryParams {
                layer_id: ids[0].clone(),
            },
        )
        .unwrap();
    assert_eq!(modified.summary.modified_object_count, 1);
    assert_eq!(modified.risk, DeleteRisk::NonEmptyDirty);
    w.move_object(&ids[0], &first, -1.0).unwrap();
    let back = w
        .svc
        .layer_summary(
            &w.doc,
            LayerSummaryParams {
                layer_id: ids[0].clone(),
            },
        )
        .unwrap();
    assert_eq!(back.summary.modified_object_count, 0);
    // Deleting an imported object counts as modified; generated text counts as generated.
    let rev = w.rev();
    w.svc
        .objects_delete(
            &w.doc,
            &rev,
            DeleteParams {
                layer_id: ids[0].clone(),
                object_ids: vec![first.clone()],
            },
        )
        .unwrap();
    let text_rev = w.rev();
    let font = builtin_stroke_font().identity;
    let created = w
        .svc
        .text_create(
            &w.doc,
            &text_rev,
            TextParams {
                layer_id: ids[0].clone(),
                layout: serde_json::from_value(json!({
                    "text": "A1", "height_mm": 2.0, "stroke_width_mm": 0.2,
                    "origin": {"x_mm": 0.0, "y_mm": 0.0}, "rotation_deg": 0.0,
                    "horizontal_align": "left", "vertical_align": "baseline",
                    "letter_spacing_mm": 0.0, "line_spacing_mm": 0.0,
                    "mirror": false, "polarity": "dark", "stroke_mode": "stroke"
                }))
                .unwrap_or_else(|_| default_layout()),
                font,
            },
        )
        .unwrap();
    assert!(!created.generated_object_ids.is_empty());
    let dirty = w
        .svc
        .layer_summary(
            &w.doc,
            LayerSummaryParams {
                layer_id: ids[0].clone(),
            },
        )
        .unwrap();
    assert_eq!(dirty.summary.modified_object_count, 1);
    assert_eq!(
        dirty.summary.generated_object_count,
        created.generated_object_ids.len()
    );
    assert_eq!(dirty.risk, DeleteRisk::NonEmptyDirty);
    assert!(dirty.summary.manufacturing_dirty);
}

fn default_layout() -> TextLayout {
    TextLayout {
        text: "A1".into(),
        x_mm: 0.,
        y_mm: 0.,
        height_mm: 3.,
        tracking_mm: 0.,
        h_align: editor_text::HorizontalAlign::Left,
        v_align: editor_text::VerticalAlign::Bottom,
        rotation_deg: 0.,
        curve_tolerance_mm: editor_text::TOLERANCE_MM,
        baseline_spacing_mm: 0.,
        stroke_width_mm: 0.15,
        outline_offset_mm: 0.,
    }
}

#[test]
fn remove_and_undo_restore_same_layer_style_order_apertures_and_active() {
    let mut w = W::new("restore");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]);
    // Give the middle layer a distinctive style and make it the active one.
    w.update(LayerUpdateParams {
        base_color: Some("#abcdef".into()),
        display_mode: Some(LayerDisplayMode::ZeroWidth),
        color_mode: Some(ColorMode::CategoryColor),
        visible: Some(false),
        selectable: Some(false),
        locked: Some(true),
        display_name: Some("Middle".into()),
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::ApertureMacro),
            locked: Some(true),
            color_override: Some("#0a0b0c".into()),
            ..Default::default()
        }],
        ..w.patch(&ids[1])
    });
    let rev = w.rev();
    w.svc
        .layers_set_active(
            &w.doc,
            &rev,
            SetActiveLayerParams {
                expected_workspace_revision: w.wrev(),
                layer_id: Some(ids[1].clone()),
            },
        )
        .unwrap();
    let before_rows = w.layers();
    let before_doc = w.svc.render_snapshot(&w.doc).unwrap();
    let before_info = w.info();
    let removed = w.remove(&ids[1], true).unwrap();
    assert_eq!(removed.summary.object_count, 5);
    assert_eq!(removed.risk, DeleteRisk::NonEmptyClean);
    assert_eq!(
        w.info().undo_entries,
        before_info.undo_entries + 1,
        "exactly one Undo"
    );
    assert_eq!(w.layers().len(), 2);
    // Active layer was deleted → its neighbour BELOW becomes active.
    assert_eq!(w.info().active_layer_id.as_deref(), Some(ids[2].as_str()));
    assert!(
        w.svc
            .render_snapshot(&w.doc)
            .unwrap()
            .apertures
            .iter()
            .all(|a| !a.id.starts_with("src-2::")),
        "its aperture namespace left with it"
    );
    w.undo();
    let restored_doc = w.svc.render_snapshot(&w.doc).unwrap();
    assert_eq!(
        w.layers(),
        before_rows,
        "identity, colours, order, category styles, flags"
    );
    assert_eq!(restored_doc.layers, before_doc.layers);
    assert_eq!(
        restored_doc.apertures, before_doc.apertures,
        "aperture namespace restored"
    );
    assert_eq!(w.info().layer_ids, before_info.layer_ids);
    assert_eq!(w.info().active_layer_id.as_deref(), Some(ids[1].as_str()));
    // Redo removes the very same layer again; a further Undo brings it back once more.
    w.redo();
    assert_eq!(w.layers().len(), 2);
    w.undo();
    assert_eq!(w.layers(), before_rows);
    // Objects of the restored layer are the very same ones (ids + geometry).
    assert_eq!(w.objects(&ids[1]).len(), 5);
}

#[test]
fn deleting_the_active_layer_picks_the_layer_below_else_above_else_none() {
    let mut w = W::new("neighbour");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]); // panel: A B C, A active
    w.remove(&ids[0], true).unwrap();
    assert_eq!(
        w.info().active_layer_id.as_deref(),
        Some(ids[1].as_str()),
        "below"
    );
    // Bottom layer active: neighbour is the one above.
    let rev = w.rev();
    w.svc
        .layers_set_active(
            &w.doc,
            &rev,
            SetActiveLayerParams {
                expected_workspace_revision: w.wrev(),
                layer_id: Some(ids[2].clone()),
            },
        )
        .unwrap();
    w.remove(&ids[2], true).unwrap();
    assert_eq!(
        w.info().active_layer_id.as_deref(),
        Some(ids[1].as_str()),
        "above"
    );
    // Last layer: allowed, Workspace stays open and empty.
    w.remove(&ids[1], true).unwrap();
    let info = w.info();
    assert!(info.layer_ids.is_empty() && info.display_order.is_empty());
    assert_eq!(info.active_layer_id, None);
    assert!(w.svc.render_snapshot(&w.doc).unwrap().layers.is_empty());
    // Undo of the three deletions walks back layer by layer and re-activates the deleted active one.
    w.undo();
    assert_eq!(w.info().active_layer_id.as_deref(), Some(ids[1].as_str()));
    w.undo();
    assert_eq!(w.info().active_layer_id.as_deref(), Some(ids[2].as_str()));
    w.undo();
    assert_eq!(w.info().active_layer_id.as_deref(), Some(ids[0].as_str()));
    assert_eq!(w.layers().len(), 3);
}

#[test]
fn empty_workspace_can_be_repopulated_after_last_layer_delete() {
    let mut w = W::new("repopulate");
    let ids = w.import_ok(&["in/A.gbr"]);
    w.remove(&ids[0], true).unwrap();
    let again = w.import_ok(&["in/A.gbr"]);
    assert_ne!(again[0], ids[0], "LayerIds are never reused");
    assert_eq!(w.layers().len(), 1);
    assert_eq!(w.info().active_layer_id.as_deref(), Some(again[0].as_str()));
}

#[test]
fn history_byte_budget_evicting_a_remove_drops_its_side_data() {
    let dir = temp_dir("evict");
    std::fs::write(dir.join("in/A.gbr"), A).unwrap();
    let canonical = dir.canonicalize().unwrap();
    let mut svc = ApplicationService::with_file_access_and_history_limits(
        FileAccessPolicy::new(canonical.clone(), [canonical.clone()], [canonical.clone()]),
        2,
        8 * 1024 * 1024,
    )
    .unwrap();
    let doc = svc.document_new().unwrap().document_id;
    let path = canonical.join("in/A.gbr").to_string_lossy().into_owned();
    let mut rev = "0".to_string();
    // Import + remove, many times: with 2 history entries the old ones are evicted.
    for _ in 0..6 {
        let imported = svc
            .import_gerber_layers(
                &doc,
                &rev,
                ImportGerberLayersParams {
                    paths: vec![path.clone()],
                },
            )
            .unwrap();
        let removed = svc
            .remove_layer(
                &doc,
                &imported.revision,
                RemoveLayerParams {
                    layer_id: imported.layers[0].layer_id.clone(),
                    allow_non_empty: true,
                },
            )
            .unwrap();
        rev = removed.revision;
    }
    let info = svc.document_get(&doc).unwrap();
    assert!(info.undo_entries <= 2);
    assert!(info.history_truncated_entries >= 4);
    assert!(info.layer_ids.is_empty());
}

// ---------------------------------------------------------- edit guards

#[test]
fn layer_lock_and_class_lock_reject_every_edit_through_the_service() {
    let mut w = W::new("locks");
    let ids = w.import_ok(&["in/B.gbr"]);
    let layer = &ids[0];
    let objects = w.objects(layer);
    let macro_object = objects
        .iter()
        .find(|o| match &o.geometry {
            SemanticGeometry::Flash { aperture_id, .. } => w.shapes().iter().any(|(id, shape)| {
                id == aperture_id && matches!(shape, ApertureShape::Macro { .. })
            }),
            _ => false,
        })
        .expect("AM flash")
        .object_id
        .clone();
    let circle_object = objects
        .iter()
        .find(|o| match &o.geometry {
            SemanticGeometry::Flash { aperture_id, .. } => w.shapes().iter().any(|(id, shape)| {
                id == aperture_id && matches!(shape, ApertureShape::Circle { .. })
            }),
            _ => false,
        })
        .unwrap()
        .object_id
        .clone();
    // Lock only the AM category: circles stay editable, AM is selectable but not editable.
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::ApertureMacro),
            locked: Some(true),
            ..Default::default()
        }],
        ..w.patch(layer)
    });
    let before = w.info();
    let hit = w
        .svc
        .objects_select_rect(
            &w.doc,
            SelectRectParams {
                layer_id: layer.clone(),
                rect_mm: editor_core::BoundsMm {
                    min_x_mm: 6.,
                    min_y_mm: 0.,
                    max_x_mm: 8.,
                    max_y_mm: 2.,
                },
                mode: editor_core::hit_test::SelectRectMode::Crossing,
                selectable_only: true,
            },
        )
        .unwrap();
    assert!(
        hit.object_ids.contains(&macro_object),
        "locked but still selectable"
    );
    let refused = w.move_object(layer, &macro_object, 1.0).unwrap_err();
    assert_eq!(refused.code, "OBJECT_CLASS_LOCKED");
    assert_eq!(refused.details["object_ids"], json!([macro_object]));
    for op in [
        "objects.rotate",
        "objects.mirror",
        "objects.duplicate",
        "objects.delete",
        "objects.set_properties",
        "edit.batch",
    ] {
        let params = match op {
            "objects.rotate" => {
                json!({"layer_id":layer,"object_ids":[macro_object],"angle_deg":90,"pivot_mm":{"x_mm":0,"y_mm":0}})
            }
            "objects.mirror" => {
                json!({"layer_id":layer,"object_ids":[macro_object],"axis":{"kind":"vertical","coordinate_mm":0}})
            }
            "objects.duplicate" => {
                json!({"layer_id":layer,"object_ids":[macro_object],"dx_mm":1,"dy_mm":0})
            }
            "objects.delete" => json!({"layer_id":layer,"object_ids":[macro_object]}),
            "objects.set_properties" => {
                json!({"layer_id":layer,"object_ids":[macro_object],"width_mm":1.0})
            }
            _ => {
                json!({"layer_id":layer,"steps":[{"op":"objects.move","object_ids":[macro_object],"dx_mm":1,"dy_mm":0}]})
            }
        };
        let reply = json_call(
            &mut w.svc,
            op,
            Some(&w.doc.clone()),
            Some(&before.revision),
            params,
        );
        assert_eq!(
            reply["error"]["code"], "OBJECT_CLASS_LOCKED",
            "{op}: {reply}"
        );
    }
    assert_eq!(
        w.info(),
        before,
        "rejections leave revision, history and dirty untouched"
    );
    w.move_object(layer, &circle_object, 1.0)
        .expect("circles are not locked");
    // Layer lock beats everything and keeps its own code.
    w.update(LayerUpdateParams {
        locked: Some(true),
        ..w.patch(layer)
    });
    assert_eq!(
        w.move_object(layer, &circle_object, 1.0).unwrap_err().code,
        "LAYER_LOCKED"
    );
    // Unlocking the class alone does not unlock the layer; both must be open.
    w.update(LayerUpdateParams {
        locked: Some(false),
        classes: vec![ClassStyleUpdate {
            class: None,
            locked: Some(false),
            ..Default::default()
        }],
        ..w.patch(layer)
    });
    w.move_object(layer, &macro_object, 1.0).unwrap();
}

#[test]
fn text_goes_to_the_target_layer_and_respects_its_locks() {
    let mut w = W::new("text");
    let ids = w.import_ok(&["in/A.gbr", "in/C.gbr"]);
    let font = builtin_stroke_font().identity;
    let params = |layer: &str| TextParams {
        layer_id: layer.into(),
        layout: default_layout(),
        font: font.clone(),
    };
    // Active layer = ids[0]; text explicitly targets it.
    let rev = w.rev();
    let created = w.svc.text_create(&w.doc, &rev, params(&ids[0])).unwrap();
    let a_objects = w.objects(&ids[0]);
    assert!(
        created
            .generated_object_ids
            .iter()
            .all(|id| a_objects.iter().any(|o| &o.object_id == id))
    );
    assert_eq!(
        w.objects(&ids[1]).len(),
        2,
        "no text leaked into the other layer"
    );
    let shapes = editor_core::workspace::aperture_shape_map(&[]);
    let generated: Vec<_> = a_objects
        .iter()
        .filter(|o| created.generated_object_ids.contains(&o.object_id))
        .collect();
    assert!(generated.iter().all(
        |o| editor_core::workspace::classify_object(o, &shapes) == DisplayClass::GeneratedText
    ));
    // Locked target layer / locked GeneratedText category → rejected, nothing added.
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::GeneratedText),
            locked: Some(true),
            ..Default::default()
        }],
        ..w.patch(&ids[0])
    });
    let before = w.info();
    let rev = w.rev();
    assert_eq!(
        w.svc
            .text_create(&w.doc, &rev, params(&ids[0]))
            .unwrap_err()
            .code,
        "OBJECT_CLASS_LOCKED"
    );
    assert_eq!(w.info(), before);
    w.update(LayerUpdateParams {
        locked: Some(true),
        ..w.patch(&ids[1])
    });
    let rev = w.rev();
    assert_eq!(
        w.svc
            .text_create(&w.doc, &rev, params(&ids[1]))
            .unwrap_err()
            .code,
        "LAYER_LOCKED"
    );
    // Copies of generated text stay classified as text after Duplicate.
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::GeneratedText),
            locked: Some(false),
            ..Default::default()
        }],
        ..w.patch(&ids[0])
    });
    let rev = w.rev();
    let copy = w
        .svc
        .objects_duplicate(
            &w.doc,
            &rev,
            MoveParams {
                layer_id: ids[0].clone(),
                object_ids: vec![created.generated_object_ids[0].clone()],
                dx_mm: 3.0,
                dy_mm: 0.0,
            },
        )
        .unwrap();
    let duplicated = w
        .objects(&ids[0])
        .into_iter()
        .find(|o| o.object_id == copy.changed_object_ids[0])
        .unwrap();
    assert!(matches!(
        duplicated.origin,
        editor_core::ObjectOrigin::GeneratedText { .. }
    ));
}

#[test]
fn selection_policy_layer_and_category_flags_filter_hit_and_window_queries() {
    let mut w = W::new("select");
    let ids = w.import_ok(&["in/B.gbr"]);
    let layer = &ids[0];
    let window = editor_core::BoundsMm {
        min_x_mm: -1.,
        min_y_mm: -1.,
        max_x_mm: 12.,
        max_y_mm: 12.,
    };
    let select = |w: &W| {
        w.svc
            .objects_select_rect(
                &w.doc,
                SelectRectParams {
                    layer_id: layer.clone(),
                    rect_mm: window,
                    mode: editor_core::hit_test::SelectRectMode::Crossing,
                    selectable_only: true,
                },
            )
            .unwrap()
            .object_ids
    };
    let hit_at = |w: &W, x: f64, y: f64| {
        w.svc
            .objects_hit_test(
                &w.doc,
                HitTestParams {
                    layer_id: layer.clone(),
                    point: HitTestPoint { x_mm: x, y_mm: y },
                    tolerance_mm: 0.,
                    selectable_only: true,
                },
            )
            .unwrap()
            .object_ids
    };
    assert_eq!(select(&w).len(), 5);
    assert_eq!(hit_at(&w, 10., 10.).len(), 1);
    // Circle category hidden: not selectable (effective_selectable needs visible).
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::FlashCircle),
            visible: Some(false),
            ..Default::default()
        }],
        ..w.patch(layer)
    });
    assert_eq!(select(&w).len(), 4);
    assert!(hit_at(&w, 10., 10.).is_empty());
    // Visible again, but not selectable.
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::FlashCircle),
            visible: Some(true),
            selectable: Some(false),
            ..Default::default()
        }],
        ..w.patch(layer)
    });
    assert_eq!(select(&w).len(), 4);
    // Whole layer: hidden or non-selectable → nothing; the pure manufacturing query is unchanged.
    w.update(LayerUpdateParams {
        selectable: Some(false),
        ..w.patch(layer)
    });
    assert!(select(&w).is_empty());
    let pure = w
        .svc
        .objects_select_rect(
            &w.doc,
            SelectRectParams {
                layer_id: layer.clone(),
                rect_mm: window,
                mode: editor_core::hit_test::SelectRectMode::Crossing,
                selectable_only: false,
            },
        )
        .unwrap();
    assert_eq!(
        pure.object_ids.len(),
        5,
        "manufacturing query ignores workspace policy"
    );
}

#[test]
fn classification_of_every_category_comes_from_semantics() {
    let mut w = W::new("classes");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]);
    let snapshot = w.svc.render_snapshot(&w.doc).unwrap();
    let shapes = editor_core::workspace::aperture_shape_map(&snapshot.apertures);
    let mut seen = std::collections::BTreeSet::new();
    for layer in &snapshot.layers {
        for object in &layer.objects {
            seen.insert(editor_core::workspace::classify_object(object, &shapes));
        }
    }
    for expected in [
        DisplayClass::Stroke,
        DisplayClass::FlashCircle,
        DisplayClass::FlashRectangle,
        DisplayClass::FlashObround,
        DisplayClass::FlashPolygon,
        DisplayClass::ApertureMacro,
        DisplayClass::RegionFreeform,
    ] {
        assert!(seen.contains(&expected), "{expected:?} missing in {seen:?}");
    }
    // Deterministic: classifying twice yields the same classes.
    let again: Vec<_> = snapshot.layers[0]
        .objects
        .iter()
        .map(|o| editor_core::workspace::classify_object(o, &shapes))
        .collect();
    let repeat: Vec<_> = w.svc.render_snapshot(&w.doc).unwrap().layers[0]
        .objects
        .iter()
        .map(|o| editor_core::workspace::classify_object(o, &shapes))
        .collect();
    assert_eq!(again, repeat);
    assert_eq!(ids.len(), 3);
}

#[test]
fn category_colour_mode_uses_overrides_and_distinguishes_block_from_region() {
    let mut w = W::new("catcolour");
    let ids = w.import_ok(&["in/C.gbr"]);
    let row = &w.layers()[0];
    assert_eq!(row.color_mode, ColorMode::LayerColor);
    let plain: Vec<_> = row.classes.iter().map(|c| c.effective_color).collect();
    assert!(plain.iter().all(|c| *c == row.base_color));
    w.update(LayerUpdateParams {
        color_mode: Some(ColorMode::CategoryColor),
        ..w.patch(&ids[0])
    });
    let row = &w.layers()[0];
    let colour_of = |row: &LayerInfo, class| {
        row.classes
            .iter()
            .find(|c| c.class == class)
            .unwrap()
            .effective_color
    };
    let block = colour_of(row, DisplayClass::ApertureBlock);
    let region = colour_of(row, DisplayClass::RegionFreeform);
    assert!(block.delta_e(region) >= 40., "{block:?} vs {region:?}");
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::RegionFreeform),
            color_override: Some("#ff0000".into()),
            ..Default::default()
        }],
        ..w.patch(&ids[0])
    });
    let row = &w.layers()[0];
    assert_eq!(
        colour_of(row, DisplayClass::RegionFreeform),
        Color::rgb(255, 0, 0)
    );
    // "inherit" removes the override again.
    w.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::RegionFreeform),
            color_override: Some("inherit".into()),
            ..Default::default()
        }],
        ..w.patch(&ids[0])
    });
    assert_eq!(
        colour_of(&w.layers()[0], DisplayClass::RegionFreeform),
        region
    );
    // Restore defaults.
    w.update(LayerUpdateParams {
        reset_classes: true,
        ..w.patch(&ids[0])
    });
    assert!(
        w.layers()[0]
            .classes
            .iter()
            .all(|c| c.color_override.is_none() && c.visible && c.selectable && !c.locked)
    );
}

#[test]
fn update_many_is_atomic_across_layers() {
    let mut w = W::new("many");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr"]);
    let before = w.layers();
    let rev = w.rev();
    let bad = w.svc.layers_update_many(
        &w.doc,
        &rev,
        UpdateLayersParams {
            expected_workspace_revision: w.wrev(),
            updates: vec![
                LayerPatch {
                    layer_id: ids[0].clone(),
                    visible: Some(false),
                    ..Default::default()
                },
                LayerPatch {
                    layer_id: ids[1].clone(),
                    base_color: Some("nope".into()),
                    ..Default::default()
                },
            ],
        },
    );
    assert!(bad.is_err());
    assert_eq!(w.layers(), before, "nothing of a failing batch is applied");
    let wrev = w.wrev();
    let info = w
        .svc
        .layers_update_many(
            &w.doc,
            &rev,
            UpdateLayersParams {
                expected_workspace_revision: wrev.clone(),
                updates: ids
                    .iter()
                    .map(|id| LayerPatch {
                        layer_id: id.clone(),
                        visible: Some(false),
                        ..Default::default()
                    })
                    .collect(),
            },
        )
        .unwrap();
    assert_eq!(
        info.workspace_revision.parse::<u64>().unwrap(),
        wrev.parse::<u64>().unwrap() + 1,
        "one workspace revision for the whole batch"
    );
    assert!(w.layers().iter().all(|r| !r.visible));
    // A stale workspace revision is a typed conflict.
    let stale = w.svc.layers_update_many(
        &w.doc,
        &rev,
        UpdateLayersParams {
            expected_workspace_revision: wrev,
            updates: vec![],
        },
    );
    assert_eq!(stale.unwrap_err().code, "REVISION_CONFLICT");
}

#[test]
fn visible_bounds_follow_layer_solo_and_category_visibility() {
    let mut w = W::new("bounds");
    let ids = w.import_ok(&["in/A.gbr", "in/C.gbr"]);
    let all = w.svc.document_bounds(&w.doc).unwrap().bounds.unwrap();
    let visible = w.svc.visible_bounds(&w.doc).unwrap().bounds.unwrap();
    assert_eq!(all, visible);
    // Hide C (holds the far flash at x=20): visible bounds shrink, document bounds do not.
    w.update(LayerUpdateParams {
        visible: Some(false),
        ..w.patch(&ids[1])
    });
    let shrunk = w.svc.visible_bounds(&w.doc).unwrap().bounds.unwrap();
    assert!(shrunk.max_x_mm < all.max_x_mm);
    assert_eq!(w.svc.document_bounds(&w.doc).unwrap().bounds.unwrap(), all);
    let layer_only = w
        .svc
        .layer_bounds(
            &w.doc,
            LayerBoundsParams {
                layer_id: ids[0].clone(),
            },
        )
        .unwrap()
        .bounds
        .unwrap();
    assert_eq!(shrunk, layer_only);
    // Everything hidden → no bounds.
    w.update(LayerUpdateParams {
        visible: Some(false),
        ..w.patch(&ids[0])
    });
    assert!(w.svc.visible_bounds(&w.doc).unwrap().bounds.is_none());
}

// ---------------------------------------------------------------- export

#[test]
fn export_only_the_chosen_layer_reopens_equal_and_never_links_or_cleans() {
    let mut w = W::new("export");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr", "in/C.gbr"]);
    let dirty_before = w.info().dirty;
    assert!(dirty_before);
    let sources_before: Vec<_> = ["A", "B", "C"]
        .iter()
        .map(|n| sha_of(&w.dir.join(format!("in/{n}.gbr"))))
        .collect();
    let mut counts = Vec::new();
    for (name, id) in ["A", "B", "C"].iter().zip(&ids) {
        let result = w.export(id, &format!("{name}.gbr")).unwrap();
        assert_eq!(result.layer_id, *id);
        let (reopened_doc, reopened) = w.reopen(&format!("{name}.gbr"));
        let original = w.objects(id);
        assert_eq!(reopened.len(), original.len(), "{name}");
        for (a, b) in reopened.iter().zip(&original) {
            assert_eq!(a.exposure, b.exposure);
            assert_eq!(
                std::mem::discriminant(&a.geometry),
                std::mem::discriminant(&b.geometry),
                "{name}"
            );
        }
        counts.push(reopened.len());
        w.svc.close(&reopened_doc, "0", true).unwrap();
    }
    assert_eq!(counts, [5, 5, 2]);
    let info = w.info();
    assert!(info.dirty, "Export never clears the Workspace dirty state");
    assert_eq!(info.source_path, "", "no source link created by Export");
    assert_eq!(info.layer_ids.len(), 3);
    // Source files are byte-identical; exported files exist next to them.
    let sources_after: Vec<_> = ["A", "B", "C"]
        .iter()
        .map(|n| sha_of(&w.dir.join(format!("in/{n}.gbr"))))
        .collect();
    assert_eq!(sources_before, sources_after);
    // Existing output is never replaced.
    let again = w.export(&ids[0], "A.gbr").unwrap_err();
    assert_eq!(again.code, "CONFIRMATION_REQUIRED");
    // Unknown layer is typed.
    assert_eq!(
        w.export("layer-404", "X.gbr").unwrap_err().code,
        "NOT_FOUND"
    );
}

#[test]
fn export_uses_document_level_precision_for_every_layer() {
    let mut w = W::new("precision");
    let ids = w.import_ok(&["in/A.gbr", "in/B.gbr"]);
    let rev = w.rev();
    w.svc
        .set_manufacturing_precision(
            &w.doc,
            &rev,
            ManufacturingPrecision {
                resolution_mm: 0.001,
            },
        )
        .unwrap();
    w.move_object(&ids[0], &w.objects(&ids[0])[0].object_id.clone(), 0.1234567)
        .unwrap();
    w.export(&ids[0], "A.gbr").unwrap();
    w.export(&ids[1], "B.gbr").unwrap();
    let (_, objects) = w.reopen("A.gbr");
    let first = objects
        .iter()
        .find_map(|o| match &o.geometry {
            SemanticGeometry::Flash { center, .. } => Some(*center),
            _ => None,
        })
        .unwrap();
    assert!(
        (first.x_mm - 0.123).abs() < 1e-9,
        "quantized to 1 µm: {first:?}"
    );
    assert!(!w.info().export_policy_dirty, "policy applied to an output");
    assert!(w.info().dirty, "but the Workspace is still unsaved");
}

#[test]
fn gerber_has_no_direct_save_operation() {
    let mut w = W::new("nosave");
    w.import_ok(&["in/A.gbr"]);
    let caps = w.svc.capabilities();
    for op in ["document.save", "document.save_as"] {
        assert!(!caps.supported_operations.iter().any(|s| s == op), "{op}");
        let reply = json_call(&mut w.svc, op, Some(&w.doc.clone()), Some("1"), json!({}));
        assert_eq!(reply["error"]["code"], "UNSUPPORTED_OPERATION", "{op}");
    }
    assert!(
        caps.supported_operations
            .iter()
            .any(|s| s == "project.save")
    );
}

#[test]
fn capabilities_are_consistent_with_the_supported_operations() {
    let service = ApplicationService::with_file_access(FileAccessPolicy::new(
        std::env::temp_dir(),
        [std::env::temp_dir()],
        [std::env::temp_dir()],
    ));
    let caps = service.capabilities();
    assert_eq!(
        caps.stage,
        "S4-C4 Alignment / Distribution (Mac-first bounded)"
    );
    for op in [
        "document.new",
        "document.import_gerber_layers",
        "document.import_gerber_layer",
        "document.create_empty_layer",
        "document.remove_layer",
        "layers.reorder",
        "layers.set_active",
        "layers.set_solo",
        "layers.update_many",
        "layer.summary",
        "document.visible_bounds",
        "document.set_manufacturing_precision",
        "gerber.export_layer",
    ] {
        assert!(caps.supported_operations.iter().any(|s| s == op), "{op}");
        assert!(!caps.unsupported_operations.iter().any(|s| s == op), "{op}");
    }
    // Gate 0.1: a supported export must not be declared unsupported elsewhere.
    assert!(
        caps.supported_operations
            .iter()
            .any(|s| s == "gerber.export_layer")
    );
    assert!(
        !caps
            .unsupported_gerber_features
            .iter()
            .any(|s| s.contains("production export"))
    );
    // Every advertised operation is dispatchable (never UNSUPPORTED_OPERATION).
    let mut service = service;
    for op in &caps.supported_operations {
        let reply = json_call(&mut service, op, None, None, json!({}));
        assert_ne!(
            reply["error"]["code"], "UNSUPPORTED_OPERATION",
            "{op}: {reply}"
        );
    }
}

// -------------------------------------------------------------- headless

#[test]
fn headless_multi_gerber_workflow_without_any_gui() {
    let mut w = W::new("headless");
    let doc = w.doc.clone();
    // Create Workspace → Import A → Import B + C → list.
    let a = w.import_ok(&["in/A.gbr"]);
    let bc = w.import_ok(&["in/B.gbr", "in/C.gbr"]);
    let rows = w.layers();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].layer_id, bc[0]);
    // Query objects per layer over the JSON boundary.
    let reply = json_call(
        &mut w.svc,
        "objects.query",
        Some(&doc),
        None,
        json!({"layer_id": bc[0], "limit": 10}),
    );
    assert_eq!(reply["result"]["total"], 5, "{reply}");
    // Edit B, hide C (workspace state only).
    let b_first = w.objects(&bc[0])[0].object_id.clone();
    w.move_object(&bc[0], &b_first, 2.0).unwrap();
    w.update(LayerUpdateParams {
        visible: Some(false),
        ..w.patch(&bc[1])
    });
    // Export A and B, reopen each, compare geometry.
    w.export(&a[0], "A.gbr").unwrap();
    w.export(&bc[0], "B.gbr").unwrap();
    let (_, a_objects) = w.reopen("A.gbr");
    let (_, b_objects) = w.reopen("B.gbr");
    assert_eq!(a_objects.len(), 5);
    assert_eq!(b_objects.len(), 5);
    let moved = match &b_objects[0].geometry {
        SemanticGeometry::Flash { center, .. } => center.x_mm,
        other => panic!("{other:?}"),
    };
    assert!((moved - 3.0).abs() < 1e-6, "edit is in the export: {moved}");
    // Undo the edit, Remove C, Undo Remove.
    w.undo();
    let moved_back = match &w.objects(&bc[0])[0].geometry {
        SemanticGeometry::Flash { center, .. } => center.x_mm,
        other => panic!("{other:?}"),
    };
    assert!((moved_back - 1.0).abs() < 1e-9);
    let removed = w.remove(&bc[1], true).unwrap();
    assert_eq!(removed.summary.object_count, 2);
    assert_eq!(w.layers().len(), 2);
    w.undo();
    assert_eq!(w.layers().len(), 3);
    let restored = w
        .layers()
        .into_iter()
        .find(|r| r.layer_id == bc[1])
        .unwrap();
    assert!(!restored.visible, "hidden state survived Remove + Undo");
    assert!(w.svc.validate(&doc).unwrap().valid);
}

#[test]
fn ten_layers_of_a_thousand_objects_stay_within_interactive_budgets() {
    let mut w = W::new("perf");
    let mut rels = Vec::new();
    for layer in 0..10 {
        let mut text =
            String::from("%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.4*%\n%ADD11R,0.6X0.3*%\nD10*\n");
        for i in 0..1000 {
            if i == 500 {
                text.push_str("D11*\n");
            }
            let x = (i % 40) as i64 * 1_000_000;
            let y = (i / 40) as i64 * 1_000_000;
            text.push_str(&format!("X{}Y{}D03*\n", x + layer * 100_000, y));
        }
        text.push_str("M02*\n");
        let rel = format!("in/perf{layer}.gbr");
        std::fs::write(w.dir.join(&rel), text).unwrap();
        rels.push(rel);
    }
    let refs: Vec<&str> = rels.iter().map(String::as_str).collect();
    let t = std::time::Instant::now();
    let ids = w.import_ok(&refs);
    let add_ms = t.elapsed().as_secs_f64() * 1e3;
    assert_eq!(ids.len(), 10);
    let t = std::time::Instant::now();
    let snapshot = w.svc.render_snapshot(&w.doc).unwrap();
    let snapshot_ms = t.elapsed().as_secs_f64() * 1e3;
    assert_eq!(
        snapshot
            .layers
            .iter()
            .map(|l| l.objects.len())
            .sum::<usize>(),
        10_000
    );
    let t = std::time::Instant::now();
    let visible = w.svc.visible_bounds(&w.doc).unwrap();
    let fit_ms = t.elapsed().as_secs_f64() * 1e3;
    assert!(visible.bounds.is_some());
    let t = std::time::Instant::now();
    w.update(LayerUpdateParams {
        visible: Some(false),
        ..w.patch(&ids[3])
    });
    let toggle_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = std::time::Instant::now();
    let hit = w
        .svc
        .objects_hit_test(
            &w.doc,
            HitTestParams {
                layer_id: ids[0].clone(),
                point: HitTestPoint {
                    x_mm: 3.0,
                    y_mm: 2.0,
                },
                tolerance_mm: 0.1,
                selectable_only: true,
            },
        )
        .unwrap();
    let hit_ms = t.elapsed().as_secs_f64() * 1e3;
    assert!(!hit.object_ids.is_empty());
    let t = std::time::Instant::now();
    w.export(&ids[0], "perf0.gbr").unwrap();
    let export_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = std::time::Instant::now();
    w.undo_all_layers_for_perf();
    let undo_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "S4B1_PERF layers=10 objects=10000 add_ms={add_ms:.1} snapshot_ms={snapshot_ms:.1} visible_fit_ms={fit_ms:.1} visible_toggle_ms={toggle_ms:.3} hit_test_ms={hit_ms:.1} export_layer_ms={export_ms:.1} undo_batch_ms={undo_ms:.1}"
    );
    // Generous CI-safe ceilings; the real numbers are printed above and recorded as evidence.
    assert!(add_ms < 30_000.0 && snapshot_ms < 5_000.0 && toggle_ms < 500.0);
}

impl W {
    fn undo_all_layers_for_perf(&mut self) {
        // The import batch is the only history entry that changed structure.
        while self.info().undo_entries > 0 {
            self.undo();
        }
        assert!(self.layers().is_empty());
    }
}
