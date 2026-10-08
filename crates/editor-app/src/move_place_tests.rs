//! Real App/raw-input/serial-worker CPU regressions; native input remains external.
use crate::{
    EditorApp,
    move_place::Phase,
    point_input::Context,
    project_ui::Transition,
    state::{Action, Model, View},
};
use editor_core::{
    MmPoint,
    command::{CommandDispatcher, ids},
};
use editor_service::{
    SelectionEdit,
    task::{CancelOutcome, TaskContext},
};
use eframe::{App, egui};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
};
type Work = (u64, rcam_diagnostics::Source, Action, TaskContext);
struct Run {
    app: EditorApp,
    model: Model,
    requests: Receiver<Work>,
    replies: SyncSender<(u64, View)>,
    ctx: egui::Context,
    frame: eframe::Frame,
    dir: PathBuf,
}
impl Run {
    fn new() -> Self {
        Self::with_count(2)
    }
    fn with_count(count: usize) -> Self {
        use std::fmt::Write;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rcam-move-place-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("synthetic.gbr");
        let mut source = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\n");
        for n in 0..count {
            writeln!(source, "X{}Y0D02*X{}Y1000000D01*", n * 100, n * 100).unwrap();
        }
        source.push_str("M02*\n");
        std::fs::write(&path, source).unwrap();
        let mut model = Model::default();
        model.run(Action::Open(path));
        model.run(Action::SelectAll);
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        assert_eq!(model.view.selected.ordered.len(), count);
        // Exercise placement while the optional material-centroid result is unavailable.
        model.view.selection_geometry_identity =
            crate::state::selection_geometry_identity(&model.view);
        model.view.selection_geometry_error = Some("等待面积统计".into());
        let mut app = crate::modal::tests::app();
        let (tx, requests) = sync_channel(16);
        let (replies, rx) = sync_channel(16);
        app.tx = tx;
        app.rx = rx;
        app.view = model.view.clone();
        app.selected_flags = Arc::new(crate::gpu::selection_flags(
            app.view.scene.as_ref().unwrap(),
            &app.view.selected.ids(),
        ));
        app.last_structure_serial = app.view.structure_serial;
        app.camera.scale = 10.;
        app.fit = false;
        app.prefs.interaction.grip_edit = true;
        app.grid.snap_enabled = false;
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let mut run = Self {
            app,
            model,
            requests,
            replies,
            ctx,
            frame: eframe::Frame::_new_kittest(),
            dir,
        };
        run.update(vec![], egui::Modifiers::NONE, true);
        run.no_edit_requests();
        run
    }
    fn update(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers, focused: bool) {
        let mut raw = egui::RawInput {
            focused,
            modifiers,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 900.),
            )),
            events,
            ..Default::default()
        };
        self.app.raw_input_hook(&self.ctx, &mut raw);
        let _ = self
            .ctx
            .run(raw, |ctx| self.app.update(ctx, &mut self.frame));
    }
    fn hover(&mut self, world: MmPoint, alt: bool) {
        let pos = self.app.camera.screen(world, self.app.canvas_rect);
        self.update(
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers {
                alt,
                ..Default::default()
            },
            true,
        );
    }
    fn pointer(&mut self, world: MmPoint, pressed: bool, alt: bool) {
        let pos = self.app.camera.screen(world, self.app.canvas_rect);
        let modifiers = egui::Modifiers {
            alt,
            ..Default::default()
        };
        self.update(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
            modifiers,
            true,
        );
    }
    fn work(&mut self, apply: bool) -> (u64, View, TaskContext) {
        let (id, _, action, task) = self.requests.try_recv().unwrap();
        assert!(if apply {
            matches!(action, Action::PointApply(..))
        } else {
            matches!(action, Action::PointPreview(..))
        });
        self.model.run_task(task.clone(), action);
        (id, self.model.view.clone(), task)
    }
    fn reply(&mut self, id: u64, view: View) {
        self.replies.send((id, view)).unwrap();
        self.update(vec![], egui::Modifiers::NONE, true);
    }
    fn no_edit_requests(&self) {
        while let Ok((_, _, action, _)) = self.requests.try_recv() {
            assert!(
                matches!(action, Action::SelectionCenters(..) | Action::Viewport(..)),
                "unexpected manufacturing/point request"
            );
        }
    }
    fn save_current(&mut self) {
        self.model
            .save_project(Some(&self.dir.join("current.rcam")), false, None)
            .unwrap();
        self.app.view = self.model.view.clone();
        self.app.selected_flags = Arc::new(crate::gpu::selection_flags(
            self.app.view.scene.as_ref().unwrap(),
            &self.app.view.selected.ids(),
        ));
        assert!(!self.app.view.info.as_ref().unwrap().project_dirty);
    }
    fn start(&mut self) {
        assert!(self.app.dispatch(ids::OBJECT_MOVE_PLACE));
        assert!(self.app.move_placing());
        assert!(self.app.modal.is_none());
    }
    fn ready(&mut self) {
        self.start();
        let (id, view, _) = self.work(false);
        assert!(view.error.is_none(), "{:?}", view.error);
        self.reply(id, view);
        assert!(matches!(
            self.app
                .point_transform
                .as_ref()
                .unwrap()
                .placement
                .as_ref()
                .unwrap()
                .phase,
            Phase::Following
        ));
    }
    fn final_preview(&mut self, point: MmPoint) -> crate::point_transform::Request {
        self.pointer(point, true, true);
        self.pointer(point, false, true);
        let phase = &self
            .app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase;
        let Phase::FinalPreview(request) = phase else {
            panic!("no final preview")
        };
        request.clone()
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn esc() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }
}

