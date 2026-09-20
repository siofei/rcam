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
                    .any(|l| l.layer_id == p.params.layer_id && l.visible && !l.locked)
        }) {
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
        if self.ime_active {
            return;
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
    pub(crate) fn text_controls(&mut self, ui: &mut egui::Ui) {
        self.invalidate_text_overlay();
        let mut changed = false;
        ui.label("单行中文 / ASCII");
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut self.text.text)
                    .id(egui::Id::new("manufacturing-text"))
                    .desired_width(f32::INFINITY),
            )
            .changed();
        if self.ime_active {
            ui.label("输入法组合中；确认候选后才生成预览");
        }
        let mut system_font = None;
        let selected = self.text.font.as_ref().map_or_else(
            || "选择系统字体…".into(),
            |f| format!("{} / {}", f.family, f.subfamily),
        );
        egui::ComboBox::from_id_salt("system-text-font")
            .width(ui.available_width())
            .selected_text(selected)
            .height(280.)
            .show_ui(ui, |ui| {
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
        if let Some(font) = system_font {
            self.text.queue_font(font.path);
            self.text.pending_postscript = Some(font.postscript);
        }
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
            self.text.queue_font(f.identity.path.into());
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
            ui.label(format!("SHA-256 {}", font.identity.sha256));
            ui.small("本地字体已验证；使用许可由用户负责，字体文件不随软件分发");
        }
        ui.separator();
        changed |= field(ui, "可见字高 mm", &mut self.text.height);
        changed |= field(ui, "轮廓补偿 Δ mm", &mut self.text.offset);
        ui.small("正值加粗 / 负值减细；不是绝对笔画线宽");
        changed |= field(ui, "字距 mm", &mut self.text.tracking);
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
        ui.label("制造精度");
        ui.horizontal(|ui| {
            for (label, tol) in text_tool::PRESETS {
                if ui
                    .selectable_label(self.text.tolerance.parse::<f64>().ok() == Some(tol), label)
                    .clicked()
                {
                    self.text.tolerance = tol.to_string();
                    changed = true;
                }
            }
        });
        changed |= field(ui, "自定义曲线误差 mm", &mut self.text.tolerance);
        ui.small("0.00001–0.00025 mm；越小对象越多、生成越慢，与缩放无关");
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
                changed |= field(ui, "X mm", &mut self.text.x);
                changed |= field(ui, "Y mm", &mut self.text.y);
                if mode == Placement::Mouse {
                    changed |= ui
                        .checkbox(&mut self.text.snap_text, "文字锚点吸附网格（独立开关）")
                        .changed();
                }
                ui.small("基准：制造世界原点 (0, 0)");
            }
            Placement::Relative => {
                let reference_changed = field(ui, "基点 Rx mm", &mut self.text.rx)
                    | field(ui, "基点 Ry mm", &mut self.text.ry);
                if reference_changed {
                    self.text.has_reference = true;
                    changed = true;
                }
                changed |= field(ui, "ΔX mm", &mut self.text.dx);
                changed |= field(ui, "ΔY mm", &mut self.text.dy);
                if ui
                    .button(if self.text.pick_reference {
                        "请点击画布基点…"
                    } else {
                        "拾取基点"
                    })
                    .clicked()
                {
                    self.text.pick_reference = true;
                }
            }
        }
        if changed {
            self.text.changed();
        }
        match self.text.anchor() {
            Ok(p) => {
                ui.label(format!("最终绝对坐标：{}, {} mm", p.x_mm, p.y_mm));
            }
            Err(e) => {
                ui.colored_label(egui::Color32::YELLOW, e);
            }
        }
        ui.label(&self.text.status);
        ui.small(format!("预览可见候选：{}", self.text.candidates));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy && !self.ime_active && self.text.preview.is_some(),
                    egui::Button::new("Apply 创建文字"),
                )
                .clicked()
                && let Some(p) = self.text.preview.take()
            {
                let request = Request {
                    generation: self.text.generation,
                    document: p.document_id.clone(),
                    revision: p.revision.clone(),
                    params: p.params.clone(),
                };
                self.text.pending_apply = Some(self.text.generation);
                self.text.submitted = Some(self.text.generation);
                self.text.status = "正在提交一次文字事务…".into();
                self.send(Action::TextCreate(request));
            }
            if ui.button("Cancel 取消").clicked() {
                self.text.cancel();
            }
        });
        ui.small("不自动搭桥；普通 Gerber 重开后仅保留制造几何");
    }
}
