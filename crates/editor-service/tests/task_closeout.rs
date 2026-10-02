use editor_service::{task::*, *};

#[test]
fn a2_every_task_version_dimension_is_fenced_independently() {
    let input = TaskVersion {
        document_id: Some("a".into()),
        document_revision: Some("7".into()),
        workspace_revision: Some("3".into()),
        generation: 2,
        rule_revision: 4,
        geometry_policy_hash: "policy".into(),
    };
    let task = TaskContext::new(1, input.clone());
    task.cancel_token.start().unwrap();
    task.validate(&input).unwrap();
    let mut changes = vec![input.clone(); 6];
    changes[0].document_id = Some("b".into());
    changes[1].document_revision = Some("8".into());
    changes[2].workspace_revision = Some("4".into());
    changes[3].generation += 1;
    changes[4].rule_revision += 1;
    changes[5].geometry_policy_hash = "another".into();
    for current in changes {
        assert_eq!(task.validate(&current).unwrap_err().code, "STALE_TASK");
    }
    let closed = TaskVersion::default();
    assert_eq!(task.validate(&closed).unwrap_err().code, "STALE_TASK");
    task.cancel_token.finish(false);
    assert_eq!(task.cancel_token.state(), TaskState::Failed);
}

#[test]
fn a2_cancel_before_commit_and_too_late_have_unambiguous_terminal_states() {
    for success in [false, true] {
        let token = CancellationToken::default();
        token.start().unwrap();
        token.begin_commit().unwrap();
        assert_eq!(token.cancel(), CancelOutcome::TooLate);
        token.finish(success);
        assert_eq!(
            token.state(),
            if success {
                TaskState::Completed
            } else {
                TaskState::Failed
            }
        );
        assert_eq!(token.cancel(), CancelOutcome::TooLate);
    }
    let token = CancellationToken::default();
    token.start().unwrap();
    assert_eq!(token.cancel(), CancelOutcome::Requested);
    assert_eq!(token.begin_commit().unwrap_err().code, "CANCELLED");
    assert_eq!(token.state(), TaskState::CancelRequested);
    token.finish(false);
    assert_eq!(token.state(), TaskState::Cancelled);
}

#[test]
fn a2_valid_first_file_then_parse_failure_has_zero_partial_import() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    let valid = root.join("s4c3/blocks.gbr");
    let invalid = std::env::temp_dir().join(format!("rcam-a2-invalid-{}.gbr", std::process::id()));
    std::fs::write(&invalid, b"invalid Gerber\n").unwrap();
    let mut service = ApplicationService::new();
    service.grant_file_access(&valid, false).unwrap();
    service.grant_file_access(&invalid, false).unwrap();
    let info = service.open(valid.to_str().unwrap()).unwrap();
    let before = serde_json::to_value(service.render_snapshot(&info.document_id).unwrap()).unwrap();
    let token = CancellationToken::default();
    token.start().unwrap();
    let result = service.import_gerber_layers_with_cancel(
        &info.document_id,
        &info.revision,
        ImportGerberLayersParams {
            paths: vec![
                valid.to_str().unwrap().into(),
                invalid.to_str().unwrap().into(),
            ],
        },
        Some(&token),
    );
    std::fs::remove_file(&invalid).unwrap();
    assert!(result.is_err());
    token.finish(false);
    assert_eq!(token.state(), TaskState::Failed);
    assert_eq!(service.document_get(&info.document_id).unwrap(), info);
    assert_eq!(
        serde_json::to_value(service.render_snapshot(&info.document_id).unwrap()).unwrap(),
        before
    );
}

#[test]
#[ignore = "release cancellation audit; requires RCAM_A2_FIXTURES JSON paths"]
fn a2_real_import_cancel_joins_worker_with_zero_mutation() {
    use std::time::{Duration, Instant};
    assert!(!cfg!(debug_assertions), "SLA audit requires release");
    let paths: Vec<String> =
        serde_json::from_slice(&std::fs::read(std::env::var("RCAM_A2_FIXTURES").unwrap()).unwrap())
            .unwrap();
    let path = paths[3].clone();
    let bytes = std::fs::read(&path).unwrap();
    let sha = editor_core::hash::sha256_hex(&bytes);
    for delay_ms in [50, 400, 1200, 2500, 4000, 5500] {
        let mut service = ApplicationService::new();
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s4c3/blocks.gbr");
        service.grant_file_access(&base, false).unwrap();
        service
            .grant_file_access(std::path::Path::new(&path), false)
            .unwrap();
        let info = service.open(base.to_str().unwrap()).unwrap();
        let before =
            serde_json::to_value(service.render_snapshot(&info.document_id).unwrap()).unwrap();
        let token = CancellationToken::default();
        let t = token.clone();
        let p = path.clone();
        let (started, ready) = std::sync::mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || {
            t.start().unwrap();
            started.send(()).unwrap();
            let result = service.import_gerber_layers_with_cancel(
                &info.document_id,
                &info.revision,
                // Two real inputs keep the latest probe within cancellable batch
                // preparation and also cover disposal of a previously prepared layer.
                ImportGerberLayersParams {
                    paths: vec![p.clone(), p],
                },
                Some(&t),
            );
            let after = service.document_get(&info.document_id).unwrap();
            let snapshot =
                serde_json::to_value(service.render_snapshot(&info.document_id).unwrap()).unwrap();
            let unchanged = info == after && before == snapshot;
            drop(service);
            t.finish(result.is_ok());
            (
                result.map(|_| ()).map_err(|e| e.code),
                unchanged,
                Instant::now(),
            )
        });
        ready.recv().unwrap();
        std::thread::sleep(Duration::from_millis(delay_ms));
        let click = Instant::now();
        let outcome = token.cancel();
        let (result, unchanged, finished) = worker.join().unwrap();
        let joined_ms = click.elapsed().as_secs_f64() * 1000.;
        println!(
            "A2_CANCEL {}",
            serde_json::json!({"fixture_sha256":sha,"batch_files":2,"delay_ms":delay_ms,"cancel_outcome":format!("{outcome:?}"),"result":result,"unchanged":unchanged,"terminal":format!("{:?}",token.state()),"worker_return_after_request_ms":finished.saturating_duration_since(click).as_secs_f64()*1000.,"worker_join_after_request_ms":joined_ms})
        );
        assert_eq!(outcome, CancelOutcome::Requested);
        assert_eq!(result, Err("CANCELLED".into()));
        assert!(unchanged);
        assert_eq!(token.state(), TaskState::Cancelled);
        assert!(joined_ms <= 2000.);
    }
}
