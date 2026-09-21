//! Layer panel, layer dialogs and the tiered Delete Layer confirmation (S4-B1).
//!
//! Everything here is workspace/view state. Rows only *emit* events; the events
//! are applied to the service through `Action`s after the frame's widgets ran.
use crate::{EditorApp, state::Action};
use editor_core::workspace::{
    Color, ColorMode, DeleteRisk, DisplayClass, LayerDisplayMode, auto_layer_color,
};
use editor_service::{ClassStyleUpdate, LayerInfo, LayerUpdateParams};
use eframe::egui::{self, Color32, RichText};
use std::cell::RefCell;

/// A modal dialog that belongs to one layer. Only one is open at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayerDialog {
    Rename {
        layer: String,
        text: String,
    },
    Settings {
        layer: String,
        name: String,
    },
    Categories {
        layer: String,
    },
    /// Waiting for the service to say whether the layer is empty, clean or dirty.
    DeletePending {
        layer: String,
    },
    Delete {
        layer: String,
        acknowledged: bool,
    },
}

#[derive(Clone, Debug)]
enum RowUpdate {
    Visible(bool),
    Selectable(bool),
    Locked(bool),
    Color(String),
    ColorMode(ColorMode),
    DisplayMode(LayerDisplayMode),
}

#[derive(Clone, Copy, Debug)]
enum MoveTo {
    Up,
    Down,
    Top,
    Bottom,
}

#[derive(Clone, Debug)]
enum RowEvent {
    Activate(String),
    Update(String, RowUpdate),
    Rename(String),
    Settings(String),
    Categories(String),
    Solo(String, bool),
    Fit(String),
    Move(String, MoveTo),
    Drop { dragged: String, target: String },
    Export(String),
    Delete(String),
}

pub(crate) fn display_mode_label(mode: LayerDisplayMode) -> &'static str {
    match mode {
        LayerDisplayMode::Filled => "填充（制造预览）",
        LayerDisplayMode::Outline => "线框（诊断）",
        LayerDisplayMode::ZeroWidth => "零线径（诊断）",
    }
}

/// Most recent colours kept in the picker.
pub(crate) const RECENT_COLOR_LIMIT: usize = 8;

/// Newest first, de-duplicated, bounded. Values that are not a colour (for
/// example the `inherit` marker) are ignored.
pub(crate) fn push_recent_color(list: &mut Vec<String>, hex: &str) {
    let Some(color) = Color::from_hex(hex.trim()) else {
        return;
    };
    let hex = color.to_hex();
    list.retain(|c| *c != hex);
    list.insert(0, hex);
    list.truncate(RECENT_COLOR_LIMIT);
}

/// `12438` → `12,438` (object counts in confirmation dialogs).
pub(crate) fn group_digits(n: impl std::fmt::Display) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Row quick-control glyph: filled / outline / zero-width are clearly distinct.
pub(crate) fn display_mode_glyph(mode: LayerDisplayMode) -> &'static str {
    match mode {
        LayerDisplayMode::Filled => "▣",
        LayerDisplayMode::Outline => "□",
        LayerDisplayMode::ZeroWidth => "─",
    }
}

pub(crate) fn color_mode_label(mode: ColorMode) -> &'static str {
    match mode {
        ColorMode::LayerColor => "图层颜色",
        ColorMode::CategoryColor => "分类颜色",
    }
}

fn color32(color: Color) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

