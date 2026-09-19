//! Real JSON, file IO, manufacturing transactions, and revision-bound selection.
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::{Value, json};
#[test]
fn select_rect_contract_edit_history_export_reopen() {
    let base = std::env::var_os("RCAM_S2B2_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("rect-workflow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\nD10*\nX2000000Y3000000D03*\nX6000000Y3000000D03*\nM02*\n";
    std::fs::write(dir.join("source.gbr"), source).unwrap();
    let mut s = ApplicationService::with_file_access(FileAccessPolicy::new(
        dir.clone(),
        [dir.clone()],
        [dir.clone()],
    ));
    let mut records = Vec::new();
    let mut call = |op: &str, id: Option<&str>, rev: Option<&str>, params: Value| {
        let mut request = json!({"api_version":1,"request_id":format!("rect-{}",records.len()),"op":op,"params":params});
        if let Some(id) = id {
            request["document_id"] = json!(id);
        }
        if let Some(rev) = rev {
            request["expected_revision"] = json!(rev);
        }
        let response = s.execute_json(&request.to_string());
        records.push(json!({"request":request,"response":response}));
        response
    };
    let opened = call("document.open", None, None, json!({"path":"source.gbr"}));
    assert_eq!(opened["status"], "completed");
    let id = opened["result"]["document_id"].as_str().unwrap();
    let layer = opened["result"]["layer_ids"][0].as_str().unwrap();
    let params = json!({"layer_id":layer,"rect_mm":{"min_x_mm":1.,"min_y_mm":2.,"max_x_mm":7.,"max_y_mm":4.},"mode":"window"});
    let before = call("document.get", Some(id), None, json!({}));
    let selected = call("objects.select_rect", Some(id), None, params.clone());
    assert_eq!(selected["status"], "completed");
    assert_eq!(selected["result"]["revision"], "0");
    assert_eq!(selected["revision"], "0");
    let ids = selected["result"]["object_ids"].clone();
    assert_eq!(ids.as_array().unwrap().len(), 2);
    for malformed in [
        json!({"layer_id":layer,"mode":"contains","rect_mm":params["rect_mm"]}),
        json!({"layer_id":layer,"mode":"window","rect_mm":{"min_x_mm":1,"min_y_mm":2,"max_x_mm":7,"max_y_mm":4,"extra":0}}),
        json!({"layer_id":layer,"mode":"window","rect_mm":params["rect_mm"],"visible":true}),
    ] {
        assert_eq!(
            call("objects.select_rect", Some(id), None, malformed)["error"]["code"],
            "INVALID_ARGUMENT"
        );
    }
    assert_eq!(
        call("objects.select_rect", Some(id), Some("0"), params.clone())["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        call("objects.select_rect", Some("missing"), None, params.clone())["error"]["code"],
        "NOT_FOUND"
    );
    assert_eq!(
        call("document.get", Some(id), None, json!({}))["result"],
        before["result"]
    );
    assert_eq!(
        call(
            "layer.update",
            Some(id),
            Some("0"),
            json!({"layer_id":layer,"expected_workspace_revision":"0","visible":false,"locked":true})
        )["status"],
        "completed"
    );
    assert_eq!(
        call("objects.select_rect", Some(id), None, params.clone())["result"],
        selected["result"]
    );
    assert_eq!(
        call(
            "objects.move",
            Some(id),
            Some("0"),
            json!({"layer_id":layer,"object_ids":ids,"dx_mm":5.,"dy_mm":-1.})
        )["error"]["code"],
        "LAYER_LOCKED"
    );
    call(
        "layer.update",
        Some(id),
        Some("0"),
        json!({"layer_id":layer,"expected_workspace_revision":"1","visible":true,"locked":false}),
    );
    let moved = call(
        "objects.move",
        Some(id),
        Some("0"),
        json!({"layer_id":layer,"object_ids":ids,"dx_mm":5.,"dy_mm":-1.}),
    );
    assert_eq!(moved["result"]["undo_entries"], 1);
    let after_params = json!({"layer_id":layer,"rect_mm":{"min_x_mm":6.,"min_y_mm":1.,"max_x_mm":12.,"max_y_mm":3.},"mode":"window"});
    assert_eq!(
        call("objects.select_rect", Some(id), None, after_params.clone())["result"]["object_ids"],
        ids
    );
    assert_eq!(
        call("objects.select_rect", Some(id), None, params.clone())["result"]["object_ids"],
        json!([])
    );
    call("history.undo", Some(id), Some("1"), json!({}));
    assert_eq!(
        call("objects.select_rect", Some(id), None, params.clone())["result"]["object_ids"],
        ids
    );
    call("history.redo", Some(id), Some("2"), json!({}));
    let duplicate = call(
        "objects.duplicate",
        Some(id),
        Some("3"),
        json!({"layer_id":layer,"object_ids":ids,"dx_mm":0,"dy_mm":0}),
    );
    let copies = duplicate["result"]["changed_object_ids"].clone();
    assert_eq!(copies.as_array().unwrap().len(), 2);
    assert_eq!(duplicate["result"]["undo_entries"], 2);
    let order =
        call("objects.select_rect", Some(id), None, after_params.clone())["result"]["object_ids"]
            .clone();
    assert_eq!(order, json!([ids[0], copies[0], ids[1], copies[1]]));
    call(
        "objects.delete",
        Some(id),
        Some("4"),
        json!({"layer_id":layer,"object_ids":copies}),
    );
    call("history.undo", Some(id), Some("5"), json!({}));
    assert_eq!(
        call("objects.select_rect", Some(id), None, after_params)["result"]["object_ids"],
        order
    );
    let saved = call(
        "gerber.export_layer",
        Some(id),
        Some("6"),
        json!({"layer_id":layer,"path":"output.gbr","overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}}),
    );
    assert_eq!(saved["status"], "completed");
    let reopened = call("document.open", None, None, json!({"path":"output.gbr"}));
    let reopened_id = reopened["result"]["document_id"].as_str().unwrap();
    let reopened_layer = reopened["result"]["layer_ids"][0].as_str().unwrap();
    let snapshot = call("render.snapshot", Some(reopened_id), None, json!({}));
    let objects = snapshot["result"]["layers"][0]["objects"]
        .as_array()
        .unwrap();
    assert_eq!(objects.len(), 4);
    let select = call(
        "objects.select_rect",
        Some(reopened_id),
        None,
        json!({"layer_id":reopened_layer,"rect_mm":{"min_x_mm":6.,"min_y_mm":1.,"max_x_mm":12.,"max_y_mm":3.},"mode":"window"}),
    );
    assert_eq!(select["result"]["object_ids"].as_array().unwrap().len(), 4);
    assert_eq!(
        std::fs::read_to_string(dir.join("source.gbr")).unwrap(),
        source
    );
    let output = std::fs::read_to_string(dir.join("output.gbr")).unwrap();
    assert!(output.contains("X7000000Y2000000D03"));
    assert!(output.contains("X11000000Y2000000D03"));
    std::fs::write(
        dir.join("requests.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
}
