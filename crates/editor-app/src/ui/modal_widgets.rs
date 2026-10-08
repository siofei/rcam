//! Shared modal footer rows (S4-B2 Final Closeout UI Component Foundation).
//! Layout/spacing/labels are unchanged from the dialogs they replace; only
//! the button styling now comes from `ui::buttons` instead of each dialog
//! picking its own colour. A caller that also wants Enter-to-submit ORs its
//! own `dialog_enter(ui)` check into the returned `apply`/`confirm` flag —
//! that check needs `&mut Model`, which this module intentionally does not
//! depend on.
use super::buttons;
use eframe::egui;

/// Viewport-sized dialog chrome is independent of values and validation text.
/// An isolated child cannot grow its parent; overflow remains scrollable.
pub fn fixed_modal<R>(
    ctx: &egui::Context,
    id: egui::Id,
    preferred: egui::Vec2,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    let size = dialog_size(ctx, preferred);
    egui::Modal::new(id).show(ctx, |ui| fixed_content(ui, size, content))
}

pub fn dialog_size(ctx: &egui::Context, preferred: egui::Vec2) -> egui::Vec2 {
    let available = (ctx.content_rect().size() - egui::Vec2::splat(super::tokens::MODAL_MARGIN))
        .max(egui::Vec2::splat(1.));
    egui::vec2(
        super::tokens::modal_width(ctx, preferred.x, 1.),
        preferred.y.min(available.y).max(1.),
    )
}

pub fn fixed_content<R>(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("fixed-dialog-content")
            .max_rect(rect),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().text_edit_width = (size.x - 140.).clamp(40., 240.);
    egui::ScrollArea::both()
        .id_salt("fixed-dialog-scroll")
        .auto_shrink([false, false])
        .max_width(size.x)
        .max_height(size.y)
        .show(&mut child, content)
        .inner
}

/// Reserve a status slot even when empty. Full text is available on hover.
pub fn status_slot(ui: &mut egui::Ui, text: &str, height: f32, warning: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.), height),
        egui::Sense::hover(),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let color = if warning {
        super::tokens::warning_text(ui.visuals())
    } else {
        ui.visuals().text_color()
    };
    child
        .add(egui::Label::new(egui::RichText::new(text).color(color)).wrap())
        .on_hover_text(text);
}

pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_sized(
        egui::vec2(ui.available_width().max(1.), 28.),
        egui::Label::new(egui::RichText::new(text).heading()).truncate(),
    )
    .on_hover_text(text);
}

pub fn fixed_region<R>(
    ui: &mut egui::Ui,
    id: &str,
    height: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let width = ui.available_width().max(1.);
    ui.push_id(id, |ui| {
        fixed_content(ui, egui::vec2(width, height), content)
    })
    .inner
}

#[cfg(test)]
pub fn record_control(ui: &egui::Ui, name: &str, response: &egui::Response) {
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new(name), response.rect));
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_and_reserved_status_keep_input_and_button_rects_stable() {
        for viewport in [egui::vec2(980., 760.), egui::vec2(320., 420.)] {
            let ctx = egui::Context::default();
            let mut baseline = None;
            for (mut value, message) in [
                ("2".to_owned(), "".to_owned()),
                ("500001".to_owned(), "正在检查…".to_owned()),
                ("".to_owned(), "非法值".repeat(200)),
                ("9".repeat(200), "成功统计\n资源错误\n详细错误".repeat(100)),
            ] {
                let mut measured = None;
                for _ in 0..3 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            let response = fixed_modal(
                                ctx,
                                egui::Id::new("layout-regression"),
                                egui::vec2(440., 240.),
                                |ui| {
                                    let input = ui.text_edit_singleline(&mut value).rect;
                                    status_slot(ui, &message, 56., true);
                                    let button = ui.button("Apply").rect;
                                    (input, button)
                                },
                            );
                            measured = Some((response.response.rect, response.inner));
                        },
                    );
                }
                let measured = measured.unwrap();
                if let Some(previous) = baseline {
                    assert_eq!(measured, previous, "viewport={viewport:?}");
                } else {
                    baseline = Some(measured);
                }
                assert!(measured.0.width() <= viewport.x);
                assert!(measured.0.height() <= viewport.y);
            }
        }
    }
}
