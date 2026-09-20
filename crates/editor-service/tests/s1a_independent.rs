//! Independent public source truth, evaluated only through ApplicationService.
//! This is S1-A normalization, not the S1-B editing acceptance workflow.
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn call(service: &mut ApplicationService, op: &str, id: Option<&str>, params: Value) -> Value {
    let mut request =
        json!({"api_version":1,"request_id":"independent-review","op":op,"params":params});
    if let Some(id) = id {
        request["document_id"] = json!(id);
    }
    if op == "gerber.export_layer" {
        request["expected_revision"] = json!(service.document_get(id.unwrap()).unwrap().revision);
    }
    service.execute_json(&request.to_string())
}

fn successful(response: Value) -> Value {
    assert_eq!(response["status"], "completed", "{response}");
    response["result"].clone()
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn public_truth_opens_or_rejects_and_normalizes_without_source_writes() {
    let root = root();
    let out = std::env::var_os("RCAM_S1A_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("rcam-s1a-independent-{}", std::process::id()))
        });
    std::fs::create_dir_all(&out).unwrap();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        root.clone(),
        [root.join("fixtures/synthetic"), out.clone()],
        [out.clone()],
    ));
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../fixtures/synthetic/s1a/manifest.json"
    ))
    .unwrap();
    let arcs: Value = serde_json::from_str(include_str!(
        "../../../fixtures/synthetic/s1a1/manifest.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for case in manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(arcs["cases"].as_array().unwrap())
    {
        let name = case["name"].as_str().unwrap();
        let path = root.join(case["path"].as_str().unwrap());
        let before = std::fs::read(&path).unwrap();
        let response = call(&mut service, "document.open", None, json!({"path":path}));
        if !case["accept"].as_bool().unwrap() {
            if response["status"] == "completed" {
                failures.push(format!("{name}: invalid input accepted"));
            }
            assert_eq!(std::fs::read(&path).unwrap(), before);
            continue;
        }
        if response["status"] != "completed" {
            failures.push(format!("{name}: open failed {response}"));
            continue;
        }
        let id = response["result"]["document_id"].as_str().unwrap();
        let layers = successful(call(&mut service, "layers.list", Some(id), json!({})));
        let layer = layers.as_array().unwrap()[0]["layer_id"].as_str().unwrap();
        let query = successful(call(
            &mut service,
            "objects.query",
            Some(id),
            json!({"layer_id":layer,"limit":1000}),
        ));
        assert!(!query["objects"].as_array().unwrap().is_empty(), "{name}");
        successful(call(&mut service, "document.validate", Some(id), json!({})));
        // This historical source-fidelity suite uses the original FS quantum.
        // Coarser default topology rejection is covered by global_units_precision.
        service
            .set_manufacturing_precision(
                id,
                "0",
                editor_service::ManufacturingPrecision {
                    resolution_mm: 0.000001,
                },
            )
            .unwrap();
        let target = out.join(format!("{name}.gbr"));
        let exported = call(
            &mut service,
            "gerber.export_layer",
            Some(id),
            json!({
                "layer_id":layer,"path":target,"overwrite":{"mode":"deny"},
                "metadata_policy":{"mode":"require_confirmation"}
            }),
        );
        if exported["status"] != "completed" {
            failures.push(format!("{name}: export failed {exported}"));
            continue;
        }
        std::fs::write(
            out.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&exported).unwrap(),
        )
        .unwrap();
        let output = std::fs::read_to_string(&target).unwrap();
        assert_eq!(output.matches("M02*").count(), 1, "{name}");
        successful(call(
            &mut service,
            "document.open",
            None,
            json!({"path":target}),
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn independent_service_permissions_conflicts_and_cursor_isolation() {
    let base = std::env::temp_dir().join(format!("rcam-s1a-policy-{}", std::process::id()));
    let allowed = base.join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    let input = allowed.join("中文 # source.gbr");
    std::fs::write(
        &input,
        include_bytes!("../../../fixtures/synthetic/s0_polarity.gbr"),
    )
    .unwrap();
    let mut denied = ApplicationService::new();
    let response = call(&mut denied, "document.open", None, json!({"path":input}));
    assert_eq!(response["error"]["code"], "PERMISSION_DENIED", "{response}");
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        allowed.clone(),
        [allowed.clone()],
        [allowed.clone()],
    ));
    let first = successful(call(
        &mut service,
        "document.open",
        None,
        json!({"path":input}),
    ));
    let second = successful(call(
        &mut service,
        "document.open",
        None,
        json!({"path":input}),
    ));
    let id = first["document_id"].as_str().unwrap();
    let second_id = second["document_id"].as_str().unwrap();
    assert_ne!(id, second_id);
    let layers = successful(call(&mut service, "layers.list", Some(id), json!({})));
    let layer = layers[0]["layer_id"].as_str().unwrap();
    let next_layers = successful(call(
        &mut service,
        "layers.list",
        Some(second_id),
        json!({}),
    ));
    let second_layer = next_layers[0]["layer_id"].as_str().unwrap();
    let page = successful(call(
        &mut service,
        "objects.query",
        Some(id),
        json!({"layer_id":layer,"limit":1}),
    ));
    assert!(page["next_cursor"].is_string());
    let crossed = call(
        &mut service,
        "objects.query",
        Some(second_id),
        json!({"layer_id":second_layer,"limit":1,"cursor":page["next_cursor"]}),
    );
    assert_ne!(
        crossed["status"], "completed",
        "cross-document cursor accepted"
    );
    let bad = call(
        &mut service,
        "objects.query",
        Some(id),
        json!({"layer_id":layer,"relation":"contains","region_mm":{"min_x_mm":0,"min_y_mm":0,"max_x_mm":1,"max_y_mm":1,"typo":true}}),
    );
    assert_eq!(bad["error"]["code"], "INVALID_ARGUMENT", "{bad}");
    let target = allowed.join("existing.gbr");
    std::fs::write(&target, b"existing sentinel").unwrap();
    let params = json!({"layer_id":layer,"path":target,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}});
    let conflict = call(&mut service, "gerber.export_layer", Some(id), params);
    assert_ne!(conflict["status"], "completed");
    assert_eq!(std::fs::read(&target).unwrap(), b"existing sentinel");
    let fresh = allowed.join("stale-revision.gbr");
    let stale=service.execute_json(&json!({"api_version":1,"request_id":"stale","op":"gerber.export_layer","document_id":id,"expected_revision":"9007199254740993","params":{"layer_id":layer,"path":fresh,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}}}).to_string());
    assert_eq!(stale["error"]["code"], "REVISION_CONFLICT", "{stale}");
    assert!(!fresh.exists());
    #[cfg(target_os = "macos")]
    {
        let outside = base.join("outside.gbr");
        std::fs::write(&outside, b"outside sentinel").unwrap();
        let link = allowed.join("escape.gbr");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let escaped = call(&mut service, "document.open", None, json!({"path":link}));
        assert_eq!(escaped["error"]["code"], "PERMISSION_DENIED", "{escaped}");
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside sentinel");
    }
    assert!(!std::fs::read_dir(&allowed).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    std::fs::remove_dir_all(base).unwrap();
}
