//! S3-FINAL headless editing, aperture COW, ordered Clear duplication and history budget.
use editor_core::{ApertureShape, Exposure, MmPoint, SemanticGeometry};
use editor_service::{
    ApplicationService, BatchParams, BatchStepParams, DeleteParams, DuplicateParams, ExportParams,
    FileAccessPolicy, MetadataPolicy, MirrorAxis, MirrorParams, MoveParams, OverwritePolicy,
    PivotMm, QueryParams, RotateParams, SetPropertiesParams,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);
const FLASHES: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\nD10*\nX10000000Y20000000D03*\nX20000000Y20000000D03*\nX30000000Y20000000D03*\nM02*\n";
const STANDARD_FLASHES: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2X0.5*%\n%ADD11R,2X3X0.5*%\n%ADD12O,2X3X0.5*%\n%ADD13P,2X5X15X0.5*%\nD10*\nX1000000Y1000000D03*\nD11*\nX2000000Y1000000D03*\nD12*\nX3000000Y1000000D03*\nD13*\nX4000000Y1000000D03*\nM02*\n";
const ORDERED_CLEAR: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,10*%\n%ADD11C,6*%\n%ADD12C,2*%\n%LPD*%\nD10*\nX0Y0D03*\n%LPC*%\nD11*\nX0Y0D03*\n%LPD*%\nD12*\nX0Y0D03*\nM02*\n";

struct Run {
    service: ApplicationService,
    dir: PathBuf,
    id: String,
    layer: String,
}

impl Run {
    fn new(name: &str, source: &str) -> Self {
        Self::with_limits(name, source, 100, 64 * 1024 * 1024)
    }

    fn with_limits(name: &str, source: &str, entries: usize, bytes: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-s3-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.gbr"), source).unwrap();
        let policy = FileAccessPolicy::new(dir.clone(), [dir.clone()], [dir.clone()]);
        let mut service =
            ApplicationService::with_file_access_and_history_limits(policy, entries, bytes)
                .unwrap();
        let opened = service.open("source.gbr").unwrap();
        Self {
            service,
            dir,
            id: opened.document_id,
            layer: opened.layer_ids[0].clone(),
        }
    }

    fn objects(&self) -> Vec<editor_service::ObjectInfo> {
        self.service
            .objects_query(
                &self.id,
                QueryParams {
                    layer_id: self.layer.clone(),
                    geometry_type: None,
                    region_mm: None,
                    relation: None,
                    limit: None,
                    cursor: None,
                },
            )
            .unwrap()
            .objects
    }

    fn revision(&self) -> String {
        self.service.document_get(&self.id).unwrap().revision
    }

    fn export(&mut self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        self.service
            .export_layer(
                &self.id,
                &self.revision(),
                ExportParams {
                    layer_id: self.layer.clone(),
                    path: path.to_string_lossy().into_owned(),
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
        path
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn flash_size_is_copy_on_write_undoable_and_roundtrips() {
    let mut run = Run::new("aperture-cow", FLASHES);
    let before = run.objects();
    let ids: Vec<_> = before
        .iter()
        .map(|object| object.object.object_id.clone())
        .collect();
    let original_aperture = match &before[0].object.geometry {
        SemanticGeometry::Flash { aperture_id, .. } => aperture_id.clone(),
        _ => panic!(),
    };
    let metrics_before = run
        .service
        .objects_metrics(
            &run.id,
            editor_service::MetricsParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[0].clone()],
            },
        )
        .unwrap();

    run.service
        .objects_set_properties(
            &run.id,
            "0",
            SetPropertiesParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[0].clone()],
                width_mm: 4.,
                height_mm: None,
            },
        )
        .unwrap();
    let resized = run.objects();
    let resized_aperture = match &resized[0].object.geometry {
        SemanticGeometry::Flash { aperture_id, .. } => aperture_id.clone(),
        _ => panic!(),
    };
    assert_ne!(resized_aperture, original_aperture);
    for object in &resized[1..] {
        assert!(
            matches!(&object.object.geometry, SemanticGeometry::Flash { aperture_id, .. } if aperture_id == &original_aperture)
        );
    }
    let snapshot = run.service.render_snapshot(&run.id).unwrap();
    assert!(matches!(
        snapshot
            .apertures
            .iter()
            .find(|aperture| aperture.id == resized_aperture)
            .unwrap()
            .shape,
        ApertureShape::Circle {
            diameter_mm: 4.,
            ..
        }
    ));
    let metrics_after = run
        .service
        .objects_metrics(
            &run.id,
            editor_service::MetricsParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[0].clone()],
            },
        )
        .unwrap();
    assert_ne!(metrics_after.items, metrics_before.items);

    run.service.history_undo(&run.id, "1").unwrap();
    assert_eq!(run.objects(), before);
    assert_eq!(
        run.service
            .objects_metrics(
                &run.id,
                editor_service::MetricsParams {
                    layer_id: run.layer.clone(),
                    object_ids: vec![ids[0].clone()],
                },
            )
            .unwrap()
            .items,
        metrics_before.items
    );
    run.service.history_redo(&run.id, "2").unwrap();
    let output = run.export("resized.gbr");
    let reopened = gerber_io::parse_s1(&std::fs::read(output).unwrap(), "reopened").unwrap();
    let shapes: Vec<_> = reopened.document.layers[0]
        .objects
        .iter()
        .map(|object| match &object.geometry {
            SemanticGeometry::Flash { aperture_id, .. } => reopened
                .document
                .apertures
                .iter()
                .find(|aperture| &aperture.id == aperture_id)
                .unwrap()
                .shape
                .clone(),
            _ => panic!(),
        })
        .collect();
    assert!(matches!(
        shapes[0],
        ApertureShape::Circle {
            diameter_mm: 4.,
            ..
        }
    ));
    assert!(shapes[1..].iter().all(|shape| matches!(
        shape,
        ApertureShape::Circle {
            diameter_mm: 2.,
            ..
        }
    )));
}

