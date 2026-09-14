//! S0-C scope inputs must not accidentally grant unimplemented semantics.
use editor_service::ApplicationService;
use serde_json::Value;
use std::path::Path;

#[test]
fn scope_fixtures_preserve_fail_closed_service_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../fixtures/synthetic/s0c/manifest.json"
    ))
    .unwrap();
    let mut service = ApplicationService::new();
    service
        .open_s0(
            "baseline",
            include_bytes!("../../../fixtures/synthetic/s0_polarity.gbr"),
        )
        .unwrap();
    let before = service.snapshot("baseline").unwrap();
    for item in manifest["fixtures"].as_array().unwrap() {
        let path = item["path"].as_str().unwrap();
        let source = std::fs::read(root.join(path)).unwrap();
        if item["current_product"] == "must_reject" {
            let error = service.open_s0(path, &source).unwrap_err();
            assert_eq!(error.code, "UNSUPPORTED_FEATURE", "{path}: {error:?}");
            assert!(service.snapshot(path).is_err(), "{path}");
            assert_eq!(service.snapshot("baseline").unwrap(), before);
        } else {
            // Existing absolute C-flash formats retain their previous support.
            service.open_s0(path, &source).unwrap();
        }
    }
}
