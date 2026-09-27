//! S4-C4 service workflow: manufacturing-space alignment and distribution.
use editor_core::{
    BoundsMm, SemanticGeometry, SemanticLayer, SemanticObject, geometries_bounds_with_blocks,
};
use editor_service::{
    AlignParams, AlignmentMode, ApplicationService, BlockDefinitionIdParams, BlockTransformParams,
    ClassStyleUpdate, CreateBlockDefinitionParams, CreateBlockInstanceParams, DeleteParams,
    DistributeParams, DistributionAxis, ExportParams, FileAccessPolicy, HorizontalAlign,
    LayerPatch, MetadataPolicy, MetricsParams, OverwritePolicy, PivotMm, QueryParams,
    SetSoloLayerParams, TextLayout, TextParams, UpdateBlockInstanceTransformParams,
    UpdateLayersParams, VerticalAlign, builtin_stroke_font,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/synthetic/s4c4");

struct Run {
    service: ApplicationService,
    dir: PathBuf,
    document: String,
    layer: String,
    source_name: String,
    source_bytes: Vec<u8>,
}

impl Run {
    fn fixture(name: &str) -> Self {
        let source = std::fs::read(Path::new(FIXTURES).join(name)).unwrap();
        Self::from_source(name, &source)
    }

    fn from_source(name: &str, bytes: &[u8]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-s4c4-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source_name = Path::new(name)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        std::fs::write(dir.join(&source_name), bytes).unwrap();
        let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let opened = service.open(&source_name).unwrap();
        Self {
            service,
            dir,
            document: opened.document_id,
            layer: opened.layer_ids[0].clone(),
            source_name,
            source_bytes: bytes.to_vec(),
        }
    }

    fn info(&self) -> editor_service::DocumentInfo {
        self.service.document_get(&self.document).unwrap()
    }

    fn snapshot(&self) -> editor_service::RenderSnapshot {
        self.service.render_snapshot(&self.document).unwrap()
    }

    fn layer(&self) -> SemanticLayer {
        self.snapshot()
            .layers
            .into_iter()
            .find(|layer| layer.id == self.layer)
            .unwrap()
    }

    fn ids(&self) -> Vec<String> {
        self.layer()
            .objects
            .into_iter()
            .map(|object| object.object_id)
            .collect()
    }

    fn object(&self, id: &str) -> SemanticObject {
        self.layer()
            .objects
            .into_iter()
            .find(|object| object.object_id == id)
            .unwrap()
    }

    fn bounds(&self, id: &str) -> BoundsMm {
        let snapshot = self.snapshot();
        let object = snapshot
            .layers
            .iter()
            .find(|layer| layer.id == self.layer)
            .unwrap()
            .objects
            .iter()
            .find(|object| object.object_id == id)
            .unwrap();
        geometries_bounds_with_blocks(
            std::iter::once(&object.geometry),
            &snapshot.apertures,
            &snapshot.block_definitions,
        )
        .unwrap()
        .unwrap()
    }

    fn metrics(&mut self, ids: &[String]) -> editor_service::MetricsResult {
        self.service
            .objects_metrics(
                &self.document,
                MetricsParams {
                    layer_id: self.layer.clone(),
                    object_ids: ids.to_vec(),
                },
            )
            .unwrap()
    }

    fn set_layer(&mut self, patch: LayerPatch) {
        let info = self.info();
        self.service
            .layers_update_many(
                &self.document,
                &info.revision,
                UpdateLayersParams {
                    expected_workspace_revision: info.workspace_revision,
                    updates: vec![patch],
                },
            )
            .unwrap();
    }

    fn align(
        &mut self,
        ids: Vec<String>,
        anchor: &str,
        mode: AlignmentMode,
    ) -> Result<editor_service::EditResult, editor_service::ServiceError> {
        let revision = self.info().revision;
        self.service.objects_align(
            &self.document,
            &revision,
            AlignParams {
                layer_id: self.layer.clone(),
                object_ids: ids,
                anchor_object_id: anchor.into(),
                mode,
            },
        )
    }

    fn distribute(
        &mut self,
        ids: Vec<String>,
        axis: DistributionAxis,
    ) -> Result<editor_service::EditResult, editor_service::ServiceError> {
        let revision = self.info().revision;
        self.service.objects_distribute(
            &self.document,
            &revision,
            DistributeParams {
                layer_id: self.layer.clone(),
                object_ids: ids,
                axis,
            },
        )
    }

    fn scene(
        &self,
    ) -> (
        Vec<SemanticLayer>,
        Vec<editor_core::ApertureDefinition>,
        Vec<editor_core::block::BlockDefinition>,
    ) {
        let snapshot = self.snapshot();
        (
            snapshot.layers,
            snapshot.apertures,
            snapshot.block_definitions,
        )
    }

    fn query_ids(&self, layer_id: &str) -> Vec<String> {
        let mut cursor = None;
        let mut ids = Vec::new();
        loop {
            let page = self
                .service
                .objects_query(
                    &self.document,
                    QueryParams {
                        layer_id: layer_id.into(),
                        geometry_type: None,
                        region_mm: None,
                        relation: None,
                        limit: Some(1_000),
                        cursor,
                    },
                )
                .unwrap();
            ids.extend(page.objects.into_iter().map(|item| item.object.object_id));
            cursor = page.next_cursor;
            if cursor.is_none() {
                return ids;
            }
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-7, "{a} != {b}");
}

fn feature(bounds: BoundsMm, mode: AlignmentMode) -> f64 {
    match mode {
        AlignmentMode::Left => bounds.min_x_mm,
        AlignmentMode::Right => bounds.max_x_mm,
        AlignmentMode::Top => bounds.max_y_mm,
        AlignmentMode::Bottom => bounds.min_y_mm,
        AlignmentMode::HCenter => (bounds.min_x_mm + bounds.max_x_mm) / 2.,
        AlignmentMode::VCenter => (bounds.min_y_mm + bounds.max_y_mm) / 2.,
    }
}

fn distribution_edges(bounds: BoundsMm, axis: DistributionAxis) -> (f64, f64) {
    match axis {
        DistributionAxis::Horizontal => (bounds.min_x_mm, bounds.max_x_mm),
        DistributionAxis::Vertical => (bounds.min_y_mm, bounds.max_y_mm),
    }
}

fn unchanged_after_failure(
    run: &Run,
    before: &editor_service::DocumentInfo,
    scene: &(
        Vec<SemanticLayer>,
        Vec<editor_core::ApertureDefinition>,
        Vec<editor_core::block::BlockDefinition>,
    ),
) {
    assert_eq!(&run.info(), before);
    assert_eq!(&run.scene(), scene);
}

#[test]
fn all_alignment_modes_are_manufacturing_bounds_transactions_with_exact_undo_redo() {
    let mut run = Run::fixture("arrangements.gbr");
    let ids = run.ids();
    assert_eq!(ids.len(), 3);
    let anchor = ids[2].clone(); // Explicit API anchor supplied by last-selected UI identity.
    let baseline = run.scene();
    let sequence: Vec<_> = run
        .layer()
        .objects
        .iter()
        .map(|o| (o.object_id.clone(), o.exposure))
        .collect();

    for mode in [
        AlignmentMode::Left,
        AlignmentMode::Right,
        AlignmentMode::Top,
        AlignmentMode::Bottom,
        AlignmentMode::HCenter,
        AlignmentMode::VCenter,
    ] {
        let before_info = run.info();
        let before_layer = run.layer();
        let anchor_before = before_layer
            .objects
            .iter()
            .find(|o| o.object_id == anchor)
            .unwrap()
            .clone();
        let anchor_feature = feature(run.bounds(&anchor), mode);
        let before_metrics = run.metrics(&ids);
        let params = ids.clone();
        let result = run.align(params, &anchor, mode).unwrap();
        assert_eq!(
            result.revision.parse::<u64>().unwrap(),
            before_info.revision.parse::<u64>().unwrap() + 1
        );
        assert_eq!(result.undo_entries_added, 1);
        assert!(!result.changed_object_ids.is_empty());
        assert!(!result.changed_object_ids.contains(&anchor));
        let after_layer = run.layer();
        assert_eq!(after_layer.objects.len(), before_layer.objects.len());
        assert_eq!(
            after_layer
                .objects
                .iter()
                .map(|o| (o.object_id.clone(), o.exposure))
                .collect::<Vec<_>>(),
            sequence
        );
        assert_eq!(
            after_layer
                .objects
                .iter()
                .find(|o| o.object_id == anchor)
                .unwrap(),
            &anchor_before
        );
        for id in &ids {
            close(feature(run.bounds(id), mode), anchor_feature);
        }
        assert_eq!(
            run.metrics(&ids).items,
            before_metrics.items,
            "rigid alignment changed manufacturing metrics"
        );

        let after = run.scene();
        let no_op_before = run.info();
        let no_op = run.align(ids.clone(), &anchor, mode).unwrap();
        assert!(no_op.changed_object_ids.is_empty());
        assert_eq!(no_op.undo_entries_added, 0);
        assert_eq!(no_op.revision, no_op_before.revision);
        assert_eq!(run.info(), no_op_before);

        let undone = run
            .service
            .history_undo(&run.document, &no_op.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
        let redone = run
            .service
            .history_redo(&run.document, &undone.revision)
            .unwrap();
        assert_eq!(run.scene(), after);
        let restored = run
            .service
            .history_undo(&run.document, &redone.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
        assert_eq!(restored.undo_entries_added, 0);
    }
}

#[test]
fn distribution_uses_world_edge_gaps_keeps_endpoints_and_ignores_selection_order() {
    let mut run = Run::fixture("negative-gap.gbr");
    let ids = run.ids();
    assert_eq!(ids.len(), 3);
    let baseline = run.scene();
    let objects_before = run.layer().objects;

    for axis in [DistributionAxis::Horizontal, DistributionAxis::Vertical] {
        let info = run.info();
        let mut order: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let bounds = run.bounds(id);
                let (min, max) = distribution_edges(bounds, axis);
                (min, max, index, id.clone())
            })
            .collect();
        order.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1).then_with(|| a.2.cmp(&b.2)))
        });
        let first = order.first().unwrap().3.clone();
        let last = order.last().unwrap().3.clone();
        let before_metrics = run.metrics(&ids);
        let result = run.distribute(ids.clone(), axis).unwrap();
        assert_eq!(
            result.revision.parse::<u64>().unwrap(),
            info.revision.parse::<u64>().unwrap() + 1
        );
        assert_eq!(result.undo_entries_added, 1);
        assert!(
            result.changed_object_ids.len() <= 1,
            "distribution should move only its middle object"
        );
        assert_eq!(
            &run.object(&first),
            objects_before
                .iter()
                .find(|o| o.object_id == first)
                .unwrap()
        );
        assert_eq!(
            &run.object(&last),
            objects_before.iter().find(|o| o.object_id == last).unwrap()
        );
        let gaps: Vec<_> = order
            .windows(2)
            .map(|pair| {
                let (_, prev_max) = distribution_edges(run.bounds(&pair[0].3), axis);
                let (next_min, _) = distribution_edges(run.bounds(&pair[1].3), axis);
                next_min - prev_max
            })
            .collect();
        close(gaps[0], gaps[1]);
        assert!(
            gaps[0] < 0.,
            "fixture must exercise a valid negative equal gap: {gaps:?}"
        );
        assert_eq!(run.metrics(&ids).items, before_metrics.items);

        let arranged = run.scene();
        let no_op_before = run.info();
        let no_op = run.distribute(ids.clone(), axis).unwrap();
        assert!(no_op.changed_object_ids.is_empty());
        assert_eq!(no_op.undo_entries_added, 0);
        assert_eq!(no_op.revision, no_op_before.revision);
        assert_eq!(run.info(), no_op_before);
        let undo_first_order = run
            .service
            .history_undo(&run.document, &no_op.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
        let mut reversed = ids.clone();
        reversed.reverse();
        run.distribute(reversed, axis).unwrap();
        assert_eq!(run.scene(), arranged);
        let revision = run.info().revision;
        let no_op = run.distribute(ids.clone(), axis).unwrap();
        assert_eq!(no_op.revision, revision);
        assert!(no_op.changed_object_ids.is_empty());
        let undo = run
            .service
            .history_undo(&run.document, &no_op.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
        let redo = run
            .service
            .history_redo(&run.document, &undo.revision)
            .unwrap();
        assert_eq!(run.scene(), arranged);
        let undo = run
            .service
            .history_undo(&run.document, &redo.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
        assert_eq!(undo.undo_entries_added, 0);
        assert_eq!(undo_first_order.undo_entries_added, 0);
    }
}

#[test]
fn positive_distribution_equalizes_gaps_along_both_world_axes() {
    let mut run = Run::fixture("arrangements.gbr");
    let ids = run.ids();
    for axis in [DistributionAxis::Horizontal, DistributionAxis::Vertical] {
        let baseline = run.scene();
        let mut order: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let (min, max) = distribution_edges(run.bounds(id), axis);
                (min, max, index, id.clone())
            })
            .collect();
        order.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1).then_with(|| a.2.cmp(&b.2)))
        });
        let endpoints = [
            run.object(&order[0].3),
            run.object(&order.last().unwrap().3),
        ];
        let arranged = run.distribute(ids.clone(), axis).unwrap();
        assert_eq!(arranged.undo_entries_added, 1);
        assert_eq!(&run.object(&order[0].3), &endpoints[0]);
        assert_eq!(&run.object(&order.last().unwrap().3), &endpoints[1]);
        let gaps: Vec<_> = order
            .windows(2)
            .map(|pair| {
                let (_, previous_max) = distribution_edges(run.bounds(&pair[0].3), axis);
                let (next_min, _) = distribution_edges(run.bounds(&pair[1].3), axis);
                next_min - previous_max
            })
            .collect();
        close(gaps[0], gaps[1]);
        assert!(
            gaps[0] > 0.,
            "arrangements fixture must exercise positive gaps: {gaps:?}"
        );
        let after = run.scene();
        let undo = run
            .service
            .history_undo(&run.document, &arranged.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
        let redo = run
            .service
            .history_redo(&run.document, &undo.revision)
            .unwrap();
        assert_eq!(run.scene(), after);
        let _ = run
            .service
            .history_undo(&run.document, &redo.revision)
            .unwrap();
        assert_eq!(run.scene(), baseline);
    }
}

