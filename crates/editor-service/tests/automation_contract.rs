use editor_service::ApplicationService;

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
    let response = service.execute_json(&open.to_string()).unwrap();
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
    let response = service.execute_json(&snapshot.to_string()).unwrap();
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
    let response = service.execute_json(&analysis.to_string()).unwrap();
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
        service
            .execute_json(&unknown_field.to_string())
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    let edit = serde_json::json!({
        "api_version": 1,
        "request_id": "bad-2",
        "op": "objects.move",
        "params": {}
    });
    assert_eq!(
        service.execute_json(&edit.to_string()).unwrap_err().code,
        "UNSUPPORTED_OPERATION"
    );
}
