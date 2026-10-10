use crate::{EditorApp, state::Action, tools};
use editor_core::command::{CommandId, ids};
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) enum Transition {
    #[cfg(test)]
    New,
    #[cfg(any(test, feature = "internal-evidence"))]
    Open(PathBuf),
    Close,
    Quit,
}

impl EditorApp {
    pub(crate) fn drop_files(&mut self, paths: Vec<PathBuf>) {
        if self.unified_editor.is_some() {
            self.ui_error = Some("请先结束当前编辑会话".into());
            return;
        }
        if paths.is_empty() {
            return;
        }
        // A second drop must not replace the intent awaiting save/discard/cancel
        // or mutate the document underneath that confirmation.
        if self.busy
            || self
                .gerber_import
                .as_ref()
                .is_some_and(|queue| queue.active())
            || self.transition.is_some()
            || self.close_prompt
            || self.waiting_save
            || self.replace_project_path.is_some()
            || self.project_error.is_some()
        {
            self.ui_error = Some("请先完成当前工程操作，再拖入文件".into());
            return;
        }
        let projects = paths
            .iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rcam"))
            })
            .count();
        if projects > 0 {
            if paths.len() != 1 {
                self.ui_error =
                    Some("工程文件必须单独拖入；不能与其他工程或 Gerber 混合拖入".into());
                return;
            }
            self.request_new_session(Action::OpenProject(
                paths.into_iter().next().unwrap(),
                false,
            ));
        } else {
            self.start_gerber_import(paths);
        }
    }

    pub(crate) fn cancel_transition(&mut self) {
        self.tabs.cancel_quit(&self.routing.owner());
        self.close_prompt = false;
        self.transition = None;
        self.routing.clear_intent();
    }

    pub(crate) fn dispatch_file_command(&mut self, id: CommandId) {
        match id {
            ids::FILE_NEW | ids::FILE_NEW_PROJECT => self.new_workspace(),
            ids::FILE_OPEN_PROJECT => self.choose_open_project(),
            ids::FILE_SAVE_PROJECT => {
                self.save_project(false);
            }
            ids::FILE_SAVE_PROJECT_AS => {
                self.save_project(true);
            }
            ids::FILE_CLOSE_PROJECT => self.close(false),
            ids::FILE_IMPORT_GERBER => self.import_gerbers(),
            ids::FILE_EXPORT_GERBER => self.save(),
            _ => {}
        }
    }

    pub(crate) fn begin_transition(&mut self, transition: Transition) {
        if self.unified_editor.is_some() {
            self.ui_error = Some("请先结束当前编辑会话".into());
            return;
        }
        if self.transition.is_some()
            || self.close_prompt
            || self.waiting_save
            || self.replace_project_path.is_some()
            || self.project_error.is_some()
        {
            return;
        }
        if let Err(cause) = self.routing.claim_intent() {
            self.ui_error = Some(cause.message);
            return;
        }
        if self.defer_canvas_transition(transition.clone()) {
            return;
        }
        if self.defer_move_place_transition(transition.clone()) {
            return;
        }
        if let Some(queue) = self.gerber_import.as_mut().filter(|q| q.active()) {
            queue.defer_transition(transition);
            if let Some(task) = &self.pending_task {
                task.cancel_token.cancel();
            }
            return;
        }
        self.modal = None;
        self.text.cancel();
        self.tool = tools::ActiveTool::Select;
        self.transition = Some(transition);
        if self.view.info.as_ref().is_some_and(|d| d.project_dirty) {
            self.close_prompt = true;
        } else {
            self.perform_transition(false);
        }
    }

    fn perform_transition(&mut self, discard: bool) {
        if !self.routing.intent_matches() {
            return;
        }
        self.close_prompt = false;
        let Some(transition) = self.transition.take() else {
            return;
        };
        self.routing.clear_intent();
        match transition {
            #[cfg(test)]
            Transition::New => self.send(if discard {
                Action::DiscardNewWorkspace
            } else {
                Action::NewWorkspace
            }),
            #[cfg(any(test, feature = "internal-evidence"))]
            Transition::Open(path) => self.send(Action::OpenProject(path, discard)),
            Transition::Close | Transition::Quit => {
                self.quit_after_close = matches!(transition, Transition::Quit);
                self.send(Action::Close(discard));
                if self.quit_after_close
                    && !self.pending_task.as_ref().is_some_and(|task| {
                        self.tabs.quit_matches(&self.routing.owner(), task.task_id)
                    })
                {
                    // No admitted Close: an early gate or full queue cannot
                    // leave an ownerless window Quit blocking every tab.
                    self.tabs.cancel_quit(&self.routing.owner());
                    self.quit_after_close = false;
                }
            }
        }
    }

    pub(crate) fn choose_open_project(&mut self) {
        match self.native_panel(|| crate::platform::choose_project(false)) {
            Ok(Some(path)) => self.request_new_session(Action::OpenProject(path, false)),
            Ok(None) => {}
            Err(error) => self.ui_error = Some(error),
        }
    }

    fn save_admitted(&mut self) -> bool {
        if self.busy {
            return true;
        }
        if matches!(self.transition, Some(Transition::Quit)) {
            self.tabs.cancel_quit(&self.routing.owner());
            self.transition = None;
            self.close_prompt = false;
            self.waiting_save = false;
            self.quit_after_close = false;
            self.routing.clear_intent();
        } else if self.waiting_save {
            // A rejected replacement has no terminal save receipt. Restore the
            // dirty-document choice instead of waiting forever for that receipt.
            self.waiting_save = false;
            self.close_prompt = self.transition.is_some();
        } else if self.transition.is_none() {
            self.routing.clear_intent();
        }
        false
    }

    /// Returns true once a save request or replacement confirmation is active.
    pub(crate) fn save_project(&mut self, as_new: bool) -> bool {
        if self.busy || self.view.info.is_none() {
            return false;
        }
        if let Err(cause) = self.routing.claim_intent() {
            self.ui_error = Some(cause.message);
            return false;
        }
        let camera = Some(rcam_project::CameraState {
            center_mm: self.camera.center,
            scale: self.camera.scale,
        });
        let existing = self.view.info.as_ref().and_then(|d| d.project_path.clone());
        if !as_new && existing.is_some() {
            self.send(Action::SaveProject(None, false, camera));
            return self.save_admitted();
        }
        let mut path = match self.native_panel(|| crate::platform::choose_project(true)) {
            Ok(Some(path)) => path,
            Ok(None) => {
                if self.transition.is_none() {
                    self.routing.clear_intent();
                }
                return false;
            }
            Err(error) => {
                if self.transition.is_none() {
                    self.routing.clear_intent();
                }
                self.ui_error = Some(error);
                return false;
            }
        };
        if path.extension().is_none() {
            path.set_extension("rcam");
        }
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rcam"))
        {
            if self.transition.is_none() {
                self.routing.clear_intent();
            }
            self.ui_error = Some("工程文件必须使用 .rcam 扩展名".into());
            return false;
        }
        if path.exists() && existing.is_none_or(|old| PathBuf::from(old) != path) {
            self.replace_project_path = Some(path);
            return true;
        }
        self.send(Action::SaveProject(Some(path), false, camera));
        self.save_admitted()
    }

    pub(crate) fn project_prompts(&mut self, ctx: &egui::Context) {
        if let Some((title, reason)) = self.project_error.clone() {
            crate::ui::modal_widgets::fixed_modal(
                ctx,
                egui::Id::new("project-error"),
                egui::vec2(440., 300.),
                |ui| {
                    crate::ui::modal_widgets::heading(ui, &title);
                    crate::ui::modal_widgets::status_slot(ui, &reason, 160., true);
                    let close = crate::ui::buttons::secondary(ui, "关闭");
                    #[cfg(test)]
                    crate::ui::modal_widgets::record_control(
                        ui,
                        "project-error-close-rect",
                        &close,
                    );
                    if close.clicked() {
                        self.project_error = None;
                        self.view.error = None;
                        if self.transition.is_some() {
                            self.close_prompt = true;
                            self.waiting_save = false;
                        }
                    }
                },
            );
            return;
        }
        if self.close_prompt {
            crate::ui::modal_widgets::fixed_modal(
                ctx,
                egui::Id::new("project-dirty-confirmation"),
                egui::vec2(440., 220.),
                |ui| {
                    ui.heading("保存当前工程的更改？");
                    ui.label("当前工程包含未保存的修改。");
                    ui.horizontal(|ui| {
                        if crate::ui::buttons::secondary(ui, "取消").clicked() {
                            self.cancel_transition();
                        }
                        if crate::ui::buttons::destructive(ui, "不保存", true).clicked() {
                            self.perform_transition(true);
                        }
                        if crate::ui::buttons::primary(ui, "保存", true).clicked()
                            && self.save_project(false)
                        {
                            self.waiting_save = true;
                            self.close_prompt = false;
                        }
                    });
                },
            );
        }
        if let Some(path) = self.replace_project_path.clone() {
            crate::ui::modal_widgets::fixed_modal(
                ctx,
                egui::Id::new("project-replace-confirmation"),
                egui::vec2(420., 320.),
                |ui| {
                    ui.heading("替换现有工程？");
                    crate::ui::modal_widgets::status_slot(
                        ui,
                        &format!(
                            "替换“{}”？",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        56.,
                        false,
                    );
                    ui.horizontal(|ui| {
                        if crate::ui::buttons::secondary(ui, "取消").clicked() {
                            self.replace_project_path = None;
                            if !self.waiting_save {
                                self.routing.clear_intent();
                            }
                            if self.waiting_save {
                                self.close_prompt = true;
                                self.waiting_save = false;
                            }
                        }
                        let replace = crate::ui::buttons::destructive(ui, "替换", !self.busy);
                        #[cfg(test)]
                        crate::ui::modal_widgets::record_control(
                            ui,
                            "project-replace-confirm-rect",
                            &replace,
                        );
                        if replace.clicked() {
                            self.replace_project_path = None;
                            self.send(Action::SaveProject(
                                Some(path),
                                true,
                                Some(rcam_project::CameraState {
                                    center_mm: self.camera.center,
                                    scale: self.camera.scale,
                                }),
                            ));
                            self.save_admitted();
                        }
                    });
                },
            );
        }
    }

    pub(crate) fn saved_for_transition(&mut self) {
        if !self.waiting_save {
            return;
        }
        let Some(succeeded) = self.routing.saved_for_intent() else {
            return;
        };
        self.waiting_save = false;
        if succeeded
            && self.view.error.is_none()
            && self.view.info.as_ref().is_some_and(|d| !d.project_dirty)
        {
            self.perform_transition(false);
        } else if self.project_error.is_none() {
            self.close_prompt = true;
        }
    }

    pub(crate) fn restore_project_view(&mut self) {
        let Some(settings) = &self.view.project_workspace else {
            return;
        };
        self.grid = tools::GridSettings {
            spacing_mm: settings.grid.spacing_mm,
            visible: settings.grid.visible,
            snap_enabled: settings.grid.snap,
        };
        self.object_snap = crate::object_snap::Settings::from_project(&settings.snap);
        self.object_snap_runtime.reset();
        self.display_unit = match settings.display_unit {
            rcam_project::DisplayUnit::Millimeters => tools::DisplayUnit::Millimeter,
            rcam_project::DisplayUnit::Inches => tools::DisplayUnit::Inch,
            rcam_project::DisplayUnit::Mils => tools::DisplayUnit::Mil,
            rcam_project::DisplayUnit::Micrometers => tools::DisplayUnit::Micrometer,
        };
        if self.text.change_unit(self.display_unit).is_err() {
            self.text = Default::default();
            self.text
                .change_unit(self.display_unit)
                .expect("default text lengths are valid");
        }
        if let Some(camera) = settings.camera {
            self.camera.center = camera.center_mm;
            self.camera.scale = camera.scale;
            self.fit = false;
        }
    }

    pub(crate) fn persist_project_view(&mut self) {
        if self.busy {
            return;
        }
        let Some(mut settings) = self.view.project_workspace.clone() else {
            return;
        };
        settings.grid.spacing_mm = self.grid.spacing_mm;
        settings.grid.visible = self.grid.visible;
        settings.grid.snap = self.grid.snap_enabled;
        self.object_snap.write_project(&mut settings.snap);
        settings.display_unit = match self.display_unit {
            tools::DisplayUnit::Millimeter => rcam_project::DisplayUnit::Millimeters,
            tools::DisplayUnit::Inch => rcam_project::DisplayUnit::Inches,
            tools::DisplayUnit::Mil => rcam_project::DisplayUnit::Mils,
            tools::DisplayUnit::Micrometer => rcam_project::DisplayUnit::Micrometers,
        };
        self.send(Action::ProjectWorkspace(settings));
    }
}