#[test]
fn hover_reuses_template_and_snapshot_without_worker_or_hold_then_one_exact_commit() {
    let mut run = Run::new();
    let before = run.model.view.snap_snapshot.clone().unwrap();
    let info = run.app.view.info.clone();
    let selected = run.app.view.selected.ordered.clone();
    run.ready();
    let template = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .placement
        .as_ref()
        .unwrap()
        .baseline
        .clone()
        .unwrap();
    let base = template.bounds.center();
    assert_eq!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .base
            .value
            .world_mm,
        base
    );
    for n in 0..64 {
        run.hover(MmPoint::new(2. + n as f64 / 100., 3.), true);
        assert!(run.requests.try_recv().is_err());
        assert!(Arc::ptr_eq(
            &template,
            run.app
                .point_transform
                .as_ref()
                .unwrap()
                .placement
                .as_ref()
                .unwrap()
                .baseline
                .as_ref()
                .unwrap()
        ));
        assert!(selected.shares_storage(&run.app.view.selected.ordered));
    }
    assert_eq!(run.app.view.info, info);
    assert!(run.app.drag.is_none() && run.app.grip.is_none());
    let request = run.final_preview(MmPoint::new(3.123456789, 2.987654321));
    let target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .world_mm;
    assert_eq!(
        request.operation,
        SelectionEdit::Move {
            dx_mm: target.x_mm - base.x_mm,
            dy_mm: target.y_mm - base.y_mm
        }
    );
    run.hover(MmPoint::new(7., 5.), false);
    assert_eq!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .target
            .value
            .world_mm,
        target
    );
    let (id, view, _) = run.work(false);
    assert!(view.error.is_none());
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    assert!(view.error.is_none());
    run.reply(id, view);
    assert!(!run.app.move_placing() && run.app.point_pick.is_none());
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    let after = run.model.view.snap_snapshot.clone().unwrap();
    let SelectionEdit::Move { dx_mm, dy_mm } = request.operation else {
        panic!()
    };
    for (a, b) in before.layers[0]
        .objects
        .iter()
        .zip(&after.layers[0].objects)
    {
        assert_eq!(
            b.geometry,
            SelectionEdit::Move { dx_mm, dy_mm }
                .preview_geometry(&a.geometry)
                .unwrap()
        );
    }
    run.no_edit_requests();
    run.model.run(Action::History(false));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        before.layers[0].objects
    );
    run.model.run(Action::History(true));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        after.layers[0].objects
    );
}
#[test]
fn target_click_before_admission_freezes_event_point_and_scale_without_committing_zero_preview() {
    let mut run = Run::new();
    let before = run.app.view.info.clone();
    run.start();
    run.pointer(MmPoint::new(3., 2.), true, true);
    run.pointer(MmPoint::new(3., 2.), false, true);
    let target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .clone();
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Frozen
    ));
    run.app.camera.scale = 8.;
    run.hover(MmPoint::new(6., 4.), false);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let request = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .requested
        .clone()
        .unwrap();
    assert_ne!(
        request.operation,
        SelectionEdit::Move {
            dx_mm: 0.,
            dy_mm: 0.
        }
    );
    assert_eq!(request.ppm, 10.);
    assert_eq!(
        run.app.point_transform.as_ref().unwrap().target.value,
        target
    );
    assert_eq!(run.app.view.info, before);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    run.reply(id, view);
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
}
#[test]
fn escape_focus_pointergone_ime_and_secondary_click_cannot_share_target_commit() {
    for conflict in 0..5 {
        let mut run = Run::new();
        run.ready();
        let before = run.model.view.info.clone();
        let point = MmPoint::new(3., 2.);
        let pos = run.app.camera.screen(point, run.app.canvas_rect);
        run.pointer(point, true, true);
        if conflict == 4 {
            run.update(
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
                egui::Modifiers::ALT,
                true,
            );
        }
        let mut events = vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::ALT,
        }];
        match conflict {
            0 => events.push(esc()),
            1 => events.push(egui::Event::WindowFocused(false)),
            2 => events.push(egui::Event::PointerGone),
            3 => events.push(egui::Event::Ime(egui::ImeEvent::Preedit("x".into()))),
            _ => events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: Default::default(),
            }),
        }
        run.update(events, egui::Modifiers::ALT, conflict != 1);
        assert!(run.requests.try_recv().is_err(), "conflict{conflict}");
        assert_eq!(run.model.view.info, before);
        if conflict != 3 {
            assert!(!run.app.move_placing());
        }
    }
}
#[test]
fn cancel_retired_baseline_and_final_previews_never_resurrect_or_commit() {
    for final_stage in [false, true] {
        let mut run = Run::new();
        if final_stage {
            run.ready();
            run.final_preview(MmPoint::new(3., 2.));
        } else {
            run.start();
        }
        let before = run.app.view.info.clone();
        run.update(vec![esc()], egui::Modifiers::NONE, true);
        let (id, view, _) = run.work(false);
        run.reply(id, view.clone());
        run.reply(id, view);
        assert!(!run.app.move_placing() && run.app.view.point_preview.is_none());
        assert_eq!(run.app.view.info, before);
        assert!(!run.app.busy);
        run.no_edit_requests();
    }
}
#[test]
fn late_cancel_installs_real_commit_and_one_undo_without_restarting_placement() {
    let mut run = Run::new();
    run.ready();
    run.final_preview(MmPoint::new(3., 2.));
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, view, task) = run.work(true);
    assert_eq!(task.cancel_token.cancel(), CancelOutcome::TooLate);
    run.update(vec![esc()], egui::Modifiers::NONE, true);
    run.reply(id, view.clone());
    run.reply(id, view);
    assert!(!run.app.move_placing());
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    assert_eq!(run.app.view.info, run.model.view.info);
    run.no_edit_requests();
}
#[test]
fn exact_context_rejects_project_selection_revision_permission_and_precision_changes() {
    for change in 0..7 {
        let mut run = Run::new();
        run.start();
        let (id, view, _) = run.work(false);
        let old = Context::capture(&run.app.view);
        match change {
            0 => run.app.view.info.as_mut().unwrap().project_id.push('x'),
            1 => {
                run.app.view.selected.ordered =
                    run.app.view.selected.ordered.iter().cloned().collect()
            }
            2 => run.app.view.selection_epoch += 1,
            3 => run.app.view.info.as_mut().unwrap().revision.push('x'),
            4 => run
                .app
                .view
                .info
                .as_mut()
                .unwrap()
                .workspace_revision
                .push('x'),
            5 => run.app.view.rule_revision += 1,
            _ => {
                run.app
                    .view
                    .info
                    .as_mut()
                    .unwrap()
                    .manufacturing_precision
                    .resolution_mm *= 2.
            }
        }
        assert!(!old.valid(&run.app.view));
        run.reply(id, view);
        assert!(!run.app.move_placing());
        assert_eq!(run.app.view.error.as_ref().unwrap().code, "STALE_TASK");
        run.no_edit_requests();
    }
}
#[test]
fn preview_error_missing_result_and_apply_unknown_or_disconnect_fail_closed() {
    for fault in 0..4 {
        let mut run = Run::new();
        if fault < 2 {
            run.start();
        } else {
            run.ready();
            run.final_preview(MmPoint::new(3., 2.));
            let (id, view, _) = run.work(false);
            run.reply(id, view);
        }
        if fault == 3 {
            run.app.move_place_disconnected();
            assert!(run.app.canvas_selection_unconfirmed);
            continue;
        }
        let (id, mut view, _) = run.work(fault == 2);
        if fault == 0 {
            view.error = Some(editor_service::ServiceError {
                code: "RESOURCE_LIMIT".into(),
                message: "synthetic budget refusal".into(),
                details: serde_json::json!({}),
            });
        } else if fault == 1 {
            view.point_preview = None;
        } else {
            view.info.as_mut().unwrap().project_id.push('x');
        }
        run.reply(id, view);
        assert!(!run.app.move_placing());
        run.no_edit_requests();
        if fault == 2 {
            assert!(run.app.canvas_selection_unconfirmed);
        } else {
            assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 0);
        }
    }
}
#[test]
fn first_open_transition_waits_for_terminal_preview_and_never_uses_retired_selection() {
    let mut run = Run::new();
    run.save_current();
    run.start();
    let path = run.dir.join("synthetic.gbr");
    // Use an existing saved project, not a Gerber passed to OpenProject.
    let project = run.dir.join("target.rcam");
    let mut target = Model::default();
    target.run(Action::NewWorkspace);
    target.save_project(Some(&project), false, None).unwrap();
    let target_id = target.view.info.as_ref().unwrap().project_id.clone();
    run.app.begin_transition(Transition::Open(project.clone()));
    run.app.begin_transition(Transition::Close);
    assert!(!run.app.move_placing() && run.app.busy);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, _, action, task) = run.requests.try_recv().unwrap_or_else(|e|panic!("request {e:?}; busy={} transition={} close={} projecterr={:?} ui={:?} error={:?} pointtask={} unconfirmed={}",run.app.busy,run.app.transition.is_some(),run.app.close_prompt,run.app.project_error,run.app.ui_error,run.app.view.error,run.app.move_place_task.is_some(),run.app.canvas_selection_unconfirmed));
    assert!(matches!(&action,Action::OpenProject(p,false)if p==&project));
    run.model.run_task(task, action);
    run.reply(id, run.model.view.clone());
    assert_eq!(run.app.view.info.as_ref().unwrap().project_id, target_id);
    assert!(run.app.view.selected.ordered.is_empty());
    assert!(path.exists());
}
#[test]
fn eighty_thousand_click_move_keeps_complete_selection_and_exact_one_undo_redo() {
    let mut run = Run::with_count(80_000);
    let before = run.model.view.snap_snapshot.clone().unwrap();
    run.ready();
    assert!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .baseline
            .as_ref()
            .unwrap()
            .simplified
    );
    let request = run.final_preview(MmPoint::new(6., 3.));
    let (id, view, _) = run.work(false);
    assert!(view.error.is_none(), "{:?}", view.error);
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    assert!(view.error.is_none(), "{:?}", view.error);
    run.reply(id, view);
    assert_eq!(run.app.view.selected.ordered.len(), 80_000);
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    let after = run.model.view.snap_snapshot.clone().unwrap();
    for (a, b) in before.layers[0]
        .objects
        .iter()
        .zip(&after.layers[0].objects)
    {
        assert_eq!(
            b.geometry,
            request.operation.preview_geometry(&a.geometry).unwrap()
        );
        assert_eq!(a.object_id, b.object_id);
    }
    run.model.run(Action::History(false));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        before.layers[0].objects
    );
    run.model.run(Action::History(true));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        after.layers[0].objects
    );
}

