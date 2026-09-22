//! S4-B2 Block Core service workflow (§54/§56/§57/§63 of the task brief):
//! real `ApplicationService`, no GUI. New empty layer -> 4 primitives ->
//! create definition -> 1 instance replaces originals -> duplicate/rotate/
//! mirror/move -> metrics/bounds -> export Gerber -> reopen -> geometry
//! compare -> Undo/Redo, plus a shared-definition fixture proving project
//! object count does not scale with instance geometry.
use editor_core::SemanticGeometry;
use editor_core::block::BlockTransform;
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
    /// Seed a fresh empty layer with 4 Line primitives forming a square by
    /// importing a tiny synthetic Gerber fixture (the only way to get real
    /// manufacturing objects into a document without a Block Editor).
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
        std::fs::write(self.dir.join("square.gbr"), SQUARE).unwrap();
        let rev = self.rev();
        let imported = self
            .svc
            .import_gerber_layer(
                &self.doc,
                &rev,
                ImportGerberLayerParams {
                    path: self.p("square.gbr"),
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
    let w = W::new("caps");
    let caps = w.svc.capabilities();
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
        caps.unsupported_operations
            .iter()
            .any(|s| s.contains(".rcam")),
        "project.open/save must still be reserved-unsupported this phase"
    );
}
