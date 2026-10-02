use crate::{EditorApp, state::Action, tools};
use editor_core::command::{CommandId, ids};
use eframe::egui;
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) enum Transition {
    New,
    Open(PathBuf),
    Close,
    Quit,
}

impl EditorApp {
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
        self.close_prompt = false;
        let Some(transition) = self.transition.take() else {
            return;
        };
        match transition {
            Transition::New => self.send(if discard {
                Action::DiscardNewWorkspace
            } else {
                Action::NewWorkspace
            }),
            Transition::Open(path) => self.send(Action::OpenProject(path, discard)),
            Transition::Close | Transition::Quit => {
                self.quit_after_close = matches!(transition, Transition::Quit);
                if self.view.info.is_some() {
                    self.send(Action::Close(discard));
                } else if self.quit_after_close {
                    self.allow_quit = true;
                }
            }
        }
    }

    pub(crate) fn choose_open_project(&mut self) {
        match crate::platform::choose_project(false) {
            Ok(Some(path)) => self.begin_transition(Transition::Open(path)),
            Ok(None) => {}
            Err(error) => self.ui_error = Some(error),
        }
    }

    /// Returns true once a save request or replacement confirmation is active.
    pub(crate) fn save_project(&mut self, as_new: bool) -> bool {
        if self.busy || self.view.info.is_none() {
            return false;
        }
        let camera = Some(rcam_project::CameraState {
            center_mm: self.camera.center,
            scale: self.camera.scale,
        });
        let existing = self
            .view
            .info
            .as_ref()
            .and_then(|d| d.project_path.as_ref());
        if !as_new && existing.is_some() {
            self.send(Action::SaveProject(None, false, camera));
            return self.busy;
        }
        let mut path = match crate::platform::choose_project(true) {
            Ok(Some(path)) => path,
            Ok(None) => return false,
            Err(error) => {
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
            self.ui_error = Some("工程文件必须使用 .rcam 扩展名".into());
            return false;
        }
        if path.exists() && existing.is_none_or(|old| PathBuf::from(old) != path) {
            self.replace_project_path = Some(path);
            return true;
        }
        self.send(Action::SaveProject(Some(path), false, camera));
        self.busy
    }

    pub(crate) fn project_prompts(&mut self, ctx: &egui::Context) {
        if let Some((title, reason)) = self.project_error.clone() {
            egui::Modal::new(egui::Id::new("project-error")).show(ctx, |ui| {
                ui.set_width(crate::ui::tokens::modal_width(ctx, 440., 180.));
                ui.heading(title);
                ui.label(reason);
                if crate::ui::buttons::secondary(ui, "关闭").clicked() {
                    self.project_error = None;
                    self.view.error = None;
                    if self.transition.is_some() {
                        self.close_prompt = true;
                        self.waiting_save = false;
                    }
                }
            });
            return;
        }
        if self.close_prompt {
            egui::Modal::new(egui::Id::new("project-dirty-confirmation")).show(ctx, |ui| {
                ui.set_width(crate::ui::tokens::modal_width(ctx, 440., 180.));
                ui.heading("保存当前工程的更改？");
                ui.label("当前工程包含未保存的修改。");
                ui.horizontal(|ui| {
                    if crate::ui::buttons::secondary(ui, "取消").clicked() {
                        self.close_prompt = false;
                        self.transition = None;
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
            });
        }
        if let Some(path) = self.replace_project_path.clone() {
            egui::Modal::new(egui::Id::new("project-replace-confirmation")).show(ctx, |ui| {
                ui.set_width(crate::ui::tokens::modal_width(ctx, 420., 180.));
                ui.heading("替换现有工程？");
                ui.label(format!(
                    "替换“{}”？",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                ui.horizontal(|ui| {
                    if crate::ui::buttons::secondary(ui, "取消").clicked() {
                        self.replace_project_path = None;
                        if self.waiting_save {
                            self.close_prompt = true;
                            self.waiting_save = false;
                        }
                    }
                    if crate::ui::buttons::destructive(ui, "替换", true).clicked() {
                        self.replace_project_path = None;
                        self.send(Action::SaveProject(
                            Some(path),
                            true,
                            Some(rcam_project::CameraState {
                                center_mm: self.camera.center,
                                scale: self.camera.scale,
                            }),
                        ));
                    }
                });
            });
        }
    }

    pub(crate) fn saved_for_transition(&mut self) {
        if !self.waiting_save {
            return;
        }
        self.waiting_save = false;
        if self.view.error.is_none() && self.view.info.as_ref().is_some_and(|d| !d.project_dirty) {
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