#[test]
fn idle_following_disconnect_releases_input_and_template_before_same_frame_click() {
    let mut run = Run::new();
    run.ready();
    assert!(run.app.move_place_task.is_none());
    let (dead, dead_rx) = sync_channel(1);
    drop(dead_rx);
    drop(std::mem::replace(&mut run.replies, dead));
    run.pointer(MmPoint::new(3., 2.), true, true);
    assert!(
        !run.app.move_placing()
            && run.app.point_pick.is_none()
            && run.app.view.point_preview.is_none()
    );
    assert!(run.app.drag.is_none() && run.app.grip.is_none());
    assert_eq!(
        run.app.view.error.as_ref().unwrap().code,
        "WORKER_DISCONNECTED"
    );
    assert!(run.requests.try_recv().is_err());
}
#[test]
fn click_release_alt_owns_raw_target_even_when_final_frame_modifiers_differ() {
    let mut run = Run::new();
    run.ready();
    run.app.grid.snap_enabled = true;
    run.app.grid.spacing_mm = 1.;
    let point = MmPoint::new(3.333333, 2.666667);
    let pos = run.app.camera.screen(point, run.app.canvas_rect);
    run.pointer(point, true, false);
    run.update(
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::ALT,
        }],
        egui::Modifiers::NONE,
        true,
    );
    let request = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .requested
        .clone()
        .unwrap();
    let target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .clone();
    assert_eq!(target.source, crate::point_input::Source::Raw);
    assert_eq!(
        target.world_mm,
        run.app.camera.world(pos, run.app.canvas_rect)
    );
    let base = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .base
        .value
        .world_mm;
    assert_eq!(
        request.operation,
        SelectionEdit::Move {
            dx_mm: target.world_mm.x_mm - base.x_mm,
            dy_mm: target.world_mm.y_mm - base.y_mm
        }
    );
}
#[test]
fn deferred_close_waits_and_does_not_discard_unknown_apply_or_late_success() {
    for outcome in 0..3 {
        let mut run = Run::new();
        run.save_current();
        run.ready();
        run.final_preview(MmPoint::new(3., 2.));
        let (id, view, _) = run.work(false);
        run.reply(id, view);
        let result = if outcome == 0 {
            None
        } else {
            Some(run.work(true))
        };
        run.app.begin_transition(Transition::Close);
        assert!(run.app.busy && !run.app.move_placing());
        let (id, mut view, _) = result.unwrap_or_else(|| run.work(true));
        if outcome == 2 {
            view.task_receipt.as_mut().unwrap().task_id += 1;
        }
        run.reply(id, view);
        match outcome {
            0 => {
                let (_, _, action, _) = run.requests.try_recv().unwrap_or_else(|e|panic!("request {e:?}; busy={} transition={} close={} projecterr={:?} ui={:?} error={:?} pointtask={} unconfirmed={}",run.app.busy,run.app.transition.is_some(),run.app.close_prompt,run.app.project_error,run.app.ui_error,run.app.view.error,run.app.move_place_task.is_some(),run.app.canvas_selection_unconfirmed));
                assert!(matches!(action, Action::Close(false)));
                assert!(!run.app.canvas_selection_unconfirmed);
            }
            1 => {
                assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
                assert!(run.app.close_prompt);
                run.no_edit_requests();
            }
            _ => {
                assert!(run.app.canvas_selection_unconfirmed && !run.app.close_prompt);
                assert!(run.requests.try_recv().is_err());
            }
        }
    }
}

