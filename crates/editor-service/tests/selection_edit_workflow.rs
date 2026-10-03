//! Real two-layer service/JSON workflows and independent coordinate/history assertions.
use editor_core::{MmPoint, SemanticGeometry};
use editor_service::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Run {
    s: ApplicationService,
    id: String,
    dir: PathBuf,
    layer_order: Vec<String>,
}
impl Run {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-i1-service-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let a = dir.join("lower.gbr");
        let b = dir.join("upper.gbr");
        std::fs::write(&a,b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX2000000Y3000000D03*\nX6000000Y3000000D03*\n%LPC*%\nX6000000Y3000000D03*\nM02*\n").unwrap();
        std::fs::write(&b,b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nM02*\n").unwrap();
        let info = s.document_new().unwrap();
        let id = info.document_id;
        let imported = s
            .import_gerber_layers(
                &id,
                &info.revision,
                ImportGerberLayersParams {
                    paths: vec![a.to_string_lossy().into(), b.to_string_lossy().into()],
                },
            )
            .unwrap();
        let layer_order = imported.layers.into_iter().map(|l| l.layer_id).collect();
        Self {
            s,
            id,
            dir,
            layer_order,
        }
    }
    fn info(&self) -> DocumentInfo {
        self.s.document_get(&self.id).unwrap()
    }
    fn scene(&self) -> RenderSnapshot {
        let mut scene = self.s.render_snapshot(&self.id).unwrap();
        scene
            .layers
            .sort_by_key(|l| self.layer_order.iter().position(|id| id == &l.id).unwrap());
        scene
    }
    fn groups(&self) -> Vec<SelectionGroup> {
        self.scene()
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect()
    }
    fn edit(
        &mut self,
        groups: Vec<SelectionGroup>,
        operation: SelectionEdit,
    ) -> Result<EditSelectionResult, ServiceError> {
        let rev = self.info().revision;
        self.s
            .objects_edit_selection(&self.id, &rev, EditSelectionParams { groups, operation })
    }
    fn request(&mut self, params: Value, revision: Option<String>) -> Value {
        let rev = revision.unwrap_or_else(|| self.info().revision);
        self.s.execute_json(&json!({"api_version":1,"request_id":"i1-real","op":"objects.edit_selection","document_id":self.id,"expected_revision":rev,"params":params}).to_string())
    }
    fn undo(&mut self) {
        self.s
            .history_undo(&self.id, &self.info().revision)
            .unwrap();
    }
    fn redo(&mut self) {
        self.s
            .history_redo(&self.id, &self.info().revision)
            .unwrap();
    }
    fn unchanged(&self, info: &DocumentInfo, scene: &RenderSnapshot) {
        assert_eq!(&self.info(), info);
        let now = self.scene();
        assert_eq!(now.layers, scene.layers);
        assert_eq!(now.apertures, scene.apertures);
        assert_eq!(now.block_definitions, scene.block_definitions);
    }
}
#[test]
fn json_move_rotate_mirror_independent_coordinates_and_exact_history() {
    let mut r = Run::new();
    let groups = r.groups();
    let before = r.scene();
    let info = r.info();
    let response = r.request(
        json!({"groups":groups,"operation":{"kind":"move","dx_mm":2.,"dy_mm":-3.}}),
        None,
    );
    assert_eq!(response["status"], "completed", "{response}");
    assert_eq!(response["result"]["edit"]["undo_entries_added"], 1);
    let moved = r.scene();
    assert_eq!(r.info().undo_entries, info.undo_entries + 1);
    assert_eq!(
        r.info().revision.parse::<u64>().unwrap(),
        info.revision.parse::<u64>().unwrap() + 1
    );
    for (old, new) in before.layers.iter().zip(&moved.layers) {
        assert_eq!(old.id, new.id);
        for (a, b) in old.objects.iter().zip(&new.objects) {
            assert_eq!(a.object_id, b.object_id);
            assert_eq!(a.exposure, b.exposure);
        }
    }
    match &moved.layers[0].objects[0].geometry {
        SemanticGeometry::Flash { center, .. } => assert_eq!(*center, MmPoint::new(4., 0.)),
        _ => panic!(),
    }
    match &moved.layers[1].objects[0].geometry {
        SemanticGeometry::Arc { path, .. } => {
            assert_eq!(path.start, MmPoint::new(3., -3.));
            assert_eq!(path.end, MmPoint::new(2., -2.));
            assert_eq!(path.center, MmPoint::new(2., -3.));
        }
        _ => panic!(),
    }
    r.undo();
    assert_eq!(r.scene().layers, before.layers);
    r.redo();
    assert_eq!(r.scene().layers, moved.layers);
    r.undo();
    r.edit(
        groups.clone(),
        SelectionEdit::Rotate {
            angle_deg: 90.,
            pivot_mm: MmPoint::new(0., 0.),
        },
    )
    .unwrap();
    match &r.scene().layers[0].objects[0].geometry {
        SemanticGeometry::Flash { center, .. } => assert_eq!(*center, MmPoint::new(-3., 2.)),
        _ => panic!(),
    };
    r.undo();
    assert_eq!(r.scene().layers, before.layers);
    r.edit(
        groups,
        SelectionEdit::Mirror {
            axis: MirrorAxis::Vertical { coordinate_mm: 0. },
        },
    )
    .unwrap();
    match (
        &before.layers[1].objects[0].geometry,
        &r.scene().layers[1].objects[0].geometry,
    ) {
        (SemanticGeometry::Arc { path: a, .. }, SemanticGeometry::Arc { path: b, .. }) => {
            assert_eq!(b.start, MmPoint::new(-1., 0.));
            assert_eq!(b.end, MmPoint::new(0., 1.));
            assert_ne!(a.direction, b.direction);
        }
        _ => panic!(),
    }
    r.undo();
    assert_eq!(r.scene().layers, before.layers);
}
#[test]
fn duplicate_delete_preserve_each_layer_exposure_and_definitions() {
    let mut r = Run::new();
    let before = r.scene();
    let groups = r.groups();
    let result = r
        .edit(
            groups,
            SelectionEdit::Duplicate {
                dx_mm: 0.,
                dy_mm: 0.,
            },
        )
        .unwrap();
    let duplicated = r.scene();
    assert_eq!(duplicated.apertures, before.apertures);
    let mut all = std::collections::HashSet::new();
    for ((a, b), g) in before
        .layers
        .iter()
        .zip(&duplicated.layers)
        .zip(&result.groups)
    {
        assert_eq!(a.id, b.id);
        assert_eq!(a.id, g.layer_id);
        assert_eq!(b.objects.len(), a.objects.len() * 2);
        for (old, pair) in a.objects.iter().zip(b.objects.chunks_exact(2)) {
            assert_eq!(old, &pair[0]);
            assert_eq!(old.geometry, pair[1].geometry);
            assert_eq!(old.exposure, pair[1].exposure);
            assert_ne!(old.object_id, pair[1].object_id);
        }
        for o in &b.objects {
            assert!(all.insert(o.object_id.clone()));
        }
    }
    r.undo();
    assert_eq!(r.scene().layers, before.layers);
    r.redo();
    assert_eq!(r.scene().layers, duplicated.layers);
    r.edit(result.groups, SelectionEdit::Delete).unwrap();
    assert_eq!(r.scene().layers, before.layers);
    r.undo();
    assert_eq!(r.scene().layers, duplicated.layers);
}
#[test]
fn late_group_errors_nonfinite_resource_and_stale_leave_redo_and_content_intact() {
    let mut r = Run::new();
    let groups = r.groups();
    r.edit(
        groups.clone(),
        SelectionEdit::Move {
            dx_mm: 1.,
            dy_mm: 0.,
        },
    )
    .unwrap();
    r.undo();
    let info = r.info();
    let scene = r.scene();
    let base = json!({"groups":groups,"operation":{"kind":"move","dx_mm":1.,"dy_mm":0.}});
    let mut failures = Vec::new();
    for key in ["layer_id", "object_ids"] {
        let mut p = base.clone();
        p["groups"][1][key] = if key == "layer_id" {
            json!("absent-layer")
        } else {
            json!(["absent-object"])
        };
        failures.push(p);
    }
    let mut p = base.clone();
    p["groups"][1] = p["groups"][0].clone();
    failures.push(p);
    let mut p = base.clone();
    p["groups"][1]["object_ids"] = json!([groups[1].object_ids[0], groups[1].object_ids[0]]);
    failures.push(p);
    let mut p = base.clone();
    p["groups"][1]["object_ids"] = json!(vec![groups[1].object_ids[0].clone(); 10001]);
    failures.push(p);
    let mut p = base.clone();
    p["operation"]["extra"] = json!(1);
    failures.push(p);
    let mut p = base.clone();
    p["groups"][1]["extra"] = json!(1);
    failures.push(p);
    let mut p = base.clone();
    p["extra"] = json!(1);
    failures.push(p);
    let mut p = base.clone();
    p["operation"] =
        json!({"kind":"rotate","angle_deg":90.,"pivot_mm":{"x_mm":0.,"y_mm":0.,"extra":1}});
    failures.push(p);
    for params in failures {
        let response = r.request(params, None);
        assert_ne!(response["status"], "completed", "{response}");
        r.unchanged(&info, &scene);
    }
    let response = r.request(base, Some("0".into()));
    assert_ne!(response["status"], "completed");
    r.unchanged(&info, &scene);
    for dx in [f64::NAN, f64::INFINITY, 1e10] {
        assert!(
            r.edit(
                groups.clone(),
                SelectionEdit::Move {
                    dx_mm: dx,
                    dy_mm: 0.
                }
            )
            .is_err()
        );
        r.unchanged(&info, &scene);
    }
    r.redo();
    assert_ne!(r.scene().layers, scene.layers);
}
#[test]
fn later_layer_permissions_fail_atomically_and_locked_geometry_remains_queryable() {
    for patch in [
        LayerPatch {
            locked: Some(true),
            ..Default::default()
        },
        LayerPatch {
            visible: Some(false),
            ..Default::default()
        },
        LayerPatch {
            selectable: Some(false),
            ..Default::default()
        },
        LayerPatch {
            classes: vec![ClassStyleUpdate {
                class: Some(DisplayClass::Stroke),
                visible: Some(false),
                ..Default::default()
            }],
            ..Default::default()
        },
        LayerPatch {
            classes: vec![ClassStyleUpdate {
                class: Some(DisplayClass::Stroke),
                selectable: Some(false),
                ..Default::default()
            }],
            ..Default::default()
        },
        LayerPatch {
            classes: vec![ClassStyleUpdate {
                class: Some(DisplayClass::Stroke),
                locked: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        },
    ] {
        let mut r = Run::new();
        let groups = r.groups();
        let info = r.info();
        let mut patch = patch;
        patch.layer_id = groups[1].layer_id.clone();
        r.s.layers_update_many(
            &r.id,
            &info.revision,
            UpdateLayersParams {
                expected_workspace_revision: info.workspace_revision,
                updates: vec![patch],
            },
        )
        .unwrap();
        let info = r.info();
        let scene = r.scene();
        assert!(
            r.edit(
                groups,
                SelectionEdit::Move {
                    dx_mm: 1.,
                    dy_mm: 0.
                }
            )
            .is_err()
        );
        r.unchanged(&info, &scene);
    }
}
#[test]
fn two_layer_edit_export_and_reopen_uses_real_writer() {
    let mut r = Run::new();
    let groups = r.groups();
    r.edit(
        groups.clone(),
        SelectionEdit::Move {
            dx_mm: 2.,
            dy_mm: 3.,
        },
    )
    .unwrap();
    let before = r.scene();
    for (n, group) in groups.into_iter().enumerate() {
        let path = r.dir.join(format!("export-{n}.gbr"));
        r.s.export_layer(
            &r.id,
            &r.info().revision,
            ExportParams {
                layer_id: group.layer_id,
                path: path.to_string_lossy().into(),
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
        .unwrap();
        let opened = r.s.open(path.to_str().unwrap()).unwrap();
        let reopened = r.s.render_snapshot(&opened.document_id).unwrap();
        assert_eq!(
            reopened.layers[0].objects.len(),
            before.layers[n].objects.len()
        );
        for (a, b) in before.layers[n]
            .objects
            .iter()
            .zip(&reopened.layers[0].objects)
        {
            assert_eq!(a.exposure, b.exposure);
            match (&a.geometry, &b.geometry) {
                (
                    SemanticGeometry::Flash { center: a, .. },
                    SemanticGeometry::Flash { center: b, .. },
                ) => assert_eq!(a, b),
                (SemanticGeometry::Arc { path: a, .. }, SemanticGeometry::Arc { path: b, .. }) => {
                    assert_eq!(a, b)
                }
                _ => panic!("unexpected geometry"),
            }
        }
    }
}

#[test]
fn text_groups_stay_independent_and_block_definitions_are_shared() {
    let mut r = Run::new();
    let base = r.groups();
    for (n, text) in ["AB", "CD"].iter().enumerate() {
        r.s.text_create(
            &r.id,
            &r.info().revision,
            TextParams {
                layer_id: base[0].layer_id.clone(),
                font: builtin_stroke_font().identity,
                layout: TextLayout {
                    text: (*text).into(),
                    x_mm: 10. + n as f64 * 10.,
                    y_mm: 10.,
                    height_mm: 3.,
                    tracking_mm: 0.,
                    h_align: HorizontalAlign::Left,
                    v_align: VerticalAlign::Bottom,
                    rotation_deg: 0.,
                    curve_tolerance_mm: editor_text::TOLERANCE_MM,
                    baseline_spacing_mm: 0.,
                    stroke_width_mm: 0.15,
                    outline_offset_mm: 0.,
                },
            },
        )
        .unwrap();
    }
    r.s.blocks_create_definition_from_objects(
        &r.id,
        &r.info().revision,
        CreateBlockDefinitionParams {
            layer_id: base[1].layer_id.clone(),
            object_ids: base[1].object_ids.clone(),
            local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
            name: "I1 shared arc".into(),
        },
    )
    .unwrap();
    let groups = r.groups();
    let original = r.scene();
    let info = r.info();
    let text_ids: Vec<_> = original.layers[0]
        .objects
        .iter()
        .filter(|o| matches!(o.origin, editor_core::ObjectOrigin::GeneratedText { .. }))
        .map(|o| o.object_id.clone())
        .collect();
    let mut partial = groups.clone();
    partial[0].object_ids = vec![text_ids[0].clone()];
    assert!(
        r.edit(
            partial,
            SelectionEdit::Move {
                dx_mm: 1.,
                dy_mm: 0.
            }
        )
        .is_err()
    );
    r.unchanged(&info, &original);
    let copies = r
        .edit(
            groups,
            SelectionEdit::Duplicate {
                dx_mm: 5.,
                dy_mm: 0.,
            },
        )
        .unwrap();
    let copied = r.scene();
    assert_eq!(copied.block_definitions, original.block_definitions);
    assert_eq!(copied.apertures, original.apertures);
    let copied_ids: std::collections::HashSet<_> = copies.groups[0].object_ids.iter().collect();
    let mut old = std::collections::BTreeMap::<String, String>::new();
    let mut new = std::collections::BTreeMap::<String, String>::new();
    for pair in copied.layers[0].objects.chunks_exact(2) {
        if let (
            editor_core::ObjectOrigin::GeneratedText { operation_id: a },
            editor_core::ObjectOrigin::GeneratedText { operation_id: b },
        ) = (&pair[0].origin, &pair[1].origin)
        {
            assert_ne!(a, b);
            assert!(copied_ids.contains(&pair[1].object_id));
            assert_eq!(old.entry(a.clone()).or_insert_with(|| b.clone()), b);
            assert_eq!(new.entry(b.clone()).or_insert_with(|| a.clone()), a);
        }
    }
    assert_eq!(old.len(), 2);
    assert_eq!(new.len(), 2);
    r.undo();
    assert_eq!(r.scene().layers, original.layers);
    r.redo();
    assert_eq!(r.scene().layers, copied.layers);
    r.edit(
        copies.groups,
        SelectionEdit::Rotate {
            angle_deg: 90.,
            pivot_mm: MmPoint::new(0., 0.),
        },
    )
    .unwrap();
    assert_eq!(r.scene().block_definitions, original.block_definitions);
    r.undo();
    assert_eq!(r.scene().layers, copied.layers);
}
#[test]
fn unsupported_later_layer_transform_never_commits_earlier_delta() {
    let mut r = Run::new();
    let file = r.dir.join("rect.gbr");
    std::fs::write(
        &file,
        b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10R,2X1*%\nD10*\nX10000000Y0D02*\nX20000000Y0D01*\nM02*\n",
    )
    .unwrap();
    let imported =
        r.s.import_gerber_layer(
            &r.id,
            &r.info().revision,
            ImportGerberLayerParams {
                path: file.to_string_lossy().into(),
            },
        )
        .unwrap();
    r.layer_order
        .extend(imported.layers.into_iter().map(|l| l.layer_id));
    let before = r.scene();
    let info = r.info();
    let error = r
        .edit(
            r.groups(),
            SelectionEdit::Rotate {
                angle_deg: 45.,
                pivot_mm: MmPoint::new(0., 0.),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "UNSUPPORTED_FEATURE");
    r.unchanged(&info, &before);
}

#[test]
fn core_history_checks_every_layer_before_undo_redo_and_budget() {
    let r = Run::new();
    let scene = r.scene();
    let mut document = editor_core::SemanticDocument {
        id: r.id.clone(),
        unit: "mm".into(),
        format: editor_core::SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: scene.layers,
        apertures: scene.apertures,
        source: Default::default(),
        block_definitions: scene.block_definitions,
    };
    let original = document.clone();
    let groups = r.groups();
    let mut small = editor_core::edit::EditHistory::with_limits(100, 1024).unwrap();
    assert!(
        small
            .edit_selection(
                &mut document,
                &groups,
                &SelectionEdit::Duplicate {
                    dx_mm: 0.,
                    dy_mm: 0.
                }
            )
            .is_err()
    );
    assert_eq!(document, original);
    assert_eq!(small.undo_len(), 0);
    let mut history = editor_core::edit::EditHistory::default();
    history
        .edit_selection(
            &mut document,
            &groups,
            &SelectionEdit::Move {
                dx_mm: 1.,
                dy_mm: 2.,
            },
        )
        .unwrap();
    let moved = document.clone();
    document.layers[1].objects[0].object_id = "externally-corrupt-id".into();
    let corrupt = document.clone();
    assert!(history.undo(&mut document).is_err());
    assert_eq!(document, corrupt);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 0);
    document = moved;
    history.undo(&mut document).unwrap();
    assert_eq!(document, original);
    document.layers[1].objects[0].object_id = "another-corrupt-id".into();
    let corrupt = document.clone();
    assert!(history.redo(&mut document).is_err());
    assert_eq!(document, corrupt);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 1);
}

#[test]
fn solo_excluded_later_layer_refuses_whole_edit() {
    let mut r = Run::new();
    let groups = r.groups();
    let info = r.info();
    r.s.layers_set_solo(
        &r.id,
        &info.revision,
        SetSoloLayerParams {
            expected_workspace_revision: info.workspace_revision,
            layer_id: Some(groups[0].layer_id.clone()),
        },
    )
    .unwrap();
    let info = r.info();
    let scene = r.scene();
    assert!(r.edit(groups, SelectionEdit::Delete).is_err());
    r.unchanged(&info, &scene);
}
