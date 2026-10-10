//! UI-only sequential import. Each worker request keeps the service's atomic
//! single-file validation, commit fence and existing resource budgets.
use crate::{EditorApp, state::Action};
use editor_service::task::{TaskState, TaskVersion};
use eframe::egui;
use std::{collections::VecDeque, path::PathBuf};

pub(crate) struct ImportQueue {
    owner: crate::session::Owner,
    settled_task: Option<u64>,
    pending: VecDeque<PathBuf>,
    current: Option<Current>,
    expected: TaskVersion,
    total: usize,
    processed: usize,
    imported: usize,
    failed: usize,
    errors: Vec<(PathBuf, String)>,
    stop_requested: bool,
    finished: bool,
    reason: Option<String>,
    after_stop: Option<crate::project_ui::Transition>,
    confirmed: Option<Confirmed>,
}

struct Confirmed {
    native_serial: u64,
    raw_batch: u64,
}

struct Current {
    owner: crate::session::Owner,
    path: PathBuf,
    task_id: u64,
    input: TaskVersion,
}

impl ImportQueue {
    pub(crate) fn active(&self) -> bool {
        // Keep ownership until advance hands off the first deferred intent.
        // Terminal replies arrive before toolbar/progress controls run.
        !self.finished || self.after_stop.is_some()
    }

    pub(crate) fn request_stop(&mut self) {
        self.stop_requested = true;
    }

    pub(crate) fn defer_transition(&mut self, transition: crate::project_ui::Transition) {
        self.request_stop();
        self.after_stop.get_or_insert(transition);
    }

    fn finish(&mut self, reason: Option<String>) {
        self.finished = true;
        self.reason = reason;
        self.pending.clear();
        self.current = None;
        self.confirmed = None;
    }
}

impl EditorApp {
    fn trace_gerber_import(
        &self,
        outcome: &'static str,
        file_count: usize,
        owner: &crate::session::Owner,
        expected: &TaskVersion,
        task_id: Option<u64>,
    ) {
        if let Some(trace) = &self.frame_trace {
            trace.gerber_import(crate::frame_trace::GerberImportState {
                outcome,
                file_count,
                owner_slot: owner.slot(),
                owner_matches: *owner == self.routing.owner(),
                version_matches: *expected
                    == TaskVersion::capture(
                        self.view.info.as_ref(),
                        self.view.task_generation,
                        self.view.rule_revision,
                    ),
                native_serial: self.tabs.input.native_serial(),
                raw_batch: self.tabs.input.raw_batch(),
                task_id,
            });
        }
    }

    pub(crate) fn pick_gerber_import(
        &mut self,
        choose: impl FnOnce() -> Result<Option<Vec<PathBuf>>, String>,
    ) {
        // Admission belongs to the command that opened the panel. Returned
        // paths are its explicit result, not an event from the old input batch.
        if self.gerber_import_admission_blocked() {
            self.ui_error = Some("请先完成当前操作，再导入文件".into());
            return;
        }
        let owner = self.routing.owner();
        let expected = TaskVersion::capture(
            self.view.info.as_ref(),
            self.view.task_generation,
            self.view.rule_revision,
        );
        match self.native_panel(choose) {
            Ok(Some(paths)) => {
                self.trace_gerber_import("panel_confirmed", paths.len(), &owner, &expected, None);
                if owner != self.routing.owner()
                    || expected
                        != TaskVersion::capture(
                            self.view.info.as_ref(),
                            self.view.task_generation,
                            self.view.rule_revision,
                        )
                {
                    self.ui_error = Some("导入工程归属或版本已变化，请重新选择文件".into());
                    return;
                }
                if self.gerber_import_paths_valid(&paths) {
                    self.queue_gerber_import(
                        paths,
                        owner,
                        expected,
                        Some(Confirmed {
                            native_serial: self.tabs.input.native_serial(),
                            raw_batch: self.tabs.input.raw_batch(),
                        }),
                    );
                }
            }
            Ok(None) => self.trace_gerber_import("panel_cancelled", 0, &owner, &expected, None),
            Err(e) => {
                self.trace_gerber_import("panel_error", 0, &owner, &expected, None);
                self.ui_error = Some(e);
            }
        }
    }

    fn gerber_import_admission_blocked(&self) -> bool {
        self.canvas_selection_unconfirmed
            || self.command_context_blocked()
            || self.selection_read_pending()
            || self.transition.is_some()
            || self.waiting_save
            || self.tabs.change_pending()
            || self.tabs.quitting
            || self.tab_input_barrier_active()
            || self.tabs.input.return_pending()
    }

