#[cfg(test)]
mod app_tests;
mod camera;
mod display;
#[cfg(test)]
mod display_tests;
mod drag;
mod gpu;
mod metrics_panel;
mod native_bench;
mod platform;
mod render_index;
mod selection;
mod state;
#[cfg(test)]
mod viewport_tests;

use camera::Camera;
use eframe::egui::{self, Color32, RichText, Vec2};
use state::{Action, Model, View};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender},
    time::Instant,
};

struct EditorApp {
    tx: SyncSender<(u64, Action)>,
    rx: Receiver<(u64, View)>,
    view: View,
    busy: bool,
    sequence: u64,
    camera: Camera,
    fit: bool,
    dx: String,
    dy: String,
    layer: Option<String>,
    rename: String,
    close_prompt: bool,
    quit_after_close: bool,
    allow_quit: bool,
    format: egui_wgpu::wgpu::TextureFormat,
    adapter: String,
    ui_error: Option<String>,
    last_title: String,
    canvas_rect: egui::Rect,
    display_error: Option<String>,
    drag: Option<drag::Gesture>,
    bench: Option<native_bench::NativeBench>,
    timing: bool,
    selected_flags: std::sync::Arc<Vec<u32>>,
    last_frame: Instant,
}
impl EditorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        // System font used in memory only; never copied into source or distribution.
        #[cfg(target_os = "macos")]
        for path in [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "system-cjk".into(),
                    egui::FontData::from_owned(bytes).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("system-cjk".into());
                cc.egui_ctx.set_fonts(fonts);
                break;
            }
        }
        let (tx, request) = mpsc::sync_channel::<(u64, Action)>(1);
        let (reply, rx) = mpsc::sync_channel(1);
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let mut model = Model::default();
            while let Ok((id, action)) = request.recv() {
                let start = Instant::now();
                model.run(action);
                eprintln!("gui_job={id} elapsed_ms={}", start.elapsed().as_millis());
                if reply.send((id, model.view.clone())).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        let gpu = cc
            .wgpu_render_state
            .as_ref()
            .expect("eframe wgpu renderer required");
        let adapter = format!("{:?}", gpu.adapter.get_info());
        eprintln!("RCam S2-B3.2 native GPU: {adapter}");
        Self {
            tx,
            rx,
            view: View::default(),
            busy: false,
            sequence: 0,
            camera: Camera::default(),
            fit: false,
            dx: "0".into(),
            dy: "0".into(),
            layer: None,
            rename: String::new(),
            close_prompt: false,
            quit_after_close: false,
            allow_quit: false,
            format: gpu.target_format,
            adapter,
            ui_error: None,
            last_title: String::new(),
            canvas_rect: egui::Rect::NOTHING,
            display_error: None,
            drag: None,
            bench: native_bench::NativeBench::from_env(gpu.device.clone()),
            selected_flags: Default::default(),
            timing: std::env::var_os("RCAM_RENDER_TIMING").is_some(),
            last_frame: Instant::now(),
        }
    }
    fn send(&mut self, a: Action) {
        if self.busy {
            return;
        }
        if !matches!(a, Action::ProbeDrag(..)) {
            self.drag = None;
        }
        self.sequence += 1;
        match self.tx.try_send((self.sequence, a)) {
            Ok(()) => {
                self.busy = true;
                self.ui_error = None;
            }
            Err(e) => self.ui_error = Some(format!("后台任务不可用：{e}")),
        }
    }
    fn usable(&self) -> bool {
        !self.busy
            && self.view.info.is_some()
            && self.view.blocked.is_none()
            && self.view.scene.is_some()
            && !self.fit
            && self.display_error.is_none()
    }
    fn open(&mut self) {
        match platform::choose_path(false, "") {
            Ok(Some(path)) => self.send(Action::Open(path)),
            Ok(None) => {}
            Err(e) => self.ui_error = Some(e),
        }
    }
    fn save(&mut self) {
        if let Some(l) = self.layer.clone() {
            let name = self
                .view
                .info
                .as_ref()
                .map(|d| {
                    format!(
                        "{}_edited.gbr",
                        Path::new(&d.source_path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                })
                .unwrap_or("edited.gbr".into());
            match platform::choose_path(true, &name) {
                Ok(Some(path)) => self.send(Action::Save(path, l, None)),
                Ok(None) => {}
                Err(e) => self.ui_error = Some(e),
            }
        }
    }
    fn close(&mut self, quit: bool) {
        self.quit_after_close = quit;
        if self.view.info.as_ref().is_some_and(|d| d.dirty) {
            self.close_prompt = true;
        } else if self.view.info.is_some() {
            self.send(Action::Close(false));
        } else if quit {
            self.allow_quit = true;
        }
    }
    fn object_buttons(&mut self, ui: &mut egui::Ui) {
        let enabled = self.usable() && drag::editable_selection(&self.view);
        if ui
            .add_enabled(enabled, egui::Button::new("原位复制  ⌘D"))
            .clicked()
        {
            self.send(Action::Duplicate);
        }
        if ui
            .add_enabled(enabled && !self.busy, egui::Button::new("删除对象  ⌫"))
            .clicked()
        {
            self.send(Action::Delete);
        }
    }
    fn history_buttons(&mut self, ui: &mut egui::Ui) {
        let undo = !self.busy && self.view.info.as_ref().is_some_and(|d| d.undo_entries > 0);
        let redo = !self.busy && self.view.info.as_ref().is_some_and(|d| d.redo_entries > 0);
        if ui
            .add_enabled(undo, egui::Button::new("撤销  ⌘Z"))
            .clicked()
        {
            self.send(Action::History(false));
        }
        if ui
            .add_enabled(redo, egui::Button::new("重做  ⇧⌘Z"))
            .clicked()
        {
            self.send(Action::History(true));
        }
    }
}
impl eframe::App for EditorApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(mut bench) = self.bench.take() {
            bench.input(self, ctx, raw);
            self.bench = Some(bench);
        }
    }
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let now = Instant::now();
        if self.timing {
            eprintln!(
                "render_frame interval_ms={:.6} canvas_physical={:.0}x{:.0} selected={} preview={} revision={}",
                now.duration_since(self.last_frame).as_secs_f64() * 1000.,
                self.canvas_rect.width() * ctx.pixels_per_point(),
                self.canvas_rect.height() * ctx.pixels_per_point(),
                self.view.selected.ordered.len(),
                self.drag.is_some(),
                self.view
                    .info
                    .as_ref()
                    .map_or("none", |i| i.revision.as_str())
            );
        }
        self.last_frame = now;

        if let Ok((id, view)) = self.rx.try_recv()
            && id == self.sequence
        {
            let changed = self.view.info.as_ref().map(|d| &d.document_id)
                != view.info.as_ref().map(|d| &d.document_id);
            self.view = view;
            self.selected_flags =
                std::sync::Arc::new(self.view.scene.as_ref().map_or_else(Vec::new, |scene| {
                    gpu::selection_flags(scene, &self.view.selected.ids())
                }));
            self.busy = false;
            if let Some(drag) = &mut self.drag {
                if self.view.error.is_none() {
                    drag.confirm(&self.view);
                } else {
                    self.drag = None;
                }
            }
            if changed {
                self.drag = None;
                self.fit = self.view.info.is_some();
                self.layer = self.view.layers.first().map(|l| l.layer_id.clone());
                self.rename = self
                    .view
                    .layers
                    .first()
                    .map_or(String::new(), |l| l.display_name.clone());
                self.dx = "0".into();
                self.dy = "0".into();
            }
            if self.quit_after_close && self.view.info.is_none() {
                self.allow_quit = true;
            }
        }
        if ctx.current_pass_index() == 0
            && let Some(mut bench) = self.bench.take()
        {
            bench.tick(self, ctx);
            self.bench = Some(bench);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.busy {
                self.close(true);
            }
        }
        if self.allow_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let title = self.view.info.as_ref().map_or("RCam".into(), |d| {
            format!(
                "RCam — {}{}",
                Path::new(&d.source_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                if d.dirty { " *" } else { "" }
            )
        });
        if self.last_title != title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        if !self.busy {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if dropped.len() == 1 {
                if let Some(p) = &dropped[0].path {
                    self.send(Action::Open(p.clone()));
                }
            } else if dropped.len() > 1 {
                self.ui_error = Some("本阶段每次打开一个文件，请拖入单个 Gerber。".into());
            }
        }
        // Validate the current view before enabling manufacturing actions.
        let validation_start = Instant::now();
        self.display_error = self.view.scene.as_ref().and_then(|scene| {
            if !self.canvas_rect.is_positive() {
                return Some("正在准备画布".into());
            }
            if self.camera.scale * f64::from(ctx.pixels_per_point()) > self.view.render_ppm {
                return Some("正在准备当前缩放的完整图形".into());
            }
            gpu::uniforms(
                scene,
                self.camera,
                self.canvas_rect,
                ctx.pixels_per_point(),
                &self.selected_flags,
            )
            .err()
        });
        let validation_ms = validation_start.elapsed().as_secs_f64() * 1000.;
        let modal_open = self.close_prompt
            || self.view.error.as_ref().is_some_and(|e| {
                e.code == "CONFIRMATION_REQUIRED" && e.details.get("categories").is_some()
            });
        let cancel_drag = ctx.input(|i| {
            drag::cancelled(
                i.key_pressed(egui::Key::Escape),
                i.focused,
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::PointerGone)),
                i.pointer.primary_down() || self.drag.as_ref().is_some_and(|d| d.released),
                i.pointer.primary_released(),
            )
        });
        if cancel_drag || modal_open || self.display_error.is_some() {
            self.drag = None;
        }
        if drag::shortcuts_allowed(ctx.wants_keyboard_input(), self.busy, modal_open) {
            ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::O) {
                    self.open();
                }
                if i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::S,
                ) && self.usable()
                {
                    self.save();
                }
                if i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                {
                    if self.view.info.as_ref().is_some_and(|d| d.redo_entries > 0) {
                        self.send(Action::History(true));
                    }
                } else if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
                    && self.view.info.as_ref().is_some_and(|d| d.undo_entries > 0)
                {
                    self.send(Action::History(false));
                }
                if self.usable() && drag::editable_selection(&self.view) {
                    if i.consume_key(egui::Modifiers::COMMAND, egui::Key::D) {
                        self.send(Action::Duplicate);
                    } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                        || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
                    {
                        self.send(Action::Delete);
                    }
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::F) {
                    self.drag = None;
                    self.fit = true;
                }
            });
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.strong("RCam");
                ui.separator();
                ui.menu_button("文件", |ui| {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("打开…  ⌘O"))
                        .clicked()
                    {
                        self.open();
                        ui.close();
                    }
                    if ui
                        .add_enabled(self.usable(), egui::Button::new("另存为当前图层…  ⇧⌘S"))
                        .clicked()
                    {
                        self.save();
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.busy && self.view.info.is_some(),
                            egui::Button::new("关闭文件…"),
                        )
                        .clicked()
                    {
                        self.close(false);
                        ui.close();
                    }
                });
                ui.menu_button("编辑", |ui| {
                    self.history_buttons(ui);
                    self.object_buttons(ui);
                });
                ui.menu_button("视图", |ui| {
                    if ui.button("适合窗口  F").clicked() {
                        self.fit = true;
                        ui.close();
                    }
                });
                ui.menu_button("帮助", |ui| {
                    ui.label("S2-B2 · Mac 多对象编辑");
                    ui.label(
                        "几何选择包括 Clear；Ctrl 点击加选，Shift 点击减选，双向框选，整组编辑。",
                    );
                    ui.label("中键拖动 / 双指滚动平移；捏合 / Cmd+滚动缩放。");
                    ui.label("另存为必须选择新文件名。Windows 延后验收。");
                    ui.separator();
                    ui.label(&self.adapter);
                });
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("打开…"))
                    .clicked()
                {
                    self.open();
                }
                if ui.button("适合窗口").clicked() {
                    self.fit = true;
                }
                ui.separator();
                self.history_buttons(ui);
                ui.separator();
                ui.label(RichText::new("几何多选").color(Color32::from_rgb(100, 206, 183)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(self.usable(), egui::Button::new("另存为…"))
                        .clicked()
                    {
                        self.save();
                    }
                    if self.busy {
                        ui.spinner();
                        ui.label("处理中…");
                    }
                });
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if let Some(d) = &self.view.info {
                    ui.label(format!(
                        "制造版本 {}  ·  工作区 {}",
                        d.revision, d.workspace_revision
                    ));
                    ui.separator();
                }
                ui.label(format!("{:.2} 点/mm", self.camera.scale));
                if let Some(o) = self.view.selected.primary() {
                    ui.label(format!("选中 {}", o.object.object_id));
                }
                ui.label(&self.view.message);
            });
            if let Some(error) = &self.view.error {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    format!("{} · {}", error.code, error.message),
                );
                ui.collapsing("错误详情", |ui| {
                    ui.label(error.details.to_string());
                });
            }
            if let Some(e) = self.ui_error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(Color32::LIGHT_RED, e);
                    if ui.small_button("关闭提示").clicked() {
                        self.ui_error = None;
                    }
                });
            }
        });
        egui::SidePanel::left("layers")
            .default_width(210.)
            .width_range(170.0..=320.)
            .show(ctx, |ui| {
                ui.add_space(8.);
                ui.heading("图层");
                ui.add_space(8.);
                let layers = self.view.layers.clone();
                for l in layers {
                    ui.group(|ui| {
                        if ui
                            .selectable_label(
                                self.layer.as_ref() == Some(&l.layer_id),
                                &l.display_name,
                            )
                            .clicked()
                        {
                            self.layer = Some(l.layer_id.clone());
                            self.rename = l.display_name.clone();
                        }
                        ui.label(format!("{} 个对象", l.object_count));
                        let mut visible = l.visible;
                        let mut locked = l.locked;
                        ui.add_enabled_ui(!self.busy, |ui| {
                            ui.horizontal(|ui| {
                                let v = ui.checkbox(&mut visible, "显示").changed();
                                let k = ui.checkbox(&mut locked, "锁定").changed();
                                if (v || k)
                                    && let Some(d) = &self.view.info
                                {
                                    self.send(Action::Layer(editor_service::LayerUpdateParams {
                                        layer_id: l.layer_id.clone(),
                                        expected_workspace_revision: d.workspace_revision.clone(),
                                        display_name: None,
                                        visible: Some(visible),
                                        locked: Some(locked),
                                    }));
                                }
                            });
                        });
                    });
                    ui.add_space(4.);
                }
                if self.layer.is_some() {
                    ui.separator();
                    ui.label("显示名称");
                    ui.add_enabled(
                        !self.busy,
                        egui::TextEdit::singleline(&mut self.rename).desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("应用名称"))
                        .clicked()
                        && let (Some(d), Some(layer)) = (&self.view.info, &self.layer)
                    {
                        self.send(Action::Layer(editor_service::LayerUpdateParams {
                            layer_id: layer.clone(),
                            expected_workspace_revision: d.workspace_revision.clone(),
                            display_name: Some(self.rename.clone()),
                            visible: None,
                            locked: None,
                        }));
                    }
                    ui.label(
                        RichText::new("显隐、锁定和名称仅在本次会话保留。")
                            .small()
                            .weak(),
                    );
                }
            });
        egui::SidePanel::right("properties")
            .default_width(260.)
            .width_range(230.0..=380.)
            .show(ctx, |ui| {
                ui.add_space(8.);
                ui.heading("对象属性");
                ui.add_space(8.);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(format!(
                        "已选择 {} 个对象",
                        self.view.selected.ordered.len()
                    ));
                    ui.label("Ctrl 点击加选，Shift 点击减选；空白处拖框：左→右包含，右→左相交");
                    if !self.view.selected.ordered.is_empty()
                        && !drag::editable_selection(&self.view)
                    {
                        ui.label("选择含锁定层或跨层：整组编辑禁止（仅可查看）");
                    }
                    for line in metrics_panel::lines(&self.view) {
                        ui.label(line);
                    }
                    if let Some(o) = self.view.selected.primary().cloned() {
                        ui.label(RichText::new(&o.object.object_id).monospace());
                        let layer = self.view.layers.iter().find(|l| l.layer_id == o.layer_id);
                        ui.label(format!(
                            "图层：{}",
                            layer.map_or(o.layer_id.as_str(), |l| l.display_name.as_str())
                        ));
                        ui.label(format!("曝光：{:?}", o.object.exposure));
                        ui.label(format!("来源：{:?}", o.object.origin));
                        geometry_properties(ui, &o.object.geometry);
                        if let editor_core::SemanticGeometry::Flash { aperture_id, .. } =
                            &o.object.geometry
                        {
                            ui.label(format!("光圈：{aperture_id}"));
                            if let Some(a) =
                                self.view.apertures.iter().find(|a| &a.id == aperture_id)
                            {
                                aperture_properties(ui, &a.shape);
                            }
                        }
                        let locked = layer.is_some_and(|l| l.locked);
                        ui.separator();
                        self.object_buttons(ui);
                        ui.strong("数值移动");
                        if locked {
                            ui.colored_label(Color32::YELLOW, "图层已锁定，请先解锁再移动。");
                        }
                        let mut enter = false;
                        ui.add_enabled_ui(
                            self.usable() && drag::editable_selection(&self.view),
                            |ui| {
                                ui.label("ΔX · mm");
                                let x = ui.add(
                                    egui::TextEdit::singleline(&mut self.dx)
                                        .desired_width(f32::INFINITY),
                                );
                                ui.label("ΔY · mm");
                                let y = ui.add(
                                    egui::TextEdit::singleline(&mut self.dy)
                                        .desired_width(f32::INFINITY),
                                );
                                enter = (x.lost_focus() || y.lost_focus())
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if ui.button("应用位移").clicked() {
                                    enter = true;
                                }
                            },
                        );
                        if enter {
                            self.send(Action::Move(self.dx.clone(), self.dy.clone()));
                        }
                    } else {
                        ui.label("点击图形查看对象，并输入毫米位移。");
                    }
                    ui.separator();
                    if let Some(d) = &self.view.info {
                        ui.strong(if d.dirty {
                            "存在未保存的制造修改"
                        } else {
                            "制造内容未修改"
                        });
                        ui.label("打开来源");
                        ui.label(&d.source_path);
                        if let Some(p) = &d.last_saved_path {
                            ui.add_space(6.);
                            ui.label("最后另存为");
                            ui.label(p);
                        }
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(14, 18, 22)))
            .show(ctx, |ui| {
                let mut cursor_label = None;
                let (r, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let rect = r.rect;
                if self.canvas_rect != rect || self.fit {
                    self.drag = None;
                }
                self.canvas_rect = rect;
                if self.fit {
                    self.camera.fit(self.view.bounds, rect);
                    self.fit = false;
                }
                if self.drag.is_none()
                    && (r.dragged_by(egui::PointerButton::Middle)
                        || r.drag_stopped_by(egui::PointerButton::Middle))
                {
                    self.camera.pan(ctx.input(|i| i.pointer.delta()));
                }
                if r.hovered()
                    && let Some(pos) = r.hover_pos()
                {
                    let (scroll, zoom) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));

                    if self.drag.is_none() {
                        self.camera.pan(scroll);
                        self.camera.zoom(f64::from(zoom), pos, rect);
                    }
                    let w = self.camera.world(pos, rect);
                    cursor_label = Some(format!("X {:.6}  Y {:.6} mm", w.x_mm, w.y_mm));
                    if let Some((press, modifiers)) = ctx.input(|i| {
                        i.events.iter().find_map(|e| match e {
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers,
                            } => Some((*pos, *modifiers)),
                            _ => None,
                        })
                    }) && self.usable()
                        && !cancel_drag
                        && !modal_open
                        && rect.contains(press)
                    {
                        self.drag = Some(drag::Gesture::arm(
                            &self.view,
                            press,
                            self.camera,
                            rect,
                            ctx.pixels_per_point(),
                            selection::SelectionMode::from_modifiers(modifiers),
                        ));
                        if self.drag.is_some() {
                            self.send(Action::ProbeDrag(
                                self.camera.world(press, rect),
                                self.camera.tolerance(ctx.pixels_per_point()),
                            ));
                        }
                    }
                }
                if let Some(drag) = &mut self.drag {
                    if !drag.released
                        && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
                    {
                        drag.update(pos);
                    }
                    drag.released |= ctx.input(|i| i.pointer.primary_released());
                    if drag.released && drag.confirmed {
                        let drag = self.drag.take().unwrap();
                        if let Some(action) = drag.release() {
                            self.send(action);
                        }
                    }
                }
                let ppm = self.camera.scale * f64::from(ctx.pixels_per_point());
                let needs_lod = self.view.info.is_some()
                    && (ppm > self.view.render_ppm
                        || (self.view.blocked.is_some() && ppm < self.view.render_ppm / 2.));
                if needs_lod && !self.busy {
                    self.send(Action::Rebuild(2f64.powf(ppm.log2().ceil())));
                }
                if let Some(scene) = &self.view.scene
                    && !needs_lod
                {
                    match gpu::prepare_measured(
                        scene,
                        self.camera,
                        rect,
                        ctx.pixels_per_point(),
                        &self.selected_flags,
                        self.drag
                            .as_ref()
                            .map_or(editor_core::MmPoint::new(0., 0.), |d| d.delta),
                    ) {
                        Ok(mut prepared) => {
                            prepared.stats.cpu_prepare_ms += validation_ms;
                            if let Some(mut bench) = self.bench.take() {
                                bench.record(self, &prepared.stats, ctx.pixels_per_point(), now);
                                self.bench = Some(bench);
                            }
                            let uniforms = prepared.uniforms;
                            let index = prepared.index;
                            self.display_error = None;
                            painter.add(egui_wgpu::Callback::new_paint_callback(
                                rect,
                                gpu::Callback {
                                    painted: self
                                        .bench
                                        .as_ref()
                                        .map(|b| (b.painted.clone(), b.frame_id)),
                                    index,
                                    scene: scene.clone(),
                                    selected: self.selected_flags.clone(),
                                    uniforms,
                                    format: self.format,
                                },
                            ));
                        }
                        Err(e) => {
                            self.display_error = Some(e);
                            self.drag = None;
                        }
                    }
                }
                if let Some((selection_rect, window)) =
                    self.drag.as_ref().and_then(|d| d.preview_rect())
                {
                    let color = if window {
                        Color32::LIGHT_BLUE
                    } else {
                        Color32::LIGHT_GREEN
                    };
                    painter.rect_filled(selection_rect, 0., color.gamma_multiply(0.12));
                    painter.rect_stroke(
                        selection_rect,
                        0.,
                        egui::Stroke::new(1., color),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        selection_rect.left_top(),
                        egui::Align2::LEFT_BOTTOM,
                        if window {
                            "Window · 完整包含"
                        } else {
                            "Crossing · 相交"
                        },
                        egui::FontId::proportional(12.),
                        color,
                    );
                }
                if let Some(label) = cursor_label {
                    painter.text(
                        rect.left_bottom() + Vec2::new(12., -14.),
                        egui::Align2::LEFT_BOTTOM,
                        label,
                        egui::FontId::monospace(12.),
                        Color32::LIGHT_GRAY,
                    );
                }
                if let Some(o) = self.view.selected.primary()
                    && let editor_core::SemanticGeometry::Flash { center, .. } = o.object.geometry
                {
                    let delta = self
                        .drag
                        .as_ref()
                        .map_or(editor_core::MmPoint::new(0., 0.), |d| d.delta);
                    let p = self.camera.screen(
                        editor_core::MmPoint::new(
                            center.x_mm + delta.x_mm,
                            center.y_mm + delta.y_mm,
                        ),
                        rect,
                    );
                    if rect.contains(p) {
                        painter.circle_stroke(
                            p,
                            3.,
                            egui::Stroke::new(1., Color32::from_rgb(255, 185, 50)),
                        );
                    }
                }
                let message =
                    if let Some(e) = self.view.blocked.as_ref().or(self.display_error.as_ref()) {
                        format!("无法安全显示 / 编辑\n{e}\n可撤销、缩小视图或关闭文件")
                    } else if needs_lod {
                        "正在准备当前缩放的完整图形…".into()
                    } else if self.view.info.is_none() {
                        "打开 Gerber 开始编辑\n使用“打开…”或从 Finder 拖入单个文件".into()
                    } else if self.view.layers.iter().all(|l| !l.visible) {
                        "所有图层已隐藏".into()
                    } else {
                        String::new()
                    };
                if !message.is_empty() {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        message,
                        egui::FontId::proportional(17.),
                        Color32::LIGHT_GRAY,
                    );
                }
                painter.text(
                    rect.left_top() + Vec2::new(12., 12.),
                    egui::Align2::LEFT_TOP,
                    "几何多选  ·  中键 / 双指平移  ·  捏合缩放",
                    egui::FontId::proportional(12.),
                    Color32::LIGHT_GRAY,
                );
            });
        if let Some(mut bench) = self.bench.take() {
            bench.ensure_record(self, ctx.pixels_per_point(), now);
            self.bench = Some(bench);
        }
        if self.close_prompt {
            egui::Modal::new(egui::Id::new("close-confirmation")).show(ctx, |ui| {
                ui.heading("保留未保存修改？");
                ui.label("当前制造修改尚未保存。放弃后无法恢复。");
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        self.close_prompt = false;
                        self.quit_after_close = false;
                    }
                    if ui.button("先另存为…").clicked() {
                        self.close_prompt = false;
                        self.quit_after_close = false;
                        self.save();
                    }
                    if ui.button("放弃修改并关闭").clicked() {
                        self.close_prompt = false;
                        self.send(Action::Close(true));
                    }
                });
            });
        }
        if let Some(e) = self.view.error.clone()
            && e.code == "CONFIRMATION_REQUIRED"
            && e.details.get("categories").is_some()
        {
            egui::Modal::new(egui::Id::new("metadata-confirmation")).show(ctx, |ui| {
                ui.heading("确认导出为几何文件");
                ui.label("导出将移除以下来源元数据：");
                ui.label(e.details["categories"].to_string());
                if ui.button("取消").clicked() {
                    self.view.error = None;
                }
                if ui.button("确认移除并导出").clicked()
                    && self.view.info.as_ref().is_some_and(|d| {
                        e.details["gui_document_id"] == d.document_id
                            && e.details["gui_revision"] == d.revision
                    })
                    && let (Some(path), Some(layer)) = (
                        e.details["gui_target_path"].as_str(),
                        e.details["gui_layer_id"].as_str(),
                    )
                {
                    let categories = serde_json::from_value(e.details["categories"].clone()).ok();
                    self.view.error = None;
                    self.send(Action::Save(path.into(), layer.into(), categories));
                }
            });
        }
    }
}
fn geometry_properties(ui: &mut egui::Ui, g: &editor_core::SemanticGeometry) {
    use editor_core::SemanticGeometry::*;
    let point = |ui: &mut egui::Ui, label: &str, p: editor_core::MmPoint| {
        ui.label(format!("{label}  {:.6}, {:.6} mm", p.x_mm, p.y_mm));
    };
    match g {
        Flash {
            center, transform, ..
        } => {
            ui.strong("Flash · 闪光对象");
            point(ui, "中心", *center);
            ui.label(format!(
                "角度 {:.4}° / 比例 {} / {:?}",
                transform.rotation_deg, transform.scale, transform.mirror
            ));
        }
        Line {
            start,
            end,
            width_mm,
        } => {
            ui.strong("Line · 线段");
            point(ui, "起点", *start);
            point(ui, "终点", *end);
            ui.label(format!("线宽 {width_mm:.6} mm"));
        }
        RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => {
            ui.strong("RectangularSweep");
            point(ui, "起点", *start);
            point(ui, "终点", *end);
            ui.label(format!("截面 {width_mm:.6} × {height_mm:.6} mm"));
        }
        Arc { path, width_mm } => {
            ui.strong("Arc · 圆弧");
            point(ui, "起点", path.start);
            point(ui, "终点", path.end);
            point(ui, "圆心", path.center);
            ui.label(format!("半径 {:.6} / 线宽 {width_mm:.6} mm", path.radius()));
            ui.label(format!("{:?} / 全圆 {}", path.direction, path.full_circle));
        }
        Region { contours } => {
            ui.strong("Region · 区域");
            ui.label(format!(
                "{} 条轮廓 / {} 条边",
                contours.len(),
                contours.iter().map(|c| c.edges.len()).sum::<usize>()
            ));
        }
    }
}
fn main() -> eframe::Result {
    let result = eframe::run_native(
        "RCam",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280., 800.])
                .with_min_inner_size(
                    if std::env::var("RCAM_NATIVE_BENCH").ok().as_deref() == Some("s2b32") {
                        [800., 400.]
                    } else {
                        [980., 620.]
                    },
                ),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    );
    if std::env::var("RCAM_NATIVE_BENCH").ok().as_deref() == Some("s2b32") {
        let passed = std::env::var_os("RCAM_BENCH_OUT")
            .and_then(|p| {
                std::fs::read(std::path::PathBuf::from(p).join("native-results.json")).ok()
            })
            .and_then(|s| serde_json::from_slice::<serde_json::Value>(&s).ok())
            .is_some_and(|r| r["status"] == "PASS");
        if !passed {
            std::process::exit(1);
        }
    }
    result
}

fn aperture_properties(ui: &mut egui::Ui, a: &editor_core::ApertureShape) {
    use editor_core::ApertureShape::*;
    let hole = match a {
        Circle {
            diameter_mm,
            hole_diameter_mm,
        } => {
            ui.label(format!("圆形 · 直径 {diameter_mm:.6} mm"));
            *hole_diameter_mm
        }
        Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        }
        | Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            ui.label(format!("宽 {width_mm:.6} / 高 {height_mm:.6} mm"));
            *hole_diameter_mm
        }
        Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            ui.label(format!("{vertices} 边形 · 外接直径 {diameter_mm:.6} mm"));
            ui.label(format!("光圈角度 {rotation_deg}°"));
            *hole_diameter_mm
        }
        Macro { primitives } => {
            ui.label(format!("宏光圈 · {} 个局部原语", primitives.len()));
            None
        }
    };
    if let Some(h) = hole {
        ui.label(format!("局部孔径 {h:.6} mm"));
    }
}
