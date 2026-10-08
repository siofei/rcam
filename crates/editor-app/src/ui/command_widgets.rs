//! Thin UI descriptors for the existing command registry. This does not
//! dispatch business actions; it gives menus/toolbars one source for the
//! current label, icon, shortcut hint and checked/enabled widget state.
use super::icons::RcamIcon;
use editor_core::command::{CommandId, ids};
use eframe::egui;

#[derive(Clone, Copy)]
pub struct CommandUi {
    pub id: CommandId,
    pub label: &'static str,
    pub icon: RcamIcon,
    pub checked_label: Option<&'static str>,
}

#[derive(Clone, Copy, Default)]
pub struct CommandState {
    pub enabled: bool,
    pub checked: bool,
}

impl CommandState {
    pub const fn enabled(enabled: bool) -> Self {
        Self {
            enabled,
            checked: false,
        }
    }
}

pub fn descriptor(id: CommandId) -> CommandUi {
    let (label, icon, checked_label) = match id {
        ids::FILE_NEW_PROJECT => ("新建工程", RcamIcon::Add, None),
        ids::FILE_OPEN_PROJECT => ("打开工程…", RcamIcon::Import, None),
        ids::FILE_SAVE_PROJECT => ("保存工程", RcamIcon::Export, None),
        ids::FILE_SAVE_PROJECT_AS => ("工程另存为…", RcamIcon::Export, None),
        ids::FILE_CLOSE_PROJECT => ("关闭工程", RcamIcon::Delete, None),
        ids::FILE_IMPORT_GERBER => ("导入 Gerber…（可多选）", RcamIcon::Import, None),
        ids::FILE_EXPORT_GERBER => ("导出当前图层为 Gerber…", RcamIcon::Export, None),
        ids::EDIT_UNDO => ("撤销", RcamIcon::Undo, None),
        ids::EDIT_REDO => ("重做", RcamIcon::Redo, None),
        ids::EDIT_DELETE => ("删除对象", RcamIcon::Delete, None),
        ids::EDIT_DUPLICATE => ("原位复制", RcamIcon::Duplicate, None),
        ids::VIEW_GRID_TOGGLE => ("显示网格", RcamIcon::Grid, None),
        ids::SNAP_TOGGLE => ("Object Snap", RcamIcon::Snap, None),
        ids::TOOL_MEASURE => ("测距", RcamIcon::Measure, None),
        ids::TOOL_TEXT => ("文本…", RcamIcon::Text, None),
        ids::LAYER_CREATE => ("新建空图层", RcamIcon::Add, None),
        ids::LAYER_DELETE => ("删除当前图层…", RcamIcon::Delete, None),
        ids::LAYER_SOLO => ("独奏此图层", RcamIcon::Solo, Some("取消独奏")),
        _ => {
            let command = editor_core::command::standard_commands()
                .into_iter()
                .find(|c| c.id == id)
                .expect("known command widget");
            (command.name, RcamIcon::Add, None)
        }
    };
    CommandUi {
        id,
        label,
        icon,
        checked_label,
    }
}

pub fn label(id: CommandId, checked: bool) -> String {
    let descriptor = descriptor(id);
    debug_assert_eq!(descriptor.id, id);
    let _icon = descriptor.icon;
    let label = if checked {
        descriptor.checked_label.unwrap_or(descriptor.label)
    } else {
        descriptor.label
    };
    label.into()
}

pub fn install_shortcuts(ctx: &egui::Context, config: &crate::shortcut_config::Config) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("current-shortcut-hints"), config.clone()));
}
pub(crate) fn hint(ui: &egui::Ui, id: CommandId) -> String {
    let hint = ui
        .ctx()
        .data(|data| {
            data.get_temp::<crate::shortcut_config::Config>(egui::Id::new("current-shortcut-hints"))
        })
        .unwrap_or_else(|| {
            crate::shortcut_config::Config::defaults(editor_core::command::Platform::current())
        });
    let keys = hint.effective_shortcuts(hint.entry(id), editor_core::command::Platform::current());
    if keys.is_empty() {
        String::new()
    } else {
        crate::shortcut_settings::display_keys(&keys, editor_core::command::Platform::current())
    }
}
fn effective_label(ui: &egui::Ui, id: CommandId, checked: bool) -> String {
    labeled_hint(ui, id, &label(id, checked))
}
fn labeled_hint(ui: &egui::Ui, id: CommandId, label: &str) -> String {
    let hint = hint(ui, id);
    if hint.is_empty() {
        label.into()
    } else {
        format!("{label}  {hint}")
    }
}
pub(crate) fn button_labeled(
    ui: &mut egui::Ui,
    id: CommandId,
    label: &str,
    state: CommandState,
) -> egui::Response {
    ui.add_enabled(
        state.enabled,
        egui::Button::new(labeled_hint(ui, id, label)).selected(state.checked),
    )
}
pub(crate) fn compact_button(
    ui: &mut egui::Ui,
    id: CommandId,
    label: &str,
    state: CommandState,
) -> egui::Response {
    let response = if state.checked {
        ui.add_enabled(state.enabled, egui::Button::new(label).selected(true))
    } else {
        super::buttons::toolbar(ui, label, state.enabled)
    };
    response.on_hover_text(labeled_hint(ui, id, label))
}

pub fn button(ui: &mut egui::Ui, id: CommandId, state: CommandState) -> egui::Response {
    ui.add_enabled(
        state.enabled,
        egui::Button::new(effective_label(ui, id, state.checked)),
    )
}

pub fn checkbox(
    ui: &mut egui::Ui,
    id: CommandId,
    checked: &mut bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Checkbox::new(checked, effective_label(ui, id, *checked)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_tracks_current_snapshot_and_cleared_aliases() {
        let ctx = egui::Context::default();
        let mut config =
            crate::shortcut_config::Config::defaults(editor_core::command::Platform::MacOs);
        let next = config
            .replace(
                ids::EDIT_DUPLICATE,
                vec![],
                editor_core::command::Platform::MacOs,
            )
            .unwrap();
        config = next.config;
        install_shortcuts(&ctx, &config);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert_eq!(effective_label(ui, ids::EDIT_DUPLICATE, false), "原位复制");
                assert!(effective_label(ui, ids::EDIT_REDO, false).contains(" / "));
            });
        });
    }
    #[test]
    fn required_closeout_commands_have_stable_ui_descriptors() {
        for id in [
            ids::FILE_IMPORT_GERBER,
            ids::FILE_EXPORT_GERBER,
            ids::EDIT_UNDO,
            ids::EDIT_REDO,
            ids::EDIT_DELETE,
            ids::EDIT_DUPLICATE,
            ids::VIEW_GRID_TOGGLE,
            ids::SNAP_TOGGLE,
            ids::TOOL_MEASURE,
            ids::TOOL_TEXT,
            ids::LAYER_CREATE,
            ids::LAYER_DELETE,
            ids::LAYER_SOLO,
        ] {
            let descriptor = descriptor(id);
            assert_eq!(descriptor.id, id);
            assert!(!descriptor.label.is_empty());
            let _ = descriptor.icon.glyph();
        }
        assert_eq!(label(ids::LAYER_SOLO, false), "独奏此图层");
        assert_eq!(label(ids::LAYER_SOLO, true), "取消独奏");
        assert_eq!(label(ids::EDIT_DUPLICATE, false), "原位复制");
    }
}