    fn gerber_import_paths_valid(&mut self, paths: &[PathBuf]) -> bool {
        if paths.is_empty() {
            return false;
        }
        if paths.iter().any(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rcam"))
        }) {
            self.ui_error =
                Some("工程文件请使用“打开工程”或单独拖入，不能混入 Gerber 导入队列".into());
            return false;
        }
        // No file bytes are read on the UI thread; bound held path storage.
        if paths.len() > editor_service::MAX_IMPORT_FILES
            || paths.iter().any(|p| p.as_os_str().len() > 4096)
        {
            self.ui_error = Some("导入队列最多包含 64 个文件；路径过长时请先调整路径".into());
            return false;
        }
        true
    }

    pub(crate) fn settle_import_intent(&mut self, id: u64) {
        if self.gerber_import.as_ref().is_some_and(|q| {
            q.finished
                && q.settled_task == Some(id)
                && q.after_stop.is_none()
                && q.owner == self.routing.owner()
        }) && self.transition.is_none()
            && !self.waiting_save
        {
            self.routing.clear_intent();
        }
    }
    pub(crate) fn migrate_import_publication(
        &mut self,
        id: u64,
        before: &crate::session::Owner,
        permit: &crate::session::Permit,
    ) {
        let Some(queue) = self.gerber_import.as_mut() else {
            return;
        };
        if &queue.owner != before
            || queue
                .current
                .as_ref()
                .is_none_or(|c| c.task_id != id || &c.owner != before)
            || !permit.empty_import(id, before)
        {
            return;
        }
        queue.owner = self.routing.owner();
        queue.current.as_mut().unwrap().owner = queue.owner.clone();
        self.routing.migrate_import_intent(id, before, permit);
    }
    pub(crate) fn start_gerber_import(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if paths.iter().any(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rcam"))
        }) {
            self.ui_error =
                Some("工程文件请使用“打开工程”或单独拖入，不能混入 Gerber 导入队列".into());
            return;
        }
        if self.gerber_import_admission_blocked() {
            self.ui_error = Some("请先完成当前操作，再导入文件".into());
            return;
        }
        if !self.gerber_import_paths_valid(&paths) {
            return;
        }
        self.queue_gerber_import(
            paths,
            self.routing.owner(),
            TaskVersion::capture(
                self.view.info.as_ref(),
                self.view.task_generation,
                self.view.rule_revision,
            ),
            None,
        );
        self.advance_gerber_import();
    }

    fn queue_gerber_import(
        &mut self,
        paths: Vec<PathBuf>,
        owner: crate::session::Owner,
        expected: TaskVersion,
        confirmed: Option<Confirmed>,
    ) {
        self.trace_gerber_import("queue_held", paths.len(), &owner, &expected, None);
        self.gerber_import = Some(ImportQueue {
            owner,
            settled_task: None,
            total: paths.len(),
            pending: paths.into(),
            current: None,
            expected,
            processed: 0,
            imported: 0,
            failed: 0,
            errors: Vec::new(),
            stop_requested: false,
            finished: false,
            reason: None,
            after_stop: None,
            confirmed,
        });
    }

    pub(crate) fn stop_gerber_import(&mut self, reason: &str) {
        if let Some(queue) = self.gerber_import.as_mut().filter(|q| q.active()) {
            queue.after_stop = None;
            queue.finish(Some(reason.into()));
            self.routing.clear_intent();
        }
    }

    pub(crate) fn advance_gerber_import(&mut self) {
        let Some(mut queue) = self.gerber_import.take() else {
            return;
        };
        let was_active = queue.active();
        if queue.owner != self.routing.owner() {
            queue.after_stop = None;
            queue.finish(Some("导入工程归属已过期，已停止".into()));
            self.routing.clear_intent();
        }
        let held = self.tabs.held_buttons.iter().any(|down| *down)
            || self.tabs.input.context().is_some_and(|ctx| {
                ctx.input(|i| i.modifiers != egui::Modifiers::NONE || i.pointer.any_down())
            });
        if queue.confirmed.is_some() && held {
            // A new hold while the confirmation waits also owns its release
            // frame. It cannot make the pending command bypass a real hold.
            self.tabs.wait_for_input_release();
        }
        if queue.active() && queue.current.is_none() && !self.busy {
            if self.transition.is_some()
                || self.close_prompt
                || self.waiting_save
                || self.replace_project_path.is_some()
                || self.project_error.is_some()
            {
                queue.after_stop = None;
                queue.finish(Some("当前工程需要确认，已停止后续导入".into()));
            } else if queue.stop_requested {
                queue.finish(Some("已取消后续导入；已成功文件保留".into()));
            } else if queue.expected
                != TaskVersion::capture(
                    self.view.info.as_ref(),
                    self.view.task_generation,
                    self.view.rule_revision,
                )
            {
                queue.finish(Some("当前工程版本已变化，已停止后续导入".into()));
                queue.after_stop = None;
            } else if queue
                .confirmed
                .as_ref()
                .is_some_and(|confirmed| confirmed.native_serial != self.tabs.input.native_serial())
            {
                queue.finish(Some("新的文件面板已取代待导入确认，请重新选择文件".into()));
                queue.after_stop = None;
            } else if self.gerber_import_admission_blocked()
                || queue.confirmed.as_ref().is_some_and(|confirmed| {
                    self.tabs.input.raw_batch() <= confirmed.raw_batch
                        || !self
                            .tabs
                            .input
                            .context()
                            .is_some_and(|ctx| ctx.input(|i| i.focused))
                })
            {
                // The queue was taken out above, so the full command admission
                // sees other owners/fences rather than blocking on this queue.
                // A cleared tab flag alone does not authorize the release frame.
                if !held
                    && (self.tabs.input.return_pending()
                        || self.tabs.input.blocked()
                        || self.tabs.block_document_input)
                    && let Some(ctx) = self
                        .tabs
                        .input
                        .context()
                        .filter(|ctx| ctx.input(|i| i.focused))
                {
                    ctx.request_repaint_after(std::time::Duration::from_millis(20));
                }
            } else if let Some(path) = queue.pending.pop_front() {
                self.send(Action::ImportGerbers(vec![path.clone()]));
                if let Some(task) = self.pending_task.as_ref().filter(|_| self.busy) {
                    self.trace_gerber_import(
                        "dispatch_accepted",
                        1,
                        &queue.owner,
                        &queue.expected,
                        Some(task.task_id),
                    );
                    queue.confirmed = None;
                    queue.current = Some(Current {
                        owner: self.routing.owner(),
                        path,
                        task_id: task.task_id,
                        input: task.input.clone(),
                    });
                } else {
                    self.trace_gerber_import(
                        "dispatch_refused",
                        1,
                        &queue.owner,
                        &queue.expected,
                        None,
                    );
                    queue.after_stop = None;
                    queue.finish(Some(
                        self.ui_error
                            .clone()
                            .unwrap_or("后台未接受导入，已停止".into()),
                    ));
                }
            } else {
                queue.finish(None);
            }
        }
        if was_active && queue.finished {
            self.trace_gerber_import(
                "queue_finished",
                queue.processed,
                &queue.owner,
                &queue.expected,
                queue.settled_task,
            );
        }
        let after_stop = queue.finished.then(|| queue.after_stop.take()).flatten();
        self.gerber_import = Some(queue);
        if let Some(transition) = after_stop {
            self.begin_transition(transition);
        }
    }

    /// Never install a rejected view. A task-bound commit is still reported,
    /// and ambiguous results never permit another file or a deferred close.
    pub(crate) fn reject_gerber_import_reply(&mut self, id: u64, view: &crate::state::View) {
        let Some(queue) = self.gerber_import.as_mut().filter(|q| q.active()) else {
            return;
        };
        let Some(current) = queue.current.as_ref().filter(|c| c.task_id == id) else {
            return;
        };
        let committed = view.import_committed_task == Some(id)
            && view
                .task_receipt
                .as_ref()
                .is_some_and(|r| r.task_id == id && r.input == current.input);
        if committed {
            queue.processed += 1;
            queue.imported += 1;
        }
        queue.after_stop = None;
        queue.finish(Some(
            if committed {
                "文件已导入，但结果视图身份失效；已停止，请检查图层，勿重复导入"
            } else {
                "后台结果身份已失效，当前文件结果未确认；已停止后续导入"
            }
            .into(),
        ));
    }

    /// Called only after the normal reply identity fence accepted and installed
    /// this view. Never infer a new commit from an old view.import or Failed alone.
    pub(crate) fn accept_gerber_import_reply(&mut self, id: u64) {
        let Some(queue) = self.gerber_import.as_mut().filter(|q| q.active()) else {
            return;
        };
        if queue.owner != self.routing.owner()
            || queue
                .current
                .as_ref()
                .is_none_or(|c| c.task_id != id || c.owner != queue.owner)
        {
            return;
        }
        let current = queue.current.take().unwrap();
        queue.settled_task = Some(id);
        let Some(receipt) = self.view.task_receipt.as_ref().filter(|r| {
            r.task_id == id
                && r.input == current.input
                && r.result_version
                    == TaskVersion::capture(
                        self.view.info.as_ref(),
                        self.view.task_generation,
                        self.view.rule_revision,
                    )
        }) else {
            queue.after_stop = None;
            queue.finish(Some("后台导入结果身份未确认，已停止后续导入".into()));
            return;
        };
        let committed = self.view.import_committed_task == Some(id);
        let result_matches = self.view.import.as_ref().is_some_and(|result| {
            Some(&result.document_id) == receipt.result_version.document_id.as_ref()
                && Some(&result.revision) == receipt.result_version.document_revision.as_ref()
                && Some(&result.workspace_revision)
                    == receipt.result_version.workspace_revision.as_ref()
        });
        let error = self.view.error.as_ref();
        let cancelled = receipt.state == TaskState::Cancelled;
        let stale = error.is_some_and(|e| matches!(e.code.as_str(), "STALE_TASK" | "TASK_STATE"));
        if !committed && (error.is_none() || receipt.result_version != current.input) {
            queue.after_stop = None;
            queue.finish(Some(
                "后台未确认文件提交或工程版本已变化，已停止后续导入".into(),
            ));
            return;
        }
        queue.processed += 1;
        if committed {
            queue.imported += 1;
        } else if !cancelled {
            queue.failed += 1;
        }
        if let Some(error) = error {
            let prefix = if committed {
                "已导入，显示刷新失败："
            } else {
                ""
            };
            queue.errors.push((
                current.path,
                format!("{prefix}{}: {}", error.code, error.message)
                    .chars()
                    .take(2048)
                    .collect(),
            ));
        }
        queue.expected = receipt.result_version.clone();
        if committed
            && (!result_matches
                || error.is_some()
                || self.view.blocked.is_some()
                || receipt.state != TaskState::Completed)
        {
            queue.after_stop = None;
            queue.finish(Some(
                "文件已导入；显示刷新未完成，已停止后续导入，请勿重复导入该文件".into(),
            ));
        } else if queue.stop_requested || cancelled || stale {
            if stale {
                queue.after_stop = None;
            }
            queue.finish(Some("已停止后续导入；已成功文件保留".into()));
        } else if queue.pending.is_empty() {
            queue.finish(None);
        }
    }

    pub(crate) fn gerber_import_progress(&mut self, ui: &mut egui::Ui) {
        let Some(queue) = self.gerber_import.as_ref() else {
            return;
        };
        let mut cancel = false;
        let mut close = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "导入 {}/{} · 成功 {} · 失败 {} · 未处理 {}",
                queue.processed,
                queue.total,
                queue.imported,
                queue.failed,
                queue.total - queue.processed
            ));
            if queue.active() {
                if let Some(current) = &queue.current {
                    ui.label(
                        current
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    );
                }
                cancel = ui
                    .add_enabled(
                        !queue.stop_requested,
                        egui::Button::new(if queue.stop_requested {
                            "正在停止…"
                        } else {
                            "取消后续导入"
                        }),
                    )
                    .clicked();
            } else {
                close = ui.button("关闭导入结果").clicked();
            }
        });
        if let Some(reason) = &queue.reason {
            ui.label(reason);
        }
        if !queue.errors.is_empty() {
            ui.collapsing("查看导入问题", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(120.)
                    .show(ui, |ui| {
                        for (path, error) in &queue.errors {
                            ui.label(format!(
                                "{}：{error}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                    });
            });
        }
        if cancel {
            self.gerber_import.as_mut().unwrap().request_stop();
            if let Some(task) = &self.pending_task {
                task.cancel_token.cancel();
            }
        }
        if close {
            self.gerber_import = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Model;
    use editor_service::task::TaskContext;
    use eframe::App;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    };

    type Request = crate::session::Request;
    struct Run {
        app: EditorApp,
        model: Model,
        requests: Receiver<Request>,
        replies: SyncSender<crate::session::Reply>,
        ctx: egui::Context,
        frame: eframe::Frame,
        dir: PathBuf,
    }
    impl Run {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "rcam-import-queue-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            for name in ["first.custom", "last.gbx", "third.gbr"] {
                std::fs::write(
                    dir.join(name),
                    b"%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X0Y0D03*M02*",
                )
                .unwrap();
            }
            std::fs::write(dir.join("bad.gbr"), b"MALFORMED").unwrap();
            let mut app = crate::modal::tests::app();
            let (tx, requests) = sync_channel(16);
            let (replies, rx) = sync_channel(16);
            app.tx = tx;
            app.rx = rx;
            let ctx = egui::Context::default();
            ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
            Self {
                app,
                model: Model::default(),
                requests,
                replies,
                ctx,
                frame: eframe::Frame::_new_kittest(),
                dir,
            }
        }
        fn start(&mut self, names: &[&str]) {
            let raw = egui::RawInput {
                dropped_files: names
                    .iter()
                    .map(|name| egui::DroppedFile {
                        path: Some(self.dir.join(name)),
                        ..Default::default()
                    })
                    .collect(),
                ..self.raw()
            };
            self.update(raw);
            assert!(self.app.gerber_import.as_ref().unwrap().active());
        }
        fn raw(&self) -> egui::RawInput {
            egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 900.),
                )),
                ..Default::default()
            }
        }
        fn update(&mut self, raw: egui::RawInput) {
            let _ = self
                .ctx
                .run(raw, |ctx| self.app.update(ctx, &mut self.frame));
        }
        fn input_batch(&mut self, mut raw: egui::RawInput) {
            self.app.raw_input_hook(&self.ctx, &mut raw);
            self.update(raw);
        }
        fn pick_confirmed(&mut self, names: &[&str]) {
            let paths: Vec<_> = names.iter().map(|name| self.dir.join(name)).collect();
            let mut raw = self.raw();
            self.app.raw_input_hook(&self.ctx, &mut raw);
            let _ = self.ctx.run(raw, |ctx| {
                self.app.enforce_tab_input_barrier(ctx);
                self.app.pick_gerber_import(|| Ok(Some(paths.clone())));
            });
            assert!(self.app.gerber_import.as_ref().unwrap().active());
            assert!(self.requests.try_recv().is_err());
        }
        fn work(&mut self) -> (u64, TaskContext) {
            let (id, _, action, task, _) = self.requests.try_recv().unwrap();
            assert!(matches!(&action, Action::ImportGerbers(paths) if paths.len() == 1));
            self.model.run_task(task.clone(), action);
            (id, task)
        }
        fn install_import_gap(&mut self, id: u64) {
            let view = self.model.view.clone();
            let route = self.app.routing.reply_fixture(id, &view);
            let crate::session::Gate::Result(permit) =
                self.app.routing.validate(id, &view, &route, &self.app.view)
            else {
                panic!("owned import publication");
            };
            let before = self.app.routing.owner();
            self.app.routing.installed(id, &view, &permit);
            self.app.view = view;
            self.app.migrate_import_publication(id, &before, &permit);
        }
        fn reply(&mut self, id: u64) {
            self.replies
                .send(self.app.fixture_reply(id, self.model.view.clone()))
                .unwrap();
            self.update(self.raw());
        }
        fn step(&mut self) {
            let (id, _) = self.work();
            self.reply(id);
        }
        fn no_import_pending(&self) {
            assert!(
                !self
                    .requests
                    .try_iter()
                    .any(|r| matches!(r.2, Action::ImportGerbers(..)))
            );
            assert!(!self.app.gerber_import.as_ref().unwrap().active());
        }
    }
    impl Drop for Run {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn native_confirmed_gerber_paths_wait_for_safe_batch_and_dispatch_once() {
        let mut run = Run::new();
        run.app.frame_trace = Some(crate::frame_trace::Recorder::for_test());
        run.ctx
            .options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(2).unwrap());
        let paths = vec![run.dir.join("first.custom"), run.dir.join("last.gbx")];
        let mut raw = run.raw();
        run.app.raw_input_hook(&run.ctx, &mut raw);
        let mut passes = 0;
        let _ = run.ctx.run(raw, |ctx| {
            passes += 1;
            run.app.enforce_tab_input_barrier(ctx);
            if passes == 1 {
                run.app.pick_gerber_import(|| Ok(Some(paths.clone())));
                ctx.request_discard("confirmed import return-frame regression");
            }
            assert!(
                run.app.gerber_import.as_ref().is_some_and(|q| q.active()),
                "a native-confirmed file list must survive the return-frame barrier"
            );
            run.app.advance_gerber_import();
            assert!(run.requests.try_recv().is_err());
            assert!(run.app.tabs.input.blocked());
        });
        assert_eq!(passes, 2);
        let mut raw = run.raw();
        raw.events = vec![
            egui::Event::Key {
                key: egui::Key::O,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("old native input".into()),
        ];
        run.app.raw_input_hook(&run.ctx, &mut raw);
        assert!(raw.events.is_empty());
        run.update(raw);
        assert!(run.requests.try_recv().is_err());
        assert!(run.app.gerber_import.as_ref().unwrap().active());
        let mut raw = run.raw();
        run.app.raw_input_hook(&run.ctx, &mut raw);
        run.update(raw);
        let (id, _) = run.work();
        assert!(run.requests.try_recv().is_err());
        run.app.advance_gerber_import();
        assert!(run.requests.try_recv().is_err());
        run.app.native_panel(|| ());
        run.reply(id);
        assert!(run.requests.try_recv().is_err());
        run.input_batch(run.raw());
        assert!(run.requests.try_recv().is_err());
        run.input_batch(run.raw());
        run.step();
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert_eq!((queue.processed, queue.imported, queue.failed), (2, 2, 0));
        assert_eq!(run.model.view.layers.len(), 2);
        assert_eq!(run.model.view.info.as_ref().unwrap().undo_entries, 2);
        run.no_import_pending();
        let rows = run.app.frame_trace.as_ref().unwrap().take_test_records();
        let imports: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "gerber_import")
            .collect();
        assert_eq!(imports[0]["state"]["outcome"], "panel_confirmed");
        assert_eq!(imports[0]["state"]["file_count"], 2);
        assert_eq!(imports[1]["state"]["outcome"], "queue_held");
        let accepted: Vec<_> = imports
            .iter()
            .filter(|r| r["state"]["outcome"] == "dispatch_accepted")
            .collect();
        assert_eq!(accepted.len(), 2);
        assert!(accepted.iter().all(|r| r["state"]["owner_matches"] == true));
        assert!(
            accepted
                .iter()
                .all(|r| r["state"]["version_matches"] == true)
        );
        assert!(
            !serde_json::to_string(&imports)
                .unwrap()
                .contains("first.custom")
        );
        assert!(
            !serde_json::to_string(&imports)
                .unwrap()
                .contains("last.gbx")
        );
    }

    #[test]
    fn native_confirmed_gerber_cancel_before_dispatch_releases_intent() {
        let mut run = Run::new();
        run.pick_confirmed(&["first.custom", "last.gbx"]);
        run.app.gerber_import.as_mut().unwrap().request_stop();
        // Stop must work even while no safe batch has arrived.
        run.app.advance_gerber_import();
        run.no_import_pending();
        assert!(run.app.pending_task.is_none() && !run.app.busy);
        for _ in 0..3 {
            run.input_batch(run.raw());
        }
        run.no_import_pending();
        assert!(run.app.view.info.is_none());
    }

    #[test]
    fn native_confirmed_gerber_close_before_dispatch_hands_off_only_first_intent() {
        let mut run = Run::new();
        run.pick_confirmed(&["first.custom", "last.gbx"]);
        run.app
            .begin_transition(crate::project_ui::Transition::Close);
        run.app
            .begin_transition(crate::project_ui::Transition::Quit);
        run.app.advance_gerber_import();
        let (id, _, action, task, _) = run.requests.try_recv().unwrap();
        assert!(matches!(action, Action::Close(false)));
        run.app.advance_gerber_import();
        assert!(run.requests.try_recv().is_err());
        run.model.run_task(task, action);
        run.reply(id);
        assert!(run.requests.try_recv().is_err());
        assert!(run.app.gerber_import.is_none());
        assert!(!run.app.allow_quit && run.app.view.info.is_none());
    }

    #[test]
    fn native_confirmed_gerber_owner_version_or_new_panel_invalidates_pending_paths() {
        for changed in 0..3 {
            let mut run = Run::new();
            run.pick_confirmed(&["first.custom", "last.gbx"]);
            match changed {
                0 => run.app.routing = Default::default(), // Same slot, fresh namespace.
                1 => run.app.view.task_generation += 1,
                2 => run.app.native_panel(|| ()),
                _ => unreachable!(),
            }
            run.app.advance_gerber_import();
            run.no_import_pending();
            for _ in 0..3 {
                run.input_batch(run.raw());
            }
            run.no_import_pending();
            assert!(run.app.view.info.is_none());
        }
    }

    #[test]
    fn native_confirmed_gerber_repeated_commands_and_tab_request_cannot_replace_owner() {
        let mut run = Run::new();
        run.pick_confirmed(&["first.custom", "last.gbx"]);
        let owner = run.app.routing.owner();
        let mut opened = false;
        run.app.pick_gerber_import(|| {
            opened = true;
            Ok(Some(vec![run.dir.join("third.gbr")]))
        });
        assert!(!opened);
        run.app.drop_files(vec![run.dir.join("third.gbr")]);
        run.app.request_new_session(Action::NewWorkspace);
        assert!(!run.app.tabs.change_pending());
        assert_eq!(run.app.routing.owner(), owner);
        assert_eq!(run.app.gerber_import.as_ref().unwrap().total, 2);
        run.input_batch(run.raw());
        run.input_batch(run.raw());
        run.step();
        run.step();
        assert_eq!(run.app.view.layers.len(), 2);
        run.no_import_pending();
    }

    #[test]
    fn native_confirmed_gerber_focus_modifier_and_release_frame_keep_paths_pending() {
        let mut run = Run::new();
        run.pick_confirmed(&["first.custom"]);
        let mut raw = run.raw();
        raw.focused = false;
        run.input_batch(raw);
        for _ in 0..2 {
            let mut raw = run.raw();
            raw.focused = false;
            run.input_batch(raw);
            assert!(run.requests.try_recv().is_err());
        }
        let serial = run.app.tabs.input.serial;
        let mut raw = run.raw();
        raw.modifiers.shift = true;
        raw.events.push(egui::Event::Key {
            key: egui::Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: raw.modifiers,
        });
        run.input_batch(raw);
        assert!(run.requests.try_recv().is_err());
        let mut raw = run.raw();
        raw.modifiers.shift = true;
        run.input_batch(raw);
        assert!(run.app.tabs.input.serial > serial);
        assert!(run.requests.try_recv().is_err());
        let mut raw = run.raw();
        raw.events.push(egui::Event::Key {
            key: egui::Key::B,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        run.input_batch(raw);
        assert!(run.app.tabs.block_document_input);
        assert!(run.requests.try_recv().is_err());
        run.input_batch(run.raw());
        run.step();
        assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 1);
        run.no_import_pending();
    }

    #[test]
    fn native_confirmed_gerber_pointer_hold_and_release_batch_cannot_dispatch() {
        let mut run = Run::new();
        run.pick_confirmed(&["first.custom"]);
        let button = |pressed| egui::Event::PointerButton {
            pos: egui::pos2(20., 20.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut raw = run.raw();
        raw.events.push(button(true));
        run.input_batch(raw);
        assert!(run.requests.try_recv().is_err());
        run.input_batch(run.raw());
        assert!(run.requests.try_recv().is_err());
        let mut raw = run.raw();
        raw.events.push(button(false));
        run.input_batch(raw);
        assert!(run.app.tabs.block_document_input);
        assert!(run.requests.try_recv().is_err());
        run.input_batch(run.raw());
        run.step();
        assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 1);
        run.no_import_pending();
    }

    #[test]
    fn native_confirmed_gerber_full_worker_channel_is_not_retried() {
        let mut run = Run::new();
        run.app.frame_trace = Some(crate::frame_trace::Recorder::for_test());
        run.pick_confirmed(&["first.custom"]);
        let (tx, requests) = sync_channel(1);
        run.app.tx = tx;
        let action = Action::FontCatalog;
        let task = TaskContext::new(
            99,
            TaskVersion::capture(
                None,
                run.app.view.task_generation,
                run.app.view.rule_revision,
            ),
        );
        let route = run.app.routing.prepare(&action, &run.app.view).unwrap();
        run.app
            .tx
            .try_send((99, rcam_diagnostics::Source::Menu, action, task, route))
            .unwrap();
        run.input_batch(run.raw());
        run.input_batch(run.raw());
        assert!(!run.app.gerber_import.as_ref().unwrap().active());
        assert!(!run.app.busy && run.app.pending_task.is_none());
        assert!(matches!(
            requests.try_recv().unwrap().2,
            Action::FontCatalog
        ));
        for _ in 0..2 {
            run.input_batch(run.raw());
        }
        assert!(requests.try_recv().is_err());
        let rows = run.app.frame_trace.as_ref().unwrap().take_test_records();
        let refused: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "gerber_import" && r["state"]["outcome"] == "dispatch_refused")
            .collect();
        assert_eq!(refused.len(), 1);
        assert!(refused[0]["state"]["task_id"].is_null());
    }

    #[test]
    fn native_confirmed_gerber_cancel_error_empty_and_bounds_never_dispatch() {
        for outcome in [
            Ok(None),
            Err("synthetic picker error".to_owned()),
            Ok(Some(vec![])),
            Ok(Some(vec![PathBuf::from("synthetic.rcam")])),
            Ok(Some(vec![
                PathBuf::from("synthetic.gbr");
                editor_service::MAX_IMPORT_FILES + 1
            ])),
            Ok(Some(vec![PathBuf::from("x".repeat(4097))])),
        ] {
            let mut run = Run::new();
            let mut raw = run.raw();
            run.app.raw_input_hook(&run.ctx, &mut raw);
            let _ = run.ctx.run(raw, |ctx| {
                run.app.enforce_tab_input_barrier(ctx);
                run.app.pick_gerber_import(|| outcome.clone());
                assert!(run.app.tabs.input.blocked());
            });
            for _ in 0..3 {
                run.input_batch(run.raw());
            }
            assert!(run.app.gerber_import.is_none());
            assert!(run.requests.try_recv().is_err());
        }
    }

    #[test]
    fn native_confirmed_gerber_worker_cancel_and_resource_refusal_keep_original_receipts() {
        for cancel in [false, true] {
            let mut run = Run::new();
            let file = std::fs::File::create(run.dir.join("oversized.gbr")).unwrap();
            file.set_len(gerber_io::S1_MAX_SOURCE_BYTES as u64 + 1)
                .unwrap();
            run.pick_confirmed(&["oversized.gbr"]);
            run.input_batch(run.raw());
            run.input_batch(run.raw());
            if cancel {
                run.app.gerber_import.as_mut().unwrap().request_stop();
                run.app.pending_task.as_ref().unwrap().cancel_token.cancel();
            }
            let (id, _) = run.work();
            assert_eq!(
                run.model.view.error.as_ref().unwrap().code,
                if cancel {
                    "CANCELLED"
                } else {
                    "RESOURCE_LIMIT"
                }
            );
            run.reply(id);
            assert!(run.app.view.info.is_none());
            assert_eq!(run.app.gerber_import.as_ref().unwrap().processed, 1);
            run.no_import_pending();
        }
    }

    #[test]
    fn native_confirmed_gerber_real_worker_empty_owner_migration_dispatches_once() {
        let mut run = Run::new();
        let (tx, requests) = sync_channel(2);
        let (replies, rx) = sync_channel(1);
        run.app.tx = tx;
        run.app.rx = rx;
        let owner = run.app.routing.owner();
        let host = crate::session::WorkerHost::new(owner.clone(), Model::default());
        let ctx = run.ctx.clone();
        let worker = std::thread::spawn(move || {
            crate::session::run_worker(host, requests, replies, ctx);
        });
        run.pick_confirmed(&["first.custom", "last.gbx"]);
        run.input_batch(run.raw());
        run.input_batch(run.raw());
        for count in 1..=2 {
            let envelope = run
                .app
                .rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("production worker terminal");
            run.app
                .receive_session_reply(envelope, std::time::Instant::now());
            assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, count);
            assert_ne!(run.app.routing.owner(), owner);
            if count < 2 {
                run.input_batch(run.raw());
            } else {
                run.app.advance_gerber_import();
                run.app.advance_gerber_import();
            }
        }
        assert_eq!(run.app.sequence, 2);
        assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 2);
        assert_eq!(run.app.view.layers.len(), 2);
        assert!(!run.app.gerber_import.as_ref().unwrap().active());
        // Close the channel without abandoning a live worker.
        let (tx, _unused) = sync_channel(1);
        run.app.tx = tx;
        worker.join().unwrap();
    }

    #[test]
    fn raw_drop_queue_commits_independent_files_and_summarizes_failure_in_order() {
        let mut run = Run::new();
        run.start(&["first.custom", "bad.gbr", "last.gbx"]);
        let (id, task) = run.work();
        assert!(task.input.document_id.is_none());
        run.reply(id);
        let version = run.model.task_version().unwrap();
        assert_eq!(run.app.pending_task.as_ref().unwrap().input, version);
        run.step();
        assert_eq!(run.app.pending_task.as_ref().unwrap().input, version);
        run.step();
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert_eq!(
            (queue.total, queue.processed, queue.imported, queue.failed),
            (3, 3, 2, 1)
        );
        assert!(queue.errors[0].0.ends_with("bad.gbr"));
        assert!(queue.errors[0].1.contains("VALIDATION_FAILED"));
        assert!(queue.reason.is_none());
        let info = run.model.view.info.as_ref().unwrap();
        assert_eq!(
            (&*info.revision, info.undo_entries, info.layer_ids.len()),
            ("2", 2, 2)
        );
        assert_eq!(
            run.model
                .view
                .layers
                .iter()
                .map(|l| l.display_name.as_str())
                .collect::<Vec<_>>(),
            ["last", "first"]
        );
        run.no_import_pending();
        run.model.run(Action::History(false));
        assert_eq!(run.model.view.layers.len(), 1);
        assert_eq!(run.model.view.layers[0].display_name, "first");
    }

    #[test]
    fn queue_cancel_before_commit_keeps_previous_success_and_skips_following_files() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx", "third.gbr"]);
        run.step();
        run.app.gerber_import.as_mut().unwrap().request_stop();
        run.app.pending_task.as_ref().unwrap().cancel_token.cancel();
        run.step();
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert_eq!((queue.processed, queue.imported, queue.failed), (2, 1, 0));
        assert!(queue.errors[0].1.contains("CANCELLED"));
        assert_eq!(run.model.view.info.as_ref().unwrap().undo_entries, 1);
        assert_eq!(run.model.view.layers.len(), 1);
        run.no_import_pending();
    }

    #[test]
    fn queue_late_cancel_records_committed_file_then_stops() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx", "third.gbr"]);
        run.step();
        let (id, task) = run.work();
        assert_eq!(
            task.cancel_token.cancel(),
            editor_service::task::CancelOutcome::TooLate
        );
        run.app.gerber_import.as_mut().unwrap().request_stop();
        run.reply(id);
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert_eq!((queue.processed, queue.imported, queue.failed), (2, 2, 0));
        assert_eq!(run.model.view.info.as_ref().unwrap().undo_entries, 2);
        run.no_import_pending();
    }

    #[test]
    fn queue_commit_marker_cannot_reuse_previous_import_on_stale_task() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx", "third.gbr"]);
        run.step();
        let info = run.model.view.info.clone().unwrap();
        run.model
            .service
            .create_empty_layer(&info.document_id, &info.revision, Default::default())
            .unwrap();
        let before = run.model.service.document_get(&info.document_id).unwrap();
        let (id, _) = run.work();
        assert_eq!(run.model.view.error.as_ref().unwrap().code, "STALE_TASK");
        assert!(run.model.view.import.is_none() && run.model.view.import_committed_task.is_none());
        run.reply(id);
        assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 1);
        assert_eq!(
            run.model.service.document_get(&info.document_id).unwrap(),
            before
        );
        run.no_import_pending();
    }

    #[test]
    fn queue_post_commit_refresh_fault_records_success_and_never_reimports() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx"]);
        let (id, _) = run.work();
        assert_eq!(run.model.view.import_committed_task, Some(id));
        // Fault injection after a real commit: a refresh error must not be
        // interpreted as a failed import or retried using the same file.
        run.model.view.error = Some(editor_service::ServiceError {
            code: "DISPLAY_TEST".into(),
            message: "synthetic refresh failure".into(),
            details: serde_json::json!({}),
        });
        run.model.view.task_receipt.as_mut().unwrap().state = TaskState::Failed;
        run.reply(id);
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert_eq!((queue.processed, queue.imported, queue.failed), (1, 1, 0));
        assert!(queue.errors[0].1.contains("已导入，显示刷新失败"));
        assert_eq!(run.model.view.info.as_ref().unwrap().undo_entries, 1);
        run.no_import_pending();
    }

    #[test]
    fn queue_rejects_invalid_receipt_and_disconnected_worker_without_dispatching_next() {
        for disconnect in [false, true] {
            let mut run = Run::new();
            run.start(&["first.custom", "last.gbx"]);
            if disconnect {
                assert!(
                    matches!(run.requests.try_recv().unwrap().2, Action::ImportGerbers(paths) if paths.len() == 1)
                );
                let (_tx, rx) = sync_channel(1);
                run.app.rx = rx;
                drop(_tx);
                run.update(run.raw());
            } else {
                let (id, _) = run.work();
                run.model.view.task_receipt.as_mut().unwrap().task_id += 1;
                run.reply(id);
            }
            assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 0);
            assert!(!run.app.busy);
            run.no_import_pending();
        }
    }

    #[test]
    fn queue_file_bound_and_repeated_drops_do_not_replace_active_intent() {
        let mut run = Run::new();
        run.app.drop_files(vec![
            run.dir.join("first.custom");
            editor_service::MAX_IMPORT_FILES + 1
        ]);
        assert!(run.app.ui_error.is_some() && run.app.gerber_import.is_none());
        assert!(run.requests.try_recv().is_err());
        run.start(&["first.custom", "last.gbx"]);
        run.app.drop_files(vec![run.dir.join("third.gbr")]);
        run.app.drop_files(vec![run.dir.join("other.rcam")]);
        assert_eq!(run.app.gerber_import.as_ref().unwrap().total, 2);
        assert!(run.app.transition.is_none() && run.app.ui_error.is_some());
        run.step();
        run.step();
        assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 2);
        run.no_import_pending();
    }

    #[test]
    fn queue_first_failure_and_existing_dirty_project_preserve_document_identity() {
        for existing in [false, true] {
            let mut run = Run::new();
            if existing {
                run.model
                    .run(Action::ImportGerbers(vec![run.dir.join("first.custom")]));
                assert!(run.model.view.error.is_none());
                run.app.view = run.model.view.clone();
                run.app.routing.bind_fixture(&run.app.view);
            }
            let before = run.model.view.info.clone();
            run.start(&["bad.gbr", "last.gbx"]);
            run.step();
            assert_eq!(run.model.view.info, before);
            assert_eq!(
                run.app.pending_task.as_ref().unwrap().input,
                run.model.task_version().unwrap()
            );
            run.step();
            let queue = run.app.gerber_import.as_ref().unwrap();
            assert_eq!((queue.processed, queue.imported, queue.failed), (2, 1, 1));
            let after = run.model.view.info.as_ref().unwrap();
            assert!(after.project_dirty);
            assert_eq!(after.layer_ids.len(), 1 + usize::from(existing));
            if let Some(before) = before {
                assert_eq!(after.document_id, before.document_id);
                assert_eq!(after.project_id, before.project_id);
                assert_eq!(after.project_path, before.project_path);
            }
            run.no_import_pending();
        }
    }

    #[test]
    fn import_picker_queue_rejects_project_paths_before_any_import() {
        let mut run = Run::new();
        for paths in [
            vec!["empty.RCAM"],
            vec!["first.custom", "empty.rcam"],
            vec!["first.rcam", "second.RCAM"],
        ] {
            run.app
                .start_gerber_import(paths.iter().map(|p| run.dir.join(p)).collect());
            assert!(run.app.ui_error.is_some() && run.app.gerber_import.is_none());
            assert!(run.requests.try_recv().is_err());
            assert!(run.model.view.info.is_none());
        }
    }

    #[test]
    fn queue_first_cancel_keeps_empty_document_and_does_not_start_next_file() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx"]);
        run.app.gerber_import.as_mut().unwrap().request_stop();
        run.app.pending_task.as_ref().unwrap().cancel_token.cancel();
        run.step();
        assert!(run.model.view.info.is_none() && run.app.view.info.is_none());
        assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 0);
        run.no_import_pending();
    }

    #[test]
    fn queue_transition_waits_for_terminal_reply_then_uses_existing_dirty_confirmation() {
        for quit in [false, true] {
            let mut run = Run::new();
            run.start(&["first.custom", "last.gbx", "third.gbr"]);
            run.step();
            run.app.begin_transition(if quit {
                crate::project_ui::Transition::Quit
            } else {
                crate::project_ui::Transition::Open(run.dir.join("other.rcam"))
            });
            assert!(!run.app.close_prompt && run.app.transition.is_none());
            // Other toolbar writes/reads cannot interleave during the stop gap.
            let sequence = run.app.sequence;
            run.app.send(Action::NewWorkspace);
            assert_eq!(run.app.sequence, sequence);
            run.step();
            assert!(run.app.close_prompt);
            assert!(!run.app.allow_quit);
            assert!(if quit {
                matches!(
                    run.app.transition,
                    Some(crate::project_ui::Transition::Quit)
                )
            } else {
                matches!(
                    run.app.transition,
                    Some(crate::project_ui::Transition::Open(_))
                )
            });
            assert_eq!(run.model.view.info.as_ref().unwrap().undo_entries, 1);
            assert_eq!(run.model.view.layers.len(), 1);
            run.no_import_pending();
        }
    }

    #[test]
    fn queue_rejected_old_view_after_real_commit_reports_success_and_cancels_deferred_close() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx"]);
        let (id, _) = run.work();
        run.app
            .begin_transition(crate::project_ui::Transition::Quit);
        // A real committed import, but a fault leaves the pre-import View info.
        run.model.view.info = None;
        run.model.view.error = Some(editor_service::ServiceError {
            code: "DISPLAY_TEST".into(),
            message: "synthetic old view".into(),
            details: serde_json::json!({}),
        });
        run.model.view.task_receipt.as_mut().unwrap().state = TaskState::Failed;
        run.reply(id);
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert_eq!((queue.processed, queue.imported, queue.failed), (1, 1, 0));
        assert!(queue.reason.as_ref().unwrap().contains("勿重复导入"));
        assert!(!run.app.allow_quit && run.app.transition.is_none() && !run.app.close_prompt);
        run.no_import_pending();
    }

    #[test]
    fn queue_prompt_guard_prevents_dispatch_during_reply_gap() {
        let mut run = Run::new();
        run.app
            .start_gerber_import(vec![run.dir.join("first.custom"), run.dir.join("last.gbx")]);
        let (id, _) = run.work();
        assert!(crate::task_reply_matches(
            run.app.pending_task.as_ref().unwrap(),
            &run.app.view,
            &run.model.view
        ));
        run.install_import_gap(id);
        run.app.busy = false;
        run.app.pending_task = None;
        run.app.accept_gerber_import_reply(id);
        run.app.close_prompt = true;
        run.app.advance_gerber_import();
        assert_eq!(run.app.gerber_import.as_ref().unwrap().imported, 1);
        assert!(run.app.close_prompt);
        run.no_import_pending();
    }

    #[test]
    fn finished_queue_keeps_first_transition_until_handoff() {
        let mut run = Run::new();
        run.start(&["first.custom", "last.gbx"]);
        let first = run.dir.join("first.rcam");
        run.app
            .begin_transition(crate::project_ui::Transition::Open(first.clone()));
        let (id, _) = run.work();
        assert!(crate::task_reply_matches(
            run.app.pending_task.as_ref().unwrap(),
            &run.app.view,
            &run.model.view
        ));
        run.install_import_gap(id);
        run.app.busy = false;
        run.app.pending_task = None;
        run.app.accept_gerber_import_reply(id);
        let queue = run.app.gerber_import.as_ref().unwrap();
        assert!(queue.finished && queue.active());
        assert!(run.app.command_context_blocked());
        let sequence = run.app.sequence;
        run.app
            .begin_transition(crate::project_ui::Transition::Open(
                run.dir.join("second.rcam"),
            ));
        run.app.send(Action::NewWorkspace);
        assert_eq!(run.app.sequence, sequence);
        assert!(run.requests.try_recv().is_err());
        // The same active predicate hides the progress panel's Close button.
        run.app.advance_gerber_import();
        assert!(!run.app.gerber_import.as_ref().unwrap().active());
        let (_, _, action, _, _) = run.requests.try_recv().unwrap();
        assert!(matches!(action, Action::OpenProject(path, _) if path == first));
        assert!(run.requests.try_recv().is_err());
    }
}