/// The auto palette as a compact 4x4 grid, a full colour picker (colour wheel /
/// HSV square) and a `#rrggbb` field. Returns a picked colour.
fn color_palette(ui: &mut egui::Ui, current: Color, recent: &[String]) -> Option<String> {
    let mut picked = None;
    if !recent.is_empty() {
        // Recent colours (newest first): session-only UI preference.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.;
            ui.label("最近");
            for hex in recent {
                let Some(color) = Color::from_hex(hex) else {
                    continue;
                };
                let mut button = egui::Button::new("")
                    .fill(color32(color))
                    .min_size(egui::vec2(20., 20.));
                if color == current {
                    button = button.stroke(egui::Stroke::new(2., Color32::WHITE));
                }
                if ui.add(button).on_hover_text(hex.as_str()).clicked() {
                    picked = Some(hex.clone());
                }
            }
        });
    }
    // `Grid` widens every column to `interact_size.x` (40 px) unless told otherwise,
    // which spread the 20 px swatches far apart.
    egui::Grid::new(ui.id().with("palette"))
        .spacing([3., 3.])
        .min_col_width(20.)
        .min_row_height(20.)
        .show(ui, |ui| {
            for index in 0..16 {
                let color = auto_layer_color(index);
                let mut button = egui::Button::new("")
                    .fill(color32(color))
                    .min_size(egui::vec2(20., 20.));
                if color == current {
                    button = button.stroke(egui::Stroke::new(2., Color32::WHITE));
                }
                if ui.add(button).on_hover_text(color.to_hex()).clicked() {
                    picked = Some(color.to_hex());
                }
                if index % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    // Colour wheel: the picker changes every frame while dragging, so the colour is
    // applied once the pointer is released (one workspace revision per pick).
    let picker_id = ui.id().with("picker");
    let current_hex = current.to_hex();
    let (base, mut rgb, mut pending): (String, [u8; 3], bool) = ui
        .data_mut(|data| data.get_temp(picker_id))
        .filter(|(base, _, _): &(String, [u8; 3], bool)| *base == current_hex)
        .unwrap_or_else(|| {
            (
                current_hex.clone(),
                [current.r, current.g, current.b],
                false,
            )
        });
    ui.horizontal(|ui| {
        ui.label("色盘");
        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
            pending = true;
        }
    });
    if pending && !ui.input(|i| i.pointer.any_down()) {
        pending = false;
        picked = Some(
            Color {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            }
            .to_hex(),
        );
    }
    ui.data_mut(|data| data.insert_temp(picker_id, (base, rgb, pending)));
    let id = ui.id().with("hex");
    let mut text: String = ui
        .data_mut(|data| data.get_temp(id))
        .unwrap_or_else(|| current.to_hex());
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut text).desired_width(80.));
        let valid = Color::from_hex(text.trim()).is_some();
        if ui.add_enabled(valid, egui::Button::new("应用")).clicked() {
            picked = Some(text.trim().to_lowercase());
        }
    });
    ui.data_mut(|data| data.insert_temp(id, text));
    picked
}

fn layer_menu(
    ui: &mut egui::Ui,
    l: &LayerInfo,
    index: usize,
    count: usize,
    busy: bool,
    events: &RefCell<Vec<RowEvent>>,
) {
    let id = l.layer_id.clone();
    let mut chosen: Option<RowEvent> = None;
    if ui
        .add_enabled(!busy && !l.is_active, egui::Button::new("设为当前图层"))
        .clicked()
    {
        chosen = Some(RowEvent::Activate(id.clone()));
    }
    if ui
        .add_enabled(!busy, egui::Button::new("重命名…"))
        .clicked()
    {
        chosen = Some(RowEvent::Rename(id.clone()));
    }
    if ui
        .add_enabled(!busy, egui::Button::new("图层设置…"))
        .clicked()
    {
        chosen = Some(RowEvent::Settings(id.clone()));
    }
    if ui
        .add_enabled(!busy, egui::Button::new("分类设置…"))
        .clicked()
    {
        chosen = Some(RowEvent::Categories(id.clone()));
    }
    if ui
        .add_enabled(
            !busy,
            egui::Button::new(if l.is_solo {
                "取消独奏"
            } else {
                "独奏此图层"
            }),
        )
        .clicked()
    {
        chosen = Some(RowEvent::Solo(id.clone(), !l.is_solo));
    }
    if ui
        .add_enabled(
            !busy,
            egui::Button::new(if l.selectable {
                "设为不可选择"
            } else {
                "设为可选择"
            }),
        )
        .clicked()
    {
        chosen = Some(RowEvent::Update(
            id.clone(),
            RowUpdate::Selectable(!l.selectable),
        ));
    }
    if ui
        .add_enabled(!busy, egui::Button::new("缩放到此图层"))
        .clicked()
    {
        chosen = Some(RowEvent::Fit(id.clone()));
    }
    ui.separator();
    for (label, to, enabled) in [
        ("上移", MoveTo::Up, index > 0),
        ("下移", MoveTo::Down, index + 1 < count),
        ("移到顶部", MoveTo::Top, index > 0),
        ("移到底部", MoveTo::Bottom, index + 1 < count),
    ] {
        if ui
            .add_enabled(enabled && !busy, egui::Button::new(label))
            .clicked()
        {
            chosen = Some(RowEvent::Move(id.clone(), to));
        }
    }
    ui.separator();
    if ui
        .add_enabled(!busy, egui::Button::new("导出此图层为 Gerber…"))
        .clicked()
    {
        chosen = Some(RowEvent::Export(id.clone()));
    }
    if ui
        .add_enabled(!busy, egui::Button::new("删除图层…"))
        .clicked()
    {
        chosen = Some(RowEvent::Delete(id));
    }
    if let Some(event) = chosen {
        events.borrow_mut().push(event);
        ui.close();
    }
}