#[test]
fn all_standard_flash_shapes_resize_with_holes_and_roundtrip() {
    let mut run = Run::new("standard-aperture-cow", STANDARD_FLASHES);
    let ids: Vec<_> = run
        .objects()
        .iter()
        .map(|object| object.object.object_id.clone())
        .collect();
    for (revision, (index, width, height)) in [
        (0, 4., None),
        (1, 4., Some(5.)),
        (2, 4., Some(5.)),
        (3, 4., None),
    ]
    .into_iter()
    .enumerate()
    {
        run.service
            .objects_set_properties(
                &run.id,
                &revision.to_string(),
                SetPropertiesParams {
                    layer_id: run.layer.clone(),
                    object_ids: vec![ids[index].clone()],
                    width_mm: width,
                    height_mm: height,
                },
            )
            .unwrap();
    }
    let snapshot = run.service.render_snapshot(&run.id).unwrap();
    let shapes: Vec<_> = run
        .objects()
        .iter()
        .map(|object| match &object.object.geometry {
            SemanticGeometry::Flash { aperture_id, .. } => snapshot
                .apertures
                .iter()
                .find(|aperture| &aperture.id == aperture_id)
                .unwrap()
                .shape
                .clone(),
            _ => panic!(),
        })
        .collect();
    assert!(matches!(
        shapes[0],
        ApertureShape::Circle {
            diameter_mm: 4.,
            hole_diameter_mm: Some(0.5)
        }
    ));
    assert!(matches!(
        shapes[1],
        ApertureShape::Rectangle {
            width_mm: 4.,
            height_mm: 5.,
            hole_diameter_mm: Some(0.5)
        }
    ));
    assert!(matches!(
        shapes[2],
        ApertureShape::Obround {
            width_mm: 4.,
            height_mm: 5.,
            hole_diameter_mm: Some(0.5)
        }
    ));
    assert!(matches!(
        shapes[3],
        ApertureShape::Polygon {
            diameter_mm: 4.,
            vertices: 5,
            rotation_deg: 15.,
            hole_diameter_mm: Some(0.5)
        }
    ));
    assert_eq!(snapshot.apertures.len(), 8);

    let before_invalid = run.objects();
    let error = run
        .service
        .objects_set_properties(
            &run.id,
            "4",
            SetPropertiesParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[1].clone()],
                width_mm: 6.,
                height_mm: None,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENT");
    assert_eq!(run.objects(), before_invalid);
    assert_eq!(run.revision(), "4");

    let output = run.export("standard-resized.gbr");
    let reopened = gerber_io::parse_s1(&std::fs::read(output).unwrap(), "reopened").unwrap();
    assert_eq!(reopened.document.layers[0].objects.len(), 4);
    for (object, expected) in reopened.document.layers[0].objects.iter().zip(shapes) {
        let SemanticGeometry::Flash { aperture_id, .. } = &object.geometry else {
            panic!()
        };
        assert_eq!(
            reopened
                .document
                .apertures
                .iter()
                .find(|aperture| &aperture.id == aperture_id)
                .unwrap()
                .shape,
            expected
        );
    }
}