#[test]
fn launch_frame_and_existing_held_press_cannot_become_the_target_click() {
    let mut run = Run::new();
    let point = MmPoint::new(3., 2.);
    let pos = run.app.camera.screen(point, run.app.canvas_rect);
    // A pre-existing press belongs to the old input owner; start the command
    // before egui consumes it, as a menu/shortcut may do in the same frame.
    let raw = egui::RawInput {
        focused: true,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200., 900.),
        )),
        events: vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::ALT,
            },
        ],
        ..Default::default()
    };
    let _ = run.ctx.run(raw, |ctx| {
        run.app.arbitrate_point_input_frame(ctx);
        run.app.start_move_place();
        run.app.update(ctx, &mut run.frame);
    });
    run.pointer(point, false, true);
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Preparing
    ));
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Following
    ));
    assert!(run.requests.try_recv().is_err());
    run.final_preview(point);
}

#[test]
fn old_release_before_new_press_and_release_uses_only_owned_event_position_and_alt() {
    let mut run = Run::new();
    run.app.grid.snap_enabled = true;
    run.app.grid.spacing_mm = 1.;
    let old = run
        .app
        .camera
        .screen(MmPoint::new(3.33333, 2.66667), run.app.canvas_rect);
    let new = run
        .app
        .camera
        .screen(MmPoint::new(6.33333, 4.66667), run.app.canvas_rect);
    let raw = egui::RawInput {
        focused: true,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200., 900.),
        )),
        events: vec![
            egui::Event::PointerMoved(old),
            egui::Event::PointerButton {
                pos: old,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        ..Default::default()
    };
    let _ = run.ctx.run(raw, |ctx| {
        run.app.arbitrate_point_input_frame(ctx);
        run.app.start_move_place();
        run.app.update(ctx, &mut run.frame);
    });
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    run.update(
        vec![
            egui::Event::PointerButton {
                pos: old,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
            egui::Event::PointerMoved(new),
            egui::Event::PointerButton {
                pos: new,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: new,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::ALT,
            },
        ],
        egui::Modifiers::NONE,
        true,
    );
    let session = run.app.point_transform.as_ref().unwrap();
    assert!(matches!(
        session.placement.as_ref().unwrap().phase,
        Phase::FinalPreview(_)
    ));
    assert_eq!(
        session.target.value.world_mm,
        run.app.camera.world(new, run.app.canvas_rect)
    );
    assert_eq!(session.target.value.source, crate::point_input::Source::Raw);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    run.reply(id, view);
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    run.no_edit_requests();
}
