//! Explicit local large-file workflow; never writes to the source directory.
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::{Value, json};
use std::path::PathBuf;

fn call(
    service: &mut ApplicationService,
    op: &str,
    id: Option<&str>,
    rev: Option<&str>,
    params: Value,
) -> Value {
    let mut request = json!({"api_version":1,"request_id":op,"op":op,"params":params});
    if let Some(id) = id {
        request["document_id"] = json!(id);
    }
    if let Some(rev) = rev {
        request["expected_revision"] = json!(rev);
    }
    let response = service.execute_json(&request.to_string());
    println!(
        "{op}: status={} error={}",
        response["status"], response["error"]
    );
    response
}

#[test]
#[ignore = "requires RCAM_LARGE_GERBER and RCAM_LARGE_OUTPUT"]
fn large_real_file_opens_edits_exports_and_reopens() {
    let source = PathBuf::from(std::env::var("RCAM_LARGE_GERBER").unwrap());
    let output = PathBuf::from(std::env::var("RCAM_LARGE_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let root = source.parent().unwrap().to_path_buf();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        root.clone(),
        [root, output.clone()],
        [output.clone()],
    ));
    let opened = call(
        &mut service,
        "document.open",
        None,
        None,
        json!({"path":source}),
    );
    assert_eq!(opened["status"], "completed", "{opened}");
    let id = opened["result"]["document_id"].as_str().unwrap().to_owned();
    let layer = opened["result"]["layer_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let queried = call(
        &mut service,
        "objects.query",
        Some(&id),
        None,
        json!({"layer_id":layer,"limit":1}),
    );
    assert_eq!(queried["status"], "completed", "{queried}");
    let object = queried["result"]["objects"][0]["object"]["object_id"]
        .as_str()
        .unwrap();
    let moved = call(
        &mut service,
        "objects.move",
        Some(&id),
        Some("0"),
        json!({"layer_id":layer,"object_ids":[object],"dx_mm":0.001,"dy_mm":0.0}),
    );
    assert_eq!(moved["status"], "completed", "{moved}");
    let path = output.join("drill-large-edited.gbr");
    let exported = call(
        &mut service,
        "gerber.export_layer",
        Some(&id),
        Some("1"),
        json!({"layer_id":layer,"path":path,"overwrite":{"mode":"deny"},
            "metadata_policy":{"mode":"require_confirmation"}}),
    );
    assert_eq!(exported["status"], "completed", "{exported}");
    let reopened = call(
        &mut service,
        "document.open",
        None,
        None,
        json!({"path":path}),
    );
    assert_eq!(reopened["status"], "completed", "{reopened}");
    println!(
        "source_objects={} export_bytes={}",
        queried["result"]["total"],
        std::fs::metadata(&path).unwrap().len()
    );
}