#[test]
fn history_evicts_complete_transactions_and_rejects_oversized_transaction() {
    let mut run = Run::with_limits("history-eviction", FLASHES, 3, 64 * 1024);
    let id = run.objects()[0].object.object_id.clone();
    for revision in 0..5 {
        run.service
            .objects_move(
                &run.id,
                &revision.to_string(),
                MoveParams {
                    layer_id: run.layer.clone(),
                    object_ids: vec![id.clone()],
                    dx_mm: 1.,
                    dy_mm: 0.,
                },
            )
            .unwrap();
    }
    let info = run.service.document_get(&run.id).unwrap();
    assert_eq!(info.undo_entries, 3);
    assert_eq!(info.history_truncated_entries, 2);
    assert!(info.history_bytes <= 64 * 1024);
    for revision in 5..8 {
        run.service
            .history_undo(&run.id, &revision.to_string())
            .unwrap();
    }
    assert_eq!(
        run.service.history_undo(&run.id, "8").unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    let center = match run.objects()[0].object.geometry {
        SemanticGeometry::Flash { center, .. } => center,
        _ => panic!(),
    };
    assert_eq!(center, MmPoint::new(12., 20.));

    let mut too_small = Run::with_limits("history-single-limit", FLASHES, 3, 1);
    let before = too_small.objects();
    let error = too_small
        .service
        .objects_move(
            &too_small.id,
            "0",
            MoveParams {
                layer_id: too_small.layer.clone(),
                object_ids: vec![before[0].object.object_id.clone()],
                dx_mm: 1.,
                dy_mm: 0.,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    assert_eq!(too_small.objects(), before);
    assert_eq!(too_small.revision(), "0");
}

#[test]
fn ordered_dark_clear_dark_duplicate_roundtrips_without_reordering() {
    let mut run = Run::new("ordered-clear", ORDERED_CLEAR);
    let ids: Vec<_> = run
        .objects()
        .iter()
        .map(|object| object.object.object_id.clone())
        .collect();
    let copies = run
        .service
        .objects_duplicate(
            &run.id,
            "0",
            DuplicateParams {
                layer_id: run.layer.clone(),
                object_ids: ids,
                dx_mm: 20.,
                dy_mm: 0.,
            },
        )
        .unwrap()
        .changed_object_ids;
    assert_eq!(copies.len(), 3);
    let after = run.objects();
    assert_eq!(
        after
            .iter()
            .map(|object| object.object.exposure)
            .collect::<Vec<_>>(),
        vec![
            Exposure::Dark,
            Exposure::Dark,
            Exposure::Clear,
            Exposure::Clear,
            Exposure::Dark,
            Exposure::Dark
        ]
    );
    let output = run.export("ordered-copy.gbr");
    let reopened = gerber_io::parse_s1(&std::fs::read(output).unwrap(), "reopened").unwrap();
    assert_eq!(
        reopened.document.layers[0]
            .objects
            .iter()
            .map(|object| object.exposure)
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|object| object.object.exposure)
            .collect::<Vec<_>>()
    );
    for (point, expected) in [
        (MmPoint::new(20., 0.), true),
        (MmPoint::new(23., 0.), false),
        (MmPoint::new(24., 0.), true),
    ] {
        assert_eq!(
            reopened
                .document
                .layer_coverage_at(&reopened.document.layers[0].id, point),
            Some(expected)
        );
    }
}

#[test]
fn complete_headless_s3_edit_validate_export_reopen_geometry_flow() {
    let mut run = Run::new("headless-complete", FLASHES);
    let original_source = std::fs::read(run.dir.join("source.gbr")).unwrap();
    let ids: Vec<_> = run
        .objects()
        .iter()
        .map(|object| object.object.object_id.clone())
        .collect();
    run.service
        .objects_move(
            &run.id,
            "0",
            MoveParams {
                layer_id: run.layer.clone(),
                object_ids: ids[..2].to_vec(),
                dx_mm: 5.,
                dy_mm: -3.,
            },
        )
        .unwrap();
    let copy = run
        .service
        .objects_duplicate(
            &run.id,
            "1",
            DuplicateParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[0].clone()],
                dx_mm: 10.,
                dy_mm: 0.,
            },
        )
        .unwrap()
        .changed_object_ids;
    run.service
        .objects_delete(
            &run.id,
            "2",
            DeleteParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[2].clone()],
            },
        )
        .unwrap();
    run.service
        .objects_rotate(
            &run.id,
            "3",
            RotateParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[0].clone(), copy[0].clone()],
                angle_deg: 37.,
                pivot_mm: PivotMm { x_mm: 0., y_mm: 0. },
            },
        )
        .unwrap();
    run.service
        .objects_mirror(
            &run.id,
            "4",
            MirrorParams {
                layer_id: run.layer.clone(),
                object_ids: vec![ids[0].clone(), copy[0].clone()],
                axis: MirrorAxis::Horizontal { coordinate_mm: 0. },
            },
        )
        .unwrap();
    let expected = run.objects();
    run.service.history_undo(&run.id, "5").unwrap();
    run.service.history_redo(&run.id, "6").unwrap();
    assert_eq!(run.objects(), expected);
    assert!(run.service.validate(&run.id).unwrap().valid);
    let output = run.export("complete.gbr");
    assert_eq!(
        std::fs::read(run.dir.join("source.gbr")).unwrap(),
        original_source
    );
    let reopened = run.service.open(output.to_str().unwrap()).unwrap();
    let reopened_objects = run
        .service
        .objects_query(
            &reopened.document_id,
            QueryParams {
                layer_id: reopened.layer_ids[0].clone(),
                geometry_type: None,
                region_mm: None,
                relation: None,
                limit: None,
                cursor: None,
            },
        )
        .unwrap()
        .objects;
    assert_eq!(reopened_objects.len(), expected.len());
    for (actual, expected) in reopened_objects.iter().zip(&expected) {
        assert_eq!(actual.object.exposure, expected.object.exposure);
        match (&actual.object.geometry, &expected.object.geometry) {
            (
                SemanticGeometry::Flash {
                    center: actual_center,
                    transform: actual_transform,
                    ..
                },
                SemanticGeometry::Flash {
                    center: expected_center,
                    transform: expected_transform,
                    ..
                },
            ) => {
                let quantized = editor_core::MmPoint::new(
                    (expected_center.x_mm * 10000.).round() / 10000.,
                    (expected_center.y_mm * 10000.).round() / 10000.,
                );
                assert!(actual_center.distance_mm(quantized) <= 1e-6);
                assert_eq!(actual_transform, expected_transform);
            }
            _ => panic!("unexpected geometry in fixed headless fixture"),
        }
    }
}