#[test]
fn mixed_manufacturing_geometry_aligns_from_analytic_world_bounds() {
    const MIXED: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX2000000Y2000000D03*\nX0Y0D02*\nG01X2000000Y0D01*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nG36*\nX10000000Y10000000D02*\nG01X12000000Y10000000D01*\nX12000000Y12000000D01*\nX10000000Y12000000D01*\nX10000000Y10000000D01*\nG37*\nM02*\n";
    let mut run = Run::from_source("mixed.gbr", MIXED.as_bytes());
    let ids = run.ids();
    assert_eq!(ids.len(), 4);
    let objects = run.layer().objects;
    assert!(matches!(
        objects[0].geometry,
        SemanticGeometry::Flash { .. }
    ));
    assert!(matches!(objects[1].geometry, SemanticGeometry::Line { .. }));
    assert!(matches!(objects[2].geometry, SemanticGeometry::Arc { .. }));
    assert!(matches!(
        objects[3].geometry,
        SemanticGeometry::Region { .. }
    ));
    let anchor = ids[3].clone();
    let anchor_before = run.object(&anchor);
    let target = run.bounds(&anchor).min_x_mm;
    let metrics_before = run.metrics(&ids);
    let aligned = run
        .align(ids.clone(), &anchor, AlignmentMode::Left)
        .unwrap();
    assert_eq!(run.object(&anchor), anchor_before);
    for id in &ids {
        close(run.bounds(id).min_x_mm, target);
    }
    assert_eq!(run.metrics(&ids).items, metrics_before.items);
    let after = run.scene();
    let undo = run
        .service
        .history_undo(&run.document, &aligned.revision)
        .unwrap();
    assert_eq!(run.layer().objects, objects);
    let _redo = run
        .service
        .history_redo(&run.document, &undo.revision)
        .unwrap();
    assert_eq!(run.scene(), after);
}