impl EditorApp {
    /// Bounded, de-duplicated, newest-first list of colours the user actually
    /// committed. Session-only UI preference: never part of the manufacturing
    /// model or the Gerber writer output.
    pub(crate) fn remember_color(&mut self, hex: &str) {
        push_recent_color(&mut self.recent_colors, hex);
    }

    fn layer_patch(&self, layer: &str) -> Option<LayerUpdateParams> {
        self.view.info.as_ref().map(|d| LayerUpdateParams {
            layer_id: layer.into(),
            expected_workspace_revision: d.workspace_revision.clone(),
            ..Default::default()
        })
    }

    fn send_update(&mut self, layer: &str, update: RowUpdate) {
        let Some(mut patch) = self.layer_patch(layer) else {
            return;
        };
        match update {
            RowUpdate::Visible(v) => patch.visible = Some(v),
            RowUpdate::Selectable(v) => patch.selectable = Some(v),
            RowUpdate::Locked(v) => patch.locked = Some(v),
            RowUpdate::Color(hex) => {
                self.remember_color(&hex);
                patch.base_color = Some(hex);
            }
            RowUpdate::ColorMode(mode) => patch.color_mode = Some(mode),
            RowUpdate::DisplayMode(mode) => patch.display_mode = Some(mode),
        }
        self.send(Action::Layer(patch));
    }

    fn send_class_update(&mut self, layer: &str, update: ClassStyleUpdate) {
        if let Some(mut patch) = self.layer_patch(layer) {
            if let Some(hex) = update.color_override.as_deref() {
                self.remember_color(hex);
            }
            patch.classes = vec![update];
            self.send(Action::Layer(patch));
        }
    }

    /// Reorder helper: `order` is the panel order, top first.
    fn moved_order(&self, layer: &str, to: MoveTo) -> Option<Vec<String>> {
        let mut order: Vec<String> = self
            .view
            .layers
            .iter()
            .map(|l| l.layer_id.clone())
            .collect();
        let index = order.iter().position(|id| id == layer)?;
        let target = match to {
            MoveTo::Up => index.checked_sub(1)?,
            MoveTo::Down => (index + 1 < order.len()).then_some(index + 1)?,
            MoveTo::Top => 0,
            MoveTo::Bottom => order.len() - 1,
        };
        let id = order.remove(index);
        order.insert(target, id);
        Some(order)
    }

