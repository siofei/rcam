use editor_service::{ApplicationService, FileAccessPolicy};
use std::path::Path;

const SAMPLE: &str = include_str!("../../../fixtures/synthetic/s0_polarity.gbr");

#[test]
fn s0_json_contract_uses_real_parser_and_read_only_snapshot() {
    let mut service = ApplicationService::new();
    let open = serde_json::json!({
        "api_version": 1,
        "request_id": "open-1",
        "op": "document.open_s0",
        "document_id": "fixture",
        "params": {"source": SAMPLE}
    });
    let response = service.execute_json(&open.to_string());
    assert_eq!(response["status"], "completed");
    assert_eq!(response["result"]["revision"], "0");
    assert_eq!(response["result"]["layer_ids"].as_array().unwrap().len(), 1);

    let snapshot = serde_json::json!({
        "api_version": 1,
        "request_id": "snapshot-1",
        "op": "document.snapshot",
        "document_id": "fixture",
        "params": {}
    });
    let response = service.execute_json(&snapshot.to_string());
    assert_eq!(
        response["result"]["layers"][0]["objects"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(response["result"]["revision"], "0");

    let analysis = serde_json::json!({
        "api_version": 1,
        "request_id": "analysis-1",
        "op": "document.analyze_s0",
        "document_id": "fixture",
        "params": {}
    });
    let response = service.execute_json(&analysis.to_string());
    let covered: Vec<_> = response["result"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["covered"].as_bool().unwrap())
        .collect();
    assert_eq!(covered, [true, false, true, false]);
}

#[test]
fn s0_json_contract_rejects_unknown_fields_and_unimplemented_edits() {
    let mut service = ApplicationService::new();
    let unknown_field = serde_json::json!({
        "api_version": 1,
        "request_id": "bad-1",
        "op": "system.capabilities",
        "params": {"future": true}
    });
    assert_eq!(
        service.execute_json(&unknown_field.to_string())["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let edit = serde_json::json!({
        "api_version": 1,
        "request_id": "bad-2",
        "op": "objects.move",
        "params": {}
    });
    assert_eq!(
        service.execute_json(&edit.to_string())["error"]["code"],
        "UNSUPPORTED_OPERATION"
    );
}

#[test]
fn s1_json_contract_publishes_only_authorized_real_operations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let service = ApplicationService::with_file_access(FileAccessPolicy::new(
        root.clone(),
        [root.join("fixtures/synthetic")],
        [std::env::temp_dir()],
    ));
    let capabilities = service.capabilities();
    for operation in [
        "system.capabilities",
        "document.open",
        "document.get",
        "document.close",
        "objects.move",
        "objects.rotate",
        "objects.mirror",
        "objects.duplicate",
        "objects.delete",
        "history.undo",
        "history.redo",
        "layers.list",
        "objects.query",
        "objects.get",
        "document.validate",
        "gerber.export_layer",
    ] {
        assert!(
            capabilities
                .supported_operations
                .iter()
                .any(|item| item == operation),
            "{operation}"
        );
    }
    {
        let operation = "text.preview";
        assert!(
            !capabilities
                .supported_operations
                .iter()
                .any(|item| item == operation),
            "{operation}"
        );
    }
}

#[test]
fn s1_json_contract_rejects_implicit_authority_and_nested_unknown_fields() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s1a/ordered_local_hole.gbr");
    let mut service = ApplicationService::new();
    let denied = serde_json::json!({
        "api_version": 1,
        "request_id": "denied",
        "op": "document.open",
        "params": {"path": source}
    });
    assert_eq!(
        service.execute_json(&denied.to_string())["error"]["code"],
        "PERMISSION_DENIED"
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut authorized = ApplicationService::with_file_access(FileAccessPolicy::new(
        root.clone(),
        [root.join("fixtures/synthetic/s1a")],
        [std::env::temp_dir()],
    ));
    let unknown = serde_json::json!({
        "api_version": 1,
        "request_id": "query",
        "op": "objects.query",
        "document_id": "missing",
        "params": {
            "layer_id": "layer-1",
            "relation": "intersects",
            "region_mm": {"min_x_mm": 0.0, "min_y_mm": 0.0, "max_x_mm": 1.0, "max_y_mm": 1.0, "unexpected": true}
        }
    });
    assert_eq!(
        authorized.execute_json(&unknown.to_string())["error"]["code"],
        "INVALID_ARGUMENT"
    );
}
