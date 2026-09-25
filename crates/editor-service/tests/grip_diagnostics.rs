//! Isolated integration process: real Grip transactions own the diagnostics sink.
use editor_core::{MmPoint, grip::GripFeatureId};
use editor_service::{ApplicationService, FileAccessPolicy, GripEditParams, QueryParams};

const SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\nD10*\nX10000000Y20000000D03*\nM02*\n";

#[test]
fn grip_operations_record_revision_metrics_and_error_without_manufacturing_payload() {
    let dir = std::env::temp_dir().join(format!("rcam-grip-diagnostics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let private_name = "PRIVATE_CUSTOMER_BOARD.gbr";
    std::fs::write(dir.join(private_name), SOURCE).unwrap();
    let guard = rcam_diagnostics::Runtime::start(dir.clone(), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        dir.clone(),
        [dir.clone()],
        [dir.clone()],
    ));
    let opened = service.open(private_name).unwrap();
    let document = &opened.document_id;
    let layer = &opened.layer_ids[0];
    let object = service
        .objects_query(
            document,
            QueryParams {
                layer_id: layer.clone(),
                geometry_type: None,
                region_mm: None,
                relation: None,
                limit: None,
                cursor: None,
            },
        )
        .unwrap()
        .objects[0]
        .object
        .object_id
        .clone();
    let params = GripEditParams {
        layer_id: layer.clone(),
        object_id: object,
        grip_id: GripFeatureId::Radius,
        target_mm: MmPoint::new(12., 20.),
    };
    let edited = service
        .objects_grip_edit(document, "0", params.clone())
        .unwrap();
    assert_eq!(edited.revision, "1");
    let failed = service
        .objects_grip_edit(document, "0", params)
        .unwrap_err();
    assert_eq!(failed.code, "REVISION_CONFLICT");
    assert_eq!(service.document_get(document).unwrap().revision, "1");

    let events = runtime.recent();
    let grips: Vec<_> = events
        .iter()
        .filter(|event| event.command_id == "objects.grip_edit")
        .collect();
    assert_eq!(grips.len(), 4);
    assert_eq!(
        grips.iter().filter(|event| event.phase == "begin").count(),
        2
    );
    let success = grips.iter().find(|event| event.phase == "ok").unwrap();
    assert_eq!(
        (success.revision_before, success.revision_after),
        (Some(0), Some(1))
    );
    assert_eq!(success.metrics.get("geometry_kind"), Some(&1));
    assert_eq!(success.metrics.get("grip_kind"), Some(&3));
    assert_eq!(success.metrics.get("selection_count"), Some(&1));
    assert_eq!(success.error_code, None);
    let error = grips.iter().find(|event| event.phase == "error").unwrap();
    assert_eq!(
        (error.revision_before, error.revision_after),
        (Some(1), Some(1))
    );
    assert_eq!(error.error_code.as_deref(), Some("REVISION_CONFLICT"));
    assert_eq!(error.metrics.get("geometry_kind"), Some(&1));
    assert_eq!(error.metrics.get("grip_kind"), Some(&3));
    for event in [&**success, &**error] {
        assert!(event.document_id_hash.is_some());
    }

    let info = service.document_get(document).unwrap();
    let layers = service.layers_list(document).unwrap();
    let snapshot = service.render_snapshot(document).unwrap();
    let context = editor_service::diagnostic_context(
        &info,
        &layers,
        Some(&snapshot),
        editor_core::units::DisplayUnit::Millimeter,
    );
    let zip = dir.join("diagnostics.zip");
    runtime.export_with_context(&zip, context).unwrap();
    let entries = rcam_project::zip_codec::read_zip(
        &std::fs::read(&zip).unwrap(),
        &rcam_project::zip_codec::ReadPolicy {
            max_entries: 32,
            max_uncompressed_bytes: 100 * 1024 * 1024,
            max_entry_bytes: 30 * 1024 * 1024,
            max_path_len: 128,
        },
    )
    .unwrap();
    let operations = entries
        .iter()
        .find(|entry| entry.path == "operations.log")
        .unwrap();
    assert!(String::from_utf8_lossy(&operations.data).contains("objects.grip_edit"));
    let diagnostic = entries
        .iter()
        .find(|entry| entry.path == "diagnostic.json")
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&diagnostic.data).unwrap();
    assert!(
        value["recent_operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["command_id"] == "objects.grip_edit" && event["phase"] == "ok")
    );
    for event in value["recent_operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            event["command_id"] == "objects.grip_edit"
                && matches!(event["phase"].as_str(), Some("ok" | "error"))
        })
    {
        assert!(event["duration_us"].is_u64());
    }
    for entry in &entries {
        let text = String::from_utf8_lossy(&entry.data);
        for forbidden in [
            private_name,
            dir.to_str().unwrap(),
            "\"geometry\":",
            "\"target_mm\":",
            "\"payload\":",
            "\"object_id\":",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} leaked {forbidden}",
                entry.path
            );
        }
    }
    assert!(runtime.flush());
    drop(guard);
    std::fs::remove_dir_all(dir).unwrap();
}
