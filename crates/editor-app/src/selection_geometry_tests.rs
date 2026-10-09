use crate::state::{Action, Model, selection_geometry_identity};
use editor_core::MmPoint;
use editor_service::{
    SelectionCentersParams, SelectionMaterialSemantics,
    task::{CancelOutcome, TaskContext, TaskState},
};
fn model() -> Model {
    let mut m = Model::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s5i2/overlap_rectangles.gbr");
    m.open(&path).unwrap();
    m.run(Action::Select(
        MmPoint::new(0.5, 1.5),
        0.,
        crate::selection::SelectionMode::Replace,
    ));
    m
}
fn action(m: &Model) -> Action {
    Action::SelectionCenters(
        selection_geometry_identity(&m.view),
        SelectionCentersParams {
            groups: m.view.selected.groups(),
            semantics: SelectionMaterialSemantics::SelectedLayerComposite,
        },
    )
}
#[test]
fn read_task_installs_metrics_without_manufacturing_or_selection_change() {
    let mut m = model();
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    let scene = m.view.scene.clone().unwrap();
    let task = TaskContext::new(1, m.task_version().unwrap());
    let query = action(&m);
    m.run_task(task.clone(), query);
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.info, before);
    assert_eq!(m.view.selected, selected);
    assert!(std::sync::Arc::ptr_eq(
        &scene,
        m.view.scene.as_ref().unwrap()
    ));
    assert!(m.view.selection_geometry.is_some());
    assert_eq!(
        m.view.task_receipt.as_ref().unwrap().state,
        TaskState::Completed
    );
    assert_eq!(task.cancel_token.cancel(), CancelOutcome::TooLate);
}
#[test]
fn all_task_versions_selection_epoch_and_request_id_fence_metric_replies() {
    let mut m = model();
    let current = m.view.clone();
    let context = selection_geometry_identity(&current);
    let task = TaskContext::new(71, m.task_version().unwrap());
    let query = action(&m);
    m.run_task(task.clone(), query);
    let result = m.view.clone();
    assert!(crate::geometry_reply_matches(
        &task, &current, &result, &context
    ));
    for field in 0..7 {
        let mut stale = current.clone();
        match field {
            0 => stale
                .info
                .as_mut()
                .unwrap()
                .document_id
                .push_str("reopened"),
            1 => stale.info.as_mut().unwrap().revision = "99".into(),
            2 => stale.info.as_mut().unwrap().workspace_revision = "99".into(),
            3 => stale.task_generation += 1,
            4 => stale.rule_revision += 1,
            5 => {
                stale
                    .info
                    .as_mut()
                    .unwrap()
                    .manufacturing_precision
                    .resolution_mm = 1e-6
            }
            _ => stale.selection_epoch += 1,
        }
        assert!(
            !crate::geometry_reply_matches(&task, &stale, &result, &context),
            "field {field}"
        );
    }
    let later = TaskContext::new(72, task.input.clone());
    assert!(!crate::geometry_reply_matches(
        &later, &current, &result, &context
    ));
    for terminal in [TaskState::Cancelled, TaskState::Failed, TaskState::Running] {
        let mut bad = result.clone();
        bad.task_receipt.as_mut().unwrap().state = terminal;
        assert!(!crate::geometry_reply_matches(
            &task, &current, &bad, &context
        ));
    }
}
#[test]
fn stale_selection_request_and_queued_cancel_cannot_install_old_center() {
    let mut m = model();
    let old = action(&m);
    let old_epoch = m.view.selection_epoch;
    let before = m.view.info.clone();
    m.run(Action::Select(
        MmPoint::new(2.5, 0.5),
        0.,
        crate::selection::SelectionMode::Replace,
    ));
    assert!(m.view.selection_epoch > old_epoch);
    let chosen = m.view.selected.clone();
    m.run_task(TaskContext::new(2, m.task_version().unwrap()), old);
    assert_eq!(m.view.error.as_ref().unwrap().code, "STALE_TASK");
    assert!(m.view.selection_geometry.is_none());
    assert_eq!(m.view.info, before);
    assert_eq!(m.view.selected, chosen);
    let task = TaskContext::new(3, m.task_version().unwrap());
    assert_eq!(task.cancel_token.cancel(), CancelOutcome::Requested);
    let query = action(&m);
    m.run_task(task, query);
    assert_eq!(m.view.error.as_ref().unwrap().code, "CANCELLED");
    assert!(m.view.selection_geometry.is_none());
    assert_eq!(m.view.selected, chosen);
}
#[test]
fn metric_request_is_background_and_user_command_cancels_it_without_disabling_controls() {
    let mut app = crate::modal::tests::app();
    let m = model();
    app.view = m.view.clone();
    app.routing.bind_fixture(&app.view);
    let (tx, requests) = std::sync::mpsc::sync_channel(2);
    app.tx = tx;
    app.send(action(&m));
    assert!(!app.busy);
    let token = app.geometry_task.as_ref().unwrap().cancel_token.clone();
    assert!(matches!(
        requests.try_recv().unwrap().2,
        Action::SelectionCenters(..)
    ));
    app.send(Action::Select(
        MmPoint::new(2.5, 0.5),
        0.,
        crate::selection::SelectionMode::Replace,
    ));
    assert!(app.busy);
    assert_eq!(token.state(), TaskState::CancelRequested);
    assert!(matches!(requests.try_recv().unwrap().2, Action::Select(..)));
}

