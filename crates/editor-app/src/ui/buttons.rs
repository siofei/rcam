//! Button semantics (S4-B2 Final Closeout UI Component Foundation):
//! Primary = Apply/Create/Import/Confirm, Secondary = Cancel/Close/Reset,
//! Destructive = Delete/Discard, Toggle = Visible/Locked/Grid/Snap/Mode.
//! Every helper reproduces the exact `egui::Button` call it replaces —
//! styling is centralized, layout and enabled/disabled logic at each call
//! site is untouched.
use super::icons::RcamIcon;
use super::tokens;
use eframe::egui;

/// Apply/Create/Import/Confirm.
pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(label))
}

/// Cancel/Close/Reset. Almost always enabled; `secondary_enabled` covers the
/// rare case (the parameter modal's Cancel, disabled while a request is
/// in flight) that needs to match a specific `enabled` condition.
pub fn secondary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.button(label)
}
pub fn secondary_enabled(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(label))
}

/// Delete/Discard: the shared destructive-red text colour, not a per-dialog
/// literal RGB triplet.
pub fn destructive(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).color(tokens::destructive())),
    )
}

pub fn icon(
    ui: &mut egui::Ui,
    content: impl Into<egui::WidgetText>,
    enabled: bool,
    framed: bool,
) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(content).frame(framed))
}

pub fn toggle_icon(
    ui: &mut egui::Ui,
    icon: RcamIcon,
    enabled: bool,
    selected: bool,
    min_width: f32,
) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(icon.glyph())
            .selected(selected)
            .min_size(egui::vec2(min_width, 0.)),
    )
}

pub fn toolbar(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(label))
}

pub fn compact_action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(label).small())
}
