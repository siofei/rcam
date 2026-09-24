use editor_service::{
    ApplicationService, CreateEmptyLayerParams, ExportParams, FileAccessPolicy,
    ImportGerberLayersParams, LayerUpdateParams, MetadataPolicy, OverwritePolicy,
    SetSoloLayerParams,
};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[path = "../../rcam-project/tests/common/mod.rs"]
mod perf_fixture;

fn setup() -> (ApplicationService, std::path::PathBuf) {
    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rcam-s4b3-test-{}-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let root = dir.canonicalize().unwrap();
    let service = ApplicationService::with_file_access(FileAccessPolicy::new(
        &root,
        [root.clone()],
        [root.clone()],
    ));
    (service, root)
}

#[test]
fn new_projects_have_distinct_stable_ids_across_service_sessions() {
    let (mut first_service, first_root) = setup();
    let (mut second_service, second_root) = setup();
    let first = first_service.document_new().unwrap();
    let second = second_service.document_new().unwrap();
    assert_eq!(first.document_id, second.document_id);
    assert_ne!(first.project_id, second.project_id);
    let path = first_root.join("identity.rcam");
    first_service
        .project_save(
            &first.document_id,
            &first.revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    let reopened = first_service.project_open(path.to_str().unwrap()).unwrap();
    assert_eq!(first.project_id, reopened.project_id);
    fs::remove_dir_all(first_root).unwrap();
    fs::remove_dir_all(second_root).unwrap();
}

#[test]
fn new_save_open_and_failed_open_preserves_existing_session() {
    let (mut service, root) = setup();
    let first = service.document_new().unwrap();
    assert!(!first.project_dirty);
    let layer = service
        .create_empty_layer(
            &first.document_id,
            &first.revision,
            CreateEmptyLayerParams::default(),
        )
        .unwrap();
    let changed = service.document_get(&first.document_id).unwrap();
    assert!(changed.project_dirty);
    let path = root.join("project.rcam");
    let saved = service
        .project_save(
            &first.document_id,
            &changed.revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    assert!(!saved.project_dirty);
    assert_eq!(saved.project_path.as_deref(), path.to_str());
    let bytes = fs::read(&path).unwrap();
    let repeated = service
        .project_save(&first.document_id, &saved.revision, None, false)
        .unwrap();
    assert_eq!(bytes, fs::read(&path).unwrap());
    assert_eq!(
        repeated.last_saved_project_hash,
        saved.last_saved_project_hash
    );
    let opened = service.project_open(path.to_str().unwrap()).unwrap();
    assert_eq!(opened.layer_ids, vec![layer.layer_id]);
    assert!(!opened.project_dirty);
    assert_eq!(
        service
            .project_snapshot(&opened.document_id)
            .unwrap()
            .project_id,
        service
            .project_snapshot(&first.document_id)
            .unwrap()
            .project_id
    );
    let broken = root.join("broken.rcam");
    fs::write(&broken, b"not a project").unwrap();
    assert!(service.project_open(broken.to_str().unwrap()).is_err());
    assert_eq!(service.document_get(&opened.document_id).unwrap(), opened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_as_failure_never_changes_original_or_session() {
    let (mut service, root) = setup();
    let doc = service.document_new().unwrap();
    let a = root.join("a.rcam");
    service
        .project_save(
            &doc.document_id,
            &doc.revision,
            Some(a.to_str().unwrap()),
            false,
        )
        .unwrap();
    let original = fs::read(&a).unwrap();
    let before = service.document_get(&doc.document_id).unwrap();
    let b = root.join("b.rcam");
    fs::write(&b, b"existing").unwrap();
    assert_eq!(
        service
            .project_save(
                &doc.document_id,
                &before.revision,
                Some(b.to_str().unwrap()),
                false
            )
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    assert_eq!(fs::read(&a).unwrap(), original);
    assert_eq!(fs::read(&b).unwrap(), b"existing");
    assert_eq!(service.document_get(&doc.document_id).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_change_refuses_save_and_keeps_dirty_state() {
    let (mut service, root) = setup();
    let doc = service.document_new().unwrap();
    let path = root.join("external.rcam");
    let saved = service
        .project_save(
            &doc.document_id,
            &doc.revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    let changed = service
        .create_empty_layer(
            &doc.document_id,
            &saved.revision,
            CreateEmptyLayerParams::default(),
        )
        .unwrap();
    let external = b"external change";
    fs::write(&path, external).unwrap();
    assert_eq!(
        service
            .project_save(&doc.document_id, &changed.revision, None, false)
            .unwrap_err()
            .code,
        "EXTERNAL_MODIFICATION"
    );
    assert_eq!(fs::read(&path).unwrap(), external);
    let after = service.document_get(&doc.document_id).unwrap();
    assert_eq!(after.project_path, saved.project_path);
    assert!(after.project_dirty);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn block_project_roundtrip_and_gerber_export_do_not_change_project_path() {
    let (mut service, root) = setup();
    let input = root.join("blocks.rcam");
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/s4b2/sample.rcam"
        ),
        &input,
    )
    .unwrap();
    let opened = service.project_open(input.to_str().unwrap()).unwrap();
    let project = service.project_snapshot(&opened.document_id).unwrap();
    assert!(!project.block_definitions.is_empty());
    assert!(
        project
            .layers
            .iter()
            .flat_map(|l| &l.layer.objects)
            .any(|o| matches!(
                o.geometry,
                editor_core::SemanticGeometry::BlockInstance { .. }
            ))
    );
    let output = root.join("copy.rcam");
    let saved = service
        .project_save(
            &opened.document_id,
            &opened.revision,
            Some(output.to_str().unwrap()),
            false,
        )
        .unwrap();
    let reopened = service.project_open(output.to_str().unwrap()).unwrap();
    let restored = service.project_snapshot(&reopened.document_id).unwrap();
    assert_eq!(project, restored);
    assert_eq!(opened.layer_ids, reopened.layer_ids);
    let gerber = root.join("export.gbr");
    let exported = service
        .export_layer(
            &reopened.document_id,
            &reopened.revision,
            ExportParams {
                layer_id: reopened.layer_ids[0].clone(),
                path: gerber.to_string_lossy().into_owned(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
                compatibility_precision_override_mm: None,
            },
        )
        .unwrap();
    assert!(exported.bytes > 0);
    assert_eq!(
        service
            .document_get(&reopened.document_id)
            .unwrap()
            .project_path,
        reopened.project_path
    );
    assert_eq!(saved.project_path.as_deref(), output.to_str());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn four_display_units_survive_save_open_without_changing_manufacturing_or_gerber() {
    let (mut service, root) = setup();
    let input = root.join("legacy-millimeters.rcam");
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/s4b2/sample.rcam"
        ),
        &input,
    )
    .unwrap();
    let opened = service.project_open(input.to_str().unwrap()).unwrap();
    let original = service.project_snapshot(&opened.document_id).unwrap();
    assert_eq!(
        original.workspace.display_unit,
        rcam_project::DisplayUnit::Millimeters
    );
    let manufacturing = original.to_semantic_document();
    let precision = original.manufacturing;
    let mut gerber_bytes = None;

    for (index, unit) in [
        rcam_project::DisplayUnit::Millimeters,
        rcam_project::DisplayUnit::Inches,
        rcam_project::DisplayUnit::Mils,
        rcam_project::DisplayUnit::Micrometers,
    ]
    .into_iter()
    .enumerate()
    {
        let mut workspace = service.project_workspace(&opened.document_id).unwrap();
        workspace.display_unit = unit;
        let changed = service
            .project_set_workspace(&opened.document_id, workspace)
            .unwrap();
        assert_eq!(changed.revision, opened.revision);
        let path = root.join(format!("unit-{index}.rcam"));
        service
            .project_save(
                &opened.document_id,
                &changed.revision,
                Some(path.to_str().unwrap()),
                false,
            )
            .unwrap();
        let reopened = service.project_open(path.to_str().unwrap()).unwrap();
        let restored = service.project_snapshot(&reopened.document_id).unwrap();
        assert_eq!(restored.workspace.display_unit, unit);
        assert_eq!(restored.to_semantic_document(), manufacturing);
        assert_eq!(restored.manufacturing, precision);
        let export_path = root.join(format!("unit-{index}.gbr"));
        service
            .export_layer(
                &reopened.document_id,
                &reopened.revision,
                ExportParams {
                    layer_id: reopened.layer_ids[0].clone(),
                    path: export_path.to_string_lossy().into_owned(),
                    overwrite: OverwritePolicy {
                        mode: "deny".into(),
                        expected_sha256: None,
                    },
                    metadata_policy: MetadataPolicy {
                        mode: "require_confirmation".into(),
                        categories: None,
                    },
                    compatibility_precision_override_mm: None,
                },
            )
            .unwrap();
        let exported = fs::read(export_path).unwrap();
        if let Some(first) = &gerber_bytes {
            assert_eq!(&exported, first);
        } else {
            gerber_bytes = Some(exported);
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_copy_is_unsaved_and_cannot_overwrite_original_implicitly() {
    let (mut service, root) = setup();
    let first = service.document_new().unwrap();
    let path = root.join("saved.rcam");
    service
        .project_save(
            &first.document_id,
            &first.revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    let original = fs::read(&path).unwrap();
    let changed = service
        .create_empty_layer(
            &first.document_id,
            &first.revision,
            CreateEmptyLayerParams::default(),
        )
        .unwrap();
    let bytes = service.project_recovery_bytes(&first.document_id).unwrap();
    let recovered = service.project_restore(&bytes).unwrap();
    assert!(recovered.project_dirty);
    assert!(recovered.project_path.is_none());
    assert_eq!(recovered.layer_ids, vec![changed.layer_id]);
    assert_eq!(
        service
            .project_save(&recovered.document_id, &recovered.revision, None, false)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(fs::read(path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persisted_style_grid_and_snap_are_dirty_but_solo_is_not() {
    let (mut service, root) = setup();
    let doc = service.document_new().unwrap();
    let layer = service
        .create_empty_layer(
            &doc.document_id,
            &doc.revision,
            CreateEmptyLayerParams::default(),
        )
        .unwrap();
    let path = root.join("dirty.rcam");
    let saved = service
        .project_save(
            &doc.document_id,
            &layer.revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    assert!(!saved.project_dirty);
    let solo = service
        .layers_set_solo(
            &doc.document_id,
            &saved.revision,
            SetSoloLayerParams {
                expected_workspace_revision: saved.workspace_revision.clone(),
                layer_id: Some(layer.layer_id.clone()),
            },
        )
        .unwrap();
    assert!(!solo.project_dirty);
    let style = service
        .layer_update(
            &doc.document_id,
            &solo.revision,
            LayerUpdateParams {
                layer_id: layer.layer_id.clone(),
                expected_workspace_revision: solo.workspace_revision,
                base_color: Some("#123456".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(style.project_dirty);
    let saved = service
        .project_save(&doc.document_id, &style.revision, None, false)
        .unwrap();
    assert!(!saved.project_dirty);
    let mut workspace = service.project_workspace(&doc.document_id).unwrap();
    workspace.grid.spacing_mm = 0.5;
    workspace.grid.visible = true;
    workspace.snap.enabled = true;
    let changed = service
        .project_set_workspace(&doc.document_id, workspace.clone())
        .unwrap();
    assert!(changed.project_dirty);
    service
        .project_save(&doc.document_id, &changed.revision, None, false)
        .unwrap();
    let reopened = service.project_open(path.to_str().unwrap()).unwrap();
    assert_eq!(
        service
            .project_workspace(&reopened.document_id)
            .unwrap()
            .grid,
        workspace.grid
    );
    assert_eq!(
        service
            .project_workspace(&reopened.document_id)
            .unwrap()
            .snap,
        workspace.snap
    );
    assert!(reopened.solo_layer_id.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn json_project_operations_obey_path_and_revision_contract() {
    let (mut service, root) = setup();
    let call = |service: &mut ApplicationService,
                op: &str,
                id: Option<&str>,
                revision: Option<&str>,
                params: serde_json::Value| {
        service.execute_json(&serde_json::json!({ "api_version": 1, "request_id": op, "op": op, "document_id": id, "expected_revision": revision, "params": params }).to_string())
    };
    let new = call(
        &mut service,
        "project.new",
        None,
        None,
        serde_json::json!({}),
    );
    let id = new["result"]["document_id"].as_str().unwrap();
    let revision = new["result"]["revision"].as_str().unwrap();
    assert_eq!(
        call(
            &mut service,
            "project.save",
            Some(id),
            Some(revision),
            serde_json::json!({})
        )["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let path = root.join("api.rcam");
    let saved = call(
        &mut service,
        "project.save_as",
        Some(id),
        Some(revision),
        serde_json::json!({ "path": path, "allow_replace": false }),
    );
    assert_eq!(saved["error"], serde_json::Value::Null);
    let opened = call(
        &mut service,
        "project.open",
        None,
        None,
        serde_json::json!({ "path": path }),
    );
    assert!(opened["result"]["document_id"].is_string());
    let stale = call(
        &mut service,
        "project.save",
        Some(id),
        Some("999"),
        serde_json::json!({}),
    );
    assert_eq!(stale["error"]["code"], "REVISION_CONFLICT");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn gerber_import_after_project_open_adds_a_layer_without_changing_project_path() {
    let (mut service, root) = setup();
    let project_path = root.join("blocks.rcam");
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/s4b2/sample.rcam"
        ),
        &project_path,
    )
    .unwrap();
    let gerber_path = root.join("new.gbr");
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/s0c/am_1.gbr"
        ),
        &gerber_path,
    )
    .unwrap();
    let opened = service
        .project_open(project_path.to_str().unwrap())
        .unwrap();
    let imported = service
        .import_gerber_layers(
            &opened.document_id,
            &opened.revision,
            ImportGerberLayersParams {
                paths: vec![gerber_path.to_string_lossy().into_owned()],
            },
        )
        .unwrap();
    assert_eq!(imported.layers.len(), 1);
    let after = service.document_get(&opened.document_id).unwrap();
    assert_eq!(after.project_path, opened.project_path);
    assert!(after.project_dirty);
    assert_eq!(after.layer_ids.len(), opened.layer_ids.len() + 1);
    let snapshot = service.project_snapshot(&opened.document_id).unwrap();
    assert_eq!(
        snapshot.layer_order,
        snapshot
            .layers
            .iter()
            .map(|layer| layer.layer.id.clone())
            .collect::<Vec<_>>()
    );
    let saved = service
        .project_save(&opened.document_id, &after.revision, None, false)
        .unwrap();
    assert!(!saved.project_dirty);
    let reopened = service
        .project_open(project_path.to_str().unwrap())
        .unwrap();
    assert_eq!(reopened.layer_ids.len(), after.layer_ids.len());
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "release/native timing evidence gate"]
fn project_400x100_save_open_performance() {
    let (mut service, root) = setup();
    let mut project = perf_fixture::big_project(400, 100);
    let mut second = project.layers[0].clone();
    second.layer.id = "l2".into();
    second.layer.objects.clear();
    second.workspace.display_name = "Second layer".into();
    project.layers.push(second);
    project.layer_order.push("l2".into());
    let seed = rcam_project::encode_v1(&project).unwrap();
    let restored = service.project_restore(&seed).unwrap();
    let started = Instant::now();
    let encoded = service
        .project_recovery_bytes(&restored.document_id)
        .unwrap();
    let encode_ms = started.elapsed().as_secs_f64() * 1000.;
    let path = root.join("benchmark.rcam");
    let started = Instant::now();
    service
        .project_save(
            &restored.document_id,
            &restored.revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    let save_ms = started.elapsed().as_secs_f64() * 1000.;
    let started = Instant::now();
    let opened = service.project_open(path.to_str().unwrap()).unwrap();
    let open_ms = started.elapsed().as_secs_f64() * 1000.;
    assert_eq!(opened.layer_ids.len(), 2);
    assert!(!opened.project_dirty);
    assert!(encoded.len() < 2 * 1024 * 1024);
    println!(
        "S4B3_PROJECT_PERF bytes={} encode_ms={encode_ms:.3} atomic_save_ms={save_ms:.3} open_ms={open_ms:.3} definition_objects=400 instances=100 layers=2",
        encoded.len()
    );
    fs::remove_dir_all(root).unwrap();
}
