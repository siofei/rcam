//! Shared modal footer rows (S4-B2 Final Closeout UI Component Foundation).
//! Layout/spacing/labels are unchanged from the dialogs they replace; only
//! the button styling now comes from `ui::buttons` instead of each dialog
//! picking its own colour. A caller that also wants Enter-to-submit ORs its
//! own `dialog_enter(ui)` check into the returned `apply`/`confirm` flag —
//! that check needs `&mut Model`, which this module intentionally does not
//! depend on.
use super::buttons;
use eframe::egui;

/// Cancel (Secondary) + a labelled Primary action, e.g. rename's "应用".
/// Returns `(cancel_clicked, apply_clicked)`.
pub fn cancel_apply_row(ui: &mut egui::Ui, apply_label: &str, apply_enabled: bool) -> (bool, bool) {
    let mut cancel = false;
    let mut apply = false;
    ui.horizontal(|ui| {
        if buttons::secondary(ui, "取消").clicked() {
            cancel = true;
        }
        if buttons::primary(ui, apply_label, apply_enabled).clicked() {
            apply = true;
        }
    });
    (cancel, apply)
}

/// Cancel (Secondary) + a labelled Destructive action, e.g. delete's
/// "删除图层". Returns `(cancel_clicked, confirm_clicked)`.
pub fn cancel_destructive_row(
    ui: &mut egui::Ui,
    destructive_label: &str,
    destructive_enabled: bool,
) -> (bool, bool) {
    let mut cancel = false;
    let mut confirm = false;
    ui.horizontal(|ui| {
        if buttons::secondary(ui, "取消").clicked() {
            cancel = true;
        }
        if buttons::destructive(ui, destructive_label, destructive_enabled).clicked() {
            confirm = true;
        }
    });
    (cancel, confirm)
}
