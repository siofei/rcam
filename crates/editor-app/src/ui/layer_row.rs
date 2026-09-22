//! The compact layer row (S4-B1), extracted into its own component (S4-B2
//! Final Closeout UI Component Foundation) without any behavior change:
//! same 240-480 resize, UTF-8 ellipsis, tooltips and compact layout as
//! before — only the file it lives in changed. Still an inherent method on
//! `EditorApp` (Rust allows an `impl Type` block to live in any module of
//! the crate), so the call site in `layer_panel.rs` is untouched.
use crate::EditorApp;
use crate::layer_panel::{
    RowEvent, RowUpdate, color32, display_mode_glyph, display_mode_label, layer_menu,
};
use editor_core::workspace::LayerDisplayMode;
use editor_service::LayerInfo;
use eframe::egui::{self, Color32, RichText};
use std::cell::RefCell;

impl EditorApp {
    pub(crate) fn layer_row(
        &self,
        ui: &mut egui::Ui,
        l: &LayerInfo,
        index: usize,
        count: usize,
        busy: bool,
        events: &RefCell<Vec<RowEvent>>,
    ) {
        let fill = if l.is_active {
            crate::ui::tokens::selection_highlight()
        } else {
            Color32::TRANSPARENT
        };
        let dim = !l.effective_visible;
        // Native-evidence probe (opt-in through an environment variable): the exact
        // screen rectangle of every row control, used to prove nothing overlaps.
        let probing = self.probe.is_some();
        let rects: RefCell<Vec<(&'static str, egui::Rect)>> = RefCell::new(Vec::new());
        let note = |key: &'static str, rect: egui::Rect| {
            if probing {
                rects.borrow_mut().push((key, rect));
            }
        };
        let tooltip_shown = std::cell::Cell::new(false);
        let row = egui::Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(3, 2))
            .corner_radius(crate::ui::tokens::ROW_CORNER_RADIUS)
            .show(ui, |ui| {
                egui::Sides::new().shrink_left().truncate().show(
                    ui,
                    |ui| {
                        // Explicit Active Layer indicator (not only a tint / bold name).
                        let (glyph, tip) = if l.is_active {
                            (
                                RichText::new(crate::ui::icons::RcamIcon::ActiveLayer.glyph())
                                    .color(Color32::from_rgb(100, 180, 255)),
                                "当前图层",
                            )
                        } else {
                            (
                                RichText::new(crate::ui::icons::RcamIcon::InactiveLayer.glyph())
                                    .weak(),
                                "点击设为当前图层",
                            )
                        };
                        let indicator = ui
                            .add_enabled(!busy, egui::Button::new(glyph).frame(false))
                            .on_hover_text(tip);
                        note("indicator", indicator.rect);
                        if indicator.clicked() && !l.is_active {
                            events
                                .borrow_mut()
                                .push(RowEvent::Activate(l.layer_id.clone()));
                        }
                        let drag = ui.dnd_drag_source(
                            egui::Id::new(("layer-drag", &l.layer_id)),
                            l.layer_id.clone(),
                            |ui| {
                                ui.label(crate::ui::icons::RcamIcon::DragHandle.glyph());
                            },
                        );
                        note("drag", drag.response.rect);
                        drag.response
                            .on_hover_text("拖动调整图层顺序（上方图层压在下方图层之上）");
                        let swatch =
                            egui::Button::new("")
                                .fill(color32(l.base_color))
                                .min_size(egui::vec2(
                                    crate::ui::tokens::ROW_SWATCH_SIZE,
                                    crate::ui::tokens::ROW_SWATCH_SIZE,
                                ));
                        let swatch = ui.add_enabled(!busy, swatch).on_hover_text(format!(
                            "颜色 {}（点击打开图层设置）",
                            l.base_color.to_hex()
                        ));
                        note("color", swatch.rect);
                        if swatch.clicked() {
                            events
                                .borrow_mut()
                                .push(RowEvent::Settings(l.layer_id.clone()));
                        }
                        let mut name = RichText::new(&l.display_name);
                        if l.is_active {
                            name = name.strong();
                        }
                        if dim {
                            name = name.weak();
                        }
                        // The clickable area is the whole remaining row, not just the text,
                        // so short names can be hit anywhere to the right of them.
                        let width = ui.available_width().max(0.);
                        let height = ui.spacing().interact_size.y;
                        let (name_rect, _) =
                            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                        note("name", name_rect);
                        if probing {
                            let galley = ui.painter().layout_no_wrap(
                                l.display_name.clone(),
                                egui::TextStyle::Body.resolve(ui.style()),
                                Color32::WHITE,
                            );
                            note(
                                "name_text",
                                egui::Rect::from_min_size(name_rect.min, galley.size()),
                            );
                        }
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(name_rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            |ui| {
                                ui.add(egui::Label::new(name).truncate().selectable(false));
                            },
                        );
                        let response = ui
                            .interact(
                                name_rect,
                                egui::Id::new(("layer-name", &l.layer_id)),
                                egui::Sense::click(),
                            )
                            .on_hover_ui(|ui| {
                                tooltip_shown.set(true);
                                ui.strong(&l.display_name);
                                ui.label(format!("{} 个对象", l.object_count));
                                if let Some(p) = &l.provenance {
                                    ui.label(format!("来源文件：{}", p.original_file_name));
                                } else {
                                    ui.label("来源：新建图层");
                                }
                                ui.label(format!(
                                    "{} · {}",
                                    crate::layer_panel::color_mode_label(l.color_mode),
                                    display_mode_label(l.display_mode)
                                ));
                                if !l.selectable {
                                    ui.label("不可选择");
                                }
                                if !l.import_diagnostics.is_empty() {
                                    ui.label(format!(
                                        "解析提示 {} 条（见图层设置）",
                                        l.import_diagnostics.len()
                                    ));
                                }
                                if l.is_solo {
                                    ui.label("独奏中：其他图层被临时隐藏");
                                }
                            });
                        if response.clicked() && !l.is_active {
                            events
                                .borrow_mut()
                                .push(RowEvent::Activate(l.layer_id.clone()));
                        }
                        // Double-click toggles Solo (renaming stays in the menu).
                        if response.double_clicked() {
                            events
                                .borrow_mut()
                                .push(RowEvent::Solo(l.layer_id.clone(), !l.is_solo));
                        }
                        response.context_menu(|ui| {
                            layer_menu(ui, l, index, count, busy, events);
                        });
                    },
                    |ui| {
                        ui.add_enabled_ui(!busy, |ui| {
                            let more = ui
                                .menu_button(crate::ui::icons::RcamIcon::More.glyph(), |ui| {
                                    layer_menu(ui, l, index, count, busy, events)
                                });
                            note("more", more.response.rect);
                        });
                        // Display mode quick control: a small menu (avoids accidental cycling).
                        ui.add_enabled_ui(!busy, |ui| {
                            let mode = ui.menu_button(display_mode_glyph(l.display_mode), |ui| {
                                for candidate in LayerDisplayMode::ALL {
                                    if ui
                                        .selectable_label(
                                            l.display_mode == candidate,
                                            format!(
                                                "{}  {}",
                                                display_mode_glyph(candidate),
                                                display_mode_label(candidate)
                                            ),
                                        )
                                        .clicked()
                                    {
                                        if l.display_mode != candidate {
                                            events.borrow_mut().push(RowEvent::Update(
                                                l.layer_id.clone(),
                                                RowUpdate::DisplayMode(candidate),
                                            ));
                                        }
                                        ui.close();
                                    }
                                }
                            });
                            note("mode", mode.response.rect);
                            mode.response.on_hover_text(format!(
                                "显示模式：{}（点击切换）",
                                display_mode_label(l.display_mode)
                            ));
                        });
                        let toggle = |ui: &mut egui::Ui,
                                      key: &'static str,
                                      on: bool,
                                      glyph: &str,
                                      tip: &str| {
                            let r = ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new(glyph).selected(on).min_size(egui::vec2(
                                        crate::ui::tokens::ROW_TOGGLE_MIN_WIDTH,
                                        0.,
                                    )),
                                )
                                .on_hover_text(tip);
                            note(key, r.rect);
                            r.clicked()
                        };
                        if toggle(
                            ui,
                            "locked",
                            l.locked,
                            crate::ui::icons::RcamIcon::Locked.glyph(),
                            if l.locked {
                                "已锁定（点击解锁）"
                            } else {
                                "未锁定（点击锁定：仅禁止编辑）"
                            },
                        ) {
                            events.borrow_mut().push(RowEvent::Update(
                                l.layer_id.clone(),
                                RowUpdate::Locked(!l.locked),
                            ));
                        }
                        if toggle(
                            ui,
                            "visible",
                            !l.visible,
                            crate::ui::icons::RcamIcon::Visible.glyph(),
                            if l.visible {
                                "可见（点击隐藏）"
                            } else {
                                "已隐藏（点击显示）"
                            },
                        ) {
                            events.borrow_mut().push(RowEvent::Update(
                                l.layer_id.clone(),
                                RowUpdate::Visible(!l.visible),
                            ));
                        }
                        if l.is_solo {
                            let solo = ui
                                .label(
                                    RichText::new(crate::ui::icons::RcamIcon::Solo.glyph())
                                        .strong()
                                        .color(Color32::YELLOW),
                                )
                                .on_hover_text("独奏中");
                            note("solo", solo.rect);
                        }
                    },
                );
            });
        // Drop target: the dragged layer takes this row's position.
        let response = row.response;
        if probing {
            self.record_row_probe(l, response.rect, &rects.borrow(), tooltip_shown.get());
        }
        if response.dnd_hover_payload::<String>().is_some() {
            ui.painter().hline(
                response.rect.x_range(),
                response.rect.top(),
                egui::Stroke::new(2., Color32::from_rgb(100, 180, 255)),
            );
        }
        if let Some(dragged) = response.dnd_release_payload::<String>() {
            events.borrow_mut().push(RowEvent::Drop {
                dragged: (*dragged).clone(),
                target: l.layer_id.clone(),
            });
        }
    }
}
