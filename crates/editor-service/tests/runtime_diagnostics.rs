//! Real operations, no test-only mutation path. One isolated integration process owns the sink.
use editor_service::{ApplicationService, FileAccessPolicy};
use serde_json::json;
#[test]
fn service_operations_record_real_revisions_and_failures_without_payloads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rcam-infra1-service-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let guard = rcam_diagnostics::Runtime::start(dir.clone(), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        &root,
        [root.join("fixtures/synthetic"), dir.clone()],
        [dir.clone()],
    ));
    let info = service
        .open(
            root.join("fixtures/synthetic/s1a/ordered_local_hole.gbr")
                .to_str()
                .unwrap(),
        )
        .unwrap();
    let doc = &info.document_id;
    let layer = &info.layer_ids[0];
    let call = |service: &mut ApplicationService, op: &str, revision: &str, params| {
        service.execute_json(&json!({"api_version":1,"request_id":"diagnostic-test","op":op,"document_id":doc,"expected_revision":revision,"params":params}).to_string())
    };
    let query = service.execute_json(&json!({"api_version":1,"request_id":"query","op":"objects.query","document_id":doc,"params":{"layer_id":layer,"limit":1}}).to_string());
    let object = &query["result"]["objects"][0]["object"]["object_id"];
    assert_eq!(
        call(
            &mut service,
            "objects.move",
            "0",
            json!({"layer_id":layer,"object_ids":[object],"dx_mm":1.0,"dy_mm":0.0})
        )["status"],
        "completed"
    );
    assert_ne!(
        call(
            &mut service,
            "objects.rotate",
            "0",
            json!({"layer_id":layer,"object_ids":[object],"angle_deg":20.0,"pivot_mm":{"x_mm":0.0,"y_mm":0.0}})
        )["status"],
        "completed"
    );
    service.history_undo(doc, "1").unwrap();
    service.history_redo(doc, "2").unwrap();
    let project = dir.join("private-project.rcam");
    service
        .project_save(doc, "3", Some(project.to_str().unwrap()), false)
        .unwrap();
    service.project_open(project.to_str().unwrap()).unwrap();
    let created = service
        .text_create(
            doc,
            "3",
            editor_service::TextParams {
                layer_id: layer.clone(),
                font: editor_service::builtin_stroke_font().identity,
                layout: editor_service::TextLayout {
                    text: "PRIVATE-TEXT".into(),
                    x_mm: 0.0,
                    y_mm: 0.0,
                    height_mm: 3.0,
                    tracking_mm: 0.0,
                    h_align: editor_service::HorizontalAlign::Left,
                    v_align: editor_service::VerticalAlign::Bottom,
                    rotation_deg: 0.0,
                    curve_tolerance_mm: editor_text::TOLERANCE_MM,
                    baseline_spacing_mm: 0.0,
                    stroke_width_mm: 0.15,
                    outline_offset_mm: 0.0,
                },
            },
        )
        .unwrap();
    assert_eq!(created.revision, "4");

    let events = runtime.recent();
    assert!(events.iter().any(|event| event.command_id == "text.create"
        && event.phase == "ok"
        && event.revision_after == Some(4)));
    let find = |command: &str, phase: &str| {
        events
            .iter()
            .find(|event| event.command_id == command && event.phase == phase)
            .unwrap()
    };
    assert_eq!(find("document.open", "ok").revision_after, Some(0));
    assert_eq!(find("project.open", "ok").revision_after, Some(0));
    assert_eq!(find("objects.move", "ok").revision_before, Some(0));
    assert_eq!(find("objects.move", "ok").revision_after, Some(1));
    assert_eq!(find("objects.rotate", "error").revision_after, Some(1));
    assert_eq!(find("history.undo", "ok").revision_after, Some(2));
    assert_eq!(find("history.redo", "ok").revision_after, Some(3));
    for command in [
        "objects.move",
        "objects.rotate",
        "history.undo",
        "history.redo",
    ] {
        let _ = find(command, "begin");
    }
    let export = call(
        &mut service,
        "gerber.export_layer",
        "4",
        json!({"layer_id":layer,"path":dir.join("export.gbr"),"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"drop_listed","categories":["layer_name"]}}),
    );
    assert_eq!(export["status"], "completed", "{export}");
    let current = service.document_get(doc).unwrap();
    let layers = service.layers_list(doc).unwrap();
    let snapshot = service.render_snapshot(doc).unwrap();
    let context = editor_service::diagnostic_context(
        &current,
        &layers,
        Some(&snapshot),
        editor_core::units::DisplayUnit::Millimeter,
    );
    assert_eq!(context.project.as_ref().unwrap().manufacturing_revision, 4);
    assert_eq!(
        context.project.as_ref().unwrap().object_count,
        layers.iter().map(|l| l.object_count).sum::<usize>()
    );
    assert_eq!(
        context.layers.layers[0]
            .source_content_hash_prefix
            .as_deref(),
        Some(&info.source_sha256[..16])
    );
    let many = vec![layers[0].clone(); 300];
    let bounded = editor_service::diagnostic_context(
        &current,
        &many,
        Some(&snapshot),
        editor_core::units::DisplayUnit::Inch,
    );
    assert_eq!(bounded.layers.layers.len(), 256);
    assert_eq!(bounded.layers.actual_count, 300);
    assert!(bounded.layers.truncated);
    runtime
        .export_with_context(&dir.join("diagnostics.zip"), context)
        .unwrap();
    let entries = rcam_project::zip_codec::read_zip(
        &std::fs::read(dir.join("diagnostics.zip")).unwrap(),
        &rcam_project::zip_codec::ReadPolicy {
            max_entries: 32,
            max_uncompressed_bytes: 100 * 1024 * 1024,
            max_entry_bytes: 30 * 1024 * 1024,
            max_path_len: 128,
        },
    )
    .unwrap();
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&entries.iter().find(|e| e.path == name).unwrap().data).unwrap()
    };
    assert_eq!(
        read("project_summary.json")["project"]["manufacturing_revision"],
        4
    );
    assert_eq!(read("layer_summary.json")["actual_count"], layers.len());
    let performance = read("performance_summary.json");
    let exported = performance["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["command_id"] == "gerber.export.summary")
        .unwrap();
    for key in [
        "flatten_us",
        "normalize_us",
        "writer_us",
        "reparse_us",
        "semantic_compare_us",
        "readback_us",
        "publish_us",
        "output_bytes",
        "success",
    ] {
        assert!(exported["metrics"][key].is_u64(), "{key}");
    }
    for entry in entries {
        let text = String::from_utf8_lossy(&entry.data);
        for forbidden in [
            "PRIVATE-TEXT",
            "private-project",
            root.to_str().unwrap(),
            "object_ids",
            "vertices",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} leaked {forbidden}",
                entry.path
            );
        }
    }

    assert!(runtime.flush());
    for name in ["rcam.log", "operations.log"] {
        let log = std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(!log.contains(root.to_str().unwrap()));
        assert!(!log.contains("private-project"));
        assert!(!log.contains("object_ids"));
        assert!(!log.contains("PRIVATE-TEXT"));
    }
    drop(guard);
    std::fs::remove_dir_all(dir).unwrap();
}