#[test]
fn result_payload_and_background_scheduling_cannot_supersede_viewport() {
    let mut m = model();
    let current = m.view.clone();
    let context = selection_geometry_identity(&current);
    let task = TaskContext::new(81, m.task_version().unwrap());
    let query = action(&m);
    m.run_task(task.clone(), query);
    let result = m.view.clone();
    for field in 0..5 {
        let mut bad = result.clone();
        let value = std::sync::Arc::make_mut(bad.selection_geometry.as_mut().unwrap());
        match field {
            0 => value.document_id.push_str("wrong"),
            1 => value.computed_revision = "999".into(),
            2 => value.resolution_mm = 1e-6,
            3 => value.selected_count += 1,
            _ => bad.selection_epoch += 1,
        };
        assert!(
            !crate::geometry_reply_matches(&task, &current, &bad, &context),
            "payload {field}"
        );
    }
    let mut app = crate::modal::tests::app();
    app.view = current;
    app.routing.bind_fixture(&app.view);
    let (tx, requests) = std::sync::mpsc::sync_channel(2);
    app.tx = tx;
    app.viewport_sequence = Some(31);
    let before = app.sequence;
    app.send(action(&m));
    assert_eq!(app.sequence, before);
    assert!(app.geometry_task.is_none());
    assert!(requests.try_recv().is_err());
    app.viewport_sequence = None;
    app.send(action(&m));
    let first = app.geometry_task.as_ref().unwrap().task_id;
    let before = app.sequence;
    app.send(action(&m));
    assert_eq!(app.sequence, before);
    assert_eq!(app.geometry_task.as_ref().unwrap().task_id, first);
}

#[test]
fn viewport_request_preempts_background_geometry_without_foreground_busy() {
    let mut app = crate::modal::tests::app();
    let m = model();
    app.view = m.view.clone();
    app.routing.bind_fixture(&app.view);
    let (tx, requests) = std::sync::mpsc::sync_channel(2);
    app.tx = tx;
    app.send(action(&m));
    let token = app.geometry_task.as_ref().unwrap().cancel_token.clone();
    let _ = requests.try_recv().unwrap();
    app.send(Action::Viewport(
        MmPoint::new(0., 0.),
        editor_core::BoundsMm {
            min_x_mm: -1.,
            min_y_mm: -1.,
            max_x_mm: 1.,
            max_y_mm: 1.,
        },
        20.,
    ));
    assert_eq!(token.state(), TaskState::CancelRequested);
    assert!(app.viewport_task.is_some());
    assert!(!app.busy);
    assert!(matches!(
        requests.try_recv().unwrap().2,
        Action::Viewport(..)
    ));
}
