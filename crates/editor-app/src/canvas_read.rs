//! Pointer queries use the serial worker without owning the global write UI.
//! Selection is worker state: its terminal result must be synchronized before
//! another selection or service action can use that state.
use crate::{EditorApp, project_ui::Transition};
use editor_service::task::{TaskContext, TaskState};
use eframe::egui;
use std::time::{Duration, Instant};

pub(crate) struct Pending {
    pub task: TaskContext,
    selection_identity: String,
    project_id: Option<String>,
    selected: crate::shared_snapshot::SnapshotVec<editor_service::ObjectInfo>,
    probe: bool,
    started: Instant,
    after_stop: Option<Transition>,
}

impl EditorApp {
    pub(crate) fn selection_read_pending(&self) -> bool {
        self.canvas_read.as_ref().is_some_and(|p| !p.probe)
    }

    pub(crate) fn cancel_canvas_probe(&mut self) {
        if self.canvas_read.as_ref().is_some_and(|p| p.probe) {
            self.canvas_read.take().unwrap().task.cancel_token.cancel();
        }
    }

    pub(crate) fn defer_canvas_transition(&mut self, transition: Transition) -> bool {
        if let Some(read) = self.canvas_read.as_mut().filter(|p| !p.probe) {
            read.after_stop.get_or_insert(transition);
            read.task.cancel_token.cancel();
            true
        } else {
            self.cancel_canvas_probe();
            self.drag = None;
            false
        }
    }

    pub(crate) fn accept_canvas_request(&mut self, task: TaskContext, probe: bool) {
        self.cancel_canvas_probe();
        if probe {
            if let Some(drag) = &mut self.drag {
                drag.probe_task_id = Some(task.task_id);
            }
        } else if let Some(geometry) = &self.geometry_task {
            geometry.cancel_token.cancel();
        }
        self.canvas_read = Some(Pending {
            task,
            selection_identity: crate::state::selection_geometry_identity(&self.view),
            project_id: self.view.info.as_ref().map(|d| d.project_id.clone()),
            selected: self.view.selected.ordered.clone(),
            probe,
            started: Instant::now(),
            after_stop: None,
        });
    }

    /// Returns true for this lane's reply, including rejected identities.
    pub(crate) fn consume_canvas_reply(&mut self, id: u64, result: &crate::state::View) -> bool {
        if self
            .canvas_read
            .as_ref()
            .is_none_or(|p| p.task.task_id != id)
        {
            return false;
        }
        let pending = self.canvas_read.take().unwrap();
        let identity = crate::task_reply_matches(&pending.task, &self.view, result)
            && result
                .task_receipt
                .as_ref()
                .is_some_and(|r| r.result_version == pending.task.input)
            && pending.selection_identity == crate::state::selection_geometry_identity(&self.view)
            && pending.project_id.as_deref()
                == self.view.info.as_ref().map(|d| d.project_id.as_str())
            && pending.project_id.as_deref() == result.info.as_ref().map(|d| d.project_id.as_str())
            && pending.selected.shares_storage(&self.view.selected.ordered);
        #[cfg(feature = "internal-evidence")]
        crate::native_a2::reply(id, identity);
        if !identity {
            if !pending.probe {
                // The worker may already have changed its selection. Never edit
                // with the old UI set or automatically execute a deferred close.
                self.view.blocked = Some("选择结果未确认；请重新打开工程后再编辑".into());
                self.canvas_selection_unconfirmed = true;
                self.ui_error = self.view.blocked.clone();
            }
            self.drag = None;
            self.view.move_admission = None;
            self.selection_presentation.synchronize(&self.view);
            return true;
        }
        if pending.probe {
            let gesture = self
                .drag
                .as_ref()
                .is_some_and(|d| d.probe_task_id == Some(id));
            if gesture
                && result.error.is_none()
                && result.selected.ordered == self.view.selected.ordered
            {
                self.view.press_hit = result.press_hit.clone();
                self.view.drag_hit = result.drag_hit;
                self.view.move_admission = result.move_admission.clone();
                self.drag.as_mut().unwrap().confirm(&self.view);
            } else {
                self.drag = None;
                if let Some(error) = &result.error {
                    self.ui_error = Some(format!("{} · {}", error.code, error.message));
                }
            }
        } else {
            // Completed (even after late Cancel), Failed and Cancelled receipts
            // all carry the worker's terminal selection or explicit rollback.
            self.view.move_admission = None;
            self.view.selected = result.selected.clone();
            self.view.click_cycle = result.click_cycle.clone();
            self.view.selection_epoch = result.selection_epoch;
            self.view.metrics = result.metrics.clone();
            self.view.metrics_error = result.metrics_error.clone();
            self.view.selection_geometry = result.selection_geometry.clone();
            self.view.selection_geometry_identity = result.selection_geometry_identity.clone();
            self.view.selection_geometry_error = result.selection_geometry_error.clone();
            self.selected_flags =
                std::sync::Arc::new(self.view.scene.as_ref().map_or_else(Vec::new, |scene| {
                    crate::gpu::selection_flags(scene, &self.view.selected.ids())
                }));
            if let Some(error) = &result.error {
                self.ui_error = Some(format!("{} · {}", error.code, error.message));
            }
            if let Some(transition) = pending.after_stop {
                self.begin_transition(transition);
            }
        }
        self.selection_presentation.synchronize(&self.view);
        true
    }

