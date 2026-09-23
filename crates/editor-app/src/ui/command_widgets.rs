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
    pub shortcut_hint: &'static str,
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
    let (label, icon, shortcut_hint, checked_label) = match id {
        ids::FILE_NEW_PROJECT => ("新建工程", RcamIcon::Add, "⌘N", None),
        ids::FILE_OPEN_PROJECT => ("打开工程…", RcamIcon::Import, "⌘O", None),
        ids::FILE_SAVE_PROJECT => ("保存工程", RcamIcon::Export, "⌘S", None),
        ids::FILE_SAVE_PROJECT_AS => ("工程另存为…", RcamIcon::Export, "Shift+⌘S", None),
        ids::FILE_CLOSE_PROJECT => ("关闭工程", RcamIcon::Delete, "⌘W", None),
        ids::FILE_IMPORT_GERBER => ("导入 Gerber…（可多选）", RcamIcon::Import, "⌘I", None),
        ids::FILE_EXPORT_GERBER => ("导出当前图层为 Gerber…", RcamIcon::Export, "Shift+⌘E", None),
        ids::EDIT_UNDO => ("撤销", RcamIcon::Undo, "⌘Z", None),
        ids::EDIT_REDO => ("重做", RcamIcon::Redo, "Shift+⌘Z", None),
        ids::EDIT_DELETE => ("删除对象", RcamIcon::Delete, "⌫", None),
        ids::EDIT_DUPLICATE => ("原位复制", RcamIcon::Duplicate, "⌘D", None),
        ids::VIEW_GRID_TOGGLE => ("显示网格", RcamIcon::Grid, "", None),
        ids::TOOL_MEASURE => ("测距", RcamIcon::Measure, "", None),
        ids::TOOL_TEXT => ("文本…", RcamIcon::Text, "", None),
        ids::LAYER_CREATE => ("新建空图层", RcamIcon::Add, "", None),
        ids::LAYER_DELETE => ("删除当前图层…", RcamIcon::Delete, "", None),
        ids::LAYER_SOLO => ("独奏此图层", RcamIcon::Solo, "", Some("取消独奏")),
        _ => panic!("missing command widget descriptor for {}", id.0),
    };
    CommandUi {
        id,
        label,
        icon,
        shortcut_hint,
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
    if descriptor.shortcut_hint.is_empty() {
        label.into()
    } else {
        format!("{label}  {}", descriptor.shortcut_hint)
    }
}

pub fn button(ui: &mut egui::Ui, id: CommandId, state: CommandState) -> egui::Response {
    ui.add_enabled(state.enabled, egui::Button::new(label(id, state.checked)))
}

pub fn checkbox(
    ui: &mut egui::Ui,
    id: CommandId,
    checked: &mut bool,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled(enabled, egui::Checkbox::new(checked, label(id, *checked)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(label(ids::EDIT_DUPLICATE, false), "原位复制  ⌘D");
    }
}
