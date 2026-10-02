use editor_service::{task::*, *};
use std::sync::{Arc, Barrier};

#[test]
fn cancellation_and_commit_are_mutually_exclusive_and_terminal() {
    for _ in 0..128 {
        let token = CancellationToken::default();
        token.start().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let t = token.clone();
        let b = barrier.clone();
        let worker = std::thread::spawn(move || {
            b.wait();
            t.begin_commit()
        });
        barrier.wait();
        let cancel = token.cancel();
        let commit = worker.join().unwrap();
        assert_eq!(commit.is_ok(), cancel == CancelOutcome::TooLate);
        token.finish(commit.is_ok());
        assert_eq!(
            token.state(),
            if commit.is_ok() {
                TaskState::Completed
            } else {
                TaskState::Cancelled
            }
        );
        assert!(token.start().is_err());
    }
}
#[test]
fn cancelled_import_is_atomic_and_success_is_one_undo_with_late_cancel() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s4c3/blocks.gbr");
    let mut service = ApplicationService::new();
    service.grant_file_access(&path, false).unwrap();
    let d = service.open(path.to_str().unwrap()).unwrap();
    let before = serde_json::to_value(service.render_snapshot(&d.document_id).unwrap()).unwrap();
    let params = || ImportGerberLayersParams {
        paths: vec![path.to_str().unwrap().into()],
    };
    for _ in 0..3 {
        let token = CancellationToken::default();
        token.start().unwrap();
        token.cancel();
        let error = service
            .import_gerber_layers_with_cancel(&d.document_id, &d.revision, params(), Some(&token))
            .unwrap_err();
        assert_eq!(error.code, "CANCELLED");
        assert_eq!(service.document_get(&d.document_id).unwrap(), d);
        assert_eq!(
            serde_json::to_value(service.render_snapshot(&d.document_id).unwrap()).unwrap(),
            before
        );
    }
    let token = CancellationToken::default();
    token.start().unwrap();
    let result = service
        .import_gerber_layers_with_cancel(&d.document_id, &d.revision, params(), Some(&token))
        .unwrap();
    assert_eq!(result.undo_entries, 1);
    assert_eq!(token.cancel(), CancelOutcome::TooLate);
    service
        .history_undo(&d.document_id, &result.revision)
        .unwrap();
    let after = service.render_snapshot(&d.document_id).unwrap();
    assert_eq!(
        serde_json::to_value(&after.layers).unwrap(),
        before["layers"]
    );
    assert_eq!(
        serde_json::to_value(&after.apertures).unwrap(),
        before["apertures"]
    );
}

#[test]
fn cancellation_request_is_not_completion_until_worker_finishes() {
    let token = CancellationToken::default();
    token.start().unwrap();
    assert_eq!(token.cancel(), CancelOutcome::Requested);
    assert_eq!(token.state(), TaskState::CancelRequested);
    assert!(token.checkpoint().is_err());
    token.finish(false);
    assert_eq!(token.state(), TaskState::Cancelled);
}
