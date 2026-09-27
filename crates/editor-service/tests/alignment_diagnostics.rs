//! Arrangement diagnostics contain fixed modes and counts, never manufacturing payloads.
use editor_service::{
    AlignParams, AlignmentMode, ApplicationService, DistributeParams, DistributionAxis,
    FileAccessPolicy,
};
use std::path::PathBuf;

#[test]
fn alignment_and_distribution_diagnostics_record_numeric_metrics_and_redact_identity() {
    let dir = std::env::temp_dir().join(format!("rcam-s4c4-diagnostics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let private_name = "PRIVATE_CUSTOMER_BOARD.gbr";
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s4c4/arrangements.gbr");
    let source = std::fs::read(fixture).unwrap();
    std::fs::write(dir.join(private_name), &source).unwrap();
    let guard = rcam_diagnostics::Runtime::start(dir.clone(), "test", "test").unwrap();
    assert!(guard.install_sink());
    let runtime = guard.runtime();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        dir.clone(),
        [dir.clone()],
        [dir.clone()],
    ));
    let opened = service.open(private_name).unwrap();
    let document = opened.document_id;
    let layer = opened.layer_ids[0].clone();
    let ids: Vec<_> = service.render_snapshot(&document).unwrap().layers[0]
        .objects
        .iter()
        .map(|object| object.object_id.clone())
        .collect();
    assert_eq!(ids.len(), 3);

    let aligned = service
        .objects_align(
            &document,
            "0",
            AlignParams {
                layer_id: layer.clone(),
                object_ids: ids.clone(),
                anchor_object_id: ids[2].clone(),
                mode: AlignmentMode::Left,
            },
        )
        .unwrap();
    let no_op = service
        .objects_align(
            &document,
            &aligned.revision,
            AlignParams {
                layer_id: layer.clone(),
                object_ids: ids.clone(),
                anchor_object_id: ids[2].clone(),
                mode: AlignmentMode::Left,
            },
        )
        .unwrap();
    assert_eq!(no_op.revision, aligned.revision);
    assert!(no_op.changed_object_ids.is_empty());

    let distributed = service
        .objects_distribute(
            &document,
            &no_op.revision,
            DistributeParams {
                layer_id: layer.clone(),
                object_ids: ids.clone(),
                axis: DistributionAxis::Horizontal,
            },
        )
        .unwrap();
    let stale = service
        .objects_align(
            &document,
            "0",
            AlignParams {
                layer_id: layer,
                object_ids: ids.clone(),
                anchor_object_id: ids[2].clone(),
                mode: AlignmentMode::Left,
            },
        )
        .unwrap_err();
    assert_eq!(stale.code, "REVISION_CONFLICT");
    assert_eq!(
        service.document_get(&document).unwrap().revision,
        distributed.revision
    );

    let events = runtime.recent();
    let operations: Vec<_> = events
        .iter()
        .filter(|event| {
            matches!(
                event.command_id.as_str(),
                "objects.align" | "objects.distribute"
            )
        })
        .collect();
    let find = |command: &str, phase: &str| {
        operations
            .iter()
            .find(|event| event.command_id == command && event.phase == phase)
            .unwrap()
    };
    let align = find("objects.align", "ok");
    assert_eq!(
        (align.revision_before, align.revision_after),
        (Some(0), Some(1))
    );
    assert_eq!(align.metrics.get("selection_count"), Some(&3));
    assert_eq!(align.metrics.get("object_count"), Some(&3));
    assert_eq!(align.metrics.get("mode"), Some(&1));
    assert_eq!(
        align.metrics.get("moved_count"),
        Some(&(aligned.changed_object_ids.len() as u64))
    );
    assert_eq!(
        align.metrics.get("changed_objects"),
        Some(&(aligned.changed_object_ids.len() as u64))
    );
    let expected_hash = editor_core::hash::sha256_hex(ids[2].as_bytes());
    assert_eq!(
        align.metrics.get("anchor_hash"),
        Some(&u64::from_str_radix(&expected_hash[..16], 16).unwrap())
    );

    let no_op_event = operations
        .iter()
        .find(|event| {
            event.command_id == "objects.align"
                && event.phase == "ok"
                && event.revision_before == Some(1)
        })
        .unwrap();
    assert_eq!(no_op_event.revision_after, Some(1));
    assert_eq!(no_op_event.metrics.get("moved_count"), Some(&0));
    assert_eq!(no_op_event.metrics.get("changed_objects"), Some(&0));

    let distribute = find("objects.distribute", "ok");
    assert_eq!(distribute.metrics.get("selection_count"), Some(&3));
    assert_eq!(distribute.metrics.get("object_count"), Some(&3));
    assert_eq!(distribute.metrics.get("axis"), Some(&1));
    assert!(!distribute.metrics.contains_key("mode"));
    let failure = find("objects.align", "error");
    assert_eq!(
        (failure.revision_before, failure.revision_after),
        (Some(2), Some(2))
    );
    assert_eq!(failure.error_code.as_deref(), Some("REVISION_CONFLICT"));
    assert_eq!(failure.metrics.get("mode"), Some(&1));
    assert_eq!(
        failure.metrics.get("anchor_hash"),
        align.metrics.get("anchor_hash")
    );
    assert!(
        operations
            .iter()
            .filter(|event| matches!(event.phase.as_str(), "ok" | "error"))
            .all(|event| serde_json::to_value(event).unwrap()["duration_us"].is_u64())
    );

    let info = service.document_get(&document).unwrap();
    let layers = service.layers_list(&document).unwrap();
    let snapshot = service.render_snapshot(&document).unwrap();
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
    let operations_log = entries
        .iter()
        .find(|entry| entry.path == "operations.log")
        .unwrap();
    let log = String::from_utf8_lossy(&operations_log.data);
    assert!(log.contains("objects.align"));
    assert!(log.contains("objects.distribute"));
    for entry in &entries {
        let text = String::from_utf8_lossy(&entry.data);
        for forbidden in [
            private_name,
            dir.to_str().unwrap(),
            "object_ids",
            "\"geometry\":",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} leaked {forbidden}",
                entry.path
            );
        }
        for id in &ids {
            assert!(
                !text.contains(id),
                "{} leaked an object identity",
                entry.path
            );
        }
    }
    assert!(runtime.flush());
    drop(guard);
    std::fs::remove_dir_all(dir).unwrap();
}
