//! Read-only validation of files produced by actual native CUA actions.
use editor_core::{SemanticGeometry, hash::sha256_hex};
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::json;
#[test]
#[ignore = "requires actual native S4-C5 CUA saved/exported artifacts"]
fn native_array_projects_and_expanded_block_export() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("RCAM_S4C5_NATIVE_OUTPUT_DIR").expect("explicit output directory"),
    )
    .canonicalize()
    .unwrap();
    let mut service =
        ApplicationService::with_file_access(FileAccessPolicy::new(dir.clone(), [dir.clone()], []));
    let mut results = vec![];
    for (name, count, defs) in [("ordinary.rcam", 36, 0), ("block.rcam", 100, 1)] {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let opened = service.project_open(name).unwrap();
        let snapshot = service.render_snapshot(&opened.document_id).unwrap();
        assert_eq!(snapshot.layers[0].objects.len(), count);
        assert_eq!(snapshot.block_definitions.len(), defs);
        let again = service.project_open(name).unwrap();
        let reopened = service.render_snapshot(&again.document_id).unwrap();
        assert_eq!(snapshot.layers, reopened.layers);
        assert_eq!(snapshot.block_definitions, reopened.block_definitions);
        if defs == 1 {
            assert_eq!(snapshot.block_definitions[0].objects.len(), 400);
            for (i, o) in snapshot.layers[0].objects.iter().enumerate() {
                let SemanticGeometry::BlockInstance { transform, .. } = o.geometry else {
                    panic!()
                };
                assert_eq!(transform.translation.x_mm, (i % 10) as f64 * 50.);
                assert_eq!(transform.translation.y_mm, (i / 10) as f64 * 50.);
            }
        }
        assert_eq!(bytes, std::fs::read(dir.join(name)).unwrap());
        results.push(json!({"file":name,"sha256":sha256_hex(&bytes),"objects":count,"definitions":defs,"reopen_exact":true}));
    }
    let bytes = std::fs::read(dir.join("block-export.gbr")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("%SR"));
    let opened = service.open("block-export.gbr").unwrap();
    let snapshot = service.render_snapshot(&opened.document_id).unwrap();
    assert_eq!(snapshot.layers[0].objects.len(), 40000);
    for (i, o) in snapshot.layers[0].objects.iter().enumerate() {
        let SemanticGeometry::Flash { center, .. } = o.geometry else {
            panic!()
        };
        let cell = i / 400;
        let source = i % 400;
        assert!(
            (center.x_mm - ((cell % 10) as f64 * 50. + (source % 20) as f64 * 2.)).abs() < 1e-7
        );
        assert!(
            (center.y_mm - ((cell / 10) as f64 * 50. + (source / 20) as f64 * 2.)).abs() < 1e-7
        );
    }
    results.push(json!({"file":"block-export.gbr","sha256":sha256_hex(&bytes),"expanded_objects":40000,"independent_positions":true}));
    std::fs::write(
        dir.join("native-roundtrip-service.json"),
        serde_json::to_vec_pretty(
            &json!({"schema_version":2,"stage":"S4-C5","status":"PASS","artifacts":results}),
        )
        .unwrap(),
    )
    .unwrap();
}
