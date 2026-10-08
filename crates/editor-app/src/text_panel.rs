use crate::{
    EditorApp,
    state::Action,
    text_tool::{self, Placement, Reply, Request},
    tools::ActiveTool,
};
use editor_service::{HorizontalAlign, VerticalAlign};
use eframe::egui;
use std::time::Instant;
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(115.))
            .changed()
    })
    .inner
}

const TEXT_EDIT_ROWS: usize = 4;

fn text_editor(ui: &mut egui::Ui, value: &mut String) -> egui::Response {
    // `TextEdit::desired_rows` is a minimum for multiline editors. Without a
    // bounded inner scroll area the editor grows with every wrapped/new line,
    // pushing the preview/status controls below it on every draft update.
    let height = ui.text_style_height(&egui::TextStyle::Body) * TEXT_EDIT_ROWS as f32 + 4.;
    egui::ScrollArea::vertical()
        .id_salt("manufacturing-text-scroll")
        .max_height(height)
        .min_scrolled_height(height)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(value)
                    .id(egui::Id::new("manufacturing-text"))
                    .desired_rows(TEXT_EDIT_ROWS)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner
}

fn fixed_status_line(ui: &mut egui::Ui, text: &str, small: bool) -> egui::Response {
    let text = if small {
        egui::RichText::new(text).small()
    } else {
        egui::RichText::new(text)
    };
    ui.add_sized(
        [
            ui.available_width(),
            ui.text_style_height(&egui::TextStyle::Body),
        ],
        egui::Label::new(text).truncate(),
    )
}
impl EditorApp {
    pub(crate) fn invalidate_text_overlay(&mut self) {
        if self.text.preview.as_ref().is_some_and(|p| {
            self.tool != ActiveTool::Text
                || self.layer.as_deref() != Some(&p.params.layer_id)
                || self
                    .view
                    .info
                    .as_ref()
                    .is_none_or(|d| d.document_id != p.document_id || d.revision != p.revision)
                || !self
                    .view
                    .layers
                    .iter()
                    .any(|l| l.layer_id == p.params.layer_id && crate::state::text_target_ok(l))
        }) {
            if self.text.floating.is_some() || self.text.pick_reference {
                self.text.cancel();
                self.tool = ActiveTool::Select;
                return;
            }
            self.text.changed();
        }
    }
    pub(crate) fn accept_text_reply(&mut self) {
        let Some(reply) = self.view.text_reply.take() else {
            return;
        };
        match reply.as_ref() {
            Reply::Catalog(result) => match result {
                Ok(fonts) => {
                    self.text.catalog = Some(fonts.clone());
                    self.text.catalog_error = None;
                }
                Err(error) => self.text.catalog_error = Some(error.clone()),
            },
            Reply::Font { generation, result } if *generation == self.text.font_generation => {
                match result {
                    Ok(font) => self.text.accept_font(font.clone()),
                    Err(e) => {
                        self.text.preview = None;
                        self.text.status = format!("{}: {}", e.code, e.message);
                    }
                }
            }
            Reply::Font { .. } => {}
            Reply::Preview {
                request,
                result,
                finished,
                worker_ms,
            } => {
                if !self.text.matches(
                    request,
                    &self.view,
                    self.layer.as_deref(),
                    self.tool == ActiveTool::Text,
                ) {
                    eprintln!("text_preview_drop generation={}", request.generation);
                    return;
                }
                self.text.publish_ms = finished.elapsed().as_secs_f64() * 1000.;
                match result {
                    Ok(result) => {
                        self.text.status = format!(
                            "预览 {} 对象 · worker {:.1} ms · 发布 {:.1} ms",
                            result.geometries.len(),
                            worker_ms,
                            self.text.publish_ms
                        );
                        self.text.preview = Some(result.clone());
                        eprintln!(
                            "text_preview_publish generation={} objects={} worker_ms={worker_ms:.3} publish_ms={:.3} read_ms={:.3} hash_ms={:.3} generation_ms={:.3} offset_ms={:.3}",
                            request.generation,
                            result.geometries.len(),
                            self.text.publish_ms,
                            result.timings.font_read_ms,
                            result.timings.font_hash_ms,
                            result.timings.geometry.generation_ms,
                            result.timings.geometry.offset_ms
                        );
                    }
                    Err(e) => {
                        self.text.preview = None;
                        self.text.status = format!("{}: {}", e.code, e.message);
                    }
                }
            }
        }
    }
    pub(crate) fn tick_text(&mut self, ctx: &egui::Context, now: Instant) {
        let context = self
            .view
            .info
            .as_ref()
            .zip(self.layer.as_ref())
            .map(|(d, l)| {
                (
                    d.document_id.clone(),
                    d.revision.clone(),
                    d.workspace_revision.clone(),
                    l.clone(),
                )
            });
        if self.text.context != context {
            self.text.context = context;
            if self.text.floating.is_some() || self.text.pick_reference {
                self.text.cancel();
                self.tool = ActiveTool::Select;
                return;
            }
            self.text.changed();
        }
        self.invalidate_text_overlay();
        if self.tool != ActiveTool::Text {
            return;
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
            return;
        }
        if let Some(path) = self.text.pending_font.take() {
            if let Some(postscript) = self.text.pending_postscript.take() {
                self.send(Action::SystemFont(
                    self.text.font_generation,
                    path,
                    postscript,
                ));
            } else {
                self.send(Action::TextFont(
                    self.text.font_generation,
                    path,
                    self.text.face,
                ));
            }
            return;
        }
        if !self.text.catalog_requested {
            self.text.catalog_requested = true;
            self.send(Action::FontCatalog);
            return;
        }
        // The commit must reach TextEdit before a worker can disable its UI.
        // raw_input_hook runs before tick_text; ime_active alone is already false
        // on the commit frame while the draft still contains the preedit text.
        if self.ime_active || self.ime_event {
            return;
        }
        let tolerance = self.precision().text_tolerance_mm().to_string();
        if self.text.tolerance != tolerance {
            self.text.tolerance = tolerance;
            self.text.changed();
        }
        if self.text.ready(now) {
            self.text.submitted = Some(self.text.generation);
            let Some(d) = &self.view.info else {
                self.text.status = "请先打开 Gerber 文件".into();
                return;
            };
            let Some(layer) = &self.layer else {
                return;
            };
            match self.text.params(layer) {
                Ok(params) => {
                    let request = Request {
                        generation: self.text.generation,
                        document: d.document_id.clone(),
                        revision: d.revision.clone(),
                        params,
                    };
                    self.text.status = "后台生成预览…".into();
                    self.send(Action::TextPreview(request));
                }
                Err(e) => self.text.status = e,
            }
        } else if self.text.changed_at.is_some()
            && self.text.submitted != Some(self.text.generation)
            && !self.text.text.is_empty()
        {
            ctx.request_repaint_after(text_tool::DEBOUNCE);
        }
    }
    pub(crate) fn commit_text(&mut self) {
        if self.busy {
            return;
        }
        if let Some(request) = self.text.placement_request() {
            if std::env::var_os("RCAM_INTERACTION_LOG").is_some() {
                eprintln!(
                    "text_placement_commit generation={} anchor=({}, {}) expected_revision={}",
                    request.generation,
                    request.params.layout.x_mm,
                    request.params.layout.y_mm,
                    request.revision
                );
            }

            self.text.pending_apply = Some(self.text.generation);
            self.text.submitted = Some(self.text.generation);
            self.text.status = "正在提交一次文字事务…".into();
            self.send(Action::TextCreate(request));
        }
    }
    pub(crate) fn text_controls(&mut self, ui: &mut egui::Ui) {
        self.invalidate_text_overlay();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("图层");
            egui::ComboBox::from_id_salt("text-layer")
                .selected_text(
                    self.view
                        .layers
                        .iter()
                        .find(|l| Some(&l.layer_id) == self.layer.as_ref())
                        .map_or("请选择图层", |l| l.display_name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for layer in &self.view.layers {
                        if ui
                            .add_enabled(
                                crate::state::text_target_ok(layer),
                                egui::Button::new(&layer.display_name),
                            )
                            .clicked()
                        {
                            self.layer = Some(layer.layer_id.clone());
                            changed = true;
                            ui.close();
                        }
                    }
                });
        });
        ui.label("字型");
        let mut builtin_selected = false;
        let mut system_font = None;
        let selected = self.text.font.as_ref().map_or_else(
            || "选择系统字体…".into(),
            |f| format!("{} / {}", f.family, f.subfamily),
        );
        egui::ComboBox::from_id_salt("system-text-font")
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(ui.available_width())
            .selected_text(selected)
            .height(280.)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(
                        self.text.font.as_ref().is_some_and(|f| {
                            f.identity.path == editor_service::BUILTIN_STROKE_PATH
                        }),
                        "RCam 默认线条字体（ASCII）",
                    )
                    .clicked()
                {
                    builtin_selected = true;
                    ui.close();
                }
                ui.separator();
                ui.add(
                    egui::TextEdit::singleline(&mut self.text.font_search)
                        .hint_text("搜索字体家族 / 字形"),
                );
                if let Some(fonts) = &self.text.catalog {
                    let query = self.text.font_search.trim().to_lowercase();
                    for font in fonts.iter().filter(|f| {
                        format!("{} {} {}", f.family, f.style, f.postscript)
                            .to_lowercase()
                            .contains(&query)
                    }) {
                        if ui
                            .selectable_label(false, format!("{} / {}", font.family, font.style))
                            .clicked()
                        {
                            system_font = Some(font.clone());
                            ui.close();
                        }
                    }
                } else {
                    ui.label(
                        self.text
                            .catalog_error
                            .as_deref()
                            .unwrap_or("正在读取系统字体…"),
                    );
                }
            });
        if builtin_selected {
            self.text.font_generation += 1;
            self.text.pending_font = None;
            self.text.pending_postscript = None;
            self.text.font_path = None;
            self.text.offset = "0".into();
            self.text.accept_font(editor_service::builtin_stroke_font());
        }
        if let Some(font) = system_font {
            self.text.queue_font(font.path);
            self.text.pending_postscript = Some(font.postscript);
        }
        ui.collapsing("更多字体选项…", |ui| {
            ui.horizontal(|ui| {
                ui.small(self.text.catalog.as_ref().map_or_else(
                    || "读取系统字体…".into(),
                    |f| format!("系统字体 {} 款", f.len()),
                ));
                if ui
                    .add_enabled(!self.busy, egui::Button::new("刷新").small())
                    .clicked()
                {
                    self.text.catalog_requested = false;
                }
            });
            if let Some(error) = &self.text.catalog_error {
                ui.label(error);
            }
            if ui.button("选择字体文件… TTF / OTF / TTC").clicked() {
                match crate::platform::choose_path(false, "font") {
                    Ok(Some(path)) => {
                        self.text.face = 0;
                        self.text.queue_font(path);
                    }
                    Ok(None) => {}
                    Err(e) => self.text.status = e,
                }
            }
            let mut recent = None;
            egui::ComboBox::from_id_salt("recent-text-font")
                .selected_text("最近字体（本次会话）")
                .show_ui(ui, |ui| {
                    for f in &self.text.recent {
                        if ui
                            .selectable_label(
                                false,
                                format!(
                                    "{} / {} · face {}",
                                    f.family, f.subfamily, f.identity.face_index
                                ),
                            )
                            .clicked()
                        {
                            recent = Some(f.clone());
                        }
                    }
                });
            if let Some(f) = recent {
                self.text.face = f.identity.face_index;
                if f.identity.path == editor_service::BUILTIN_STROKE_PATH {
                    self.text.font_generation += 1;
                    self.text.pending_font = None;
                    self.text.pending_postscript = None;
                    self.text.font_path = None;
                    self.text.offset = "0".into();
                    self.text.accept_font(f);
                } else {
                    self.text.queue_font(f.identity.path.into());
                }
            }
            let face_changed = ui
                .horizontal(|ui| {
                    ui.label("Face Index");
                    ui.add(egui::DragValue::new(&mut self.text.face).range(0..=1024))
                        .changed()
                })
                .inner;
            if face_changed && let Some(path) = self.text.font_path.clone() {
                self.text.queue_font(path);
            }
            if let Some(font) = &self.text.font {
                ui.label(format!("{} / {}", font.family, font.subfamily));
                ui.label(
                    std::path::Path::new(&font.identity.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                );

                if font.identity.path == editor_service::BUILTIN_STROKE_PATH {
                    ui.small("RCam 原创 ASCII 线条字体，随软件提供");
                } else {
                    ui.small("本地字体已验证；使用许可由用户负责，字体文件不随软件分发");
                }
            }
        });
        ui.separator();
        changed |= field(
            ui,
            &format!("可见字高 {}", self.display_unit.suffix()),
            &mut self.text.height,
        );
        let builtin = self
            .text
            .font
            .as_ref()
            .is_some_and(|f| f.identity.path == editor_service::BUILTIN_STROKE_PATH);
        if builtin {
            changed |= field(
                ui,
                &format!("线条宽度 {}", self.display_unit.suffix()),
                &mut self.text.stroke_width,
            );
            ui.small("内置 ASCII 线条字体；中文请选择其他字型");
        } else {
            changed |= field(
                ui,
                &format!("轮廓补偿 Δ {}", self.display_unit.suffix()),
                &mut self.text.offset,
            );
            ui.small("正值加粗 / 负值减细；不是绝对笔画线宽");
        }
        changed |= field(
            ui,
            &format!("基线距离 {}", self.display_unit.suffix()),
            &mut self.text.baseline_spacing,
        );
        ui.small("0 = 自动（1.3 × 字高）；换行向下排列");
        changed |= field(
            ui,
            &format!("字距 {}", self.display_unit.suffix()),
            &mut self.text.tracking,
        );
        changed |= field(ui, "旋转 °", &mut self.text.rotation);
        ui.horizontal(|ui| {
            for (value, label) in [
                (HorizontalAlign::Left, "左"),
                (HorizontalAlign::Center, "中"),
                (HorizontalAlign::Right, "右"),
            ] {
                changed |= ui
                    .selectable_value(&mut self.text.h_align, value, label)
                    .changed();
            }
        });
        ui.horizontal(|ui| {
            for (value, label) in [
                (VerticalAlign::Baseline, "基线"),
                (VerticalAlign::Bottom, "底"),
                (VerticalAlign::Middle, "中"),
                (VerticalAlign::Top, "顶"),
            ] {
                changed |= ui
                    .selectable_value(&mut self.text.v_align, value, label)
                    .changed();
            }
        });
        ui.label(format!(
            "全局制造分辨率：{} µm",
            self.precision().resolution_mm * 1000.
        ));
        ui.small("曲线误差由全局制造策略推导；独立于显示位数和缩放");
        let mut mode = self.text.placement;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, Placement::Mouse, "鼠标");
            ui.selectable_value(&mut mode, Placement::Absolute, "绝对");
            ui.selectable_value(&mut mode, Placement::Relative, "相对");
        });
        if mode != self.text.placement {
            self.text.set_mode(mode);
        }
        match self.text.placement {
            Placement::Mouse | Placement::Absolute => {
                if mode == Placement::Absolute {
                    changed |= field(
                        ui,
                        &format!("X {}", self.display_unit.suffix()),
                        &mut self.text.x,
                    );
                    changed |= field(
                        ui,
                        &format!("Y {}", self.display_unit.suffix()),
                        &mut self.text.y,
                    );
                }
                if mode == Placement::Mouse {
                    changed |= ui
                        .checkbox(&mut self.text.snap_text, "文字锚点吸附网格（独立开关）")
                        .changed();
                }
                ui.small("基准：制造世界原点 (0, 0)");
            }
            Placement::Relative => {
                let reference_changed = field(
                    ui,
                    &format!("基点 Rx {}", self.display_unit.suffix()),
                    &mut self.text.rx,
                ) | field(
                    ui,
                    &format!("基点 Ry {}", self.display_unit.suffix()),
                    &mut self.text.ry,
                );
                if reference_changed {
                    self.text.has_reference = true;
                    changed = true;
                }
                changed |= field(
                    ui,
                    &format!("ΔX {}", self.display_unit.suffix()),
                    &mut self.text.dx,
                );
                changed |= field(
                    ui,
                    &format!("ΔY {}", self.display_unit.suffix()),
                    &mut self.text.dy,
                );
                if ui.button("共同基点：数值 / 拾取 / 双中心…").clicked() {
                    let point = editor_core::MmPoint::new(
                        self.display_unit.parse_length(&self.text.rx).unwrap_or(0.),
                        self.display_unit.parse_length(&self.text.ry).unwrap_or(0.),
                    );
                    self.open_point_adapter(crate::point_adapter::Adapter::TextReference, point);
                }
            }
        }
        ui.separator();
        ui.label(format!(
            "文本（{} / 128 字符，可多行）",
            self.text.text.chars().count()
        ));
        changed |= text_editor(ui, &mut self.text.text).changed();
        if self.ime_active {
            ui.label("输入法组合中；确认候选后才生成预览");
        }
        if changed {
            self.text.changed();
        }
        match self.text.anchor() {
            Ok(p) => {
                ui.label(format!(
                    "最终绝对坐标：{}",
                    self.display_unit
                        .point_label(p, self.precision().resolution_mm)
                ));
            }
            Err(e) => {
                ui.colored_label(crate::ui::tokens::warning_text(ui.visuals()), e);
            }
        }
        fixed_status_line(ui, &self.text.status, false).on_hover_text(&self.text.status);
        let preview_summary = if let Some(p) = &self.text.preview {
            let (mut contours, mut edges) = (0, 0);
            for g in &p.geometries {
                if let editor_core::SemanticGeometry::Region { contours: cs } = g {
                    contours += cs.len();
                    edges += cs.iter().map(|c| c.edges.len()).sum::<usize>();
                } else if matches!(g, editor_core::SemanticGeometry::Line { .. }) {
                    edges += 1;
                }
            }
            format!(
                "预览统计：对象 {} · 轮廓 {contours} · 线段/边界 {edges}",
                p.geometries.len()
            )
        } else {
            "预览统计：等待更新…".into()
        };
        fixed_status_line(ui, &preview_summary, true).on_hover_text(&preview_summary);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy && !self.ime_active && self.text.preview.is_some(),
                    egui::Button::new("确定"),
                )
                .clicked()
                || (self.dialog_enter(ui)
                    && !ui.memory(|m| m.has_focus(egui::Id::new("manufacturing-text")))
                    && self.text.preview.is_some())
            {
                if self.text.placement == Placement::Mouse {
                    if self.text.start_placement() {
                        self.modal = None;
                        ui.memory_mut(|m| {
                            if let Some(id) = m.focused() {
                                m.surrender_focus(id);
                            }
                        });
                    }
                } else {
                    self.commit_text();
                }
            }
        });
        ui.small("确定后点击画布放置；Esc 返回此弹窗修改。普通 Gerber 不保留原文。");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn below_editor_y(text: &str) -> f32 {
        let ctx = egui::Context::default();
        let mut value = text.to_owned();
        let mut below_y = 0.;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500., 500.),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                text_editor(ui, &mut value);
                below_y = ui.label("preview below editor").rect.min.y;
            });
        });
        below_y
    }

    #[test]
    fn multiline_editor_keeps_preview_position_stable_as_text_grows() {
        let short = below_editor_y("A");
        let long = below_editor_y(&("many wrapped words ".repeat(80) + "\nline 2\nline 3\nline 4"));
        assert_eq!(short, long);
    }

    #[test]
    fn preview_status_rows_keep_following_controls_stable() {
        fn below_status_y(status: &str, summary: &str) -> f32 {
            let ctx = egui::Context::default();
            let mut below_y = 0.;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(440., 300.),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    fixed_status_line(ui, status, false);
                    fixed_status_line(ui, summary, true);
                    below_y = ui.button("确定").rect.min.y;
                });
            });
            below_y
        }

        let waiting = below_status_y("等待预览…", "预览统计：等待更新…");
        let published = below_status_y(
            &"很长的预览状态".repeat(40),
            &"预览统计：对象 128 · 轮廓 9999 · 线段/边界 99999".repeat(8),
        );
        assert_eq!(waiting, published);
    }
}
