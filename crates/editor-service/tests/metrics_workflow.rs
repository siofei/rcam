use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::{Value, json};
use std::f64::consts::PI;
fn call(
    s: &mut ApplicationService,
    op: &str,
    id: Option<&str>,
    rev: Option<&str>,
    params: Value,
) -> Value {
    let request = json!({"api_version":1,"request_id":"metrics-test","op":op,"document_id":id,"expected_revision":rev,"params":params});
    s.execute_json(&request.to_string())
}
#[test]
fn metrics_readonly_batch_edits_export_reopen_contract() {
    let dir = std::env::temp_dir().join(format!("rcam-metrics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\n%AMOVER*1,1,2,0,0,0*1,0,1,0,0,0*%\n%ADD11OVER*%\nD10*\nX2000000Y3000000D03*\nX6000000Y3000000D03*\nD11*\nX10000000Y3000000D03*\nM02*\n";
    std::fs::write(dir.join("source.gbr"), source).unwrap();
    let mut s = ApplicationService::with_file_access(FileAccessPolicy::new(
        &dir,
        [dir.clone()],
        [dir.clone()],
    ));
    let opened = call(
        &mut s,
        "document.open",
        None,
        None,
        json!({"path":"source.gbr"}),
    );
    assert_eq!(opened["status"], "completed", "{opened}");
    let id = opened["result"]["document_id"].as_str().unwrap();
    let layer = opened["result"]["layer_ids"][0].as_str().unwrap();
    let query = call(
        &mut s,
        "objects.query",
        Some(id),
        None,
        json!({"layer_id":layer}),
    );
    let ids: Vec<_> = query["result"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["object_id"].as_str().unwrap().to_string())
        .collect();
    let params = json!({"layer_id":layer,"object_ids":[ids[1],ids[0],ids[2]]});
    let before = call(&mut s, "document.get", Some(id), None, json!({}));
    let m = call(&mut s, "objects.metrics", Some(id), None, params.clone());
    assert_eq!(m["status"], "completed", "{m}");
    assert_eq!(m["result"]["items"][0]["object_id"], ids[1]);
    assert_eq!(m["result"]["items"][1]["object_id"], ids[0]);
    assert_eq!(m["result"]["items"][2]["status"], "unsupported");
    assert!(m["result"]["items"][2].get("area_mm2").is_none());
    let summary = &m["result"]["summary"];
    assert_eq!(summary["exact_count"], 2);
    assert_eq!(summary["unsupported_count"], 1);
    assert!((summary["object_area_sum_mm2"].as_f64().unwrap() - 2. * PI).abs() < 1e-12);
    assert_eq!(
        call(&mut s, "document.get", Some(id), None, json!({})),
        before
    );
    for (params, code) in [
        (
            json!({"layer_id":layer,"object_ids":[ids[0],"missing"]}),
            "NOT_FOUND",
        ),
        (json!({"layer_id":"missing","object_ids":[]}), "NOT_FOUND"),
        (
            json!({"layer_id":layer,"object_ids":vec!["missing";10001]}),
            "RESOURCE_LIMIT",
        ),
        (
            json!({"layer_id":layer,"object_ids":[ids[0],ids[0]]}),
            "INVALID_ARGUMENT",
        ),
        (
            json!({"layer_id":layer,"object_ids":[],"extra":true}),
            "INVALID_ARGUMENT",
        ),
    ] {
        let failure = call(&mut s, "objects.metrics", Some(id), None, params);
        assert_eq!(failure["error"]["code"], code, "{failure}");
        assert!(failure.get("result").is_none() || failure["result"].is_null());
    }
    assert_eq!(
        call(
            &mut s,
            "objects.metrics",
            Some(id),
            Some("0"),
            params.clone()
        )["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        call(&mut s, "document.get", Some(id), None, json!({})),
        before
    );
    let exports = |path: &str| json!({"layer_id":layer,"path":path,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}});
    assert_eq!(
        call(
            &mut s,
            "gerber.export_layer",
            Some(id),
            Some("0"),
            exports("before.gbr")
        )["status"],
        "completed"
    );
    call(&mut s, "objects.metrics", Some(id), None, params.clone());
    assert_eq!(
        call(
            &mut s,
            "gerber.export_layer",
            Some(id),
            Some("0"),
            exports("after.gbr")
        )["status"],
        "completed"
    );
    assert_eq!(
        std::fs::read(dir.join("before.gbr")).unwrap(),
        std::fs::read(dir.join("after.gbr")).unwrap()
    );
    let mut rev = 0;
    for (op, extra) in [
        ("objects.move", json!({"dx_mm":5.,"dy_mm":-3.})),
        (
            "objects.rotate",
            json!({"angle_deg":37.,"pivot_mm":{"x_mm":0.,"y_mm":0.}}),
        ),
        (
            "objects.mirror",
            json!({"axis":{"kind":"horizontal","coordinate_mm":0.}}),
        ),
    ] {
        let mut args = json!({"layer_id":layer,"object_ids":[ids[0],ids[1]]});
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        assert_eq!(
            call(&mut s, op, Some(id), Some(&rev.to_string()), args)["status"],
            "completed"
        );
        rev += 1;
        assert_eq!(
            call(&mut s, "objects.metrics", Some(id), None, params.clone())["result"]["items"],
            m["result"]["items"]
        );
    }
    let duplicate = call(
        &mut s,
        "objects.duplicate",
        Some(id),
        Some(&rev.to_string()),
        json!({"layer_id":layer,"object_ids":[ids[0]],"dx_mm":0.,"dy_mm":0.}),
    );
    rev += 1;
    let copy = duplicate["result"]["changed_object_ids"][0]
        .as_str()
        .unwrap();
    let copy_params = json!({"layer_id":layer,"object_ids":[copy]});
    assert_eq!(
        call(
            &mut s,
            "objects.metrics",
            Some(id),
            None,
            copy_params.clone()
        )["result"]["items"][0]["area_mm2"],
        PI
    );
    assert_eq!(
        call(
            &mut s,
            "objects.delete",
            Some(id),
            Some(&rev.to_string()),
            json!({"layer_id":layer,"object_ids":[copy]})
        )["status"],
        "completed"
    );
    rev += 1;
    assert_eq!(
        call(
            &mut s,
            "objects.metrics",
            Some(id),
            None,
            copy_params.clone()
        )["error"]["code"],
        "NOT_FOUND"
    );
    assert_eq!(
        call(
            &mut s,
            "history.undo",
            Some(id),
            Some(&rev.to_string()),
            json!({})
        )["status"],
        "completed"
    );
    rev += 1;
    assert_eq!(
        call(&mut s, "objects.metrics", Some(id), None, copy_params)["result"]["items"][0]["area_mm2"],
        PI
    );
    let reopened = call(
        &mut s,
        "document.open",
        None,
        None,
        json!({"path":"after.gbr"}),
    );
    let new_id = reopened["result"]["document_id"].as_str().unwrap();
    let new_layer = reopened["result"]["layer_ids"][0].as_str().unwrap();
    let q = call(
        &mut s,
        "objects.query",
        Some(new_id),
        None,
        json!({"layer_id":new_layer}),
    );
    let new_ids: Vec<_> = q["result"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["object_id"].clone())
        .collect();
    let re = call(
        &mut s,
        "objects.metrics",
        Some(new_id),
        None,
        json!({"layer_id":new_layer,"object_ids":new_ids}),
    );
    assert_eq!(re["result"]["summary"], m["result"]["summary"]);
    assert_eq!(
        std::fs::read_to_string(dir.join("source.gbr")).unwrap(),
        source
    );
    assert_eq!(
        call(
            &mut s,
            "document.close",
            Some(id),
            Some(&rev.to_string()),
            json!({"discard_changes":true})
        )["status"],
        "completed"
    );
    assert_eq!(
        call(&mut s, "objects.metrics", Some(id), None, params)["error"]["code"],
        "NOT_FOUND"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