    fn apply_row_event(&mut self, event: RowEvent) {
        match event {
            RowEvent::Activate(id) => self.send(Action::SetActiveLayer(Some(id))),
            RowEvent::Update(id, update) => self.send_update(&id, update),
            RowEvent::Rename(id) => self.open_layer_dialog(LayerDialog::Rename {
                text: self
                    .view
                    .layers
                    .iter()
                    .find(|l| l.layer_id == id)
                    .map(|l| l.display_name.clone())
                    .unwrap_or_default(),
                layer: id,
            }),
            RowEvent::Settings(id) => self.open_layer_dialog(LayerDialog::Settings {
                name: self
                    .view
                    .layers
                    .iter()
                    .find(|l| l.layer_id == id)
                    .map(|l| l.display_name.clone())
                    .unwrap_or_default(),
                layer: id,
            }),
            RowEvent::Categories(id) => {
                self.open_layer_dialog(LayerDialog::Categories { layer: id })
            }
            RowEvent::Solo(id, on) => self.send(Action::SetSoloLayer(on.then_some(id))),
            RowEvent::Fit(id) => self.send(Action::FitLayer(id)),
            RowEvent::Move(id, to) => {
                if let Some(order) = self.moved_order(&id, to) {
                    self.send(Action::ReorderLayers(order));
                }
            }
            RowEvent::Drop { dragged, target } => {
                let mut order: Vec<String> = self
                    .view
                    .layers
                    .iter()
                    .map(|l| l.layer_id.clone())
                    .collect();
                if let (Some(from), Some(to)) = (
                    order.iter().position(|id| *id == dragged),
                    order.iter().position(|id| *id == target),
                ) && from != to
                {
                    let id = order.remove(from);
                    order.insert(to, id);
                    self.send(Action::ReorderLayers(order));
                }
            }
            RowEvent::Export(id) => self.export_layer(id),
            RowEvent::Delete(id) => {
                if self.busy {
                    return;
                }
                self.layer_dialog = Some(LayerDialog::DeletePending { layer: id.clone() });
                self.send(Action::LayerSummary(id));
            }
        }
    }

    pub(crate) fn open_layer_dialog(&mut self, dialog: LayerDialog) {
        if self.busy || self.close_prompt || self.modal.is_some() {
            return;
        }
        self.drag = None;
        self.ui_error = None;
        self.view.error = None;
        self.layer_dialog = Some(dialog);
    }

    /// Menu / toolbar entry: create an empty layer.
    pub(crate) fn create_empty_layer(&mut self) {
        self.send(Action::CreateEmptyLayer(None));
    }

    /// Menu / toolbar / drag-drop entry: batch import (each file its own layer).
    pub(crate) fn import_gerbers(&mut self) {
        match crate::platform::choose_gerbers() {
            Ok(Some(paths)) => self.send(Action::ImportGerbers(paths)),
            Ok(None) => {}
            Err(e) => self.ui_error = Some(e),
        }
    }