    pub(crate) fn canvas_read_status(&mut self, ui: &mut egui::Ui) {
        let Some(read) = &self.canvas_read else {
            return;
        };
        if read.started.elapsed() < Duration::from_millis(150) {
            ui.ctx().request_repaint_after(Duration::from_millis(150));
            return;
        }
        ui.spinner();
        ui.label(if read.probe {
            "点击探测中…"
        } else {
            "确认选择中…"
        });
        if matches!(
            read.task.cancel_token.state(),
            TaskState::CancelRequested | TaskState::Cancelled
        ) {
            ui.label("正在取消…");
        } else if ui.button("取消选择任务").clicked() {
            read.task.cancel_token.cancel();
            self.drag = None;
        }
    }

    pub(crate) fn canvas_read_disconnected(&mut self) {
        if let Some(read) = self.canvas_read.take() {
            read.task.cancel_token.cancel();
            self.drag = None;
            self.ui_error = Some("后台连接已关闭，点击任务未确认".into());
            if !read.probe {
                self.canvas_selection_unconfirmed = true;
                self.view.blocked = self.ui_error.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        camera::Camera,
        selection::SelectionMode,
        state::{Action, Model},
    };
    use editor_core::{MmPoint, command::ids};
    use eframe::App;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc::{Receiver, SyncSender, sync_channel},
        },
    };
    type Request = (u64, rcam_diagnostics::Source, Action, TaskContext);
    struct Run {
        app: EditorApp,
        model: Model,
        requests: Receiver<Request>,
        replies: SyncSender<(u64, crate::state::View)>,
        ctx: egui::Context,
        frame: eframe::Frame,
        dir: PathBuf,
    }
    impl Run {
        fn new() -> Self {
            Self::with_history_budget(None)
        }
        fn with_history_budget(budget: Option<usize>) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "rcam-canvas-read-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("synthetic.gbr");
            std::fs::write(
                &path,
                b"%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X0Y0D03*X2000000Y0D03*M02*",
            )
            .unwrap();
            let mut model = Model::default();
            if let Some(bytes) = budget {
                model.service =
                    editor_service::ApplicationService::with_file_access_and_history_limits(
                        editor_service::FileAccessPolicy::new(&dir, [dir.clone()], [dir.clone()]),
                        100,
                        bytes,
                    )
                    .unwrap();
                model.run(Action::Open(path));
            } else {
                model.run(Action::ImportGerbers(vec![path]));
            }
            assert!(model.view.error.is_none());
            let mut app = crate::modal::tests::app();
            let (tx, requests) = sync_channel(16);
            let (replies, rx) = sync_channel(16);
            app.tx = tx;
            app.rx = rx;
            app.view = model.view.clone();
            app.selected_flags = std::sync::Arc::new(crate::gpu::selection_flags(
                app.view.scene.as_ref().unwrap(),
                &app.view.selected.ids(),
            ));
            app.last_structure_serial = app.view.structure_serial;
            app.fit = false;
            app.prefs.interaction.grip_edit = false;
            app.camera = Camera::default();
            app.view.render_ppm = 10.;
            app.view.render_coverage_complete = true;
            app.dirty_since = Instant::now();
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
            run.update(vec![]);
            assert!(run.requests.try_recv().is_err());
            run
        }
        fn update(&mut self, events: Vec<egui::Event>) {
            let mut raw = egui::RawInput {
                focused: true,
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
        fn pointer(&mut self, pos: egui::Pos2, pressed: bool) {
            self.update(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        fn work(&mut self) -> (u64, TaskContext) {
            let (id, _, action, task) = self.requests.try_recv().unwrap();
            self.model.run_task(task.clone(), action);
            (id, task)
        }
        fn reply(&mut self, id: u64) {
            self.replies.send((id, self.model.view.clone())).unwrap();
            self.update(vec![]);
        }
        fn probe(&mut self, point: MmPoint) {
            let pos = self.app.camera.screen(point, self.app.canvas_rect);
            self.pointer(pos, true);
        }
        fn selection(&mut self, point: MmPoint) {
            let pos = self.app.camera.screen(point, self.app.canvas_rect);
            self.app.send(Action::CanvasSelect(
                crate::selection::ClickContext::new(pos, self.app.camera, self.app.canvas_rect, 1.),
                SelectionMode::Replace,
            ));
        }
        fn drain_geometry(&mut self) {
            while let Ok((id, _, action, task)) = self.requests.try_recv() {
                assert!(matches!(action, Action::SelectionCenters(..)));
                self.model.run_task(task, action);
                self.reply(id);
            }
        }
    }
    impl Drop for Run {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn real_pointer_empty_and_hit_clicks_never_claim_write_busy_or_dirty_history() {
        let mut run = Run::new();
        let before = run.model.view.info.clone();
        for point in [
            MmPoint::new(5., 5.),
            MmPoint::new(0., 0.),
            MmPoint::new(5., 5.),
            MmPoint::new(0., 0.),
        ] {
            let pos = run.app.camera.screen(point, run.app.canvas_rect);
            run.pointer(pos, true);
            assert!(
                run.app.canvas_read.as_ref().is_some_and(|p| p.probe),
                "usable={} camera={:?} canvas={:?} pos={:?} drag={} display={:?} pending={} modal={:?} select_tool={} keyboard={} blocked={:?}",
                run.app.usable(),
                run.app.camera,
                run.app.canvas_rect,
                pos,
                run.app.drag.is_some(),
                run.app.display_error,
                run.app.display_pending,
                run.app.modal,
                run.app.tool == crate::tools::ActiveTool::Select,
                run.ctx.wants_keyboard_input(),
                run.app.view.blocked
            );
            assert!(!run.app.busy && run.app.pending_task.is_none());
            assert!(run.app.command_enabled(ids::FILE_OPEN_PROJECT));
            let (probe, _) = run.work();
            run.pointer(pos, false);
            run.reply(probe);
            assert!(run.app.selection_read_pending());
            assert!(!run.app.busy && !run.app.command_context_blocked());
            let (select, _) = run.work();
            run.reply(select);
            assert_eq!(
                run.app.view.selected.ordered.len(),
                usize::from(point.x_mm == 0.)
            );
            assert_eq!(run.app.view.selection_epoch, run.model.view.selection_epoch);
            run.drain_geometry();
            assert_eq!(run.model.view.info, before);
        }
    }
    #[test]
    fn probe_replacement_and_retired_reply_cannot_confirm_another_gesture() {
        let mut run = Run::new();
        run.probe(MmPoint::new(0., 0.));
        let old = run.app.canvas_read.as_ref().unwrap().task.clone();
        run.probe(MmPoint::new(5., 5.));
        assert_eq!(old.cancel_token.state(), TaskState::CancelRequested);
        let current = run.app.canvas_read.as_ref().unwrap().task.task_id;
        let (id, _) = run.work();
        run.reply(id);
        assert_eq!(run.app.canvas_read.as_ref().unwrap().task.task_id, current);
        assert!(!run.app.drag.as_ref().is_some_and(|d| d.confirmed));
        let (id, _) = run.work();
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        assert!(
            run.app.drag.as_ref().unwrap().confirmed && run.app.drag.as_ref().unwrap().box_select
        );
    }
    #[test]
    fn cancelled_probe_does_not_advance_epoch_or_trigger_metrics_on_next_probe() {
        let mut run = Run::new();
        run.probe(MmPoint::new(0., 0.));
        let epoch = run.model.view.selection_epoch;
        run.app
            .canvas_read
            .as_ref()
            .unwrap()
            .task
            .cancel_token
            .cancel();
        let (id, _) = run.work();
        run.reply(id);
        run.probe(MmPoint::new(0., 0.));
        let (id, _) = run.work();
        assert_eq!(run.model.view.selection_epoch, epoch);
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        assert_eq!(run.app.view.selection_epoch, epoch);
    }
    #[test]
    fn selection_serializes_writes_next_selection_and_preserves_terminal_bundle() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        let sequence = run.app.sequence;
        run.app.send(Action::Delete);
        run.selection(MmPoint::new(2., 0.));
        assert_eq!(run.app.sequence, sequence);
        assert!(!run.app.busy);
        let (id, _) = run.work();
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        assert!(run.requests.try_recv().is_err());
        assert_eq!(
            run.app.view.selected.ordered,
            run.model.view.selected.ordered
        );
        assert_eq!(run.app.view.metrics, run.model.view.metrics);
        assert_eq!(run.app.view.selection_epoch, run.model.view.selection_epoch);
        run.app.send(Action::Delete);
        assert!(run.app.busy && run.app.pending_task.is_some());
    }
    #[test]
    fn late_cancel_syncs_completed_selection_and_first_close_intent() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        let (id, task) = run.work();
        assert_eq!(
            task.cancel_token.cancel(),
            editor_service::task::CancelOutcome::TooLate
        );
        run.app
            .begin_transition(Transition::Open(run.dir.join("first.rcam")));
        run.app.begin_transition(Transition::Quit);
        assert!(!run.app.close_prompt && !run.app.allow_quit);
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        assert_eq!(
            run.app.view.selected.ordered,
            run.model.view.selected.ordered
        );
        assert!(run.app.close_prompt && matches!(run.app.transition, Some(Transition::Open(_))));
        assert!(!run.app.busy && !run.app.allow_quit);
    }
    #[test]
    fn cancelled_and_failed_selection_terminal_epoch_remains_worker_owned() {
        for cancel in [false, true] {
            let mut run = Run::new();
            run.selection(MmPoint::new(0., 0.));
            if cancel {
                run.app
                    .canvas_read
                    .as_ref()
                    .unwrap()
                    .task
                    .cancel_token
                    .cancel();
            }
            let (id, _) = run.work();
            if !cancel {
                run.model.view.error = Some(editor_service::ServiceError {
                    code: "SYNTHETIC".into(),
                    message: "terminal test".into(),
                    details: serde_json::json!({}),
                });
                run.model.view.task_receipt.as_mut().unwrap().state = TaskState::Failed;
            }
            assert!(run.app.consume_canvas_reply(id, &run.model.view));
            assert_eq!(
                run.app.view.selected.ordered,
                run.model.view.selected.ordered
            );
            assert_eq!(run.app.view.selection_epoch, run.model.view.selection_epoch);
            assert!(run.app.ui_error.is_some() && !run.app.busy);
        }
    }
    #[test]
    fn unknown_selection_identity_quarantines_editing_and_discards_deferred_quit() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        run.app.begin_transition(Transition::Quit);
        let (id, _) = run.work();
        run.model.view.task_receipt.as_mut().unwrap().task_id += 1;
        let old = run.app.view.selected.ordered.clone();
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        assert_eq!(run.app.view.selected.ordered, old);
        assert!(run.app.view.blocked.is_some() && !run.app.usable());
        assert!(!run.app.allow_quit && !run.app.close_prompt && run.app.transition.is_none());
        let sequence = run.app.sequence;
        for action in [
            Action::History(false),
            Action::History(true),
            Action::CreateEmptyLayer(None),
            Action::Delete,
            Action::Viewport(
                MmPoint::new(0., 0.),
                editor_core::BoundsMm {
                    min_x_mm: -1.,
                    min_y_mm: -1.,
                    max_x_mm: 3.,
                    max_y_mm: 1.,
                },
                10.,
            ),
        ] {
            run.app.send(action);
        }
        run.app
            .start_gerber_import(vec![run.dir.join("synthetic.gbr")]);
        assert_eq!(run.app.sequence, sequence);
        assert!(run.requests.try_recv().is_err());
        assert!(
            !run.app.command_enabled(ids::EDIT_UNDO) && !run.app.command_enabled(ids::LAYER_CREATE)
        );
        assert!(run.app.command_enabled(ids::FILE_OPEN_PROJECT));
        // An old render reply cannot clear the independent quarantine latch.
        let before = run.app.view.info.clone();
        run.replies
            .send((run.app.sequence, run.model.view.clone()))
            .unwrap();
        run.update(vec![]);
        assert!(run.app.canvas_selection_unconfirmed);
        assert_eq!(run.app.view.info, before);
        // An explicit discard-and-new transition provides an owned new document.
        run.app.begin_transition(Transition::New);
        assert!(run.app.close_prompt);
        run.app.close_prompt = false;
        run.app.transition = None;
        run.app.send(Action::DiscardNewWorkspace);
        let (id, _) = run.work();
        run.reply(id);
        assert!(!run.app.canvas_selection_unconfirmed && run.app.view.blocked.is_none());
    }
    #[test]
    fn probe_cannot_claim_import_write_task_or_confirm_after_escape() {
        let mut run = Run::new();
        run.app.send(Action::NewWorkspace);
        let write = run.app.pending_task.as_ref().unwrap().task_id;
        let sequence = run.app.sequence;
        run.probe(MmPoint::new(0., 0.));
        assert!(run.app.canvas_read.is_none() && run.app.busy);
        assert_eq!(run.app.pending_task.as_ref().unwrap().task_id, write);
        assert_eq!(run.app.sequence, sequence);
        let mut run = Run::new();
        let pos = run.app.canvas_rect.center();
        run.pointer(pos, true);
        let (id, _) = run.work();
        run.update(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        run.reply(id);
        assert!(run.app.drag.is_none() && !run.app.busy);
        assert!(
            !run.requests
                .try_iter()
                .any(|r| matches!(r.2, Action::DragMove(..)))
        );
    }
    #[test]
    fn retired_latest_probe_never_installs_a_full_view_even_with_faulted_payload() {
        let mut run = Run::new();
        run.probe(MmPoint::new(0., 0.));
        let (id, _) = run.work();
        let info = run.app.view.info.clone();
        let scene = run.app.view.scene.clone().unwrap();
        let epoch = run.app.view.selection_epoch;
        let selected = run.app.view.selected.ordered.clone();
        run.app.cancel_canvas_probe();
        run.app.drag = None;
        assert_eq!(run.app.sequence, id);
        let mut fault = run.model.view.clone();
        fault.info = None;
        fault.scene = None;
        fault.selection_epoch += 55;
        run.replies.send((id, fault)).unwrap();
        run.update(vec![]);
        assert_eq!(run.app.view.info, info);
        assert_eq!(run.app.view.selection_epoch, epoch);
        assert_eq!(run.app.view.selected.ordered, selected);
        assert!(std::sync::Arc::ptr_eq(
            run.app.view.scene.as_ref().unwrap(),
            &scene
        ));
        assert!(run.app.drag.is_none() && !run.app.busy);
    }
    #[test]
    fn delayed_probe_hands_box_selection_to_read_lane_and_drag_to_one_write() {
        for object_drag in [false, true] {
            let mut run = Run::new();
            if object_drag {
                run.selection(MmPoint::new(0., 0.));
                let (id, _) = run.work();
                assert!(run.app.consume_canvas_reply(id, &run.model.view));
                run.update(vec![]);
                run.drain_geometry();
            }
            let before = run.model.view.info.clone().unwrap();
            let start = if object_drag {
                MmPoint::new(0., 0.)
            } else {
                MmPoint::new(-1., 1.)
            };
            let end = if object_drag {
                MmPoint::new(1., 0.)
            } else {
                MmPoint::new(3., -1.)
            };
            run.probe(start);
            let (id, _) = run.work();
            let pos = run.app.camera.screen(end, run.app.canvas_rect);
            run.pointer(pos, false);
            run.reply(id);
            assert_eq!(run.app.busy, object_drag);
            assert_eq!(run.app.selection_read_pending(), !object_drag);
            let (id, _) = run.work();
            run.reply(id);
            if object_drag {
                let after = run.model.view.info.as_ref().unwrap();
                assert_eq!(after.undo_entries, before.undo_entries + 1);
                assert_eq!(after.revision, "2");
            } else {
                assert_eq!(run.model.view.info, Some(before));
                assert_eq!(run.app.view.selected.ordered.len(), 2);
            }
        }
    }
    #[test]
    fn disconnected_selection_isolates_edits_without_executing_deferred_close() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        run.app.begin_transition(Transition::Quit);
        run.app.canvas_read_disconnected();
        assert!(run.app.canvas_selection_unconfirmed && run.app.canvas_read.is_none());
        assert!(!run.app.allow_quit && run.app.transition.is_none() && !run.app.close_prompt);
        let sequence = run.app.sequence;
        run.app.send(Action::Delete);
        assert_eq!(run.app.sequence, sequence);
    }
    #[test]
    fn selection_terminal_and_window_close_same_frame_keep_first_dirty_open_intent() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        let (id, _) = run.work();
        let first = run.dir.join("first.rcam");
        run.app.begin_transition(Transition::Open(first.clone()));
        run.replies.send((id, run.model.view.clone())).unwrap();
        let mut raw = egui::RawInput {
            focused: true,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 900.),
            )),
            ..Default::default()
        };
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let _ = run.ctx.run(raw, |ctx| run.app.update(ctx, &mut run.frame));
        assert!(
            run.app.close_prompt
                && matches!(&run.app.transition,Some(Transition::Open(path)) if path==&first)
        );
        assert!(!run.app.allow_quit && !run.app.busy);
        assert!(
            run.requests
                .try_iter()
                .all(|r| matches!(r.2, Action::SelectionCenters(..)))
        );
    }
    #[test]
    fn cancelled_selection_preserves_metric_identity_for_next_geometry_request() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        let (id, _) = run.work();
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        let epoch = run.app.view.selection_epoch;
        run.selection(MmPoint::new(2., 0.));
        run.app
            .canvas_read
            .as_ref()
            .unwrap()
            .task
            .cancel_token
            .cancel();
        let (id, _) = run.work();
        assert!(run.app.consume_canvas_reply(id, &run.model.view));
        run.update(vec![]);
        run.drain_geometry();
        assert_eq!(run.model.view.selection_epoch, epoch);
        assert_eq!(run.app.view.selection_epoch, epoch);
        assert!(run.app.view.selection_geometry.is_some());
        assert!(crate::point_input::current_centers(&run.app.view).is_ok());
    }
    #[test]
    fn selection_escape_and_pointer_gone_wait_for_explicit_rollback() {
        for event in [
            egui::Event::PointerGone,
            egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ] {
            let mut run = Run::new();
            run.selection(MmPoint::new(0., 0.));
            let before = run.app.view.info.clone();
            run.update(vec![event]);
            assert_eq!(
                run.app
                    .canvas_read
                    .as_ref()
                    .unwrap()
                    .task
                    .cancel_token
                    .state(),
                TaskState::CancelRequested
            );
            let (id, _) = run.work();
            run.reply(id);
            assert!(run.app.view.selected.ordered.is_empty() && !run.app.busy);
            assert_eq!(run.app.view.info, before);
            assert!(!run.app.canvas_selection_unconfirmed);
        }
    }
    #[test]
    fn slow_selection_cancel_button_keeps_terminal_synchronization() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        run.app.canvas_read.as_mut().unwrap().started = Instant::now() - Duration::from_millis(200);
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let mut draw = |events| {
            ctx.run(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| run.app.canvas_read_status(ui));
                    });
                },
            )
        };
        let output = draw(vec![]);
        let center = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.job.text == "取消选择任务" => {
                    Some(t.pos + t.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            let _ = draw(vec![
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert_eq!(
            run.app
                .canvas_read
                .as_ref()
                .unwrap()
                .task
                .cancel_token
                .state(),
            TaskState::CancelRequested
        );
        assert!(!run.app.busy && !run.app.command_context_blocked());
        let (id, _) = run.work();
        run.reply(id);
        assert!(run.app.canvas_read.is_none() && run.app.view.selected.ordered.is_empty());
    }
    #[test]
    fn presentation_cache_respects_cancelled_and_stale_selection_replies() {
        for stale in [false, true] {
            let mut run = Run::new();
            run.selection(MmPoint::new(0., 0.));
            let (id, _) = run.work();
            assert!(run.app.consume_canvas_reply(id, &run.model.view));
            run.drain_geometry();
            let before = run.app.view.selected.clone();
            assert!(run.app.selection_presentation.editable(&run.app.view));
            let _ = run.app.selection_presentation.metric_lines(
                &run.app.view,
                crate::tools::DisplayUnit::Millimeter,
                0.0001,
            );
            run.selection(MmPoint::new(2., 0.));
            if !stale {
                run.app
                    .canvas_read
                    .as_ref()
                    .unwrap()
                    .task
                    .cancel_token
                    .cancel();
            }
            let (id, _) = run.work();
            if stale {
                run.model
                    .view
                    .task_receipt
                    .as_mut()
                    .unwrap()
                    .result_version
                    .workspace_revision = Some("stale".into());
            }
            assert!(run.app.consume_canvas_reply(id, &run.model.view));
            assert_eq!(run.app.view.selected, before);
            assert_eq!(
                run.app.selection_presentation.editable(&run.app.view),
                crate::drag::editable_selection(&run.app.view)
            );
            assert_eq!(
                run.app.selection_presentation.metric_lines(
                    &run.app.view,
                    crate::tools::DisplayUnit::Millimeter,
                    0.0001
                ),
                crate::metrics_panel::lines(
                    &run.app.view,
                    crate::tools::DisplayUnit::Millimeter,
                    0.0001
                )
            );
            if stale {
                assert!(!run.app.command_enabled(ids::OBJECT_MOVE));
            }
        }
    }
    #[test]
    fn actual_close_retires_cached_manufacturing_snapshot_before_ui_queries() {
        let mut run = Run::new();
        run.selection(MmPoint::new(0., 0.));
        let (id, _) = run.work();
        run.reply(id);
        run.drain_geometry();
        assert!(run.app.selection_presentation.editable(&run.app.view));
        let weak = std::sync::Arc::downgrade(run.app.view.snap_snapshot.as_ref().unwrap());
        run.app.send(Action::Close(true));
        let (id, _) = run.work();
        run.reply(id);
        assert!(run.app.view.info.is_none());
        assert!(
            weak.upgrade().is_none(),
            "closed snapshot must not be retained behind usable/empty-selection guards"
        );
    }
    #[test]
    fn capacity_refusal_survives_snap_and_release_but_keeps_click_selection() {
        let mut r = Run::with_history_budget(Some(1));
        r.selection(MmPoint::new(0., 0.));
        let (id, _) = r.work();
        r.reply(id);
        r.drain_geometry();
        let before = r.model.view.info.clone();
        r.probe(MmPoint::new(0., 0.));
        let (id, _) = r.work();
        let error = r
            .model
            .view
            .move_admission
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap_err();
        assert_eq!(error.code, "RESOURCE_LIMIT");
        assert_eq!(error.details["demand"]["history_limit_bytes"], 1);
        let start = r.app.camera.screen(MmPoint::new(0., 0.), r.app.canvas_rect);
        r.update(vec![egui::Event::PointerMoved(
            start + egui::vec2(20., 10.),
        )]);
        r.reply(id);
        let gesture = r.app.drag.as_mut().unwrap();
        assert!(gesture.error().unwrap().contains("RESOURCE_LIMIT"));
        gesture.set_snap_error(None);
        gesture.update(start + egui::vec2(40., 20.));
        assert_eq!(gesture.delta, MmPoint::new(0., 0.));
        assert!(gesture.error().is_some());
        let gesture = r.app.drag.take().unwrap();
        assert!(gesture.release().is_none());
        assert_eq!(r.model.view.info, before);
        assert!(r.requests.try_recv().is_err());
        let session = crate::point_transform::Session::new(
            &r.model.view,
            crate::point_transform::Mode::Move,
            Default::default(),
        );
        let request = session
            .request(&r.model.view, Default::default(), 20.)
            .unwrap();
        let error = r.model.point_preview(request).unwrap_err();
        assert_eq!(error.code, "RESOURCE_LIMIT");
        assert!(r.model.view.point_preview.is_none());
        assert_eq!(r.model.view.info, before);
        // A click under the same capacity refusal remains a selection action.
        let mut g = crate::drag::Gesture::arm(
            &r.model.view,
            start,
            r.app.camera,
            r.app.canvas_rect,
            1.,
            SelectionMode::Replace,
        );
        g.confirm(&r.model.view);
        assert!(matches!(g.release(), Some(Action::CanvasSelect(..))));
    }
    #[test]
    fn resource_probe_release_before_reply_commits_only_after_current_admission() {
        let mut r = Run::new();
        r.selection(MmPoint::new(0., 0.));
        let (id, _) = r.work();
        r.reply(id);
        r.drain_geometry();
        let before = r.model.view.info.clone().unwrap();
        r.probe(MmPoint::new(0., 0.));
        let (id, _) = r.work();
        assert!(r.model.view.move_admission.as_ref().unwrap().result.is_ok());
        let start = r.app.camera.screen(MmPoint::new(0., 0.), r.app.canvas_rect);
        r.pointer(start + egui::vec2(20., 10.), false);
        assert!(r.requests.try_recv().is_err());
        assert!(!r.app.drag.as_ref().unwrap().confirmed);
        assert_eq!(r.app.drag.as_ref().unwrap().delta, MmPoint::new(0., 0.));
        r.reply(id);
        let (move_id, _) = r.work();
        assert_eq!(
            r.model.view.info.as_ref().unwrap().undo_entries,
            before.undo_entries + 1
        );
        r.reply(move_id);
        assert!(r.app.drag.is_none());
    }
    #[test]
    fn selection_terminal_releases_old_move_admission_owner() {
        let mut r = Run::new();
        r.selection(MmPoint::new(0., 0.));
        let (id, _) = r.work();
        r.reply(id);
        r.drain_geometry();
        r.probe(MmPoint::new(0., 0.));
        let (id, _) = r.work();
        r.reply(id);
        let weak = std::sync::Arc::downgrade(r.app.view.move_admission.as_ref().unwrap());
        r.app.drag = None;
        r.selection(MmPoint::new(2., 0.));
        let (id, _) = r.work();
        r.reply(id);
        assert!(r.app.view.move_admission.is_none());
        assert!(r.model.view.move_admission.is_none());
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn select_all_and_modifier_box_use_serial_read_terminal_lane() {
        for box_mode in [None, Some(SelectionMode::Add), Some(SelectionMode::Remove)] {
            let mut run = Run::new();
            let before = run.model.view.info.clone();
            let action = box_mode.map_or(Action::SelectAll, |mode| {
                Action::CanvasSelectRect(
                    editor_core::BoundsMm {
                        min_x_mm: -1.,
                        min_y_mm: -1.,
                        max_x_mm: 3.,
                        max_y_mm: 1.,
                    },
                    editor_core::hit_test::SelectRectMode::Window,
                    mode,
                )
            });
            run.app.send(action);
            assert!(run.app.selection_read_pending());
            assert!(!run.app.busy);
            run.app.send(Action::Move("1".into(), "0".into()));
            assert!(run.app.ui_error.is_some());
            let (id, _) = run.work();
            run.reply(id);
            assert!(!run.app.selection_read_pending());
            assert!(!run.app.canvas_selection_unconfirmed);
            assert_eq!(run.app.view.selected, run.model.view.selected);
            assert_eq!(run.model.view.info, before);
            run.drain_geometry();
        }
    }
    #[test]
    fn select_all_cancel_and_late_cancel_preserve_authoritative_terminal_set() {
        for early in [true, false] {
            let mut run = Run::new();
            let original = run.model.view.selected.clone();
            run.app.send(Action::SelectAll);
            if early {
                run.app
                    .canvas_read
                    .as_ref()
                    .unwrap()
                    .task
                    .cancel_token
                    .cancel();
            }
            let (id, task) = run.work();
            if !early {
                task.cancel_token.cancel();
            }
            run.reply(id);
            assert!(!run.app.canvas_selection_unconfirmed);
            assert_eq!(run.app.view.selected, run.model.view.selected);
            if early {
                assert!(
                    original
                        .ordered
                        .shares_storage(&run.model.view.selected.ordered)
                );
            } else {
                assert_eq!(run.app.view.selected.ordered.len(), 2);
            }
            run.drain_geometry();
        }
    }
    #[test]
    fn select_all_rejects_project_and_selection_snapshot_identity_changes() {
        for change in ["project", "selection"] {
            let mut run = Run::new();
            run.app.send(Action::SelectAll);
            let (id, _) = run.work();
            if change == "project" {
                run.app.view.info.as_mut().unwrap().project_id.push('x');
            } else {
                run.app.view.selected.ordered = Vec::new().into();
            }
            run.reply(id);
            assert!(run.app.canvas_selection_unconfirmed);
            assert!(run.app.view.blocked.is_some());
            assert!(run.app.view.selected.ordered.is_empty());
        }
    }
    #[test]
    fn select_all_deferred_project_transition_waits_for_rollback_receipt() {
        let mut run = Run::new();
        let original = run.model.view.selected.clone();
        run.app.send(Action::SelectAll);
        assert!(run.app.defer_canvas_transition(Transition::Quit));
        assert!(!run.app.close_prompt);
        let (id, _) = run.work();
        assert!(
            original
                .ordered
                .shares_storage(&run.model.view.selected.ordered)
        );
        run.reply(id);
        assert!(!run.app.selection_read_pending());
        assert!(!run.app.canvas_selection_unconfirmed);
        assert!(run.app.close_prompt && matches!(run.app.transition, Some(Transition::Quit)));
    }
    #[test]
    fn actual_raw_control_a_queues_all_and_modifier_pointer_retains_press_mode() {
        let mut run = Run::new();
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: editor_core::command::Platform::current()
                == editor_core::command::Platform::Windows,
            ..Default::default()
        };
        run.update(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }]);
        assert!(run.app.selection_read_pending());
        assert!(!run.app.busy);
        let (id, _) = run.work();
        run.reply(id);
        run.drain_geometry();
        assert_eq!(run.app.view.selected.ordered.len(), 2);
        run.update(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let p = run
            .app
            .camera
            .screen(MmPoint::new(0., 0.), run.app.canvas_rect);
        run.update(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::SHIFT,
            },
        ]);
        let (id, _) = run.work();
        // Modifier released before the late probe reply; original Shift intent owns release.
        run.update(vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: ctrl,
        }]);
        run.reply(id);
        let (id, _) = run.work();
        run.reply(id);
        run.drain_geometry();
        assert_eq!(run.app.view.selected.ordered.len(), 1);
        assert_eq!(
            run.app.view.selected.primary().unwrap().object.object_id,
            "src-1::object-2"
        );
    }
}