#[test]
fn edit_batch_is_one_atomic_revision_and_rejects_external_io_steps() {
    let mut run = Run::new("atomic-batch", FLASHES);
    let before = run.objects();
    let id = before[0].object.object_id.clone();
    let result = run
        .service
        .edit_batch(
            &run.id,
            "0",
            BatchParams {
                layer_id: run.layer.clone(),
                steps: vec![
                    BatchStepParams::Move {
                        object_ids: vec![id.clone()],
                        dx_mm: 5.,
                        dy_mm: 0.,
                    },
                    BatchStepParams::Rotate {
                        object_ids: vec![id.clone()],
                        angle_deg: 90.,
                        pivot_mm: PivotMm { x_mm: 0., y_mm: 0. },
                    },
                    BatchStepParams::SetProperties {
                        object_ids: vec![id.clone()],
                        width_mm: 4.,
                        height_mm: None,
                    },
                ],
            },
        )
        .unwrap();
    assert_eq!(result.revision, "1");
    assert_eq!(result.undo_entries_added, 1);
    assert_eq!(result.undo_entries, 1);
    let changed = run.objects();
    assert_ne!(changed, before);

    run.service.history_undo(&run.id, "1").unwrap();
    assert_eq!(run.objects(), before);
    run.service.history_redo(&run.id, "2").unwrap();
    assert_eq!(run.objects(), changed);

    let before_failure = run.objects();
    let info_before_failure = run.service.document_get(&run.id).unwrap();
    let error = run
        .service
        .edit_batch(
            &run.id,
            "3",
            BatchParams {
                layer_id: run.layer.clone(),
                steps: vec![
                    BatchStepParams::Move {
                        object_ids: vec![id.clone()],
                        dx_mm: 1.,
                        dy_mm: 0.,
                    },
                    BatchStepParams::SetProperties {
                        object_ids: vec![id.clone()],
                        width_mm: f64::NAN,
                        height_mm: None,
                    },
                ],
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENT");
    assert_eq!(run.objects(), before_failure);
    assert_eq!(
        run.service.document_get(&run.id).unwrap(),
        info_before_failure
    );

    let mut over_budget = Run::with_limits("batch-budget", FLASHES, 10, 1);
    let over_before = over_budget.objects();
    let over_id = over_before[0].object.object_id.clone();
    let error = over_budget
        .service
        .edit_batch(
            &over_budget.id,
            "0",
            BatchParams {
                layer_id: over_budget.layer.clone(),
                steps: vec![BatchStepParams::Move {
                    object_ids: vec![over_id],
                    dx_mm: 1.,
                    dy_mm: 0.,
                }],
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    assert_eq!(over_budget.objects(), over_before);
    assert_eq!(over_budget.revision(), "0");

    let reply = run.service.execute_json(
        &json!({
            "api_version": 1,
            "request_id": "batch-external-io",
            "op": "edit.batch",
            "document_id": run.id,
            "expected_revision": "3",
            "params": {
                "layer_id": run.layer,
                "steps": [{
                    "op": "gerber.export_layer",
                    "path": "forbidden.gbr"
                }]
            }
        })
        .to_string(),
    );
    assert_eq!(reply["error"]["code"], "INVALID_ARGUMENT", "{reply}");
    assert_eq!(run.objects(), before_failure);
    assert_eq!(
        run.service.document_get(&run.id).unwrap(),
        info_before_failure
    );
}
