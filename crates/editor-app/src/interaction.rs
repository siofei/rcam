//! App preferences and screen overlays, never manufacturing commands.
use crate::EditorApp;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cursor {
    #[default]
    Normal,
    SmallCross,
    LargeCross,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub drag_move: bool,
    pub grip_edit: bool,
    pub cursor: Cursor,
}
impl Default for Preferences {
    fn default() -> Self {
        // Migration policy: retain the pre-C mouse behavior. Users can disable
        // each gesture independently; this is not a claim of user preference.
        Self {
            drag_move: true,
            grip_edit: true,
            cursor: Cursor::Normal,
        }
    }
}
impl EditorApp {
    pub(crate) fn fence_mouse_preferences(&mut self) {
        let preferences = self.prefs.interaction;
        if !preferences.drag_move
            && self
                .drag
                .as_ref()
                .is_some_and(crate::drag::Gesture::movement_armed)
        {
            self.drag = None;
            self.object_snap_runtime.reset();
        }
        if !preferences.grip_edit && self.grip.take().is_some() {
            self.object_snap_runtime.reset();
        }
    }
    pub(crate) fn set_interaction_preferences(&mut self, preferences: Preferences) {
        if self.prefs.interaction != preferences {
            self.prefs.interaction = preferences;
            // Called in the menu frame before the canvas handles release.
            self.fence_mouse_preferences();
            if let Some(path) = crate::preferences::AppPreferences::path()
                && let Err(error) = self.prefs.save(&path)
            {
                self.ui_error = Some(format!("交互设置保存失败：{error}"));
            }
        }
    }
    pub(crate) fn interaction_controls(&mut self, ui: &mut egui::Ui) {
        let mut p = self.prefs.interaction;
        ui.label("鼠标编辑（独立开关）");
        let drag = ui.checkbox(&mut p.drag_move, "鼠标拖动移动");
        #[cfg(feature = "internal-evidence")]
        crate::native_i1::widget("interaction-drag", &drag);
        drag.on_hover_text(
            "关闭后仍可点击/框选、导航和使用明确的移动命令；关闭立即取消鼠标移动预览。",
        );
        let grip = ui.checkbox(&mut p.grip_edit, "鼠标握柄改变形状");
        #[cfg(feature = "internal-evidence")]
        crate::native_i1::widget("interaction-grip", &grip);
        grip.on_hover_text(
            "关闭后隐藏并禁用鼠标 Grip；数值和明确命令仍可用。关闭取消正在进行的 Grip。",
        );
        ui.separator();
        ui.label("画布光标");
        for (value, label, _tag) in [
            (Cursor::Normal, "普通", "cursor-normal"),
            (Cursor::SmallCross, "小十字", "cursor-small"),
            (Cursor::LargeCross, "大十字", "cursor-large"),
        ] {
            let response = ui.selectable_value(&mut p.cursor, value, label);
            #[cfg(feature = "internal-evidence")]
            crate::native_i1::widget(_tag, &response);
            let _ = response;
        }
        self.set_interaction_preferences(p);
    }
}
/// Logical screen endpoints for a physical-pixel cross. Geometry is unrelated
/// to zoom, manufacturing coordinates or renderer meshes.
pub(crate) fn segments(style: Cursor, point: Pos2, rect: Rect, ppp: f32) -> Vec<[Pos2; 2]> {
    if style == Cursor::Normal || !ppp.is_finite() || ppp <= 0. || !rect.contains(point) {
        return vec![];
    }
    let half = 18. / ppp;
    let (left, right, top, bottom) = if style == Cursor::LargeCross {
        (rect.left(), rect.right(), rect.top(), rect.bottom())
    } else {
        (
            (point.x - half).max(rect.left()),
            (point.x + half).min(rect.right()),
            (point.y - half).max(rect.top()),
            (point.y + half).min(rect.bottom()),
        )
    };
    vec![
        [Pos2::new(left, point.y), Pos2::new(right, point.y)],
        [Pos2::new(point.x, top), Pos2::new(point.x, bottom)],
    ]
}
pub(crate) fn paint_cursor(
    ctx: &egui::Context,
    response: &egui::Response,
    painter: &egui::Painter,
    style: Cursor,
    modal: bool,
) -> bool {
    let point = ctx.input(|i| i.focused.then(|| i.pointer.hover_pos()).flatten());
    let Some(point) = point.filter(|p| {
        !modal
            && response.contains_pointer()
            && response.rect.contains(*p)
            && ctx.layer_id_at(*p) == Some(response.layer_id)
    }) else {
        return false;
    };
    let lines = segments(style, point, response.rect, ctx.pixels_per_point());
    if lines.is_empty() {
        return false;
    }
    ctx.set_cursor_icon(egui::CursorIcon::None);
    let clipped = painter.with_clip_rect(painter.clip_rect().intersect(response.rect));
    for line in lines {
        clipped.line_segment(
            line,
            Stroke::new(1. / ctx.pixels_per_point(), Color32::LIGHT_GRAY),
        );
    }
    true
}