    /// The Layer panel (left side). Compact single-column rows, top layer first.
    pub(crate) fn layer_panel(&mut self, ui: &mut egui::Ui) {
        let busy = self.busy;
        let events: RefCell<Vec<RowEvent>> = RefCell::new(Vec::new());
        let mut new_layer = false;
        let mut import = false;
        let mut all_visible: Option<bool> = None;
        let any_layers = !self.view.layers.is_empty();
        ui.add_space(6.);
        ui.horizontal(|ui| {
            ui.heading("图层");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.menu_button("＋", |ui| {
                        if ui.button("新建空图层").clicked() {
                            new_layer = true;
                            ui.close();
                        }
                        if ui.button("导入 Gerber…（可多选）").clicked() {
                            import = true;
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("新建空图层 / 导入 Gerber");
                });
                // right_to_left: the last added button is the leftmost one.
                ui.add_enabled_ui(!busy && any_layers, |ui| {
                    if ui
                        .small_button("全隐")
                        .on_hover_text("隐藏所有图层（只改显示，不改制造内容）")
                        .clicked()
                    {
                        all_visible = Some(false);
                    }
                    if ui
                        .small_button("全显")
                        .on_hover_text("显示所有图层（同时结束独奏）")
                        .clicked()
                    {
                        all_visible = Some(true);
                    }
                });
            });
        });
        ui.add_space(4.);
        let layers = self.view.layers.clone();
        if layers.is_empty() {
            ui.label(RichText::new("工作区还没有图层").weak());
            ui.label(
                RichText::new("使用 ＋ 导入 Gerber，或拖入多个文件；也可新建空图层。")
                    .small()
                    .weak(),
            );
        }
        let count = layers.len();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for (index, l) in layers.iter().enumerate() {
                    self.layer_row(ui, l, index, count, busy, &events);
                    ui.add_space(2.);
                }
            });
        if let Some(visible) = all_visible {
            self.send(Action::SetAllLayersVisible(visible));
        }
        if new_layer {
            self.create_empty_layer();
        }
        if import {
            self.import_gerbers();
        }
        for event in events.into_inner() {
            self.apply_row_event(event);
        }
    }

    fn layer_row(
        &self,
        ui: &mut egui::Ui,
        l: &LayerInfo,
        index: usize,
        count: usize,
        busy: bool,
        events: &RefCell<Vec<RowEvent>>,
    ) {
        let fill = if l.is_active {
            Color32::from_rgba_unmultiplied(80, 130, 200, 46)
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
            .corner_radius(3.)
            .show(ui, |ui| {
                egui::Sides::new().shrink_left().truncate().show(
                    ui,
                    |ui| {
                        // Explicit Active Layer indicator (not only a tint / bold name).
                        let (glyph, tip) = if l.is_active {
                            (
                                RichText::new("●").color(Color32::from_rgb(100, 180, 255)),
                                "当前图层",
                            )
                        } else {
                            (RichText::new("○").weak(), "点击设为当前图层")
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
                                ui.label("≡");
                            },
                        );
                        note("drag", drag.response.rect);
                        drag.response
                            .on_hover_text("拖动调整图层顺序（上方图层压在下方图层之上）");
                        let swatch = egui::Button::new("")
                            .fill(color32(l.base_color))
                            .min_size(egui::vec2(14., 14.));
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
                                    color_mode_label(l.color_mode),
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
                            let more = ui.menu_button("⋯", |ui| {
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
                                    egui::Button::new(glyph)
                                        .selected(on)
                                        .min_size(egui::vec2(22., 0.)),
                                )
                                .on_hover_text(tip);
                            note(key, r.rect);
                            r.clicked()
                        };
                        if toggle(
                            ui,
                            "locked",
                            l.locked,
                            "🔒",
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
                            "👁",
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
                                .label(RichText::new("S").strong().color(Color32::YELLOW))
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

    /// Called every frame: drives the layer dialogs and the delete flow.
    pub(crate) fn layer_dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.layer_dialog.clone() else {
            return;
        };
        let layer_id = match &dialog {
            LayerDialog::Rename { layer, .. }
            | LayerDialog::Settings { layer, .. }
            | LayerDialog::Categories { layer }
            | LayerDialog::DeletePending { layer }
            | LayerDialog::Delete { layer, .. } => layer.clone(),
        };
        let Some(l) = self
            .view
            .layers
            .iter()
            .find(|l| l.layer_id == layer_id)
            .cloned()
        else {
            // The layer vanished (for example through Undo): close the dialog.
            self.layer_dialog = None;
            return;
        };
        match dialog {
            LayerDialog::DeletePending { .. } => {
                // Resolved in `accept_layer_replies`; nothing to draw meanwhile.
            }
            LayerDialog::Rename { text, .. } => {
                let mut text = text;
                let mut apply = false;
                let mut cancel = false;
                egui::Modal::new(egui::Id::new("layer-rename")).show(ctx, |ui| {
                    ui.set_width(340_f32.min(ctx.content_rect().width() - 48.).max(180.));
                    ui.heading("重命名图层");
                    let edit =
                        ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
                    edit.request_focus();
                    let valid = !text.trim().is_empty();
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                        if ui
                            .add_enabled(valid && !self.busy, egui::Button::new("应用"))
                            .clicked()
                            || (valid && self.dialog_enter(ui))
                        {
                            apply = true;
                        }
                    });
                    if let Some(error) = &self.view.error {
                        ui.colored_label(
                            Color32::YELLOW,
                            format!("{}: {}", error.code, error.message),
                        );
                    }
                });
                self.layer_dialog = Some(LayerDialog::Rename {
                    layer: layer_id.clone(),
                    text: text.clone(),
                });
                if apply {
                    if let Some(mut patch) = self.layer_patch(&layer_id) {
                        patch.display_name = Some(text.trim().to_string());
                        self.layer_dialog_close_on_success = true;
                        self.send(Action::Layer(patch));
                    }
                } else if cancel {
                    self.layer_dialog = None;
                    self.view.error = None;
                }
            }
            LayerDialog::Settings { name, .. } => {
                let mut name = name;
                let recent = self.recent_colors.clone();
                let mut close = false;
                let mut apply_name = false;
                let mut events: Vec<RowEvent> = Vec::new();
                egui::Modal::new(egui::Id::new("layer-settings")).show(ctx, |ui| {
                    ui.set_width(380_f32.min(ctx.content_rect().width() - 48.).max(200.));
                    ui.heading(format!("图层设置 · {}", l.display_name));
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 180.).max(120.))
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.busy, |ui| {
                                ui.label("名称");
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.));
                                    if ui
                                        .add_enabled(!name.trim().is_empty(), egui::Button::new("应用名称"))
                                        .clicked()
                                    {
                                        apply_name = true;
                                    }
                                });
                                ui.separator();
                                ui.label(format!("图层颜色 {}", l.base_color.to_hex()));
                                if let Some(hex) = color_palette(ui, l.base_color, &recent) {
                                    events.push(RowEvent::Update(layer_id.clone(), RowUpdate::Color(hex)));
                                }
                                ui.separator();
                                let (mut selectable, mut locked) = (l.selectable, l.locked);
                                if ui
                                    .checkbox(&mut selectable, "可选择（关闭后画布点选/框选忽略此图层）")
                                    .changed()
                                {
                                    events.push(RowEvent::Update(
                                        layer_id.clone(),
                                        RowUpdate::Selectable(selectable),
                                    ));
                                }
                                if ui
                                    .checkbox(&mut locked, "锁定（仅禁止编辑，不影响显示与选择）")
                                    .changed()
                                {
                                    events.push(RowEvent::Update(
                                        layer_id.clone(),
                                        RowUpdate::Locked(locked),
                                    ));
                                }
                                ui.separator();
                                let mut color_mode = l.color_mode;
                                ui.horizontal(|ui| {
                                    ui.label("颜色模式");
                                    for candidate in [ColorMode::LayerColor, ColorMode::CategoryColor] {
                                        ui.selectable_value(
                                            &mut color_mode,
                                            candidate,
                                            color_mode_label(candidate),
                                        );
                                    }
                                });
                                if color_mode != l.color_mode {
                                    events.push(RowEvent::Update(
                                        layer_id.clone(),
                                        RowUpdate::ColorMode(color_mode),
                                    ));
                                }
                                let mut display_mode = l.display_mode;
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("显示模式");
                                    for candidate in LayerDisplayMode::ALL {
                                        ui.selectable_value(
                                            &mut display_mode,
                                            candidate,
                                            display_mode_label(candidate),
                                        );
                                    }
                                });
                                if display_mode != l.display_mode {
                                    events.push(RowEvent::Update(
                                        layer_id.clone(),
                                        RowUpdate::DisplayMode(display_mode),
                                    ));
                                }
                                if display_mode != LayerDisplayMode::Filled {
                                    ui.label(
                                        RichText::new("线框/零线径为诊断显示，不代表最终制造填充；不影响导出。")
                                            .small()
                                            .weak(),
                                    );
                                }
                            });
                            ui.separator();
                            ui.label(format!("{} 个对象 · z 顺序 {}", l.object_count, l.z_index + 1));
                            match &l.provenance {
                                Some(p) => {
                                    ui.label(format!("来源文件：{}", p.original_file_name));
                                    ui.label(
                                        RichText::new("仅记录来源，不建立文件关联；导出不会覆盖来源文件。")
                                            .small()
                                            .weak(),
                                    );
                                }
                                None => {
                                    ui.label("来源：新建图层");
                                }
                            }
                            if !l.import_diagnostics.is_empty() {
                                ui.strong("导入解析提示");
                                for line in &l.import_diagnostics {
                                    ui.label(RichText::new(line).small());
                                }
                            }
                        });
                    if ui.button("关闭").clicked() {
                        close = true;
                    }
                });
                self.layer_dialog = Some(LayerDialog::Settings {
                    layer: layer_id.clone(),
                    name: name.clone(),
                });
                if apply_name && let Some(mut patch) = self.layer_patch(&layer_id) {
                    patch.display_name = Some(name.trim().to_string());
                    self.send(Action::Layer(patch));
                } else if let Some(event) = events.into_iter().next() {
                    self.apply_row_event(event);
                }
                if close {
                    self.layer_dialog = None;
                }
            }
            LayerDialog::Categories { .. } => {
                let recent = self.recent_colors.clone();
                let mut close = false;
                let mut events: Vec<(ClassStyleUpdate, bool)> = Vec::new();
                let mut color_mode = l.color_mode;
                let mut reset = false;
                egui::Modal::new(egui::Id::new("layer-categories")).show(ctx, |ui| {
                    ui.set_width(460_f32.min(ctx.content_rect().width() - 48.).max(240.));
                    ui.heading(format!("分类设置 · {}", l.display_name));
                    ui.label(
                        RichText::new("显示 / 可选择 / 锁定 与颜色均只属于工作区视图，不改变制造几何和导出内容。")
                            .small()
                            .weak(),
                    );
                    ui.add_enabled_ui(!self.busy, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("颜色模式");
                            for candidate in [ColorMode::LayerColor, ColorMode::CategoryColor] {
                                ui.selectable_value(&mut color_mode, candidate, color_mode_label(candidate));
                            }
                            if ui.button("全部重置").clicked() {
                                reset = true;
                            }
                        });
                        egui::ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() - 260.).max(120.))
                            .show(ui, |ui| {
                                egui::Grid::new("class-grid").striped(true).show(ui, |ui| {
                                    ui.label("类别");
                                    ui.label("显示");
                                    ui.label("可选");
                                    ui.label("锁定");
                                    ui.label("颜色");
                                    ui.end_row();
                                    for style in &l.classes {
                                        let class: DisplayClass = style.class;
                                        ui.label(class.label());
                                        let (mut visible, mut selectable, mut locked) =
                                            (style.visible, style.selectable, style.locked);
                                        let mut update = ClassStyleUpdate {
                                            class: Some(class),
                                            ..Default::default()
                                        };
                                        let mut changed = false;
                                        if ui.checkbox(&mut visible, "").changed() {
                                            update.visible = Some(visible);
                                            changed = true;
                                        }
                                        if ui.checkbox(&mut selectable, "").changed() {
                                            update.selectable = Some(selectable);
                                            changed = true;
                                        }
                                        if ui.checkbox(&mut locked, "").changed() {
                                            update.locked = Some(locked);
                                            changed = true;
                                        }
                                        let swatch = RichText::new("■").color(color32(style.effective_color));
                                        ui.menu_button(swatch, |ui| {
                                            ui.label(format!("{} 颜色", class.label()));
                                            if let Some(hex) = color_palette(ui, style.effective_color, &recent) {
                                                update.color_override = Some(hex);
                                                changed = true;
                                                ui.close();
                                            }
                                            if ui
                                                .add_enabled(
                                                    style.color_override.is_some(),
                                                    egui::Button::new("恢复自动颜色"),
                                                )
                                                .clicked()
                                            {
                                                update.color_override = Some("inherit".into());
                                                changed = true;
                                                ui.close();
                                            }
                                        });
                                        if changed {
                                            events.push((update, true));
                                        }
                                        ui.end_row();
                                    }
                                });
                            });
                    });
                    if let Some(error) = &self.view.error {
                        ui.colored_label(Color32::YELLOW, format!("{}: {}", error.code, error.message));
                    }
                    if ui.button("关闭").clicked() {
                        close = true;
                    }
                });
                if color_mode != l.color_mode {
                    self.send_update(&layer_id, RowUpdate::ColorMode(color_mode));
                } else if reset {
                    if let Some(mut patch) = self.layer_patch(&layer_id) {
                        patch.reset_classes = true;
                        self.send(Action::Layer(patch));
                    }
                } else if let Some((update, _)) = events.into_iter().next() {
                    self.send_class_update(&layer_id, update);
                }
                if close {
                    self.layer_dialog = None;
                }
            }
            LayerDialog::Delete { acknowledged, .. } => {
                let Some(summary) = self
                    .view
                    .layer_summary
                    .clone()
                    .filter(|s| s.layer_id == layer_id)
                    .or_else(|| {
                        self.pending_summary
                            .clone()
                            .filter(|s| s.layer_id == layer_id)
                    })
                else {
                    self.layer_dialog = None;
                    return;
                };
                let mut acknowledged = acknowledged;
                let mut confirm = false;
                let mut cancel = false;
                let dirty = summary.risk == DeleteRisk::NonEmptyDirty;
                egui::Modal::new(egui::Id::new("layer-delete")).show(ctx, |ui| {
                    ui.set_width(420_f32.min(ctx.content_rect().width() - 48.).max(220.));
                    // Only RCam workspace risk is discussed: a Gerber import is decoupled
                    // from the file on disk, so the disk `.gbr` is never mentioned here.
                    if dirty {
                        ui.heading(format!(
                            "删除包含未保存工程修改的图层“{}”？",
                            summary.display_name
                        ));
                        ui.label(format!(
                            "对象：{}",
                            group_digits(summary.summary.object_count)
                        ));
                        ui.label(format!(
                            "修改/删除的导入对象：{}",
                            group_digits(summary.summary.modified_object_count)
                        ));
                        ui.label(format!(
                            "生成对象：{}",
                            group_digits(summary.summary.generated_object_count)
                        ));
                        ui.add_space(4.);
                        ui.label("这些修改将从当前 RCam 工程中移除。");
                        ui.label("可通过“撤销”恢复。");
                        ui.checkbox(&mut acknowledged, "我了解这些工程修改将被移除");
                    } else {
                        ui.heading(format!("删除图层“{}”？", summary.display_name));
                        ui.label(format!(
                            "对象：{}",
                            group_digits(summary.summary.object_count)
                        ));
                        ui.add_space(4.);
                        ui.label("删除会从当前 RCam 工程中移除此图层。");
                        ui.label("可通过“撤销”恢复。");
                    }
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                        let enabled = !self.busy && (!dirty || acknowledged);
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(
                                    RichText::new("删除图层")
                                        .color(Color32::from_rgb(255, 120, 110)),
                                ),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                    });
                    if let Some(error) = &self.view.error {
                        ui.colored_label(
                            Color32::YELLOW,
                            format!("{}: {}", error.code, error.message),
                        );
                    }
                });
                self.layer_dialog = Some(LayerDialog::Delete {
                    layer: layer_id.clone(),
                    acknowledged,
                });
                if cancel {
                    self.layer_dialog = None;
                    self.pending_summary = None;
                } else if confirm {
                    self.layer_dialog = None;
                    self.pending_summary = None;
                    self.send(Action::RemoveLayer(layer_id, true));
                }
            }
        }
    }

    /// Runs after every worker reply: finishes the delete flow, raises the Undo
    /// toast and follows the active layer.
    pub(crate) fn accept_layer_replies(&mut self, now: std::time::Instant) {
        if let Some(summary) = self.view.layer_summary.clone() {
            let pending = matches!(
                &self.layer_dialog,
                Some(LayerDialog::DeletePending { layer }) if *layer == summary.layer_id
            );
            if pending {
                if summary.risk == DeleteRisk::Empty {
                    // Empty layers are deleted directly; Undo brings them back.
                    self.layer_dialog = None;
                    self.send(Action::RemoveLayer(summary.layer_id, false));
                } else {
                    self.layer_dialog = Some(LayerDialog::Delete {
                        layer: summary.layer_id.clone(),
                        acknowledged: false,
                    });
                    self.pending_summary = Some(summary);
                }
            }
        }
        if let Some(removed) = self.view.removed.clone() {
            self.toast = Some((
                if removed.risk == DeleteRisk::Empty {
                    format!("已删除空图层“{}”", removed.display_name)
                } else {
                    format!("已删除图层“{}”", removed.display_name)
                },
                now,
            ));
        }
        // A rename applied from its dialog closes it once the service accepted it.
        if self.layer_dialog_close_on_success {
            self.layer_dialog_close_on_success = false;
            if self.view.error.is_none() {
                self.layer_dialog = None;
            }
        }
        self.layer = self
            .view
            .layers
            .iter()
            .find(|l| l.is_active)
            .map(|l| l.layer_id.clone());
        if let Some(dialog_layer) = self.layer_dialog.as_ref().map(|d| match d {
            LayerDialog::Rename { layer, .. }
            | LayerDialog::Settings { layer, .. }
            | LayerDialog::Categories { layer }
            | LayerDialog::DeletePending { layer }
            | LayerDialog::Delete { layer, .. } => layer.clone(),
        }) && !self.view.layers.iter().any(|l| l.layer_id == dialog_layer)
        {
            self.layer_dialog = None;
        }
    }
}
