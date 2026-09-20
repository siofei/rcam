#[cfg(test)]
mod app_tests;
mod camera;
mod display;
#[cfg(test)]
mod display_tests;
mod drag;
mod font_catalog;
mod gpu;
mod metrics_panel;
mod modal;
use modal::ActiveModal;
mod native_bench;
mod platform;
mod render_index;
mod selection;
mod state;
mod text_panel;
mod text_tool;
mod tools;
#[cfg(test)]
mod viewport_tests;
mod world_index;

use camera::Camera;
use eframe::egui::{self, Color32, RichText, Vec2};
use state::{Action, MirrorDirection, Model, PivotInput, View};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender},
    time::Instant,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum PivotMode {
    #[default]
    SelectionCenter,
    WorldOrigin,
    Custom,
}

struct LastFrame {
    scene: std::sync::Arc<display::Scene>,
    selected: std::sync::Arc<Vec<u32>>,
    index: std::sync::Arc<render_index::RenderIndex>,
    uniforms: gpu::Uniforms,
    camera: Camera,
}
struct EditorApp {
    tx: SyncSender<(u64, Action)>,
    rx: Receiver<(u64, View)>,
    view: View,
    busy: bool,
    sequence: u64,
    camera: Camera,
    last_good: Option<LastFrame>,
    grid: tools::GridSettings,
    grid_visual: tools::GridVisualState,
    spacing: String,
    tool: tools::ActiveTool,
    text: text_tool::Draft,
    modal: Option<ActiveModal>,
    modal_pending: Option<u64>,
    draft_snap: bool,
    ime_event: bool,
    measure: tools::MeasureState,
    fit: bool,
    dx: String,
    dy: String,
    angle: String,
    pivot_mode: PivotMode,
    mirror_direction: MirrorDirection,
    pivot_x: String,
    pivot_y: String,
    size_aperture_id: Option<String>,
    size_width: String,
    size_height: String,
    display_unit: tools::DisplayUnit,
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
    display_pending: bool,
    drag: Option<drag::Gesture>,
    bench: Option<native_bench::NativeBench>,
    timing: bool,
    selected_flags: std::sync::Arc<Vec<u32>>,
    last_frame: Instant,
    text_input_at_event: bool,
    ime_active: bool,
    reported_ppp: f32,
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
        eprintln!("RCam S4-A2 native GPU: {adapter}");
        Self {
            tx,
            rx,
            view: View::default(),
            busy: false,
            sequence: 0,
            camera: Camera::default(),
            last_good: None,
            grid: Default::default(),
            grid_visual: Default::default(),
            spacing: "0.1".into(),
            tool: Default::default(),
            text: Default::default(),
            modal: None,
            modal_pending: None,
            draft_snap: false,
            ime_event: false,
            measure: Default::default(),
            fit: false,
            dx: "0".into(),
            dy: "0".into(),
            angle: "90".into(),
            pivot_mode: PivotMode::SelectionCenter,
            mirror_direction: MirrorDirection::Horizontal,
            pivot_x: "0".into(),
            pivot_y: "0".into(),
            size_aperture_id: None,
            size_width: String::new(),
            size_height: String::new(),
            display_unit: Default::default(),
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
            display_pending: false,
            drag: None,
            bench: native_bench::NativeBench::from_env(gpu.device.clone()),
            selected_flags: Default::default(),
            timing: std::env::var_os("RCAM_RENDER_TIMING").is_some(),
            last_frame: Instant::now(),
            text_input_at_event: false,
            ime_active: false,
            reported_ppp: 0.,
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
        if self.modal.is_some()
            && matches!(
                a,
                Action::Move(..)
                    | Action::Rotate(..)
                    | Action::Mirror(..)
                    | Action::SetFlashSize(..)
                    | Action::TextCreate(..)
                    | Action::Layer(..)
            )
        {
            self.modal_pending = Some(self.sequence);
        }
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
            && !self.display_pending
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
        self.modal = None;
        self.text.cancel();
        self.tool = tools::ActiveTool::Select;
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
    fn transform_controls(&mut self, ui: &mut egui::Ui) {
        let enabled = self.usable() && drag::editable_selection(&self.view);
        let center = state::selected_center(&self.view).ok();
        ui.separator();
        ui.strong("变换");
        ui.label("制造坐标 f64；不经过 Grid Snap");
        if self.modal == Some(ActiveModal::Rotate) {
            ui.add_enabled_ui(enabled, |ui| {
                ui.label("旋转角度 · °");
                ui.add(
                    egui::TextEdit::singleline(&mut self.angle)
                        .id(egui::Id::new("transform-angle"))
                        .desired_width(f32::INFINITY),
                );
                ui.label("Pivot");
                ui.radio_value(
                    &mut self.pivot_mode,
                    PivotMode::SelectionCenter,
                    "选择集制造边界中心",
                );
                if let Some(center) = center {
                    ui.label(format!("X {:.6} / Y {:.6} mm", center.x_mm, center.y_mm));
                } else {
                    ui.colored_label(Color32::YELLOW, "选择集中心不可用，请使用明确 Pivot");
                }
                ui.radio_value(
                    &mut self.pivot_mode,
                    PivotMode::WorldOrigin,
                    "世界原点 (0, 0)",
                );
                ui.radio_value(&mut self.pivot_mode, PivotMode::Custom, "自定义 X / Y mm");
                if self.pivot_mode == PivotMode::Custom {
                    ui.horizontal(|ui| {
                        ui.label("X");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pivot_x)
                                .id(egui::Id::new("transform-pivot-x")),
                        );
                        ui.label("Y");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pivot_y)
                                .id(egui::Id::new("transform-pivot-y")),
                        );
                    });
                }
            });
            let pivot_ready = self.pivot_mode != PivotMode::SelectionCenter || center.is_some();
            let mut rotate = None;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(enabled && pivot_ready, egui::Button::new("-90°"))
                    .clicked()
                {
                    rotate = Some("-90".into());
                }
                if ui
                    .add_enabled(enabled && pivot_ready, egui::Button::new("+90°"))
                    .clicked()
                {
                    rotate = Some("90".into());
                }
                if ui
                    .add_enabled(enabled && pivot_ready, egui::Button::new("应用旋转"))
                    .clicked()
                {
                    rotate = Some(self.angle.clone());
                }
            });
            if self.dialog_enter(ui) && enabled && pivot_ready {
                rotate = Some(self.angle.clone());
            }
            if let Some(angle) = rotate {
                let pivot = match self.pivot_mode {
                    PivotMode::SelectionCenter => PivotInput::SelectionCenter,
                    PivotMode::WorldOrigin => PivotInput::WorldOrigin,
                    PivotMode::Custom => {
                        PivotInput::Custom(self.pivot_x.clone(), self.pivot_y.clone())
                    }
                };
                self.send(Action::Rotate(angle, pivot));
            }
        }
        if self.modal == Some(ActiveModal::Mirror) {
            ui.add_space(6.);
            ui.label("镜像轴（选择集制造边界中心）");
            if let Some(center) = center {
                ui.radio_value(
                    &mut self.mirror_direction,
                    MirrorDirection::Horizontal,
                    format!("水平镜像 · y = {:.6} mm", center.y_mm),
                );
                ui.radio_value(
                    &mut self.mirror_direction,
                    MirrorDirection::Vertical,
                    format!("垂直镜像 · x = {:.6} mm", center.x_mm),
                );
                if ui
                    .add_enabled(enabled, egui::Button::new("应用镜像"))
                    .clicked()
                    || (enabled && self.dialog_enter(ui))
                {
                    self.send(Action::Mirror(self.mirror_direction));
                }
            }
        }
    }
    fn sync_size_fields(&mut self) {
        let Some(primary) = self.view.selected.primary() else {
            self.size_aperture_id = None;
            return;
        };
        let editor_core::SemanticGeometry::Flash { aperture_id, .. } = &primary.object.geometry
        else {
            self.size_aperture_id = None;
            return;
        };
        if self.size_aperture_id.as_deref() == Some(aperture_id) {
            return;
        }
        let Some(aperture) = self
            .view
            .apertures
            .iter()
            .find(|aperture| aperture.id == *aperture_id)
        else {
            self.size_aperture_id = None;
            return;
        };
        let (width, height) = match aperture.shape {
            editor_core::ApertureShape::Circle { diameter_mm, .. }
            | editor_core::ApertureShape::Polygon { diameter_mm, .. } => (diameter_mm, None),
            editor_core::ApertureShape::Rectangle {
                width_mm,
                height_mm,
                ..
            }
            | editor_core::ApertureShape::Obround {
                width_mm,
                height_mm,
                ..
            } => (width_mm, Some(height_mm)),
            editor_core::ApertureShape::Macro { .. } => {
                self.size_aperture_id = Some(aperture_id.clone());
                self.size_width.clear();
                self.size_height.clear();
                return;
            }
        };
        self.size_aperture_id = Some(aperture_id.clone());
        self.size_width = width.to_string();
        self.size_height = height.map_or_else(String::new, |value| value.to_string());
    }

    fn flash_size_controls(&mut self, ui: &mut egui::Ui) {
        self.sync_size_fields();
        if self.view.selected.ordered.len() != 1 || self.size_aperture_id.is_none() {
            return;
        }
        let Some(primary) = self.view.selected.primary() else {
            return;
        };
        let editor_core::SemanticGeometry::Flash { aperture_id, .. } = &primary.object.geometry
        else {
            return;
        };
        let Some(aperture) = self
            .view
            .apertures
            .iter()
            .find(|aperture| aperture.id == *aperture_id)
        else {
            return;
        };
        if matches!(aperture.shape, editor_core::ApertureShape::Macro { .. }) {
            ui.label("Macro Flash 尺寸编辑不在 V1 范围");
            return;
        }
        ui.separator();
        ui.strong("Flash 尺寸属性");
        let rectangular = matches!(
            aperture.shape,
            editor_core::ApertureShape::Rectangle { .. }
                | editor_core::ApertureShape::Obround { .. }
        );
        ui.horizontal(|ui| {
            ui.label(if rectangular {
                "宽度 mm"
            } else {
                "直径 mm"
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.size_width)
                    .id(egui::Id::new("flash-size-width")),
            );
        });
        if rectangular {
            ui.horizontal(|ui| {
                ui.label("高度 mm");
                ui.add(
                    egui::TextEdit::singleline(&mut self.size_height)
                        .id(egui::Id::new("flash-size-height")),
                );
            });
        }
        if ui.button("应用尺寸（写时复制）").clicked() || self.dialog_enter(ui) {
            self.send(Action::SetFlashSize(
                self.size_width.clone(),
                rectangular.then(|| self.size_height.clone()),
            ));
        }
    }
}
impl eframe::App for EditorApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // egui clears text focus on Escape before update; retain its event-time owner.
        self.text_input_at_event = ctx.wants_keyboard_input() || self.ime_active;
        self.ime_event =
            self.ime_active || raw.events.iter().any(|e| matches!(e, egui::Event::Ime(_)));
        for event in &raw.events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                    self.ime_active = !text.is_empty();
                    eprintln!("ime_preedit scalars={}", text.chars().count());
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    self.ime_active = false;
                    eprintln!("ime_commit scalars={}", text.chars().count());
                }
                egui::Event::Ime(egui::ImeEvent::Disabled) => self.ime_active = false,
                _ => {}
            }
        }
        if let Some(mut bench) = self.bench.take() {
            bench.input(self, ctx, raw);
            self.bench = Some(bench);
        }
    }
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let now = Instant::now();
        if self.reported_ppp != ctx.pixels_per_point() {
            self.reported_ppp = ctx.pixels_per_point();
            eprintln!("native_pixels_per_point={}", self.reported_ppp);
        }
        self.tick_text(ctx, now);
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
            if self.modal_pending == Some(id) {
                self.modal_pending = None;
                if self.view.error.is_none() {
                    self.modal = None;
                }
            }
            self.accept_text_reply();
            if let Some(diagnostic) = &self.view.display_transient {
                eprintln!("display_transient={diagnostic}");
                if let Some(last) = &self.last_good {
                    self.camera = last.camera;
                }
            }
            if let Some(generation) = self.text.pending_apply.take()
                && generation == self.text.generation
            {
                if self.view.error.is_none() {
                    self.text.cancel();
                    self.tool = tools::ActiveTool::Select;
                    self.modal = None;
                    self.text.context =
                        self.view
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
                }
                if self.view.error.is_some() {
                    self.text.floating = None;
                    self.modal = Some(ActiveModal::Text);
                }
                self.text.status = self.view.error.as_ref().map_or_else(
                    || self.view.message.clone(),
                    |e| format!("{}: {}", e.code, e.message),
                );
            }
            if let Some(drag) = &mut self.drag {
                if self.view.error.is_none() {
                    drag.confirm(&self.view);
                } else {
                    self.drag = None;
                }
            }
            if changed {
                self.last_good = None;
                self.modal = None;
                self.text.cancel();
                self.measure.clear();
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
                self.angle = "90".into();
                self.pivot_mode = PivotMode::SelectionCenter;
                self.pivot_x = "0".into();
                self.pivot_y = "0".into();
                self.size_aperture_id = None;
                self.size_width.clear();
                self.size_height.clear();
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
        if !self.busy && self.modal.is_none() {
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
        let modal_open = self.modal.is_some()
            || self.close_prompt
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
        let text_focus = self.text_input_at_event || ctx.wants_keyboard_input();
        if !modal_open && !self.ime_active && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.measure.clear();
            if self.tool == tools::ActiveTool::Text {
                self.text.cancel();
                self.tool = tools::ActiveTool::Select;
            }
        }
        if cancel_drag || modal_open || self.display_error.is_some() {
            self.drag = None;
        }
        if drag::shortcuts_allowed(
            text_focus,
            self.busy,
            modal_open || self.text.floating.is_some(),
        ) {
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
            if modal_open {
                ui.disable();
            }
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
                ui.menu_button("插入", |ui| {
                    if ui
                        .add_enabled(self.usable(), egui::Button::new("文字…"))
                        .clicked()
                    {
                        self.open_modal(ActiveModal::Text);
                        ui.close();
                    }
                });
                ui.menu_button("视图", |ui| {
                    if ui.button("适合窗口  F").clicked() {
                        self.fit = true;
                        ui.close();
                    }
                });
                ui.menu_button("帮助", |ui| {
                    ui.label("S2-C2 · Mac Rotate / Mirror GUI");
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
                if modal_open {
                    ui.disable();
                }
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
                    if ui.button("图层名称…").clicked() {
                        self.open_modal(ActiveModal::Rename);
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
                if modal_open {
                    ui.disable();
                }
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
                        ui.separator();
                        self.object_buttons(ui);
                        ui.add_enabled_ui(
                            self.usable() && drag::editable_selection(&self.view),
                            |ui| {
                                for (label, modal) in [
                                    ("移动…", ActiveModal::Move),
                                    ("旋转…", ActiveModal::Rotate),
                                    ("镜像…", ActiveModal::Mirror),
                                    ("Flash 属性…", ActiveModal::Flash),
                                ] {
                                    if ui.button(label).clicked() {
                                        self.open_modal(modal);
                                    }
                                }
                            },
                        );
                    } else {
                        ui.label("点击图形查看对象，并输入毫米位移或变换参数。");
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
        egui::TopBottomPanel::top("grid-tools").show(ctx, |ui| {
            if modal_open {
                ui.disable();
            }
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.grid.visible, "网格");
                if ui.button("网格 / 吸附设置…").clicked() {
                    self.open_modal(ActiveModal::Grid);
                }
                ui.selectable_value(&mut self.display_unit, tools::DisplayUnit::Mm, "mm");
                ui.selectable_value(&mut self.display_unit, tools::DisplayUnit::Inch, "inch");
                let old = self.tool;
                ui.selectable_value(&mut self.tool, tools::ActiveTool::Select, "选择");
                ui.selectable_value(&mut self.tool, tools::ActiveTool::Measure, "测距");
                if ui
                    .add_enabled(self.usable(), egui::Button::new("文字…"))
                    .clicked()
                {
                    self.open_modal(ActiveModal::Text);
                }
                if old != self.tool {
                    if self.tool != tools::ActiveTool::Text {
                        self.text.cancel();
                    }
                    self.drag = None;
                    self.measure.clear();
                }
                if self.tool == tools::ActiveTool::Measure {
                    ui.label(format!(
                        "标注 {} · 距离∠角度 · Esc清除",
                        self.measure.completed.len()
                    ));
                }
            });
        });
        let modal_open = modal_open || self.modal.is_some();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(14, 18, 22)))
            .show(ctx, |ui| {
                let mut cursor_label = None;
                let mut measure_hover = None;
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
                if !modal_open
                    && self.drag.is_none()
                    && (r.dragged_by(egui::PointerButton::Middle)
                        || r.drag_stopped_by(egui::PointerButton::Middle))
                {
                    self.camera.pan(ctx.input(|i| i.pointer.delta()));
                }
                let camera_before = self.camera;
                if !modal_open
                    && r.hovered()
                    && let Some(pos) = r.hover_pos()
                {
                    let (scroll, zoom) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));

                    if self.drag.is_none() {
                        self.camera.pan(scroll);
                        let extent = self.view.scene.as_ref().map_or(1e-12, |s| {
                            s.objects
                                .iter()
                                .flat_map(|o| o.bounds)
                                .map(|v| f64::from(v).abs())
                                .fold(1e-12, f64::max)
                        });
                        self.camera.zoom_view(
                            f64::from(zoom),
                            pos,
                            rect,
                            ctx.pixels_per_point(),
                            self.view.bounds,
                            extent,
                        );
                    }
                    let w = self.camera.world(pos, rect);
                    cursor_label = Some(self.display_unit.point_label(w));
                    if camera_before.center != self.camera.center
                        || camera_before.scale != self.camera.scale
                    {
                        eprintln!(
                            "native_camera center={:?} scale={} ppp={}",
                            self.camera.center,
                            self.camera.scale,
                            ctx.pixels_per_point()
                        );
                    }
                    if self.tool == tools::ActiveTool::Text && !modal_open && !text_focus {
                        let point = if self.text.snap_text {
                            tools::snap_point(
                                w,
                                tools::GridSettings {
                                    snap_enabled: true,
                                    ..self.grid
                                },
                                &[],
                                self.camera,
                                None,
                                false,
                            )
                            .unwrap_or(w)
                        } else {
                            w
                        };
                        if self.text.floating.is_some() {
                            if self.text.floating != Some(point) && std::env::var_os("RCAM_INTERACTION_LOG").is_some() {
                                eprintln!("text_placement_move generation={} anchor={:?} scale={} revision={:?}",self.text.generation,point,self.camera.scale,self.view.info.as_ref().map(|d|&d.revision));
                            }
                            self.text.floating = Some(point);
                            if r.clicked_by(egui::PointerButton::Primary) && self.usable() {
                                self.commit_text();
                            }
                            if r.clicked_by(egui::PointerButton::Secondary) {
                                self.text.cancel();
                                self.tool = tools::ActiveTool::Select;
                            }
                        } else if self.text.pick_reference
                            && r.clicked_by(egui::PointerButton::Primary)
                        {
                            self.text.canvas_click(point);
                            self.modal = Some(ActiveModal::Text);
                        }
                    }
                    if self.tool == tools::ActiveTool::Measure
                        && self.usable()
                        && !modal_open
                        && !ctx.wants_keyboard_input()
                    {
                        match tools::snap_point(
                            w,
                            self.grid,
                            &self.view.snap_points,
                            self.camera,
                            None,
                            ctx.input(|input| input.modifiers.alt),
                        ) {
                            Ok(p) => {
                                measure_hover = Some(p);
                                if r.clicked_by(egui::PointerButton::Primary) {
                                    self.measure.click(p);
                                }
                            }
                            Err(e) => self.ui_error = Some(e),
                        }
                    }
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
                    }) && self.tool == tools::ActiveTool::Select
                        && !ctx.wants_keyboard_input()
                        && self.usable()
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
                        if let Some(d) = &mut self.drag {
                            d.set_grid(self.grid);
                            self.send(Action::ProbeDrag(
                                self.camera.world(press, rect),
                                self.camera.tolerance(ctx.pixels_per_point()),
                            ));
                        }
                    }
                }
                if let Some(drag) = &mut self.drag {
                    drag.set_snap_disabled(ctx.input(|input| input.modifiers.alt));
                    if !drag.released
                        && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
                    {
                        drag.update(pos);
                    }
                    drag.released |= ctx.input(|i| i.pointer.primary_released());
                    if let Some(error) = drag.error() {
                        self.ui_error = Some(error.to_string());
                    }
                    if drag.released && drag.confirmed {
                        let drag = self.drag.take().unwrap();
                        if let Some(action) = drag.release() {
                            self.send(action);
                        }
                    }
                }
                self.measure.hover(measure_hover);
                let ppm = self.camera.scale * f64::from(ctx.pixels_per_point());
                let coverage = self.view.render_viewport;
                let lo = self.camera.world(rect.left_bottom(), rect);
                let hi = self.camera.world(rect.right_top(), rect);
                let outside = coverage.is_none_or(|b| {
                    lo.x_mm < b.min_x_mm
                        || lo.y_mm < b.min_y_mm
                        || hi.x_mm > b.max_x_mm
                        || hi.y_mm > b.max_y_mm
                });
                let needs_lod = self.view.info.is_some()
                    && (outside || ppm > self.view.render_ppm || ppm < self.view.render_ppm / 4.);
                if needs_lod && !self.busy {
                    let margin_x = (hi.x_mm - lo.x_mm) * 0.5 + 4. / ppm;
                    let margin_y = (hi.y_mm - lo.y_mm) * 0.5 + 4. / ppm;
                    self.send(Action::Viewport(
                        self.camera.center,
                        editor_core::BoundsMm {
                            min_x_mm: lo.x_mm - margin_x,
                            min_y_mm: lo.y_mm - margin_y,
                            max_x_mm: hi.x_mm + margin_x,
                            max_y_mm: hi.y_mm + margin_y,
                        },
                        2f64.powf(ppm.log2().ceil()),
                    ));
                }
                let mut rendered = false;
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
                            self.last_good = Some(LastFrame {
                                scene: scene.clone(),
                                selected: self.selected_flags.clone(),
                                index,
                                uniforms,
                                camera: self.camera,
                            });
                            rendered = true;
                        }
                        Err(e) => {
                            if self.display_error.as_ref() != Some(&e) {
                                eprintln!("display_prepare_diagnostic={e}");
                            }
                            self.display_error = Some(e);
                            if let Some(last) = &self.last_good {
                                self.camera = last.camera;
                            }
                            self.drag = None;
                        }
                    }
                }
                self.display_pending = self.view.info.is_some() && !rendered;
                if self.view.blocked.is_some() {
                    self.last_good = None;
                }
                if let Some(last) = &self.last_good {
                    if !rendered {
                        self.display_error = None;
                    }
                    painter.add(egui_wgpu::Callback::new_paint_callback(
                        rect,
                        gpu::Callback {
                            painted: self.bench.as_ref().map(|b| (b.painted.clone(), b.frame_id)),
                            index: last.index.clone(),
                            scene: last.scene.clone(),
                            selected: last.selected.clone(),
                            uniforms: last.uniforms,
                            format: self.format,
                        },
                    ));
                }
                self.invalidate_text_overlay();
                if self.tool == tools::ActiveTool::Text {
                    self.text.paint(&painter, self.camera, rect);
                }
                let grid_opacity_before = self.grid_visual.opacity;
                if self
                    .grid_visual
                    .advance(self.grid.visible, ctx.input(|i| i.stable_dt))
                {
                    ctx.request_repaint();
                }
                if self.grid_visual.opacity != grid_opacity_before {
                    eprintln!(
                        "native_grid visible={} opacity={} ppp={}",
                        self.grid.visible,
                        self.grid_visual.opacity,
                        ctx.pixels_per_point()
                    );
                }
                self.grid.paint(
                    &painter,
                    self.camera,
                    rect,
                    ctx.pixels_per_point(),
                    self.grid_visual.opacity,
                );
                if self.tool == tools::ActiveTool::Measure {
                    self.measure
                        .paint_in(&painter, self.camera, rect, self.display_unit);
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
                    tools::overlay_label(
                        &painter,
                        rect.left_bottom() + Vec2::new(12., -14.),
                        egui::Align2::LEFT_BOTTOM,
                        label,
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
                    } else if needs_lod && self.last_good.is_none() {
                        "正在准备画布…".into()
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
            self.modal = None;
            self.text.cancel();
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
        if !self.close_prompt {
            self.parameter_modal(ctx);
        }
        if self.modal.is_none()
            && !self.close_prompt
            && let Some(e) = self.view.error.clone()
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