#[cfg(test)]
mod drop_tests {
    use super::*;
    use editor_service::task::TaskVersion;
    use std::sync::mpsc::{Receiver, sync_channel};

    type Request = crate::session::Request;

    fn app(dirty: bool) -> (EditorApp, Receiver<Request>) {
        let mut app = crate::modal::tests::app();
        let (tx, requests) = sync_channel(8);
        app.tx = tx;
        let mut service = editor_service::ApplicationService::default();
        let info = service.document_new().unwrap();
        if dirty {
            service
                .create_empty_layer(&info.document_id, &info.revision, Default::default())
                .unwrap();
        }
        app.view.info = Some(service.document_get(&info.document_id).unwrap());
        app.view.task_generation = 7;
        app.view.rule_revision = 3;
        app.routing.bind_fixture(&app.view);
        (app, requests)
    }

    fn pending_open(app: &EditorApp, expected: &str) {
        assert!(
            matches!(app.transition.as_ref(), Some(Transition::Open(path)) if path == &PathBuf::from(expected))
        );
    }

    fn advance(app: &mut EditorApp) {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            app.advance_tab_change(ctx);
        });
    }
    #[test]
    fn single_project_drop_opens_independent_record_with_empty_captured_version() {
        for suffix in ["rcam", "RCAM", "rCaM"] {
            for current_document in [false, true] {
                let (mut app, requests) = app(false);
                if !current_document {
                    app.view.info = None;
                    app.routing.bind_fixture(&app.view);
                }
                let before = app.view.info.clone();
                let previous_owner = app.routing.owner();
                let path = PathBuf::from(format!("中文 # empty.{suffix}"));
                app.drop_files(vec![path.clone()]);
                assert!(requests.try_recv().is_err());
                advance(&mut app);
                let (sequence, _, action, task, _) = requests.try_recv().unwrap();
                assert!(matches!(action, Action::OpenProject(actual, false) if actual == path));
                assert_eq!(sequence, 1);
                assert_eq!(task.input, TaskVersion::capture(None, 0, 0));
                assert_ne!(app.routing.owner().slot(), previous_owner.slot());
                assert!(app.busy);
                assert_eq!(app.tabs.parked.len(), 1);
                // The container itself remains private; restoring the old state
                // proves its captured document is retained, not discarded.
                assert_eq!(app.pending_project_error_title, Some("无法打开工程"));
                assert!(app.transition.is_none() && !app.close_prompt);
                assert_eq!(app.tabs.parked[0].view().info, before);
                assert_eq!(before.is_some(), current_document);
            }
        }
    }
    #[test]
    fn dirty_drop_queues_new_record_and_second_drop_cannot_replace_first_intent() {
        let (mut app, requests) = app(true);
        let before = app.view.info.clone();
        app.drop_files(vec!["first.rcam".into()]);
        assert!(!app.close_prompt && !app.busy);
        assert!(app.tabs.change_pending());
        assert!(requests.try_recv().is_err());
        app.drop_files(vec!["second.rcam".into()]);
        assert!(app.ui_error.is_some());
        assert_eq!(app.view.info, before);
        advance(&mut app);
        assert!(
            matches!(requests.try_recv().unwrap().2, Action::OpenProject(path, false) if path == PathBuf::from("first.rcam"))
        );
        assert_eq!(app.tabs.parked.len(), 1);
        assert!(app.view.info.is_none());
        assert!(!app.close_prompt);
    }

    #[test]
    fn dirty_drop_save_success_opens_after_save_and_failure_preserves_intent() {
        for success in [false, true] {
            let (mut app, requests) = app(true);
            app.view.info.as_mut().unwrap().project_path = Some("current.rcam".into());
            // Explicit legacy replacement still exercises original save/discard
            // intent fencing; ordinary drops now create independent records.
            app.begin_transition(Transition::Open("next.rcam".into()));
            assert!(app.save_project(false));
            assert!(matches!(
                requests.try_recv().unwrap().2,
                Action::SaveProject(None, false, Some(_))
            ));
            // These are the existing prompt's state changes and the worker
            // reply's busy release, before saved_for_transition is invoked.
            app.waiting_save = true;
            app.close_prompt = false;
            app.busy = false;
            app.drop_files(vec!["other.rcam".into()]);
            pending_open(&app, "next.rcam");
            assert!(requests.try_recv().is_err());
            app.view.info.as_mut().unwrap().project_dirty = !success;
            if !success {
                app.project_error = Some(("无法保存工程".into(), "write failed".into()));
            }
            app.routing.saved_fixture();
            app.saved_for_transition();
            assert!(!app.waiting_save);
            if success {
                assert!(
                    matches!(requests.try_recv().unwrap().2, Action::OpenProject(path, false) if path == PathBuf::from("next.rcam"))
                );
            } else {
                pending_open(&app, "next.rcam");
                assert!(app.project_error.is_some());
                assert!(app.view.info.as_ref().unwrap().project_dirty);
                assert!(requests.try_recv().is_err());
            }
        }
    }

    #[test]
    fn mixed_and_multiple_project_drops_reject_without_partial_import() {
        for paths in [
            vec!["first.rcam", "second.RCAM"],
            vec!["first.rcam", "layer.gbx"],
            vec!["layer.custom", "first.RCAM"],
        ] {
            let (mut app, requests) = app(true);
            let before = app.view.info.clone();
            app.drop_files(paths.into_iter().map(PathBuf::from).collect());
            assert!(app.ui_error.is_some());
            assert!(!app.busy && !app.close_prompt && app.transition.is_none());
            assert_eq!(app.view.info, before);
            assert_eq!(app.sequence, 0);
            assert!(requests.try_recv().is_err());
        }
    }

    #[test]
    fn pending_project_states_keep_first_intent_and_block_gerber_drops() {
        for state in 0..6 {
            let (mut app, requests) = app(false);
            match state {
                0 => app.busy = true,
                1 => app.transition = Some(Transition::Open("first.rcam".into())),
                2 => app.close_prompt = true,
                3 => app.waiting_save = true,
                4 => app.replace_project_path = Some("save.rcam".into()),
                5 => app.project_error = Some(("original".into(), "failure".into())),
                _ => unreachable!(),
            }
            let before = app.view.info.clone();
            let error = app.project_error.clone();
            let replace = app.replace_project_path.clone();
            for path in ["second.rcam", "layer.custom"] {
                app.drop_files(vec![path.into()]);
                assert!(app.ui_error.is_some());
                assert_eq!(app.project_error, error);
                assert_eq!(app.replace_project_path, replace);
                assert_eq!(app.view.info, before);
                assert_eq!(app.sequence, 0);
                assert!(requests.try_recv().is_err());
                if state == 1 {
                    pending_open(&app, "first.rcam");
                }
            }
        }
    }

    #[test]
    fn gerber_only_drop_starts_sequential_import_in_input_order() {
        let (mut app, requests) = app(true);
        let paths = vec![PathBuf::from("中文 # layer.gbx"), "other.custom".into()];
        app.drop_files(paths.clone());
        assert!(
            matches!(requests.try_recv().unwrap().2, Action::ImportGerbers(actual) if actual == paths[..1])
        );
        assert!(app.busy && !app.close_prompt && app.transition.is_none());
        assert!(app.gerber_import.as_ref().unwrap().active());
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn production_project_error_keeps_close_rect_stable() {
        for viewport in [egui::vec2(980., 760.), egui::vec2(320., 420.)] {
            let ctx = egui::Context::default();
            let mut app = crate::modal::tests::app();
            let mut baseline = None;
            for reason in [
                "".to_owned(),
                "正在处理…".to_owned(),
                "无法保存工程：长错误与路径\n".repeat(200),
            ] {
                app.project_error = Some((
                    "错误".repeat(if reason.len() > 100 { 100 } else { 1 }),
                    reason,
                ));
                for _ in 0..3 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
                            ..Default::default()
                        },
                        |ctx| app.project_prompts(ctx),
                    );
                }
                let rect = ctx
                    .data(|data| {
                        data.get_temp::<egui::Rect>(egui::Id::new("project-error-close-rect"))
                    })
                    .unwrap();
                if let Some(previous) = baseline {
                    assert_eq!(rect, previous);
                } else {
                    baseline = Some(rect);
                }
            }
        }
    }
}
