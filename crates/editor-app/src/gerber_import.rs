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
    }
}

impl EditorApp {
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
        if self.canvas_selection_unconfirmed
            || self.command_context_blocked()
            || self.selection_read_pending()
            || self.transition.is_some()
            || self.waiting_save
        {
            self.ui_error = Some("请先完成当前操作，再导入文件".into());
            return;
        }
        // Retain the existing file-count bound, and bound queued path storage.
        // No files are read or parsed on the UI thread.
        if paths.len() > editor_service::MAX_IMPORT_FILES
            || paths.iter().any(|p| p.as_os_str().len() > 4096)
        {
            self.ui_error = Some("导入队列最多包含 64 个文件；路径过长时请先调整路径".into());
            return;
        }
        self.gerber_import = Some(ImportQueue {
            owner: self.routing.owner(),
            settled_task: None,
            total: paths.len(),
            pending: paths.into(),
            current: None,
            expected: TaskVersion::capture(
                self.view.info.as_ref(),
                self.view.task_generation,
                self.view.rule_revision,
            ),
            processed: 0,
            imported: 0,
            failed: 0,
            errors: Vec::new(),
            stop_requested: false,
            finished: false,
            reason: None,
            after_stop: None,
        });
        self.advance_gerber_import();
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
        if queue.owner != self.routing.owner() {
            queue.after_stop = None;
            queue.finish(Some("导入工程归属已过期，已停止".into()));
            self.routing.clear_intent();
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
            } else if let Some(path) = queue.pending.pop_front() {
                self.send(Action::ImportGerbers(vec![path.clone()]));
                if let Some(task) = self.pending_task.as_ref().filter(|_| self.busy) {
                    queue.current = Some(Current {
                        owner: self.routing.owner(),
                        path,
                        task_id: task.task_id,
                        input: task.input.clone(),
                    });
                } else {
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
