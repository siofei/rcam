//! Real service-only S1-A workflow: open, query, validate, export, reopen.
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn request(
    service: &mut ApplicationService,
    op: &str,
    document_id: Option<&str>,
    expected_revision: Option<&str>,
    params: Value,
) -> Value {
    let mut value = json!({
        "api_version": 1,
        "request_id": format!("headless-{op}"),
        "op": op,
        "params": params,
    });
    if let Some(document_id) = document_id {
        value["document_id"] = json!(document_id);
    }
    if let Some(expected_revision) = expected_revision {
        value["expected_revision"] = json!(expected_revision);
    }
    service.execute_json(&value.to_string())
}

fn result(response: Value) -> Value {
    assert_eq!(response["status"], "completed", "{response}");
    response["result"].clone()
}

#[test]
fn real_headless_open_query_export_reopen_preserves_source() {
    let root = root();
    let source = root.join("fixtures/synthetic/s1a/ordered_local_hole.gbr");
    let output_dir = std::env::temp_dir().join(format!("rcam-s1a-headless-{}", std::process::id()));
    std::fs::create_dir_all(&output_dir).unwrap();
    let output = output_dir.join("ordered-local-hole.gbr");
    let source_before = std::fs::read(&source).unwrap();

    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        root.clone(),
        [root.join("fixtures/synthetic"), output_dir.clone()],
        [output_dir.clone()],
    ));
    let capabilities = result(request(
        &mut service,
        "system.capabilities",
        None,
        None,
        json!({}),
    ));
    for operation in [
        "document.open",
        "document.get",
        "layers.list",
        "objects.query",
        "objects.get",
        "document.validate",
        "gerber.export_layer",
    ] {
        assert!(
            capabilities["supported_operations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == operation)
        );
    }

    let opened = result(request(
        &mut service,
        "document.open",
        None,
        None,
        json!({"path": source}),
    ));
    let document_id = opened["document_id"].as_str().unwrap().to_owned();
    assert_eq!(opened["revision"], "0");
    let info = result(request(
        &mut service,
        "document.get",
        Some(&document_id),
        None,
        json!({}),
    ));
    let layer_id = info["layer_ids"][0].as_str().unwrap().to_owned();
    let layers = result(request(
        &mut service,
        "layers.list",
        Some(&document_id),
        None,
        json!({}),
    ));
    assert_eq!(layers.as_array().unwrap().len(), 1);

    let first_page = result(request(
        &mut service,
        "objects.query",
        Some(&document_id),
        None,
        json!({"layer_id": layer_id, "limit": 1}),
    ));
    assert_eq!(first_page["total"].as_u64().unwrap(), 3);
    let object_id = first_page["objects"][0]["object"]["object_id"]
        .as_str()
        .unwrap();
    let fetched = result(request(
        &mut service,
        "objects.get",
        Some(&document_id),
        None,
        json!({"layer_id": layer_id, "object_id": object_id}),
    ));
    assert_eq!(fetched["object"]["object_id"], object_id);
    let next_cursor = first_page["next_cursor"].as_str().unwrap();
    let second_page = result(request(
        &mut service,
        "objects.query",
        Some(&document_id),
        None,
        json!({"layer_id": layer_id, "limit": 1, "cursor": next_cursor}),
    ));
    assert_eq!(second_page["objects"].as_array().unwrap().len(), 1);

    let validation = result(request(
        &mut service,
        "document.validate",
        Some(&document_id),
        None,
        json!({}),
    ));
    assert_eq!(validation["valid"], true);

    let exported = result(request(
        &mut service,
        "gerber.export_layer",
        Some(&document_id),
        Some("0"),
        json!({
            "layer_id": layer_id,
            "path": output,
            "overwrite": {"mode": "deny"},
            "metadata_policy": {"mode": "require_confirmation"}
        }),
    ));
    assert_eq!(exported["exported_revision"], "0");
    assert_eq!(exported["current_revision"], "0");
    assert!(output.is_file());
    assert_eq!(std::fs::read(&source).unwrap(), source_before);
    assert_eq!(
        std::fs::read_to_string(&output)
            .unwrap()
            .matches("M02*")
            .count(),
        1
    );

    let reopened = result(request(
        &mut service,
        "document.open",
        None,
        None,
        json!({"path": output}),
    ));
    let reopened_id = reopened["document_id"].as_str().unwrap();
    let reopened_validation = result(request(
        &mut service,
        "document.validate",
        Some(reopened_id),
        None,
        json!({}),
    ));
    assert_eq!(reopened_validation["valid"], true);

    std::fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn metadata_policy_requires_confirmation_then_accepts_explicit_drop_list() {
    let root = root();
    let dir = std::env::temp_dir().join(format!("rcam-s1a-metadata-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("metadata.gbr");
    let target = dir.join("metadata-output.gbr");
    let mut bytes = b"%LNmetadata-layer*%\n".to_vec();
    bytes.extend_from_slice(include_bytes!(
        "../../../fixtures/synthetic/s1a/ordered_local_hole.gbr"
    ));
    std::fs::write(&source, &bytes).unwrap();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        root,
        [dir.clone()],
        [dir.clone()],
    ));
    let opened = result(request(
        &mut service,
        "document.open",
        None,
        None,
        json!({"path": source}),
    ));
    let document_id = opened["document_id"].as_str().unwrap().to_owned();
    let layer_id = opened["layer_ids"][0].as_str().unwrap().to_owned();
    let require_confirmation = request(
        &mut service,
        "gerber.export_layer",
        Some(&document_id),
        Some("0"),
        json!({
            "layer_id": layer_id,
            "path": target,
            "overwrite": {"mode": "deny"},
            "metadata_policy": {"mode": "require_confirmation"}
        }),
    );
    assert_eq!(require_confirmation["status"], "confirmation_required");
    assert_eq!(
        require_confirmation["error"]["code"],
        "CONFIRMATION_REQUIRED"
    );
    assert!(!target.exists());

    let exported = result(request(
        &mut service,
        "gerber.export_layer",
        Some(&document_id),
        Some("0"),
        json!({
            "layer_id": layer_id,
            "path": target,
            "overwrite": {"mode": "deny"},
            "metadata_policy": {"mode": "drop_listed", "categories": ["layer_name"]}
        }),
    ));
    assert_eq!(exported["exported_revision"], "0");
    assert!(target.is_file());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}
