//! Read-only verification of artifacts saved/exported by the actual native UI.
use editor_core::{SemanticGeometry, hash::sha256_hex, individual_geometries_bounds_with_blocks};
use editor_service::{ApplicationService, FileAccessPolicy, MetricsParams, RenderSnapshot};
use serde_json::json;
use std::path::PathBuf;

fn bounds(snapshot: &RenderSnapshot) -> Vec<[f64; 4]> {
    individual_geometries_bounds_with_blocks(
        snapshot.layers[0]
            .objects
            .iter()
            .map(|object| &object.geometry),
        &snapshot.apertures,
        &snapshot.block_definitions,
    )
    .unwrap()
    .into_iter()
    .map(|bounds| {
        let b = bounds.unwrap();
        [b.min_x_mm, b.min_y_mm, b.max_x_mm, b.max_y_mm]
    })
    .collect()
}

#[test]
#[ignore = "requires actual native UI saved/exported public S4-C4 artifacts"]
fn native_saved_project_and_export_reopen_preserve_manufacturing_geometry() {
    let dir = PathBuf::from(
        std::env::var_os("RCAM_S4C4_NATIVE_OUTPUT_DIR").expect("explicit native output directory"),
    )
    .canonicalize()
    .unwrap();
    let project_bytes = std::fs::read(dir.join("native-saved.rcam")).unwrap();
    let gerber_bytes = std::fs::read(dir.join("native-export.gbr")).unwrap();
    let mut service =
        ApplicationService::with_file_access(FileAccessPolicy::new(dir.clone(), [dir.clone()], []));
    let project = service.project_open("native-saved.rcam").unwrap();
    let snapshot = service.render_snapshot(&project.document_id).unwrap();
    assert_eq!(snapshot.layers.len(), 1);
    assert_eq!(snapshot.layers[0].objects.len(), 3);
    assert!(snapshot.block_definitions.is_empty());
    assert!(
        snapshot.layers[0]
            .objects
            .iter()
            .all(|object| matches!(object.geometry, SemanticGeometry::Flash { .. }))
    );
    let original = include_bytes!("../../../fixtures/synthetic/s4c4/arrangements.gbr");
    let layers = service.layers_list(&project.document_id).unwrap();
    assert_eq!(
        layers[0].provenance.as_ref().unwrap().imported_sha256,
        sha256_hex(original)
    );
    let reopened = service.project_open("native-saved.rcam").unwrap();
    let again = service.render_snapshot(&reopened.document_id).unwrap();
    assert_eq!(snapshot.layers, again.layers);
    assert_eq!(snapshot.apertures, again.apertures);
    let gerber = service.open("native-export.gbr").unwrap();
    let imported = service.render_snapshot(&gerber.document_id).unwrap();
    assert_eq!(imported.layers.len(), 1);
    assert_eq!(imported.layers[0].objects.len(), 3);
    let before_bounds = bounds(&snapshot);
    let after_bounds = bounds(&imported);
    for (before, after) in before_bounds.iter().zip(&after_bounds) {
        for (a, b) in before.iter().zip(after) {
            assert!(
                (a - b).abs() < 1e-7,
                "native export bound changed: {a} -> {b}"
            );
        }
    }
    assert_eq!(
        snapshot.layers[0]
            .objects
            .iter()
            .map(|o| o.exposure)
            .collect::<Vec<_>>(),
        imported.layers[0]
            .objects
            .iter()
            .map(|o| o.exposure)
            .collect::<Vec<_>>()
    );
    let mut summaries = Vec::new();
    for scene in [&snapshot, &imported] {
        summaries.push(
            service
                .objects_metrics(
                    &scene.document_id,
                    MetricsParams {
                        layer_id: scene.layers[0].id.clone(),
                        object_ids: scene.layers[0]
                            .objects
                            .iter()
                            .map(|o| o.object_id.clone())
                            .collect(),
                    },
                )
                .unwrap()
                .summary,
        );
    }
    assert_eq!(summaries[0], summaries[1]);
    assert_eq!(summaries[0].exact_count, 3);
    assert_eq!(summaries[0].object_area_sum_mm2, 13.0);
    assert_eq!(summaries[0].object_perimeter_sum_mm, 26.0);
    assert_eq!(
        std::fs::read(dir.join("native-saved.rcam")).unwrap(),
        project_bytes
    );
    assert_eq!(
        std::fs::read(dir.join("native-export.gbr")).unwrap(),
        gerber_bytes
    );
    let report = json!({
        "schema_version":2,"stage":"S4-C4","status":"PASS",
        "evidence_kind":"read-only service verification of native CUA produced files",
        "project_sha256":sha256_hex(&project_bytes),"gerber_sha256":sha256_hex(&gerber_bytes),
        "project_semantic_objects_sha256":sha256_hex(&serde_json::to_vec(&snapshot.layers[0].objects).unwrap()),
        "project_bounds_mm":before_bounds,"gerber_bounds_mm":after_bounds,
        "metrics":summaries,"project_reopen_exact":true,"input_files_unchanged":true
    });
    std::fs::write(
        dir.join("native-roundtrip-service.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}
