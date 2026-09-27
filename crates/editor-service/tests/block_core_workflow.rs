//! S4-B2 Block Core service workflow (§54/§56/§57/§63 of the task brief):
//! real `ApplicationService`, no GUI. New empty layer -> 4 primitives ->
//! create definition -> 1 instance replaces originals -> duplicate/rotate/
//! mirror/move -> metrics/bounds -> export Gerber -> reopen -> geometry
//! compare -> Undo/Redo, plus a shared-definition fixture proving project
//! object count does not scale with instance geometry.
use editor_core::block::BlockTransform;
use editor_core::{RegionEdge, SemanticGeometry};
use editor_service::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

struct W {
    svc: ApplicationService,
    dir: PathBuf,
    doc: String,
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rcam-b2-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

impl W {
    fn new(tag: &str) -> Self {
        let canonical = temp_dir(tag).canonicalize().unwrap();
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
    fn rev(&self) -> String {
        self.svc.document_get(&self.doc).unwrap().revision
    }
    fn wrev(&self) -> String {
        self.svc.document_get(&self.doc).unwrap().workspace_revision
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
    fn undo(&mut self) -> EditResult {
        let rev = self.rev();
        self.svc.history_undo(&self.doc, &rev).unwrap()
    }
    fn redo(&mut self) -> EditResult {
        let rev = self.rev();
        self.svc.history_redo(&self.doc, &rev).unwrap()
    }
    fn create_definition(
        &mut self,
        layer: &str,
        object_ids: Vec<String>,
        origin: (f64, f64),
        name: &str,
    ) -> CreateBlockDefinitionResult {
        let rev = self.rev();
        self.svc
            .blocks_create_definition_from_objects(
                &self.doc,
                &rev,
                CreateBlockDefinitionParams {
                    layer_id: layer.into(),
                    object_ids,
                    local_origin_mm: PivotMm {
                        x_mm: origin.0,
                        y_mm: origin.1,
                    },
                    name: name.into(),
                },
            )
            .unwrap()
    }
    fn create_instance(
        &mut self,
        layer: &str,
        definition_id: &str,
        transform: BlockTransform,
    ) -> BlockInstanceResult {
        let rev = self.rev();
        self.svc
            .blocks_create_instance(
                &self.doc,
                &rev,
                CreateBlockInstanceParams {
                    layer_id: layer.into(),
                    definition_id: definition_id.into(),
                    transform: BlockTransformParams {
                        translation_mm: PivotMm {
                            x_mm: transform.translation.x_mm,
                            y_mm: transform.translation.y_mm,
                        },
                        rotation_deg: transform.rotation_deg,
                        mirror: transform.mirror,
                    },
                },
            )
            .unwrap()
    }
    /// Import a synthetic Gerber fixture into a fresh layer (the only way to
    /// get real manufacturing objects into a document without a Block Editor).
    fn seed_source(&mut self, filename: &str, text: &str) -> (String, Vec<String>) {
        std::fs::write(self.dir.join(filename), text).unwrap();
        let rev = self.rev();
        let imported = self
            .svc
            .import_gerber_layer(
                &self.doc,
                &rev,
                ImportGerberLayerParams {
                    path: self.p(filename),
                },
            )
            .unwrap();
        let layer_id = imported.layers[0].layer_id.clone();
        let ids = self
            .objects(&layer_id)
            .into_iter()
            .map(|o| o.object_id)
            .collect();
        (layer_id, ids)
    }
    /// Seed a fresh empty layer with 4 Line primitives forming a square.
    fn seed_square(&mut self) -> (String, Vec<String>) {
        const SQUARE: &str = "%FSLAX26Y26*%
%MOMM*%
%ADD10C,0.2*%
D10*
X0Y0D02*
G01*
X1000000Y0D01*
X1000000Y1000000D01*
X0Y1000000D01*
X0Y0D01*
M02*
";
        self.seed_source("square.gbr", SQUARE)
    }
}

#[test]
fn create_definition_duplicate_rotate_mirror_move_metrics_bounds_export_reopen_undo_redo() {
    let mut w = W::new("workflow");
    let (layer, object_ids) = w.seed_square();
    assert_eq!(object_ids.len(), 4);

    let create = w.create_definition(&layer, object_ids, (0.5, 0.5), "square");
    assert_eq!(
        w.objects(&layer).len(),
        1,
        "4 primitives replaced by 1 instance"
    );
    let SemanticGeometry::BlockInstance { .. } = &w.objects(&layer)[0].geometry else {
        panic!("expected the replacement to be a block instance");
    };

    // duplicate (reuses objects.duplicate; cheap, shares the definition)
    let dup_rev = w.rev();
    let dup = w
        .svc
        .objects_duplicate(
            &w.doc,
            &dup_rev,
            DuplicateParams {
                layer_id: layer.clone(),
                object_ids: vec![create.instance_object_id.clone()],
                dx_mm: 5.0,
                dy_mm: 0.0,
            },
        )
        .unwrap();
    let dup_id = dup.changed_object_ids[0].clone();
    assert_eq!(w.svc.blocks_list_definitions(&w.doc).unwrap().len(), 1);

    // rotate the duplicate
    let rot_rev = w.rev();
    w.svc
        .objects_rotate(
            &w.doc,
            &rot_rev,
            RotateParams {
                layer_id: layer.clone(),
                object_ids: vec![dup_id.clone()],
                angle_deg: 90.0,
                pivot_mm: PivotMm {
                    x_mm: 5.5,
                    y_mm: 0.5,
                },
            },
        )
        .unwrap();

    // mirror the original instance
    let mir_rev = w.rev();
    w.svc
        .objects_mirror(
            &w.doc,
            &mir_rev,
            MirrorParams {
                layer_id: layer.clone(),
                object_ids: vec![create.instance_object_id.clone()],
                axis: editor_core::edit::MirrorAxis::Vertical { coordinate_mm: 0.0 },
            },
        )
        .unwrap();

    // move the original instance
    let mov_rev = w.rev();
    w.svc
        .objects_move(
            &w.doc,
            &mov_rev,
            MoveParams {
                layer_id: layer.clone(),
                object_ids: vec![create.instance_object_id.clone()],
                dx_mm: -10.0,
                dy_mm: 0.0,
            },
        )
        .unwrap();

    // metrics/bounds resolve through the shared definition
    let current_objects = w.objects(&layer);
    let metrics = w
        .svc
        .objects_metrics(
            &w.doc,
            MetricsParams {
                layer_id: layer.clone(),
                object_ids: current_objects
                    .iter()
                    .map(|o| o.object_id.clone())
                    .collect(),
            },
        )
        .unwrap();
    assert_eq!(metrics.items.len(), 2);
    for item in &metrics.items {
        let MetricValue::Exact { area_mm2, .. } = &item.value else {
            panic!("expected exact metrics for a block instance, got {item:?}");
        };
        assert!(*area_mm2 > 0.0);
    }
    let bounds = w
        .svc
        .layer_bounds(
            &w.doc,
            LayerBoundsParams {
                layer_id: layer.clone(),
            },
        )
        .unwrap();
    assert!(bounds.bounds.is_some());

    // export flattens both instances into plain Gerber primitives
    let export = w
        .svc
        .export_layer(
            &w.doc,
            &w.rev(),
            ExportParams {
                layer_id: layer.clone(),
                path: w.p("out.gbr"),
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
    assert!(export.bytes > 0);
    let exported = std::fs::read_to_string(w.p("out.gbr")).unwrap();
    assert!(
        !exported.contains("AB"),
        "flattened export must not use Gerber aperture blocks"
    );

    // reopen the exported Gerber and compare object counts (4 lines per
    // instance x 2 instances = 8 plain objects, since export flattens).
    let rev = w.rev();
    let reopened = w
        .svc
        .import_gerber_layer(
            &w.doc,
            &rev,
            ImportGerberLayerParams {
                path: w.p("out.gbr"),
            },
        )
        .unwrap();
    let reopened_layer = reopened.layers[0].layer_id.clone();
    assert_eq!(w.objects(&reopened_layer).len(), 8);

    // Undo/Redo unwind every step above, including the block-definition
    // creation itself.
    for _ in 0..6 {
        w.undo();
    }
    assert_eq!(w.svc.blocks_list_definitions(&w.doc).unwrap().len(), 0);
    assert_eq!(w.objects(&layer).len(), 4);
    for _ in 0..6 {
        w.redo();
    }
    assert_eq!(w.svc.blocks_list_definitions(&w.doc).unwrap().len(), 1);
    assert_eq!(w.objects(&layer).len(), 2);
}

#[test]
fn shared_definition_fixture_does_not_scale_project_object_count_with_instance_geometry() {
    let mut w = W::new("fixture");
    let (layer, object_ids) = w.seed_square();
    let create = w.create_definition(&layer, object_ids, (0.5, 0.5), "steel-mesh-opening");

    // 20 more instances at 0/90/37/mirror, all sharing the same definition.
    let placements = [
        (0.0, false),
        (90.0, false),
        (37.0, false),
        (0.0, true),
        (90.0, true),
    ];
    for i in 0..20u32 {
        let (rotation_deg, mirror) = placements[i as usize % placements.len()];
        w.create_instance(
            &layer,
            &create.definition_id,
            BlockTransform {
                translation: editor_core::MmPoint::new(2.0 * f64::from(i), 0.0),
                rotation_deg,
                mirror,
            },
        );
    }
    assert_eq!(
        w.objects(&layer).len(),
        21,
        "21 instances, not 84 primitives"
    );
    assert_eq!(
        w.svc.blocks_list_definitions(&w.doc).unwrap().len(),
        1,
        "still one shared definition"
    );
    let detail = w
        .svc
        .blocks_get_definition(
            &w.doc,
            BlockDefinitionIdParams {
                definition_id: create.definition_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(detail.objects.len(), 4, "definition geometry stored once");

    // Bounds/metrics still resolve for every instance without a GUI.
    let bounds = w
        .svc
        .layer_bounds(
            &w.doc,
            LayerBoundsParams {
                layer_id: layer.clone(),
            },
        )
        .unwrap();
    assert!(bounds.bounds.is_some());

    // Explode one instance, then deleting the (still-referenced) definition
    // is rejected; delete after exploding everything succeeds.
    let objects = w.objects(&layer);
    let one_instance = objects[0].object_id.clone();
    let rev = w.rev();
    w.svc
        .blocks_explode_instance(
            &w.doc,
            &rev,
            ExplodeBlockInstanceParams {
                layer_id: layer.clone(),
                object_id: one_instance,
            },
        )
        .unwrap();
    assert_eq!(
        w.objects(&layer).len(),
        24,
        "20 remaining instances + 4 exploded lines"
    );

    let rev = w.rev();
    let reject = w.svc.blocks_delete_definition(
        &w.doc,
        &rev,
        BlockDefinitionIdParams {
            definition_id: create.definition_id.clone(),
        },
    );
    assert_eq!(reject.unwrap_err().code, "BLOCK_DEFINITION_REFERENCED");
}

/// S4-B2 Final Closeout regression (found via the native GUI smoke path,
/// which calls this on every geometry-affecting `refresh`): `visible_bounds`
/// used the block-unaware `geometries_bounds`, so any document containing a
/// `BlockInstance` made it fail with `VALIDATION_FAILED: unknown block
/// definition <id>` — even though the id was perfectly valid — as soon as
/// anything called it, not only when a viewport happened to cull the scene.
#[test]
fn visible_bounds_resolves_a_block_instance() {
    let mut w = W::new("visible-bounds");
    let (layer, object_ids) = w.seed_square();
    let create = w.create_definition(&layer, object_ids, (0.5, 0.5), "square");
    let bounds = w.svc.visible_bounds(&w.doc).unwrap().bounds.unwrap();
    assert!(bounds.max_x_mm > bounds.min_x_mm && bounds.max_y_mm > bounds.min_y_mm);
    let _ = create;
}

#[test]
fn locked_layer_rejects_block_mutations() {
    let mut w = W::new("locked");
    let (layer, object_ids) = w.seed_square();
    let create = w.create_definition(&layer, object_ids, (0.5, 0.5), "sq");

    let rev = w.rev();
    w.svc
        .layer_update(
            &w.doc,
            &rev,
            LayerUpdateParams {
                layer_id: layer.clone(),
                locked: Some(true),
                expected_workspace_revision: w.wrev(),
                ..Default::default()
            },
        )
        .unwrap();

    let rev = w.rev();
    let err = w
        .svc
        .blocks_explode_instance(
            &w.doc,
            &rev,
            ExplodeBlockInstanceParams {
                layer_id: layer.clone(),
                object_id: create.instance_object_id.clone(),
            },
        )
        .unwrap_err();
    assert_eq!(err.code, "LAYER_LOCKED");

    let rev = w.rev();
    let err = w
        .svc
        .blocks_create_instance(
            &w.doc,
            &rev,
            CreateBlockInstanceParams {
                layer_id: layer.clone(),
                definition_id: create.definition_id.clone(),
                transform: BlockTransformParams {
                    translation_mm: PivotMm {
                        x_mm: 0.0,
                        y_mm: 0.0,
                    },
                    rotation_deg: 0.0,
                    mirror: false,
                },
            },
        )
        .unwrap_err();
    assert_eq!(err.code, "LAYER_LOCKED");
}

#[test]
fn capabilities_advertise_every_block_op_as_dispatchable() {
    let mut w = W::new("caps");
    let response = w.svc.execute_json(
        &serde_json::json!({
            "api_version": 1, "request_id": "caps", "op": "system.capabilities", "params": {}
        })
        .to_string(),
    );
    assert_eq!(response["status"], "completed");
    assert_eq!(
        response["result"]["stage"],
        "S4-C4 Alignment / Distribution (Mac-first bounded)"
    );
    let caps = w.svc.capabilities();
    assert_eq!(serde_json::to_value(&caps).unwrap(), response["result"]);
    println!("CAPABILITY_OUTPUT={}", response["result"]);
    for op in ["objects.grips", "objects.grip_edit"] {
        assert!(caps.supported_operations.iter().any(|s| s == op));
        assert!(!caps.unsupported_operations.iter().any(|s| s == op));
    }
    for op in [
        "drill.import",
        "components.search",
        "snap.resolve",
        "layers.merge",
    ] {
        assert!(caps.unsupported_operations.iter().any(|s| s == op));
        assert!(!caps.supported_operations.iter().any(|s| s == op));
    }
    for op in [
        "blocks.list_definitions",
        "blocks.get_definition",
        "blocks.create_definition_from_objects",
        "blocks.create_instance",
        "blocks.update_instance_transform",
        "blocks.rename_definition",
        "blocks.explode_instance",
        "blocks.delete_definition",
    ] {
        assert!(caps.supported_operations.iter().any(|s| s == op), "{op}");
        assert!(!caps.unsupported_operations.iter().any(|s| s == op), "{op}");
    }
    assert!(
        caps.supported_operations
            .iter()
            .any(|s| s == "project.open")
    );
    assert!(
        caps.supported_operations
            .iter()
            .any(|s| s == "project.save")
    );

    // Successful JSON requests exercise the public dispatcher, not only the table
    // or Rust methods. Read-only requests omit expected_revision.
    fn call(w: &mut W, op: &str, params: serde_json::Value, edit: bool) -> serde_json::Value {
        let before = w.rev();
        let mut request = serde_json::json!({
            "api_version": 1, "request_id": op, "op": op,
            "document_id": w.doc, "params": params
        });
        if edit {
            request["expected_revision"] = serde_json::json!(before);
        }
        let response = w.svc.execute_json(&request.to_string());
        assert_eq!(response["request_id"], op);
        assert_eq!(response["status"], "completed", "{op}: {response}");
        if edit {
            assert_eq!(
                w.rev().parse::<u64>().unwrap(),
                before.parse::<u64>().unwrap() + 1
            );
        } else {
            assert_eq!(w.rev(), before);
        }
        response["result"].clone()
    }
    let (layer, ids) = w.seed_square();
    let created = call(
        &mut w,
        "blocks.create_definition_from_objects",
        serde_json::json!({
            "layer_id": layer, "object_ids": ids, "local_origin_mm": {"x_mm": 0.5, "y_mm": 0.5},
            "name": "JSON block"
        }),
        true,
    );
    let definition = created["definition_id"].clone();
    let original = created["instance_object_id"].clone();
    assert_eq!(w.objects(&layer).len(), 1);
    let list = call(
        &mut w,
        "blocks.list_definitions",
        serde_json::json!({}),
        false,
    );
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], definition);
    let detail = call(
        &mut w,
        "blocks.get_definition",
        serde_json::json!({"definition_id": definition}),
        false,
    );
    assert_eq!(detail["objects"].as_array().unwrap().len(), 4);
    let transform = serde_json::json!({
        "translation_mm": {"x_mm": 5.0, "y_mm": 6.0}, "rotation_deg": 90.0, "mirror": true
    });
    let placed = call(
        &mut w,
        "blocks.create_instance",
        serde_json::json!({
            "layer_id": layer, "definition_id": definition, "transform": transform
        }),
        true,
    );
    assert_eq!(w.objects(&layer).len(), 2);
    call(
        &mut w,
        "blocks.update_instance_transform",
        serde_json::json!({
            "layer_id": layer, "object_id": original, "transform": transform
        }),
        true,
    );
    for object in w.objects(&layer) {
        let SemanticGeometry::BlockInstance { transform, .. } = object.geometry else {
            panic!("expected instance")
        };
        assert_eq!(transform.translation.x_mm, 5.0);
        assert_eq!(transform.translation.y_mm, 6.0);
        assert_eq!(transform.rotation_deg, 90.0);
        assert!(transform.mirror);
    }
    call(
        &mut w,
        "blocks.rename_definition",
        serde_json::json!({
            "definition_id": definition, "name": "重命名 JSON"
        }),
        true,
    );
    let detail = call(
        &mut w,
        "blocks.get_definition",
        serde_json::json!({"definition_id": definition}),
        false,
    );
    assert_eq!(detail["name"], "重命名 JSON");
    for object_id in [original, placed["object_id"].clone()] {
        call(
            &mut w,
            "blocks.explode_instance",
            serde_json::json!({
                "layer_id": layer, "object_id": object_id
            }),
            true,
        );
    }
    assert_eq!(w.objects(&layer).len(), 8);
    assert!(
        w.objects(&layer)
            .iter()
            .all(|o| !matches!(o.geometry, SemanticGeometry::BlockInstance { .. }))
    );
    call(
        &mut w,
        "blocks.delete_definition",
        serde_json::json!({"definition_id": definition}),
        true,
    );
    assert_eq!(
        call(
            &mut w,
            "blocks.list_definitions",
            serde_json::json!({}),
            false
        ),
        serde_json::json!([])
    );
}

/// Every declared circular-aperture diameter in `gerber_text` (Flash apertures
/// and the writer's dynamic stroke apertures for Line/Arc widths are both
/// emitted this way, per `emit_aperture_definition`).
fn aperture_diameters(gerber_text: &str) -> Vec<f64> {
    gerber_text
        .split("%ADD")
        .skip(1)
        .filter_map(|chunk| {
            let comma = chunk.find(',')?;
            let rest = &chunk[comma + 1..];
            let end = rest.find(['X', '*'])?;
            rest[..end].parse::<f64>().ok()
        })
        .collect()
}

fn assert_on_grid(value: f64, resolution_mm: f64, what: &str) {
    let grid = (value / resolution_mm).round() * resolution_mm;
    assert!(
        (value - grid).abs() < 1e-6,
        "{what} = {value} is not on the {resolution_mm}mm export grid \
         (block geometry was not requantized to the current precision)"
    );
}

/// S4-B2 Final Closeout B0 regression: a `BlockDefinition` captured while one
/// `ManufacturingPrecision` is active must still have its *exported* geometry
/// governed by whichever precision is active at export time, not whichever
/// precision happened to be active when the definition was captured. Every
/// coordinate below is offset by 0.123450mm so it is deliberately *not*
/// already aligned to the coarser export grid this test switches to.
#[test]
fn block_geometry_follows_current_manufacturing_precision_on_export() {
    const MIXED_OFFSET: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.23456*%\nD10*\nX2123450Y2123450D03*\nX123450Y123450D02*\nG01X2123450Y123450D01*\nG75*\nX1123450Y123450D02*\nG03X123450Y1123450I-1000000J0D01*\nG36*\nX10123450Y10123450D02*\nG01X12123450Y10123450D01*\nX12123450Y12123450D01*\nX10123450Y12123450D01*\nX10123450Y10123450D01*\nG37*\nM02*\n";

    let mut w = W::new("precision-follow");
    w.svc
        .set_manufacturing_precision(
            &w.doc,
            &w.rev(),
            ManufacturingPrecision {
                resolution_mm: 0.0001,
            },
        )
        .unwrap();
    let (layer, object_ids) = w.seed_source("mixed.gbr", MIXED_OFFSET);
    assert_eq!(object_ids.len(), 4, "Flash + Line + Arc + Region");
    w.create_definition(&layer, object_ids, (0.0, 0.0), "mixed");
    assert_eq!(
        w.objects(&layer).len(),
        1,
        "4 primitives replaced by 1 instance"
    );

    // Precision changes *after* the definition already exists.
    w.svc
        .set_manufacturing_precision(
            &w.doc,
            &w.rev(),
            ManufacturingPrecision {
                resolution_mm: 0.001,
            },
        )
        .unwrap();

    let export = w
        .svc
        .export_layer(
            &w.doc,
            &w.rev(),
            ExportParams {
                layer_id: layer.clone(),
                path: w.p("precision_out.gbr"),
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
    assert!(export.bytes > 0);

    let exported_text = std::fs::read_to_string(w.p("precision_out.gbr")).unwrap();
    let diameters = aperture_diameters(&exported_text);
    assert!(
        !diameters.is_empty(),
        "expected declared apertures in {exported_text}"
    );
    for diameter in diameters {
        assert_on_grid(diameter, 0.001, "aperture diameter");
    }

    let rev = w.rev();
    let reopened = w
        .svc
        .import_gerber_layer(
            &w.doc,
            &rev,
            ImportGerberLayerParams {
                path: w.p("precision_out.gbr"),
            },
        )
        .unwrap();
    let reopened_layer = reopened.layers[0].layer_id.clone();
    let objects = w.objects(&reopened_layer);
    assert_eq!(
        objects.len(),
        4,
        "Flash + Line + Arc + Region flattened from the one instance"
    );
    for object in &objects {
        match &object.geometry {
            SemanticGeometry::Flash { center, .. } => {
                assert_on_grid(center.x_mm, 0.001, "flash center x");
                assert_on_grid(center.y_mm, 0.001, "flash center y");
            }
            SemanticGeometry::Line {
                start,
                end,
                width_mm,
            } => {
                assert_on_grid(start.x_mm, 0.001, "line start x");
                assert_on_grid(start.y_mm, 0.001, "line start y");
                assert_on_grid(end.x_mm, 0.001, "line end x");
                assert_on_grid(end.y_mm, 0.001, "line end y");
                assert_on_grid(*width_mm, 0.001, "line width");
            }
            SemanticGeometry::Arc { path, width_mm } => {
                assert_on_grid(path.start.x_mm, 0.001, "arc start x");
                assert_on_grid(path.start.y_mm, 0.001, "arc start y");
                assert_on_grid(path.end.x_mm, 0.001, "arc end x");
                assert_on_grid(path.end.y_mm, 0.001, "arc end y");
                assert_on_grid(path.center.x_mm, 0.001, "arc center x");
                assert_on_grid(path.center.y_mm, 0.001, "arc center y");
                assert_on_grid(*width_mm, 0.001, "arc width");
            }
            SemanticGeometry::Region { contours } => {
                for contour in contours {
                    for edge in &contour.edges {
                        match edge {
                            RegionEdge::Line { start, end } => {
                                assert_on_grid(start.x_mm, 0.001, "region edge start x");
                                assert_on_grid(start.y_mm, 0.001, "region edge start y");
                                assert_on_grid(end.x_mm, 0.001, "region edge end x");
                                assert_on_grid(end.y_mm, 0.001, "region edge end y");
                            }
                            RegionEdge::Arc(a) => {
                                assert_on_grid(a.start.x_mm, 0.001, "region arc start x");
                                assert_on_grid(a.start.y_mm, 0.001, "region arc start y");
                                assert_on_grid(a.end.x_mm, 0.001, "region arc end x");
                                assert_on_grid(a.end.y_mm, 0.001, "region arc end y");
                                assert_on_grid(a.center.x_mm, 0.001, "region arc center x");
                                assert_on_grid(a.center.y_mm, 0.001, "region arc center y");
                            }
                        }
                    }
                }
            }
            other => panic!("unexpected flattened geometry kind: {other:?}"),
        }
    }
}

/// S4-B2 Final Closeout B0: a block definition whose internal geometry
/// collapses under a coarser export precision must fail closed — no target
/// file, working project (revision, precision, history) untouched — exactly
/// like the existing top-level-geometry collapse case
/// (`gerber_io::precision::tests`), now exercised through a `BlockInstance`.
#[test]
fn block_export_fails_closed_when_precision_collapses_definition_geometry() {
    const TINY_REGION: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG36*\nX0Y0D02*\nG01X200Y0D01*\nX200Y1000000D01*\nX0Y1000000D01*\nX0Y0D01*\nG37*\nM02*\n";

    let mut w = W::new("precision-collapse");
    w.svc
        .set_manufacturing_precision(
            &w.doc,
            &w.rev(),
            ManufacturingPrecision {
                resolution_mm: 0.0001,
            },
        )
        .unwrap();
    let (layer, object_ids) = w.seed_source("tiny_region.gbr", TINY_REGION);
    assert_eq!(object_ids.len(), 1);
    w.create_definition(&layer, object_ids, (0.0, 0.0), "tiny");

    w.svc
        .set_manufacturing_precision(
            &w.doc,
            &w.rev(),
            ManufacturingPrecision {
                resolution_mm: 0.001,
            },
        )
        .unwrap();

    let before_export = w.svc.document_get(&w.doc).unwrap();
    let target = w.p("should_not_exist.gbr");
    let result = w.svc.export_layer(
        &w.doc,
        &before_export.revision,
        ExportParams {
            layer_id: layer.clone(),
            path: target.clone(),
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
    );
    assert!(
        result.is_err(),
        "0.001mm precision must collapse the 0.0002mm block edge and fail closed"
    );
    assert!(
        !std::path::Path::new(&target).exists(),
        "no target file must appear on export failure"
    );
    assert_eq!(
        w.svc.document_get(&w.doc).unwrap(),
        before_export,
        "working project must be untouched by a failed export"
    );
}

#[test]
fn c3_service_rejects_hidden_nonselectable_and_block_class_targets_without_mutation() {
    use editor_core::workspace::DisplayClass;
    for policy in [
        "hidden",
        "nonselectable",
        "class_hidden",
        "class_locked",
        "class_nonselectable",
    ] {
        let mut w = W::new(policy);
        let (layer, ids) = w.seed_square();
        let created = w.create_definition(&layer, ids, (0., 0.), "test");
        let mut patch = LayerUpdateParams {
            layer_id: layer.clone(),
            expected_workspace_revision: w.wrev(),
            ..Default::default()
        };
        match policy {
            "hidden" => patch.visible = Some(false),
            "nonselectable" => patch.selectable = Some(false),
            _ => patch.classes.push(ClassStyleUpdate {
                class: Some(DisplayClass::BlockInstance),
                visible: (policy == "class_hidden").then_some(false),
                selectable: (policy == "class_nonselectable").then_some(false),
                locked: (policy == "class_locked").then_some(true),
                ..Default::default()
            }),
        }
        w.svc.layer_update(&w.doc, &w.rev(), patch).unwrap();
        let before = w.svc.document_get(&w.doc).unwrap();
        let snapshot = w.svc.project_snapshot(&w.doc).unwrap();
        assert!(
            w.svc
                .blocks_create_instance(
                    &w.doc,
                    &w.rev(),
                    CreateBlockInstanceParams {
                        layer_id: layer.clone(),
                        definition_id: created.definition_id.clone(),
                        transform: BlockTransformParams {
                            translation_mm: PivotMm { x_mm: 2., y_mm: 3. },
                            rotation_deg: 0.,
                            mirror: false
                        }
                    }
                )
                .is_err(),
            "{policy}"
        );
        assert!(
            w.svc
                .blocks_explode_instance(
                    &w.doc,
                    &w.rev(),
                    ExplodeBlockInstanceParams {
                        layer_id: layer,
                        object_id: created.instance_object_id
                    }
                )
                .is_err(),
            "{policy}"
        );
        assert_eq!(w.svc.document_get(&w.doc).unwrap(), before, "{policy}");
        assert_eq!(
            w.svc.project_snapshot(&w.doc).unwrap(),
            snapshot,
            "{policy}"
        );
    }
}

#[test]
fn c3_create_checks_ordinary_selection_visibility_and_revision() {
    for policy in ["hidden", "nonselectable", "stale", "empty", "nested"] {
        let mut w = W::new(policy);
        let (layer, mut ids) = w.seed_square();
        let original_revision = w.rev();
        if policy == "nested" {
            let created = w.create_definition(&layer, ids, (0., 0.), "one");
            ids = vec![created.instance_object_id];
        }
        if policy == "empty" {
            ids.clear();
        }
        if policy == "hidden" || policy == "nonselectable" {
            w.svc
                .layer_update(
                    &w.doc,
                    &w.rev(),
                    LayerUpdateParams {
                        layer_id: layer.clone(),
                        expected_workspace_revision: w.wrev(),
                        visible: (policy == "hidden").then_some(false),
                        selectable: (policy == "nonselectable").then_some(false),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let rev = if policy == "stale" {
            "0".into()
        } else {
            w.rev()
        };
        let before = w.svc.document_get(&w.doc).unwrap();
        let err = w
            .svc
            .blocks_create_definition_from_objects(
                &w.doc,
                &rev,
                CreateBlockDefinitionParams {
                    layer_id: layer,
                    object_ids: ids,
                    local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
                    name: "test".into(),
                },
            )
            .unwrap_err();
        assert_eq!(
            w.svc.document_get(&w.doc).unwrap(),
            before,
            "{policy}: {err:?}, original={original_revision}"
        );
    }
}