#[test]
fn arrangement_json_dispatch_and_failures_are_atomic() {
    let mut run = Run::fixture("arrangements.gbr");
    let ids = run.ids();
    let info = run.info();
    let align_request = json!({
        "api_version": 1,
        "request_id": "align-json",
        "op": "objects.align",
        "document_id": run.document,
        "expected_revision": info.revision,
        "params": {"layer_id":run.layer,"object_ids":ids,"anchor_object_id":run.ids()[2],"mode":"left"}
    });
    let json_result = run.service.execute_json(&align_request.to_string());
    assert_eq!(json_result["status"], "completed", "{json_result}");
    assert_eq!(json_result["result"]["undo_entries_added"], 1);
    let rev = json_result["result"]["revision"]
        .as_str()
        .unwrap()
        .to_owned();
    let distribute_request = json!({
        "api_version": 1,
        "request_id": "distribute-json",
        "op": "objects.distribute",
        "document_id": run.document,
        "expected_revision": rev,
        "params": {"layer_id":run.layer,"object_ids":run.ids(),"axis":"horizontal"}
    });
    let distributed = run.service.execute_json(&distribute_request.to_string());
    assert_eq!(distributed["status"], "completed", "{distributed}");
    assert_eq!(distributed["result"]["undo_entries_added"], 1);

    let mut blocked = Run::fixture("arrangements.gbr");
    let ids = blocked.ids();
    for (name, patch, expected_code) in [
        (
            "hidden-layer",
            LayerPatch {
                layer_id: blocked.layer.clone(),
                visible: Some(false),
                ..Default::default()
            },
            "INVALID_ARGUMENT",
        ),
        (
            "locked-layer",
            LayerPatch {
                layer_id: blocked.layer.clone(),
                locked: Some(true),
                ..Default::default()
            },
            "LAYER_LOCKED",
        ),
        (
            "nonselectable-layer",
            LayerPatch {
                layer_id: blocked.layer.clone(),
                selectable: Some(false),
                ..Default::default()
            },
            "INVALID_ARGUMENT",
        ),
        (
            "hidden-category",
            LayerPatch {
                layer_id: blocked.layer.clone(),
                classes: vec![ClassStyleUpdate {
                    class: Some(editor_service::DisplayClass::FlashRectangle),
                    visible: Some(false),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "INVALID_ARGUMENT",
        ),
        (
            "locked-category",
            LayerPatch {
                layer_id: blocked.layer.clone(),
                classes: vec![ClassStyleUpdate {
                    class: Some(editor_service::DisplayClass::FlashRectangle),
                    locked: Some(true),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "OBJECT_CLASS_LOCKED",
        ),
        (
            "nonselectable-category",
            LayerPatch {
                layer_id: blocked.layer.clone(),
                classes: vec![ClassStyleUpdate {
                    class: Some(editor_service::DisplayClass::FlashRectangle),
                    selectable: Some(false),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "INVALID_ARGUMENT",
        ),
    ] {
        blocked.set_layer(patch);
        let before = blocked.info();
        let scene = blocked.scene();
        let error = blocked
            .align(ids.clone(), &ids[2], AlignmentMode::Left)
            .unwrap_err();
        assert_eq!(error.code, expected_code, "{name}: {error:?}");
        unchanged_after_failure(&blocked, &before, &scene);
        blocked.set_layer(LayerPatch {
            layer_id: blocked.layer.clone(),
            visible: Some(true),
            locked: Some(false),
            selectable: Some(true),
            reset_classes: true,
            ..Default::default()
        });
    }

    let before = blocked.info();
    let scene = blocked.scene();
    let one_target = blocked
        .align(vec![ids[0].clone()], &ids[0], AlignmentMode::Left)
        .unwrap_err();
    assert_eq!(one_target.code, "INVALID_ARGUMENT");
    unchanged_after_failure(&blocked, &before, &scene);
    let two_targets = blocked
        .distribute(ids[..2].to_vec(), DistributionAxis::Horizontal)
        .unwrap_err();
    assert_eq!(two_targets.code, "INVALID_ARGUMENT");
    unchanged_after_failure(&blocked, &before, &scene);
    let duplicate = blocked
        .align(
            vec![ids[0].clone(), ids[0].clone()],
            &ids[0],
            AlignmentMode::Left,
        )
        .unwrap_err();
    assert_eq!(duplicate.code, "INVALID_ARGUMENT");
    unchanged_after_failure(&blocked, &before, &scene);
    let missing_anchor = blocked
        .align(
            vec![ids[0].clone(), ids[1].clone()],
            &ids[2],
            AlignmentMode::Left,
        )
        .unwrap_err();
    assert_eq!(missing_anchor.code, "INVALID_ARGUMENT");
    unchanged_after_failure(&blocked, &before, &scene);
    let missing = blocked
        .align(
            vec![ids[0].clone(), "missing-object".into()],
            &ids[0],
            AlignmentMode::Left,
        )
        .unwrap_err();
    assert_eq!(missing.code, "NOT_FOUND");
    unchanged_after_failure(&blocked, &before, &scene);

    let success = blocked
        .align(ids.clone(), &ids[2], AlignmentMode::Left)
        .unwrap();
    let before_stale = blocked.info();
    let scene_stale = blocked.scene();
    let stale = blocked
        .service
        .objects_align(
            &blocked.document,
            "0",
            AlignParams {
                layer_id: blocked.layer.clone(),
                object_ids: ids.clone(),
                anchor_object_id: ids[2].clone(),
                mode: AlignmentMode::Right,
            },
        )
        .unwrap_err();
    assert_eq!(stale.code, "REVISION_CONFLICT");
    unchanged_after_failure(&blocked, &before_stale, &scene_stale);
    assert_eq!(success.revision, before_stale.revision);

    let cross_path = blocked.dir.join("second.gbr");
    std::fs::write(&cross_path, &blocked.source_bytes).unwrap();
    let imported = blocked
        .service
        .import_gerber_layer(
            &blocked.document,
            &blocked.info().revision,
            editor_service::ImportGerberLayerParams {
                path: cross_path.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
    let other_layer = imported.layers[0].layer_id.clone();
    let other_id = blocked.query_ids(&other_layer)[0].clone();
    let mixed = vec![ids[0].clone(), ids[1].clone(), other_id];
    let before_cross = blocked.info();
    let scene_cross = blocked.scene();
    let align_cross = blocked
        .service
        .objects_align(
            &blocked.document,
            &before_cross.revision,
            AlignParams {
                layer_id: blocked.layer.clone(),
                object_ids: mixed.clone(),
                anchor_object_id: ids[0].clone(),
                mode: AlignmentMode::Left,
            },
        )
        .unwrap_err();
    assert_eq!(align_cross.code, "CROSS_LAYER_EDIT_UNSUPPORTED");
    unchanged_after_failure(&blocked, &before_cross, &scene_cross);
    let distribute_cross = blocked
        .service
        .objects_distribute(
            &blocked.document,
            &before_cross.revision,
            DistributeParams {
                layer_id: blocked.layer.clone(),
                object_ids: mixed,
                axis: DistributionAxis::Horizontal,
            },
        )
        .unwrap_err();
    assert_eq!(distribute_cross.code, "CROSS_LAYER_EDIT_UNSUPPORTED");
    unchanged_after_failure(&blocked, &before_cross, &scene_cross);

    let info = blocked.info();
    blocked
        .service
        .layers_set_solo(
            &blocked.document,
            &info.revision,
            SetSoloLayerParams {
                expected_workspace_revision: info.workspace_revision,
                layer_id: Some(other_layer),
            },
        )
        .unwrap();
    let solo_hidden_before = blocked.info();
    let solo_hidden_scene = blocked.scene();
    let solo_hidden = blocked
        .align(ids.clone(), &ids[2], AlignmentMode::Left)
        .unwrap_err();
    assert_eq!(solo_hidden.code, "INVALID_ARGUMENT");
    unchanged_after_failure(&blocked, &solo_hidden_before, &solo_hidden_scene);
}

#[test]
fn generated_text_is_one_atomic_alignment_and_distribution_unit() {
    let mut run = Run::fixture("arrangements.gbr");
    let flash_ids = run.ids();
    let font = builtin_stroke_font().identity;
    let create_revision = run.info().revision;
    let created = run
        .service
        .text_create(
            &run.document,
            &create_revision,
            TextParams {
                layer_id: run.layer.clone(),
                font,
                layout: TextLayout {
                    text: "AB".into(),
                    x_mm: 16.,
                    y_mm: 14.,
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
    let text_ids = created.generated_object_ids;
    assert!(
        text_ids.len() > 1,
        "fixture text must span multiple generated geometries"
    );
    let anchor = flash_ids[0].clone();
    let baseline = run.scene();
    let before_partial = run.info();
    let partial_revision = before_partial.revision.clone();
    let original_bounds: Vec<_> = text_ids
        .iter()
        .map(|id| (id.clone(), run.bounds(id)))
        .collect();
    let partial_scene = run.scene();
    let partial = run
        .service
        .objects_align(
            &run.document,
            &partial_revision,
            AlignParams {
                layer_id: run.layer.clone(),
                object_ids: vec![text_ids[0].clone(), anchor.clone()],
                anchor_object_id: anchor.clone(),
                mode: AlignmentMode::HCenter,
            },
        )
        .unwrap_err();
    assert_eq!(partial.code, "INVALID_ARGUMENT");
    unchanged_after_failure(&run, &before_partial, &partial_scene);

    let mut selected = text_ids.clone();
    selected.push(anchor.clone());
    let aligned = run.align(selected, &anchor, AlignmentMode::Left).unwrap();
    assert!(
        text_ids
            .iter()
            .all(|id| aligned.changed_object_ids.contains(id))
    );
    let translations: Vec<_> = original_bounds
        .iter()
        .map(|(id, before)| {
            let after = run.bounds(id);
            (
                after.min_x_mm - before.min_x_mm,
                after.min_y_mm - before.min_y_mm,
            )
        })
        .collect();
    for movement in &translations[1..] {
        close(movement.0, translations[0].0);
        close(movement.1, translations[0].1);
    }
    let aligned_scene = run.scene();
    let revision_before_repeat = run.info().revision;
    let bounds_before_repeat: Vec<_> = text_ids
        .iter()
        .map(|id| (id.clone(), run.bounds(id)))
        .collect();
    let logical_min_x_before_repeat = text_ids
        .iter()
        .map(|id| run.bounds(id).min_x_mm)
        .fold(f64::INFINITY, f64::min);
    let anchor_min_x = run.bounds(&anchor).min_x_mm;
    let correction = run
        .align(
            text_ids
                .iter()
                .cloned()
                .chain(std::iter::once(anchor.clone()))
                .collect(),
            &anchor,
            AlignmentMode::Left,
        )
        .unwrap();
    let logical_min_x_after_repeat = text_ids
        .iter()
        .map(|id| run.bounds(id).min_x_mm)
        .fold(f64::INFINITY, f64::min);
    assert_eq!(correction.undo_entries_added, 1);
    assert_eq!(
        correction.revision.parse::<u64>().unwrap(),
        revision_before_repeat.parse::<u64>().unwrap() + 1
    );
    assert!(!correction.changed_object_ids.is_empty());
    assert!(!correction.changed_object_ids.contains(&anchor));
    assert_eq!(logical_min_x_after_repeat, anchor_min_x);
    let corrected_bounds: Vec<_> = text_ids
        .iter()
        .map(|id| (id.clone(), run.bounds(id)))
        .collect();
    let refinement_deltas: Vec<_> = bounds_before_repeat
        .iter()
        .zip(&corrected_bounds)
        .map(|((before_id, before), (after_id, after))| {
            assert_eq!(before_id, after_id);
            (
                after.min_x_mm - before.min_x_mm,
                after.min_y_mm - before.min_y_mm,
            )
        })
        .collect();
    for delta in &refinement_deltas[1..] {
        close(delta.0, refinement_deltas[0].0);
        close(delta.1, refinement_deltas[0].1);
    }
    let corrected_scene = run.scene();
    println!(
        "S4C4_TEXT_ALIGN_ULP min_x_before={logical_min_x_before_repeat:.17} target={anchor_min_x:.17} delta={:.17} min_x_after={logical_min_x_after_repeat:.17} changed_objects={}",
        anchor_min_x - logical_min_x_before_repeat,
        correction.changed_object_ids.len()
    );

    let before_true_noop = run.info();
    let no_op = run
        .align(
            text_ids
                .iter()
                .cloned()
                .chain(std::iter::once(anchor.clone()))
                .collect(),
            &anchor,
            AlignmentMode::Left,
        )
        .unwrap();
    assert!(no_op.changed_object_ids.is_empty());
    assert_eq!(no_op.undo_entries_added, 0);
    assert_eq!(no_op.revision, before_true_noop.revision);
    assert_eq!(run.info(), before_true_noop);

    let undo_correction = run
        .service
        .history_undo(&run.document, &no_op.revision)
        .unwrap();
    assert_eq!(run.scene(), aligned_scene);
    let redo_correction = run
        .service
        .history_redo(&run.document, &undo_correction.revision)
        .unwrap();
    assert_eq!(run.scene(), corrected_scene);
    let undo_correction = run
        .service
        .history_undo(&run.document, &redo_correction.revision)
        .unwrap();
    assert_eq!(run.scene(), aligned_scene);
    let undo_initial = run
        .service
        .history_undo(&run.document, &undo_correction.revision)
        .unwrap();
    assert_eq!(run.scene(), baseline);
    let redo_initial = run
        .service
        .history_redo(&run.document, &undo_initial.revision)
        .unwrap();
    assert_eq!(run.scene(), aligned_scene);
    let redo_correction = run
        .service
        .history_redo(&run.document, &redo_initial.revision)
        .unwrap();
    assert_eq!(run.scene(), corrected_scene);
    let final_undo_correction = run
        .service
        .history_undo(&run.document, &redo_correction.revision)
        .unwrap();
    assert_eq!(run.scene(), aligned_scene);
    let _final_undo_initial = run
        .service
        .history_undo(&run.document, &final_undo_correction.revision)
        .unwrap();
    assert_eq!(run.scene(), baseline);

    let mut distributed_ids = text_ids.clone();
    distributed_ids.push(flash_ids[0].clone());
    distributed_ids.push(flash_ids[2].clone());
    let original_bounds: Vec<_> = text_ids
        .iter()
        .map(|id| (id.clone(), run.bounds(id)))
        .collect();
    let endpoints = [run.object(&flash_ids[0]), run.object(&flash_ids[2])];
    let distribution = run
        .distribute(distributed_ids.clone(), DistributionAxis::Horizontal)
        .unwrap();
    assert!(
        text_ids
            .iter()
            .all(|id| distribution.changed_object_ids.contains(id))
    );
    assert_eq!(&run.object(&flash_ids[0]), &endpoints[0]);
    assert_eq!(&run.object(&flash_ids[2]), &endpoints[1]);
    let translations: Vec<_> = original_bounds
        .iter()
        .map(|(id, before)| {
            let after = run.bounds(id);
            (
                after.min_x_mm - before.min_x_mm,
                after.min_y_mm - before.min_y_mm,
            )
        })
        .collect();
    for movement in &translations[1..] {
        close(movement.0, translations[0].0);
        close(movement.1, translations[0].1);
    }
    let distributed_scene = run.scene();
    let no_op = run
        .distribute(distributed_ids, DistributionAxis::Horizontal)
        .unwrap();
    assert!(no_op.changed_object_ids.is_empty());
    let undo = run
        .service
        .history_undo(&run.document, &no_op.revision)
        .unwrap();
    assert_eq!(run.scene(), baseline);
    let _redo = run
        .service
        .history_redo(&run.document, &undo.revision)
        .unwrap();
    assert_eq!(run.scene(), distributed_scene);
    assert_eq!(undo.undo_entries_added, 0);
}

#[test]
fn alignment_roundtrips_recovery_project_and_gerber_without_writing_the_source() {
    let mut run = Run::fixture("arrangements.gbr");
    let ids = run.ids();
    let source_sha = editor_core::hash::sha256_hex(&run.source_bytes);
    let aligned = run
        .align(ids.clone(), &ids[1], AlignmentMode::VCenter)
        .unwrap();
    let distributed = run
        .distribute(ids.clone(), DistributionAxis::Horizontal)
        .unwrap();
    assert!(run.info().dirty);
    let expected = run.scene();

    let recovery = run.service.project_recovery_bytes(&run.document).unwrap();
    let recovered = run.service.project_restore(&recovery).unwrap();
    let recovered_snapshot = run.service.render_snapshot(&recovered.document_id).unwrap();
    assert_eq!(recovered_snapshot.layers, expected.0);
    assert_eq!(recovered_snapshot.apertures, expected.1);
    assert_eq!(recovered_snapshot.block_definitions, expected.2);

    let project_path = run.dir.join("aligned.rcam");
    let saved = run
        .service
        .project_save(
            &run.document,
            &distributed.revision,
            Some(project_path.to_str().unwrap()),
            false,
        )
        .unwrap();
    assert!(!saved.project_dirty);
    let reopened = run
        .service
        .project_open(project_path.to_str().unwrap())
        .unwrap();
    let reopened_snapshot = run.service.render_snapshot(&reopened.document_id).unwrap();
    assert_eq!(reopened_snapshot.layers, expected.0);
    assert_eq!(reopened_snapshot.apertures, expected.1);
    assert_eq!(reopened_snapshot.block_definitions, expected.2);

    let gerber_path = run.dir.join("aligned.gbr");
    let layer_id = reopened.layer_ids[0].clone();
    let exported = run
        .service
        .export_layer(
            &reopened.document_id,
            &reopened.revision,
            ExportParams {
                layer_id: layer_id.clone(),
                path: gerber_path.to_string_lossy().into_owned(),
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
    let gerber_opened = run.service.open(gerber_path.to_str().unwrap()).unwrap();
    let gerber_snapshot = run
        .service
        .render_snapshot(&gerber_opened.document_id)
        .unwrap();
    let source_objects = &expected.0[0].objects;
    let imported_objects = &gerber_snapshot.layers[0].objects;
    assert_eq!(imported_objects.len(), source_objects.len());
    for (expected_object, imported_object) in source_objects.iter().zip(imported_objects) {
        assert_eq!(expected_object.exposure, imported_object.exposure);
        let left = geometries_bounds_with_blocks(
            std::iter::once(&expected_object.geometry),
            &expected.1,
            &expected.2,
        )
        .unwrap()
        .unwrap();
        let right = geometries_bounds_with_blocks(
            std::iter::once(&imported_object.geometry),
            &gerber_snapshot.apertures,
            &gerber_snapshot.block_definitions,
        )
        .unwrap()
        .unwrap();
        for (a, b) in [
            (left.min_x_mm, right.min_x_mm),
            (left.min_y_mm, right.min_y_mm),
            (left.max_x_mm, right.max_x_mm),
            (left.max_y_mm, right.max_y_mm),
        ] {
            close(a, b);
        }
    }
    assert_eq!(
        std::fs::read(run.dir.join(&run.source_name)).unwrap(),
        run.source_bytes
    );
    assert_eq!(
        editor_core::hash::sha256_hex(&std::fs::read(run.dir.join(&run.source_name)).unwrap()),
        source_sha
    );
    assert_eq!(exported.exported_revision, reopened.revision);
    assert_eq!(
        aligned.revision.parse::<u64>().unwrap() + 1,
        distributed.revision.parse::<u64>().unwrap()
    );
}

#[test]
fn one_hundred_block_instances_align_as_atoms_without_definition_changes() {
    let mut run = Run::fixture("arrangements.gbr");
    let ordinary = run.ids()[1].clone();
    let mut revision = run.info().revision;
    let created = run
        .service
        .blocks_create_definition_from_objects(
            &run.document,
            &revision,
            CreateBlockDefinitionParams {
                layer_id: run.layer.clone(),
                object_ids: vec![run.ids()[0].clone()],
                local_origin_mm: PivotMm { x_mm: 5., y_mm: 5. },
                name: "alignment sample".into(),
            },
        )
        .unwrap();
    let definition_id = created.definition_id.clone();
    let mut instance_ids = vec![created.instance_object_id.clone()];
    revision = created.edit.revision;
    for index in 1..100 {
        let created = run
            .service
            .blocks_create_instance(
                &run.document,
                &revision,
                CreateBlockInstanceParams {
                    layer_id: run.layer.clone(),
                    definition_id: definition_id.clone(),
                    transform: BlockTransformParams {
                        translation_mm: PivotMm {
                            x_mm: f64::from(index) * 2.,
                            y_mm: f64::from(index % 7),
                        },
                        rotation_deg: f64::from(index % 4) * 90.,
                        mirror: index % 2 == 0,
                    },
                },
            )
            .unwrap();
        revision = created.edit.revision;
        instance_ids.push(created.object_id);
    }
    let definition_before = run
        .service
        .blocks_get_definition(
            &run.document,
            BlockDefinitionIdParams {
                definition_id: definition_id.clone(),
            },
        )
        .unwrap();
    let before_layer = run.layer();
    let before_transforms: Vec<_> = instance_ids
        .iter()
        .map(|id| {
            let object = before_layer
                .objects
                .iter()
                .find(|object| &object.object_id == id)
                .unwrap();
            match &object.geometry {
                SemanticGeometry::BlockInstance {
                    definition_id,
                    transform,
                } => (id.clone(), definition_id.clone(), *transform),
                other => panic!("expected a BlockInstance, got {other:?}"),
            }
        })
        .collect();
    let params = AlignParams {
        layer_id: run.layer.clone(),
        object_ids: instance_ids.clone(),
        anchor_object_id: instance_ids[0].clone(),
        mode: AlignmentMode::Left,
    };
    let aligned = run
        .service
        .objects_align(&run.document, &revision, params.clone())
        .unwrap();
    assert_eq!(aligned.undo_entries_added, 1);
    assert_eq!(aligned.changed_object_ids.len(), 99);
    assert_eq!(aligned.changed_object_ids.len() + 1, instance_ids.len());
    let after_layer = run.layer();
    for (id, before_definition, before_transform) in &before_transforms {
        let object = after_layer
            .objects
            .iter()
            .find(|object| &object.object_id == id)
            .unwrap();
        let SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } = &object.geometry
        else {
            panic!("instance identity changed")
        };
        assert_eq!(definition_id, before_definition);
        assert_eq!(transform.rotation_deg, before_transform.rotation_deg);
        assert_eq!(transform.mirror, before_transform.mirror);
        assert_eq!(
            transform.translation.y_mm,
            before_transform.translation.y_mm
        );
        if id == &instance_ids[0] {
            assert_eq!(transform, before_transform);
        }
    }
    assert_eq!(
        &run.object(&ordinary),
        before_layer
            .objects
            .iter()
            .find(|object| object.object_id == ordinary)
            .unwrap()
    );
    let definition_after = run
        .service
        .blocks_get_definition(&run.document, BlockDefinitionIdParams { definition_id })
        .unwrap();
    assert_eq!(definition_after, definition_before);
    let aligned_scene = run.scene();
    let undo = run
        .service
        .history_undo(&run.document, &aligned.revision)
        .unwrap();
    assert_eq!(run.layer(), before_layer);
    let redo = run
        .service
        .history_redo(&run.document, &undo.revision)
        .unwrap();
    assert_eq!(run.scene(), aligned_scene);
    let _ = run
        .service
        .history_undo(&run.document, &redo.revision)
        .unwrap();
    assert_eq!(run.layer(), before_layer);
}

fn synthetic_dots(count: usize) -> Vec<u8> {
    let mut source = String::from("%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.25*%\nD10*\n");
    for index in 0..count {
        let x = 1_000_000 + (index % 100) * 250_000;
        let y = 1_000_000 + (index / 100) * 250_000;
        source.push_str(&format!("X{x}Y{y}D03*\n"));
    }
    source.push_str("M02*\n");
    source.into_bytes()
}

#[test]
#[ignore = "release performance and 10,000 edit limit evidence gate"]
fn arrangement_performance_and_edit_resource_limit() {
    let mut thousand = Run::fixture("grid-1000.gbr");
    let ids = thousand.ids();
    assert_eq!(ids.len(), 1_000);
    let snapshot = thousand.snapshot();
    let objects = &snapshot.layers[0].objects;
    let started = std::time::Instant::now();
    let bounds = editor_core::individual_geometries_bounds_with_blocks(
        objects.iter().map(|object| &object.geometry),
        &snapshot.apertures,
        &snapshot.block_definitions,
    )
    .unwrap();
    let bounds_us = started.elapsed().as_micros();
    let object_bounds: Vec<_> = objects
        .iter()
        .zip(bounds)
        .enumerate()
        .map(|(index, (object, bounds))| editor_core::ObjectBoundsMm {
            object_ids: vec![object.object_id.clone()],
            layer_order: index,
            bounds: bounds.unwrap(),
        })
        .collect();
    let started = std::time::Instant::now();
    let deltas =
        editor_core::compute_alignment_deltas(&object_bounds, &ids[0], AlignmentMode::Left)
            .unwrap();
    let delta_us = started.elapsed().as_micros();
    let expected_1k_changed = deltas
        .iter()
        .filter(|delta| delta.dx_mm != 0.0 || delta.dy_mm != 0.0)
        .map(|delta| delta.object_ids.len())
        .sum::<usize>();
    let started = std::time::Instant::now();
    let thousand_result = thousand
        .align(ids.clone(), &ids[0], AlignmentMode::Left)
        .unwrap();
    let service_total_us = started.elapsed().as_micros();
    assert_eq!(
        thousand_result.changed_object_ids.len(),
        expected_1k_changed
    );
    println!(
        "S4C4_PERF selected=1000 bounds_us={bounds_us} delta_us={delta_us} service_total_us={service_total_us} changed={}",
        thousand_result.changed_object_ids.len()
    );
    assert_eq!(deltas.len(), 1_000);

    let mut ten_thousand = Run::from_source("grid-10000.gbr", &synthetic_dots(10_000));
    let ten_ids = ten_thousand.ids();
    assert_eq!(ten_ids.len(), 10_000);
    let ten_snapshot = ten_thousand.snapshot();
    let ten_objects = &ten_snapshot.layers[0].objects;
    let ten_bounds = editor_core::individual_geometries_bounds_with_blocks(
        ten_objects.iter().map(|object| &object.geometry),
        &ten_snapshot.apertures,
        &ten_snapshot.block_definitions,
    )
    .unwrap();
    let ten_object_bounds: Vec<_> = ten_objects
        .iter()
        .zip(ten_bounds)
        .enumerate()
        .map(|(index, (object, bounds))| editor_core::ObjectBoundsMm {
            object_ids: vec![object.object_id.clone()],
            layer_order: index,
            bounds: bounds.unwrap(),
        })
        .collect();
    let ten_deltas =
        editor_core::compute_alignment_deltas(&ten_object_bounds, &ten_ids[0], AlignmentMode::Left)
            .unwrap();
    let expected_10k_changed = ten_deltas
        .iter()
        .filter(|delta| delta.dx_mm != 0.0 || delta.dy_mm != 0.0)
        .map(|delta| delta.object_ids.len())
        .sum::<usize>();
    let started = std::time::Instant::now();
    let accepted = ten_thousand
        .align(ten_ids.clone(), &ten_ids[0], AlignmentMode::Left)
        .unwrap();
    let ten_thousand_us = started.elapsed().as_micros();
    assert_eq!(accepted.changed_object_ids.len(), expected_10k_changed);
    println!(
        "S4C4_PERF selected=10000 service_total_us={ten_thousand_us} changed={}",
        accepted.changed_object_ids.len()
    );

    let mut over_limit = Run::from_source("grid-10001.gbr", &synthetic_dots(10_001));
    let too_many = over_limit.ids();
    assert_eq!(too_many.len(), 10_001);
    let before = over_limit.info();
    let before_scene = over_limit.scene();
    let anchor = over_limit.ids()[0].clone();
    let error = over_limit
        .align(too_many, &anchor, AlignmentMode::Left)
        .unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    unchanged_after_failure(&over_limit, &before, &before_scene);
}

#[test]
#[ignore = "writes the checked-in public native S4-C4 project fixture"]
fn generate_native_blocks_project_fixture() {
    let fixture_dir = PathBuf::from(FIXTURES);
    let output = fixture_dir.join("native-blocks.rcam");
    assert!(!output.exists(), "refusing to overwrite a frozen fixture");
    let dir = std::env::temp_dir().join(format!("rcam-s4c4-fixture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = std::fs::read(Path::new(FIXTURES).join("arrangements.gbr")).unwrap();
    std::fs::write(dir.join("arrangements.gbr"), &source).unwrap();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        dir.clone(),
        [dir.clone(), fixture_dir.clone()],
        [fixture_dir.clone()],
    ));
    let opened = service.open("arrangements.gbr").unwrap();
    let document = opened.document_id;
    let layer = opened.layer_ids[0].clone();
    let ids: Vec<_> = service
        .objects_query(
            &document,
            QueryParams {
                layer_id: layer.clone(),
                geometry_type: None,
                region_mm: None,
                relation: None,
                limit: None,
                cursor: None,
            },
        )
        .unwrap()
        .objects
        .into_iter()
        .map(|item| item.object.object_id)
        .collect();
    assert_eq!(ids.len(), 3);
    let mut revision = service.document_get(&document).unwrap().revision;
    let definition = service
        .blocks_create_definition_from_objects(
            &document,
            &revision,
            CreateBlockDefinitionParams {
                layer_id: layer.clone(),
                object_ids: vec![ids[0].clone()],
                local_origin_mm: PivotMm { x_mm: 5., y_mm: 5. },
                name: "native alignment block".into(),
            },
        )
        .unwrap();
    revision = definition.edit.revision;
    let first_update = service
        .blocks_update_instance_transform(
            &document,
            &revision,
            UpdateBlockInstanceTransformParams {
                layer_id: layer.clone(),
                object_id: definition.instance_object_id.clone(),
                transform: BlockTransformParams {
                    translation_mm: PivotMm {
                        x_mm: 10.,
                        y_mm: 10.,
                    },
                    rotation_deg: 30.,
                    mirror: true,
                },
            },
        )
        .unwrap();
    revision = first_update.revision;
    let removed = service
        .objects_delete(
            &document,
            &revision,
            DeleteParams {
                layer_id: layer.clone(),
                object_ids: vec![ids[1].clone()],
            },
        )
        .unwrap();
    revision = removed.revision;
    for (x, y, rotation, mirror) in [(25., 10., 90., false), (15., 28., 270., true)] {
        let created = service
            .blocks_create_instance(
                &document,
                &revision,
                CreateBlockInstanceParams {
                    layer_id: layer.clone(),
                    definition_id: definition.definition_id.clone(),
                    transform: BlockTransformParams {
                        translation_mm: PivotMm { x_mm: x, y_mm: y },
                        rotation_deg: rotation,
                        mirror,
                    },
                },
            )
            .unwrap();
        revision = created.edit.revision;
    }
    let before_save = service.render_snapshot(&document).unwrap();
    assert_eq!(before_save.layers[0].objects.len(), 4);
    assert_eq!(
        before_save.layers[0]
            .objects
            .iter()
            .filter(|object| matches!(object.geometry, SemanticGeometry::BlockInstance { .. }))
            .count(),
        3
    );
    assert_eq!(
        before_save.layers[0]
            .objects
            .iter()
            .filter(|object| matches!(object.geometry, SemanticGeometry::Flash { .. }))
            .count(),
        1
    );
    service
        .project_save(&document, &revision, Some(output.to_str().unwrap()), false)
        .unwrap();
    let reopened = service.project_open(output.to_str().unwrap()).unwrap();
    let after_open = service.render_snapshot(&reopened.document_id).unwrap();
    assert_eq!(after_open.layers[0].objects, before_save.layers[0].objects);
    std::fs::remove_dir_all(dir).unwrap();
}
