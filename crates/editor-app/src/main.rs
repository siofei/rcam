#[cfg(test)]
mod app_tests;
mod block_display;
mod camera;
mod display;
#[cfg(test)]
mod display_tests;
mod drag;
mod font_catalog;
mod gpu;
mod layer_panel;
#[cfg(test)]
mod layer_tests;
mod metrics_panel;
mod modal;
#[cfg(test)]
mod perf_tests;
use modal::ActiveModal;
mod native_bench;
mod native_probe;
mod object_snap;
mod platform;
mod preferences;
mod project_ui;
mod recovery;
mod render_index;
mod selection;
mod state;
mod text_panel;
mod text_tool;
mod tools;
mod ui;
mod units;
#[cfg(test)]
mod viewport_tests;
mod world_index;

use camera::Camera;
use editor_core::command::{
    CommandDispatcher, CommandId, Key, Keymap, Modifiers, Resolution, Shortcut, ShortcutContext,
    ShortcutResolver, ids as command_ids,
};
use eframe::egui::{self, Color32, RichText, Vec2};
use state::{Action, MirrorDirection, Model, PivotInput, View};
use std::{
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
}
struct EditorApp {
    operation_source: rcam_diagnostics::Source,
    diagnostic_export: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    tx: SyncSender<(u64, rcam_diagnostics::Source, Action)>,
    rx: Receiver<(u64, View)>,
    view: View,
    busy: bool,
    sequence: u64,
    camera: Camera,
    last_good: Option<LastFrame>,
    grid: tools::GridSettings,
    grid_visual: tools::GridVisualState,
    object_snap: object_snap::Settings,
    object_snap_runtime: object_snap::Runtime,
    draft_object_snap: object_snap::Settings,
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
    /// The active layer (follows the service's `is_active` flag).
    layer: Option<String>,
    layer_dialog: Option<layer_panel::LayerDialog>,
    layer_dialog_close_on_success: bool,
    pending_summary: Option<editor_service::LayerSummaryResult>,
    /// Recently committed layer/category colours (session-only UI preference).
    recent_colors: Vec<String>,
    prefs: preferences::AppPreferences,
    recovery_candidate: Option<recovery::RecoveryMetadata>,
    recovery_ignore_confirm: bool,
    last_dirty_identity: String,
    dirty_since: Instant,
    last_recovery_at: Instant,
    last_recovered_identity: String,
    pending_recovery_identity: Option<String>,
    /// Message plus its birth time; drives the "deleted … [Undo]" notice.
    toast: Option<(String, Instant)>,
    last_structure_serial: u64,
    /// The unexported-changes prompt is for "new workspace", not for quitting.
    transition: Option<project_ui::Transition>,
    close_prompt: bool,
    waiting_save: bool,
    replace_project_path: Option<std::path::PathBuf>,
    pending_project_error_title: Option<&'static str>,
    project_error: Option<(String, String)>,
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
    /// Opt-in native evidence probe (`RCAM_NATIVE_PROBE_DIR`); observation only.
    probe: Option<native_probe::Probe>,
    row_probes: std::cell::RefCell<Vec<serde_json::Value>>,
    layer_panel_rect: egui::Rect,
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
        let (tx, request) = mpsc::sync_channel::<(u64, rcam_diagnostics::Source, Action)>(1);
        let (reply, rx) = mpsc::sync_channel(1);
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let mut model = Model::default();
            // Dev-only native GUI smoke path (S4-B2 Final Closeout, task
            // §17): no Block Editor GUI ships this phase, so a synthetic
            // Block fixture is loaded this way instead of through the UI.
            if std::env::var_os("RCAM_NATIVE_PROBE_AUTOLOAD_BLOCK_FIXTURE").is_some() {
                match model.autoload_block_fixture() {
                    Ok(()) => {
                        // The GUI thread's `sequence` starts at 0 and has not
                        // sent a request yet; push the autoloaded view directly
                        // so the very first frame already shows it.
                        let _ = reply.send((0, model.view.clone()));
                        ctx.request_repaint();
                    }
                    Err(e) => eprintln!("RCAM_NATIVE_PROBE_AUTOLOAD_BLOCK_FIXTURE failed: {e:?}"),
                }
            }
            while let Ok((id, source, action)) = request.recv() {
                let start = Instant::now();
                rcam_diagnostics::with_source(source, || model.run(action));
                if start.elapsed().as_millis() > 100 {
                    rcam_diagnostics::runtime_event(
                        rcam_diagnostics::Level::Warn,
                        "gui.worker.slow",
                    );
                }
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
        let adapter_info = gpu.adapter.get_info();
        if let Some(runtime) = rcam_diagnostics::global() {
            runtime.set_gpu(&adapter_info.name, &format!("{:?}", adapter_info.backend));
        }
        let adapter = format!("{:?}", adapter_info);
        eprintln!("RCam S4-A2 native GPU: {adapter}");
        let prefs = preferences::AppPreferences::path()
            .map_or_else(preferences::AppPreferences::default, |path| {
                preferences::AppPreferences::load(&path)
            });
        let recovery_candidate =
            recovery::directory().and_then(|dir| recovery::discover(&dir).into_iter().next());
        let mut app = Self {
            operation_source: rcam_diagnostics::Source::System,
            diagnostic_export: None,
            tx,
            rx,
            view: View::default(),
            busy: false,
            sequence: 0,
            camera: Camera::default(),
            last_good: None,
            grid: Default::default(),
            grid_visual: Default::default(),
            object_snap: Default::default(),
            object_snap_runtime: Default::default(),
            draft_object_snap: Default::default(),
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
            layer_dialog: None,
            layer_dialog_close_on_success: false,
            pending_summary: None,
            recent_colors: prefs.recent_colors.clone(),
            prefs,
            recovery_candidate,
            recovery_ignore_confirm: false,
            last_dirty_identity: String::new(),
            dirty_since: Instant::now(),
            last_recovery_at: Instant::now() - std::time::Duration::from_secs(60),
            last_recovered_identity: String::new(),
            pending_recovery_identity: None,
            toast: None,
            last_structure_serial: 0,
            transition: None,
            close_prompt: false,
            waiting_save: false,
            replace_project_path: None,
            pending_project_error_title: None,
            project_error: None,
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
            probe: native_probe::Probe::from_env(),
            row_probes: Default::default(),
            layer_panel_rect: egui::Rect::NOTHING,
            selected_flags: Default::default(),
            timing: std::env::var_os("RCAM_RENDER_TIMING").is_some(),
            last_frame: Instant::now(),
            text_input_at_event: false,
            ime_active: false,
            reported_ppp: 0.,
        };
        // The Workspace always exists; layers are imported into it or created empty.
        app.send(Action::NewWorkspace);
        app
    }
    fn send(&mut self, a: Action) {
        let source = if matches!(&a, Action::RestoreProject(..) | Action::RecoveryWrite(..)) {
            rcam_diagnostics::Source::Recovery
        } else if self.modal.is_some() {
            rcam_diagnostics::Source::Modal
        } else {
            self.operation_source
        };
        let a = match self.length_action(a) {
            Ok(a) => a,
            Err(e) => {
                self.ui_error = Some(e);
                return;
            }
        };
        if self.busy {
            return;
        }
        self.pending_project_error_title = match &a {
            Action::OpenProject(..) | Action::RestoreProject(..) => Some("无法打开工程"),
            Action::SaveProject(..) => Some("无法保存工程"),
            _ => None,
        };
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
                    | Action::Precision(..)
            )
        {
            self.modal_pending = Some(self.sequence);
        }
        if let Some(probe) = &self.probe {
            probe.action(&native_probe::action_text(&a));
        }
        match self.tx.try_send((self.sequence, source, a)) {
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
    /// Export one layer as a new Gerber. The Workspace is not saved, not linked
    /// to the file, and stays dirty: Gerber is an interchange format here.
    fn export_layer(&mut self, layer: String) {
        let name = self
            .view
            .layers
            .iter()
            .find(|l| l.layer_id == layer)
            .map(|l| {
                let stem: String = l
                    .display_name
                    .chars()
                    .map(|c| {
                        if c.is_control() || "/\\:*?\"<>|".contains(c) {
                            '_'
                        } else {
                            c
                        }
                    })
                    .collect();
                format!("{}.gbr", stem.trim())
            })
            .unwrap_or_else(|| "layer.gbr".into());
        match platform::choose_path(true, &name) {
            Ok(Some(path)) => self.send(Action::Save(path, layer, None)),
            Ok(None) => {}
            Err(e) => self.ui_error = Some(e),
        }
    }
    fn save(&mut self) {
        if let Some(l) = self.layer.clone() {
            self.export_layer(l);
        } else {
            self.ui_error = Some("请先选择要导出的图层".into());
        }
    }
    fn new_workspace(&mut self) {
        self.begin_transition(project_ui::Transition::New);
    }
    fn close(&mut self, quit: bool) {
        self.begin_transition(if quit {
            project_ui::Transition::Quit
        } else {
            project_ui::Transition::Close
        });
    }
    fn object_buttons(&mut self, ui: &mut egui::Ui) {
        let enabled = self.usable() && drag::editable_selection(&self.view);
        if crate::ui::command_widgets::button(
            ui,
            command_ids::EDIT_DUPLICATE,
            crate::ui::command_widgets::CommandState::enabled(enabled),
        )
        .clicked()
        {
            self.send(Action::Duplicate);
        }
        if crate::ui::command_widgets::button(
            ui,
            command_ids::EDIT_DELETE,
            crate::ui::command_widgets::CommandState::enabled(enabled && !self.busy),
        )
        .clicked()
        {
            self.send(Action::Delete);
        }
    }
    fn history_buttons(&mut self, ui: &mut egui::Ui) {
        let undo = !self.busy && self.view.info.as_ref().is_some_and(|d| d.undo_entries > 0);
        let redo = !self.busy && self.view.info.as_ref().is_some_and(|d| d.redo_entries > 0);
        if crate::ui::command_widgets::button(
            ui,
            command_ids::EDIT_UNDO,
            crate::ui::command_widgets::CommandState::enabled(undo),
        )
        .clicked()
        {
            self.send(Action::History(false));
        }
        if crate::ui::command_widgets::button(
            ui,
            command_ids::EDIT_REDO,
            crate::ui::command_widgets::CommandState::enabled(redo),
        )
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
                    ui.label(
                        self.display_unit
                            .point_label(center, self.precision().resolution_mm),
                    );
                } else {
                    ui.colored_label(Color32::YELLOW, "选择集中心不可用，请使用明确 Pivot");
                }
                ui.radio_value(
                    &mut self.pivot_mode,
                    PivotMode::WorldOrigin,
                    "世界原点 (0, 0)",
                );
                ui.radio_value(
                    &mut self.pivot_mode,
                    PivotMode::Custom,
                    format!("自定义 X / Y {}", self.display_unit.suffix()),
                );
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
            ui.add_space(crate::ui::tokens::SPACING_LG);
            ui.label("镜像轴（选择集制造边界中心）");
            if let Some(center) = center {
                let horizontal = format!("水平镜像 · y = {}", self.length(center.y_mm));
                let vertical = format!("垂直镜像 · x = {}", self.length(center.x_mm));
                ui.radio_value(
                    &mut self.mirror_direction,
                    MirrorDirection::Horizontal,
                    horizontal,
                );
                ui.radio_value(
                    &mut self.mirror_direction,
                    MirrorDirection::Vertical,
                    vertical,
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
        self.size_width = self.display_unit.input(width);
        self.size_height = height.map_or_else(String::new, |value| self.display_unit.input(value));
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
            ui.label(format!(
                "{} {}",
                if rectangular { "宽度" } else { "直径" },
                self.display_unit.suffix()
            ));
            ui.add(
                egui::TextEdit::singleline(&mut self.size_width)
                    .id(egui::Id::new("flash-size-width")),
            );
        });
        if rectangular {
            ui.horizontal(|ui| {
                ui.label(format!("高度 {}", self.display_unit.suffix()));
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

impl CommandDispatcher for EditorApp {
    type Outcome = bool;

    fn dispatch(&mut self, command: CommandId) -> Self::Outcome {
        match command {
            command_ids::SNAP_TOGGLE => {
                self.object_snap.enabled = !self.object_snap.enabled;
                rcam_diagnostics::measurements(
                    rcam_diagnostics::Level::Info,
                    "snap.toggle",
                    &[("enabled", u64::from(self.object_snap.enabled))],
                );
                self.object_snap_runtime.reset();
                self.persist_project_view();
                true
            }
            _ => false,
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
        if let Some(rx) = &self.diagnostic_export {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.toast = Some(("诊断包已导出".into(), Instant::now()));
                    self.diagnostic_export = None;
                }
                Ok(Err(error)) => {
                    self.ui_error = Some(error);
                    self.diagnostic_export = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ui_error = Some("诊断包导出线程已停止".into());
                    self.diagnostic_export = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
            }
        }

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
            recovery::complete_write(
                &mut self.pending_recovery_identity,
                &mut self.last_recovered_identity,
                self.view.error.is_none(),
                self.view.info.as_ref(),
            );
            if let (Some(title), Some(error)) = (
                self.pending_project_error_title.take(),
                self.view.error.as_ref(),
            ) {
                self.project_error =
                    Some((title.into(), format!("{}: {}", error.code, error.message)));
            }
            if self.view.error.is_none()
                && matches!(self.view.message.as_str(), "工程已打开" | "工程已保存")
            {
                if let Some(path) = self
                    .view
                    .info
                    .as_ref()
                    .and_then(|d| d.project_path.as_ref())
                {
                    self.prefs.remember(std::path::PathBuf::from(path));
                    if let Some(store) = preferences::AppPreferences::path() {
                        let _ = self.prefs.save(&store);
                    }
                }
                if self.view.message == "工程已保存"
                    && let (Some(dir), Some(info)) =
                        (recovery::directory(), self.view.info.as_ref())
                {
                    recovery::remove(&dir, &info.project_id);
                }
            }
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
            if self.view.display_transient.is_some() {
                rcam_diagnostics::runtime_event(
                    rcam_diagnostics::Level::Warn,
                    "render.last_good_frame_fallback",
                );
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
                self.object_snap_runtime.clear_cache();
                self.last_good = None;
                self.modal = None;
                self.text.cancel();
                self.measure.clear();
                self.drag = None;
                self.fit = self.view.info.is_some();
                self.layer_dialog = None;
                self.toast = None;
                self.dx = "0".into();
                self.dy = "0".into();
                self.angle = "90".into();
                self.pivot_mode = PivotMode::SelectionCenter;
                self.pivot_x = "0".into();
                self.pivot_y = "0".into();
                self.size_aperture_id = None;
                self.size_width.clear();
                self.size_height.clear();
                self.restore_project_view();
            }
            self.saved_for_transition();
            if self.quit_after_close && self.view.info.is_none() {
                self.allow_quit = true;
            }
            self.accept_layer_replies(now);
            if self.view.import.is_some() {
                // A new import shows everything that is now visible.
                self.fit = true;
            }
            if let Some(bounds) = self.view.focus_bounds {
                self.camera.fit(Some(bounds), self.canvas_rect);
            }
            self.last_structure_serial = self.view.structure_serial;
        }
        self.tick_recovery(now);
        if self
            .view
            .info
            .as_ref()
            .is_some_and(|info| info.project_dirty)
        {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, born)| now.duration_since(*born).as_secs() >= 10)
        {
            self.toast = None;
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
            let name = d
                .project_path
                .as_ref()
                .and_then(|path| std::path::Path::new(path).file_name())
                .map_or_else(
                    || "Untitled".into(),
                    |name| name.to_string_lossy().into_owned(),
                );
            format!("RCam — {name}{}", if d.project_dirty { " *" } else { "" })
        });
        if self.last_title != title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        if !self.busy && self.modal.is_none() {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            let paths: Vec<_> = dropped.iter().filter_map(|f| f.path.clone()).collect();
            if !paths.is_empty()
                && let Some(probe) = self.probe.as_mut()
            {
                probe.drops += 1;
                probe.action(&format!("DROP_FILES n={}", paths.len()));
            }
            if !paths.is_empty() && self.layer_dialog.is_none() {
                // Every dropped file becomes its own layer; all succeed or none is added.
                self.send(Action::ImportGerbers(paths));
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
            || self.layer_dialog.is_some()
            || self.close_prompt
            || self.replace_project_path.is_some()
            || self.project_error.is_some()
            || self.recovery_candidate.is_some()
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
        self.operation_source = rcam_diagnostics::Source::Shortcut;
        let text_focus = self.text_input_at_event || ctx.wants_keyboard_input();
        if !modal_open && !self.ime_active && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.measure.clear();
            if self.text.floating.is_some() {
                self.text.resume_dialog();
                self.modal = Some(ActiveModal::Text);
                ctx.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                });
            } else if self.tool == tools::ActiveTool::Text {
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
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::O) && !self.busy {
                    self.file_shortcut('o', false);
                }
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::N) && !self.busy {
                    self.file_shortcut('n', false);
                }
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::I) && !self.busy {
                    self.file_shortcut('i', false);
                }
                if i.consume_key(egui::Modifiers::COMMAND, egui::Key::W) && !self.busy {
                    self.file_shortcut('w', false);
                }
                if i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::E,
                ) && self.usable()
                {
                    self.file_shortcut('e', true);
                }
                if i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::S,
                ) {
                    self.file_shortcut('s', true);
                } else if i.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
                    self.file_shortcut('s', false);
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
                if i.consume_key(egui::Modifiers::NONE, egui::Key::F3)
                    && let Resolution::Command(command) = ShortcutResolver::resolve(
                        &Keymap::standard(),
                        &[ShortcutContext::Canvas],
                        Shortcut::new(Modifiers::NONE, Key::F(3)),
                    )
                {
                    self.dispatch(command);
                }
            });
        }
        self.operation_source = rcam_diagnostics::Source::Menu;
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if modal_open {
                ui.disable();
            }
            egui::MenuBar::new().ui(ui, |ui| {
                ui.strong("RCam");
                ui.separator();
                ui.menu_button("文件", |ui| {
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_NEW_PROJECT, crate::ui::command_widgets::CommandState::enabled(!self.busy)).clicked()
                    {
                        self.dispatch_file_command(command_ids::FILE_NEW_PROJECT);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_OPEN_PROJECT, crate::ui::command_widgets::CommandState::enabled(!self.busy)).clicked() {
                        self.dispatch_file_command(command_ids::FILE_OPEN_PROJECT);
                        ui.close();
                    }
                    ui.menu_button("打开最近使用的工程", |ui| {
                        for path in self.prefs.recent_projects.clone() {
                            let label = path.file_name().unwrap_or_default().to_string_lossy();
                            if ui.button(label).clicked() {
                                if path.is_file() { self.begin_transition(project_ui::Transition::Open(path)); }
                                else { self.ui_error = Some("最近使用的工程文件不存在；可从列表移除".into()); }
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("移除失效路径").clicked() {
                            self.prefs.recent_projects.retain(|path| path.is_file());
                            if let Some(store) = preferences::AppPreferences::path() { let _ = self.prefs.save(&store); }
                            ui.close();
                        }
                    });
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_SAVE_PROJECT, crate::ui::command_widgets::CommandState::enabled(!self.busy && self.view.info.is_some())).clicked() {
                        self.dispatch_file_command(command_ids::FILE_SAVE_PROJECT);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_SAVE_PROJECT_AS, crate::ui::command_widgets::CommandState::enabled(!self.busy && self.view.info.is_some())).clicked() {
                        self.dispatch_file_command(command_ids::FILE_SAVE_PROJECT_AS);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_CLOSE_PROJECT, crate::ui::command_widgets::CommandState::enabled(!self.busy && self.view.info.is_some())).clicked() {
                        self.dispatch_file_command(command_ids::FILE_CLOSE_PROJECT);
                        ui.close();
                    }
                    ui.separator();
                    if crate::ui::command_widgets::button(
                        ui,
                        command_ids::FILE_IMPORT_GERBER,
                        crate::ui::command_widgets::CommandState::enabled(!self.busy),
                    )
                    .clicked()
                    {
                        self.dispatch_file_command(command_ids::FILE_IMPORT_GERBER);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(
                        ui,
                        command_ids::LAYER_CREATE,
                        crate::ui::command_widgets::CommandState::enabled(!self.busy),
                    )
                    .clicked()
                    {
                        self.create_empty_layer();
                        ui.close();
                    }
                    ui.separator();
                    if crate::ui::command_widgets::button(
                        ui,
                        command_ids::FILE_EXPORT_GERBER,
                        crate::ui::command_widgets::CommandState::enabled(
                            self.usable() && self.layer.is_some(),
                        ),
                    )
                    .clicked()
                    {
                        self.dispatch_file_command(command_ids::FILE_EXPORT_GERBER);
                        ui.close();
                    }
                });
                ui.menu_button("编辑", |ui| {
                    self.history_buttons(ui);
                    self.object_buttons(ui);
                    ui.separator();
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
                                    ui.close();
                                }
                            }
                        },
                    );
                });
                ui.menu_button("插入", |ui| {
                    if crate::ui::command_widgets::button(
                        ui,
                        command_ids::TOOL_TEXT,
                        crate::ui::command_widgets::CommandState::enabled(self.usable()),
                    )
                    .clicked()
                    {
                        self.open_modal(ActiveModal::Text);
                        ui.close();
                    }
                });
                ui.menu_button("工具", |ui| {
                    for (label, tool) in [
                        ("选择", tools::ActiveTool::Select),
                        (
                            crate::ui::command_widgets::descriptor(command_ids::TOOL_MEASURE)
                                .label,
                            tools::ActiveTool::Measure,
                        ),
                    ] {
                        if ui.button(label).clicked() {
                            self.text.cancel();
                            self.tool = tool;
                            self.measure.clear();
                            ui.close();
                        }
                    }
                });
                ui.menu_button("图层", |ui| {
                    if crate::ui::command_widgets::button(
                        ui,
                        command_ids::LAYER_CREATE,
                        crate::ui::command_widgets::CommandState::enabled(!self.busy),
                    )
                    .clicked()
                    {
                        self.create_empty_layer();
                        ui.close();
                    }
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("导入 Gerber…"))
                        .clicked()
                    {
                        self.import_gerbers();
                        ui.close();
                    }
                    ui.separator();
                    let active = self.layer.clone().filter(|_| !self.busy);
                    for (label, dialog) in [
                        ("重命名当前图层…", 0),
                        ("当前图层设置…", 1),
                        ("当前图层分类设置…", 2),
                    ] {
                        if ui
                            .add_enabled(active.is_some(), egui::Button::new(label))
                            .clicked()
                            && let Some(layer) = active.clone()
                        {
                            let name = self
                                .view
                                .layers
                                .iter()
                                .find(|l| l.layer_id == layer)
                                .map(|l| l.display_name.clone())
                                .unwrap_or_default();
                            self.open_layer_dialog(match dialog {
                                0 => layer_panel::LayerDialog::Rename { layer, text: name },
                                1 => layer_panel::LayerDialog::Settings { layer, name },
                                _ => layer_panel::LayerDialog::Categories { layer },
                            });
                            ui.close();
                        }
                    }
                    if crate::ui::command_widgets::button(
                        ui,
                        command_ids::LAYER_DELETE,
                        crate::ui::command_widgets::CommandState::enabled(active.is_some()),
                    )
                    .clicked()
                        && let Some(layer) = active
                    {
                        self.layer_dialog =
                            Some(layer_panel::LayerDialog::DeletePending { layer: layer.clone() });
                        self.send(Action::LayerSummary(layer));
                        ui.close();
                    }
                    ui.separator();
                    for (label, visible) in [("显示全部图层", true), ("隐藏全部图层", false)] {
                        if ui
                            .add_enabled(
                                !self.busy && !self.view.layers.is_empty(),
                                egui::Button::new(label),
                            )
                            .clicked()
                        {
                            self.send(Action::SetAllLayersVisible(visible));
                            ui.close();
                        }
                    }
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("重置全部图层颜色"))
                        .clicked()
                    {
                        self.send(Action::ResetLayerColors);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            !self.busy && self.view.layers.iter().any(|l| l.is_solo),
                            egui::Button::new("取消独奏"),
                        )
                        .clicked()
                    {
                        self.send(Action::SetSoloLayer(None));
                        ui.close();
                    }
                });
                ui.menu_button("视图", |ui| {
                    if crate::ui::command_widgets::checkbox(
                        ui,
                        command_ids::VIEW_GRID_TOGGLE,
                        &mut self.grid.visible,
                        true,
                    ).changed() { self.persist_project_view(); }
                    if ui.button("网格 / 吸附设置…").clicked() {
                        self.open_modal(ActiveModal::Grid);
                        ui.close();
                    }
                    let mut object_snap_enabled = self.object_snap.enabled;
                    if crate::ui::command_widgets::checkbox(
                        ui,
                        command_ids::SNAP_TOGGLE,
                        &mut object_snap_enabled,
                        true,
                    )
                    .changed()
                    {
                        self.dispatch(command_ids::SNAP_TOGGLE);
                    }
                    if ui.button("Object Snap 设置…").clicked() {
                        self.open_modal(ActiveModal::ObjectSnap);
                        ui.close();
                    }
                    self.unit_controls(ui);
                    if ui.button("适合窗口  F").clicked() {
                        self.fit = true;
                        ui.close();
                    }
                });
                ui.menu_button("帮助", |ui| {
                    ui.label("S4-B1 · 多 Gerber 图层工作区");
                    ui.label(
                        "几何选择包括 Clear；Ctrl 点击加选，Shift 点击减选，双向框选，整组编辑。",
                    );
                    ui.label("中键拖动 / 双指滚动平移；捏合 / Cmd+滚动缩放。");
                    ui.label("Gerber 只导入 / 导出：导出必须选择新文件名，不会保存工作区。Windows 延后验收。");
                    ui.separator();
                    ui.label(&self.adapter);
                    ui.separator();
                    if let Some(runtime) = rcam_diagnostics::global() {
                        ui.label("日志仅保存在本机；诊断包不包含工程、Gerber 或字体。");
                        for (label, level) in [("Info", rcam_diagnostics::Level::Info), ("Debug", rcam_diagnostics::Level::Debug), ("Trace（仅本次）", rcam_diagnostics::Level::Trace)] {
                            if ui.button(label).clicked() {
                                runtime.set_level(level);
                                self.prefs.logging_level = if level == rcam_diagnostics::Level::Trace { rcam_diagnostics::Level::Info } else { level };
                                if let Some(path) = preferences::AppPreferences::path() { let _ = self.prefs.save(&path); }
                            }
                        }
                        if ui.button("打开日志文件夹").clicked() {
                            let _ = std::process::Command::new("open").arg(runtime.directory()).spawn();
                            ui.close();
                        }
                        if ui.add_enabled(self.diagnostic_export.is_none(), egui::Button::new("导出诊断包…")).clicked() {
                            match platform::choose_diagnostics() {
                                Ok(Some(path)) => {
                                    let runtime = runtime.clone();
                                    let (tx, rx) = std::sync::mpsc::channel();
                                    self.diagnostic_export = Some(rx);
                                    std::thread::spawn(move || { let _ = tx.send(runtime.export(&path).map_err(|_| "诊断包导出失败：请使用新文件名并检查写入权限".to_string())); });
                                }
                                Ok(None) => {},
                                Err(error) => self.ui_error = Some(error),
                            }
                            ui.close();
                        }
                    } else { ui.label("本次日志不可用：无法打开本机日志目录。"); }

                });
            });
            ui.horizontal(|ui| {
                self.operation_source = rcam_diagnostics::Source::Toolbar;
                if crate::ui::buttons::toolbar(ui, "导入…", !self.busy).clicked() {
                    self.import_gerbers();
                }
                if ui.button("适合窗口").clicked() {
                    self.fit = true;
                }
                if ui
                    .add_enabled(self.layer.is_some() && !self.busy, egui::Button::new("适合当前图层"))
                    .clicked()
                    && let Some(layer) = self.layer.clone()
                {
                    self.send(Action::FitLayer(layer));
                }
                ui.separator();
                self.history_buttons(ui);
                ui.separator();
                ui.label(RichText::new("几何多选").color(Color32::from_rgb(100, 206, 183)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui::buttons::toolbar(
                    ui,
                    "导出图层…",
                    self.usable() && self.layer.is_some(),
                )
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
                ui.label(format!(
                    "{:.2} 点/{}",
                    self.camera.scale * self.display_unit.mm_per_unit(),
                    self.display_unit.suffix()
                ));
                ui.label(format!("网格 {}", self.length(self.grid.spacing_mm)));
                ui.label(if self.object_snap.enabled {
                    "Object Snap ON"
                } else {
                    "Object Snap OFF"
                });
                if let Some(resolution) = &self.object_snap_runtime.current
                    && let Some(kind) = resolution.kind
                {
                    ui.label(format!(
                        "{}  X {}  Y {}",
                        object_snap::kind_label(kind),
                        self.length(resolution.point.x_mm),
                        self.length(resolution.point.y_mm)
                    ));
                }
                if let Some(o) = self.view.selected.primary() {
                    ui.label(format!("选中 {}", o.object.object_id));
                }
                ui.label(&self.view.message);
                // Display-transient diagnostics keep the last-good frame on screen and
                // are reported here instead of covering the canvas.
                if let (Some(e), Some(_)) = (&self.display_error, &self.last_good) {
                    ui.label(RichText::new(format!("显示诊断：{e}")).weak());
                }
            });
            if let Some((text, _)) = self.toast.clone() {
                ui.horizontal(|ui| {
                    ui.label(text);
                    if ui
                        .add_enabled(
                            !self.busy
                                && self.view.info.as_ref().is_some_and(|d| d.undo_entries > 0),
                            egui::Button::new("撤销"),
                        )
                        .clicked()
                    {
                        self.toast = None;
                        self.send(Action::History(false));
                    }
                    if ui.small_button("×").clicked() {
                        self.toast = None;
                    }
                });
            }
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
        self.operation_source = rcam_diagnostics::Source::Menu;
        let layer_panel = egui::SidePanel::left("layers")
            .resizable(true)
            .default_width(self.prefs.panel_width.unwrap_or(250.))
            // Below ~240 px the six fixed controls leave no room for the name and the
            // truncated label would draw over them (found in the native §107 check).
            .width_range(240.0..=480.)
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
                self.layer_panel(ui);
            });
        self.layer_panel_rect = layer_panel.response.rect;
        if ctx.input(|i| i.pointer.any_released())
            && (self.prefs.panel_width.unwrap_or(250.) - self.layer_panel_rect.width()).abs() > 1.
        {
            self.prefs.panel_width = Some(self.layer_panel_rect.width().clamp(240., 480.));
            if let Some(path) = preferences::AppPreferences::path() {
                let _ = self.prefs.save(&path);
            }
        }
        self.operation_source = rcam_diagnostics::Source::Modal;
        egui::SidePanel::right("properties")
            .default_width(260.)
            .width_range(230.0..=380.)
            .show(ctx, |ui| {
                ui.add_space(crate::ui::tokens::SPACING_XL);
                if modal_open {
                    ui.disable();
                }
                ui.heading("对象属性");
                ui.add_space(crate::ui::tokens::SPACING_XL);
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
                    for line in metrics_panel::lines(
                        &self.view,
                        self.display_unit,
                        self.precision().resolution_mm,
                    ) {
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
                        geometry_properties(
                            ui,
                            &o.object.geometry,
                            self.display_unit,
                            self.precision().resolution_mm,
                        );
                        if let editor_core::SemanticGeometry::Flash { aperture_id, .. } =
                            &o.object.geometry
                        {
                            ui.label(format!("光圈：{aperture_id}"));
                            if let Some(a) =
                                self.view.apertures.iter().find(|a| &a.id == aperture_id)
                            {
                                aperture_properties(
                                    ui,
                                    &a.shape,
                                    self.display_unit,
                                    self.precision().resolution_mm,
                                );
                            }
                        }
                        ui.separator();
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
                        ui.label("点击图形查看对象，并输入当前单位的位移或变换参数。");
                    }
                    ui.separator();
                    if let Some(d) = &self.view.info {
                        ui.strong(if d.project_dirty { "工程有未保存更改" } else { "工程已保存" });
                        ui.label(
                            RichText::new(
                                "Gerber 导出只写所选图层，不保存 .rcam 工程，也不清除工程未保存标记。",
                            )
                            .small()
                            .weak(),
                        );
                    }
                });
            });
        self.operation_source = rcam_diagnostics::Source::Toolbar;
        egui::TopBottomPanel::top("grid-tools").show(ctx, |ui| {
            if modal_open {
                ui.disable();
            }
            ui.horizontal_wrapped(|ui| {
                if ui.checkbox(&mut self.grid.visible, "网格").changed() {
                    self.persist_project_view();
                }
                if ui.button("网格 / 吸附设置…").clicked() {
                    self.open_modal(ActiveModal::Grid);
                }
                if ui
                    .selectable_label(
                        self.object_snap.enabled,
                        if self.object_snap.enabled {
                            "Object Snap ON"
                        } else {
                            "Object Snap OFF"
                        },
                    )
                    .clicked()
                {
                    self.dispatch(command_ids::SNAP_TOGGLE);
                }
                if ui.button("Object Snap 设置…").clicked() {
                    self.open_modal(ActiveModal::ObjectSnap);
                }
                self.unit_controls(ui);
                let old = self.tool;
                ui.selectable_value(&mut self.tool, tools::ActiveTool::Select, "选择");
                ui.selectable_value(&mut self.tool, tools::ActiveTool::Measure, "测距");
                if ui
                    .add_enabled(self.usable(), egui::Button::new("文本…"))
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
        self.operation_source = rcam_diagnostics::Source::Canvas;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(14, 18, 22)))
            .show(ctx, |ui| {
                let mut cursor_label = None;
                let mut measure_hover = None;
                self.object_snap_runtime.current = None;
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
                let (scroll, zoom, pinch) = ctx.input(|i| {
                    (
                        i.smooth_scroll_delta,
                        i.zoom_delta(),
                        i.raw
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Zoom(_))),
                    )
                });
                if !modal_open && !text_focus && self.drag.is_none() {
                    if r.hovered() {
                        self.camera.pan(scroll);
                    }
                    // Native trackpad pinch can arrive without a hovered pointer.
                    // In that case, keep the gesture anchored to the canvas center.
                    if (r.hovered() || pinch) && zoom != 1. {
                        let extent = self.view.scene.as_ref().map_or(1e-12, |s| {
                            s.objects
                                .iter()
                                .flat_map(|o| o.bounds)
                                .map(|v| f64::from(v).abs())
                                .fold(1e-12, f64::max)
                        });
                        self.camera.zoom_view(
                            f64::from(zoom),
                            r.hover_pos().unwrap_or(rect.center()),
                            rect,
                            ctx.pixels_per_point(),
                            self.view.bounds,
                            extent,
                        );
                    }
                }
                if !modal_open
                    && r.hovered()
                    && let Some(pos) = r.hover_pos()
                {
                    let w = self.camera.world(pos, rect);
                    cursor_label = Some(format!("X {}  Y {}", self.length(w.x_mm), self.length(w.y_mm)));
                    if self.tool == tools::ActiveTool::Text && !modal_open && !text_focus {
                        let text_grid = tools::GridSettings {
                            snap_enabled: self.grid.snap_enabled && self.text.snap_text,
                            ..self.grid
                        };
                        let point = match self.object_snap_runtime.resolve(
                            w,
                            &self.object_snap,
                            text_grid,
                            self.camera,
                            ctx.pixels_per_point(),
                            self.view.snap_snapshot.as_deref(),
                            &self.view.snap_index,
                            &self.view.layers,
                            None,
                            ctx.input(|input| input.modifiers.alt),
                        ) {
                            Ok(resolution) => resolution.point,
                            Err(error) => {
                                self.ui_error = Some(error);
                                w
                            }
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
                        match self.object_snap_runtime.resolve(
                            w,
                            &self.object_snap,
                            self.grid,
                            self.camera,
                            ctx.pixels_per_point(),
                            self.view.snap_snapshot.as_deref(),
                            &self.view.snap_index,
                            &self.view.layers,
                            None,
                            ctx.input(|input| input.modifiers.alt),
                        ) {
                            Ok(resolution) => {
                                measure_hover = Some((resolution.point, resolution.kind));
                                if r.clicked_by(egui::PointerButton::Primary) {
                                    self.measure
                                        .click_snapped(resolution.point, resolution.kind);
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
                        if self.drag.is_some() {
                            self.send(Action::ProbeDrag(
                                self.camera.world(press, rect),
                                self.camera.tolerance(ctx.pixels_per_point()),
                            ));
                        }
                    }
                }
                let drag_update = self.drag.as_ref().and_then(|drag| {
                    (!drag.released)
                        .then(|| ctx.input(|input| input.pointer.interact_pos()))
                        .flatten()
                        .map(|position| {
                            (
                                position,
                                drag.snap_exclusions().cloned().unwrap_or_default(),
                            )
                        })
                });
                let (snapped_drag, drag_snap_error) = drag_update.as_ref().map_or(
                    (None, None),
                    |(position, excluded)| {
                        let raw = self.camera.world(*position, rect);
                        match self.object_snap_runtime.resolve(
                        raw,
                        &self.object_snap,
                        self.grid,
                        self.camera,
                        ctx.pixels_per_point(),
                        self.view.snap_snapshot.as_deref(),
                        &self.view.snap_index,
                        &self.view.layers,
                        Some(excluded),
                        ctx.input(|input| input.modifiers.alt),
                    ) {
                        Ok(resolution) => (Some(resolution.point), None),
                        Err(error) => {
                            self.ui_error = Some(error.clone());
                            (None, Some(error))
                        }
                    }
                    },
                );
                if let Some(drag) = &mut self.drag {
                    if let Some((position, _)) = drag_update {
                        drag.set_snap_error(drag_snap_error);
                        drag.update_snapped(position, snapped_drag);
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
                match measure_hover {
                    Some((point, kind)) => self.measure.hover_snapped(Some(point), kind),
                    None => self.measure.hover(None),
                }
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
                    && (outside
                        || ppm > self.view.render_ppm
                        || ppm < self.view.render_ppm / display::LOD_MAX_ZOOM_OUT);
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
                            });
                            rendered = true;
                        }
                        Err(e) => {
                            if self.display_error.as_ref() != Some(&e) {
                                rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Warn, "render.display_prepare_failed");
                            }
                            self.display_error = Some(e);
                            self.drag = None;
                        }
                    }
                }
                self.display_pending = self.view.info.is_some() && !rendered;
                if self.view.blocked.is_some() {
                    self.last_good = None;
                }
                if let Some(last) = &self.last_good {
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
                if let Some(resolution) = &self.object_snap_runtime.current {
                    object_snap::paint_marker(
                        &painter,
                        self.camera,
                        rect,
                        ctx.pixels_per_point(),
                        resolution,
                    );
                }
                if self.tool == tools::ActiveTool::Measure {
                    self.measure
                        .paint_in(&painter, self.camera, rect, self.display_unit, self.precision().resolution_mm);
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
                    if let Some(e) = self.view.blocked.as_ref() {
                        // Semantic / manufacturing blocked: real error, no "zoom out" advice.
                        format!("无法安全编辑\n{e}\n可撤销最近修改或关闭/修复输入文件")
                    } else if let (Some(e), None) = (self.display_error.as_ref(), &self.last_good) {
                        // Display-only problem and nothing to show yet.
                        if e.starts_with("正在准备") {
                            format!("{e}…")
                        } else {
                            format!("暂时无法显示\n{e}")
                        }
                    } else if needs_lod && self.last_good.is_none() {
                        "正在准备画布…".into()
                    } else if self.view.info.is_none() {
                        "正在准备工作区…".into()
                    } else if self.view.layers.is_empty() {
                        "工作区为空\n使用左侧 ＋ 导入 Gerber（可多选），或新建空图层\n也可从 Finder 拖入多个文件".into()
                    } else if self.view.layers.iter().all(|l| !l.effective_visible) {
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
        self.project_prompts(ctx);
        self.recovery_prompt(ctx);
        if !self.close_prompt {
            self.parameter_modal(ctx);
            self.layer_dialogs(ctx);
        }
        self.probe_frame(ctx);
        if self.modal.is_none()
            && !self.close_prompt
            && let Some(e) = self.view.error.clone()
            && e.code == "CONFIRMATION_REQUIRED"
            && e.details.get("categories").is_some()
        {
            egui::Modal::new(egui::Id::new("metadata-confirmation")).show(ctx, |ui| {
                ui.heading("确认导出为几何文件");
                if e.details["categories"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item == "compatibility_issues"))
                {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "此图层含非规范几何。导出保留 RCam 的兼容解释；其他 Gerber 软件可能显示不同。",
                    );
                }
                if e.details["compatibility_warning"]["contains_lossy_zero_aperture_conversion"] == true {
                    ui.colored_label(egui::Color32::RED, "源文件的零直径光圈已被转换成 2 µm 有面积开口；导出会改变制造图形。");
                }
                if e.details["compatibility_warning"]["contains_nonstandard_compatibility_region"] == true {
                    ui.colored_label(egui::Color32::RED, format!(
                        "含 {} 个非标准兼容 Region；未获独立 CAM 制造等价认证。",
                        e.details["compatibility_warning"]["nonstandard_compatibility_region_count"]
                    ));
                }
                if let Some(issues) = e.details["compatibility_warning"]["issue_categories"].as_array() {
                    for issue in issues.iter().take(12).filter_map(|value| value.as_str()) {
                        ui.label(issue);
                    }
                    if issues.len() > 12 {
                        ui.label(format!("另有 {} 条兼容问题，见图层设置", issues.len() - 12));
                    }
                }
                ui.label("导出需确认以下来源信息或兼容告警：");
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
        if self.modal.is_none()
            && !self.close_prompt
            && let Some(e) = self.view.error.clone()
            && e.code == "CONFIRMATION_REQUIRED"
            && e.details["reason"] == "compatibility_precision_override"
        {
            egui::Modal::new(egui::Id::new("compatibility-precision-confirmation")).show(
                ctx,
                |ui| {
                    ui.heading("确认兼容几何导出精度");
                    ui.label(format!(
                        "工程精度 {} mm 会破坏此图层的兼容几何；本次导出需要 {} mm。工程设置不变。",
                        e.details["project_resolution_mm"], e.details["required_resolution_mm"]
                    ));
                    if ui.button("取消").clicked() {
                        self.view.error = None;
                    }
                    if ui.button("仅本次按所需精度导出").clicked()
                        && self.view.info.as_ref().is_some_and(|d| {
                            e.details["gui_document_id"] == d.document_id
                                && e.details["gui_revision"] == d.revision
                        })
                        && let (Some(path), Some(layer), Some(q)) = (
                            e.details["gui_target_path"].as_str(),
                            e.details["gui_layer_id"].as_str(),
                            e.details["required_resolution_mm"].as_f64(),
                        )
                    {
                        let categories =
                            serde_json::from_value(e.details["gui_confirmed_categories"].clone())
                                .ok()
                                .flatten();
                        self.view.error = None;
                        self.send(Action::SaveWithPrecision(
                            path.into(),
                            layer.into(),
                            categories,
                            q,
                        ));
                    }
                },
            );
        }
    }
}
fn geometry_properties(
    ui: &mut egui::Ui,
    g: &editor_core::SemanticGeometry,
    unit: tools::DisplayUnit,
    resolution: f64,
) {
    let length = |v| unit.format_length(v, resolution);
    use editor_core::SemanticGeometry::*;
    let point = |ui: &mut egui::Ui, label: &str, p: editor_core::MmPoint| {
        ui.label(format!("{label}  {}, {}", length(p.x_mm), length(p.y_mm)));
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
            ui.label(format!("线宽 {}", length(*width_mm)));
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
            ui.label(format!(
                "截面 {} × {}",
                length(*width_mm),
                length(*height_mm)
            ));
        }
        Arc { path, width_mm } => {
            ui.strong("Arc · 圆弧");
            point(ui, "起点", path.start);
            point(ui, "终点", path.end);
            point(ui, "圆心", path.center);
            ui.label(format!(
                "半径 {} / 线宽 {}",
                length(path.radius()),
                length(*width_mm)
            ));
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
        // No Block Editor ships this phase; a live document cannot contain
        // one yet (S4-B2 §27/§67).
        BlockInstance {
            definition_id,
            transform,
        } => {
            ui.strong("Block Instance · 块实例");
            ui.label(format!("定义 {}", definition_id.0));
            ui.label(format!(
                "角度 {:.4}° / 镜像 {}",
                transform.rotation_deg, transform.mirror
            ));
        }
    }
}
fn main() -> eframe::Result {
    let diagnostics = std::env::var_os("HOME").and_then(|home| {
        rcam_diagnostics::Runtime::start(
            std::path::PathBuf::from(home).join("Library/Logs/RCam"),
            env!("CARGO_PKG_VERSION"),
            option_env!("RCAM_BUILD_COMMIT").unwrap_or("unknown"),
        )
        .ok()
    });
    if let Some(guard) = &diagnostics {
        guard.install();
        if let Some(path) = preferences::AppPreferences::path() {
            guard
                .runtime()
                .set_level(preferences::AppPreferences::load(&path).logging_level);
        }
    }

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

fn aperture_properties(
    ui: &mut egui::Ui,
    a: &editor_core::ApertureShape,
    unit: tools::DisplayUnit,
    resolution: f64,
) {
    let length = |v| unit.format_length(v, resolution);
    use editor_core::ApertureShape::*;
    let hole = match a {
        Circle {
            diameter_mm,
            hole_diameter_mm,
        } => {
            ui.label(format!("圆形 · 直径 {}", length(*diameter_mm)));
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
            ui.label(format!(
                "宽 {} / 高 {}",
                length(*width_mm),
                length(*height_mm)
            ));
            *hole_diameter_mm
        }
        Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            ui.label(format!(
                "{vertices} 边形 · 外接直径 {}",
                length(*diameter_mm)
            ));
            ui.label(format!("光圈角度 {rotation_deg}°"));
            *hole_diameter_mm
        }
        Macro { primitives } => {
            ui.label(format!("宏光圈 · {} 个局部原语", primitives.len()));
            None
        }
    };
    if let Some(h) = hole {
        ui.label(format!("局部孔径 {}", length(h)));
    }
}
