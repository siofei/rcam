#[cfg(test)]
mod app_tests;
mod array_ui;
mod block_display;
mod block_ui;
mod camera;
mod candidates_ui;
mod components_ui;
mod display;
#[cfg(test)]
mod display_tests;
mod drag;
mod font_catalog;
mod gpu;
mod grip;
mod layer_panel;
#[cfg(test)]
mod layer_tests;
mod metrics_panel;
mod modal;
#[cfg(test)]
mod perf_tests;
use modal::ActiveModal;
#[cfg(test)]
mod batch_drag_tests;
#[cfg(feature = "internal-evidence")]
mod native_a2;
#[cfg(feature = "internal-evidence")]
mod native_batch_drag;
mod native_bench;
#[cfg(feature = "internal-evidence")]
mod native_d1;
#[cfg(feature = "internal-evidence")]
mod native_d2;
#[cfg(feature = "internal-evidence")]
mod native_i1;
mod native_probe;
#[cfg(feature = "internal-evidence")]
mod native_s5m1;
mod object_snap;
mod platform;
mod preferences;
mod project_ui;
mod recovery;
mod render_index;
#[cfg(test)]
mod s5m1_tests;
#[cfg(test)]
mod s5m2_tests;
mod selection;
mod shortcut_config;
mod shortcut_settings;
mod shortcut_store;
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
    CommandDispatcher, CommandId, Resolution, Shortcut, ShortcutContext, ShortcutResolver,
    ids as command_ids,
};
use editor_service::{AlignmentMode, DistributionAxis};
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
    components: components_ui::UiState,
    block: block_ui::UiState,
    operation_source: rcam_diagnostics::Source,
    diagnostic_export: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    tx: SyncSender<(
        u64,
        rcam_diagnostics::Source,
        Action,
        editor_service::task::TaskContext,
    )>,
    pending_task: Option<editor_service::task::TaskContext>,
    viewport_task: Option<editor_service::task::TaskContext>,
    geometry_task: Option<editor_service::task::TaskContext>,
    geometry_context: Option<String>,
    rx: Receiver<(u64, View)>,
    view: View,
    busy: bool,
    viewport_sequence: Option<u64>,
    sequence: u64,
    request_failure_serial: u64,
    camera: Camera,
    click_navigation: selection::ClickNavigation,
    last_good: Option<LastFrame>,
    grid: tools::GridSettings,
    grid_visual: tools::GridVisualState,
    object_snap: object_snap::Settings,
    object_snap_runtime: object_snap::Runtime,
    draft_object_snap: object_snap::Settings,
    spacing: String,
    tool: tools::ActiveTool,
    text: text_tool::Draft,
    array: array_ui::Draft,
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
    shortcuts: shortcut_settings::Settings,
    recovery_candidate: Option<recovery::RecoveryMetadata>,
    recovery_prompt_reported: Option<String>,
    recovery_attempted_identity: Option<String>,
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
    grip: Option<grip::Session>,
    bench: Option<native_bench::NativeBench>,
    #[cfg(feature = "internal-evidence")]
    s5m1: Option<native_s5m1::Run>,
    #[cfg(feature = "internal-evidence")]
    a2: Option<native_a2::Run>,
    #[cfg(feature = "internal-evidence")]
    batch_drag: Option<native_batch_drag::Run>,
    #[cfg(feature = "internal-evidence")]
    i1: Option<native_i1::Run>,
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
fn task_reply_matches(
    task: &editor_service::task::TaskContext,
    current: &View,
    result: &View,
) -> bool {
    use editor_service::task::{TaskState, TaskVersion};
    result.task_receipt.as_ref().is_some_and(|receipt| {
        receipt.task_id == task.task_id
            && receipt.input == task.input
            && receipt.input
                == TaskVersion::capture(
                    current.info.as_ref(),
                    current.task_generation,
                    current.rule_revision,
                )
            && receipt.result_version
                == TaskVersion::capture(
                    result.info.as_ref(),
                    result.task_generation,
                    result.rule_revision,
                )
            && matches!(
                receipt.state,
                TaskState::Completed | TaskState::Cancelled | TaskState::Failed
            )
    })
}
fn geometry_reply_matches(
    task: &editor_service::task::TaskContext,
    current: &View,
    result: &View,
    context: &str,
) -> bool {
    context == state::selection_geometry_identity(current)
        && result.selection_geometry_identity == context
        && result.selection_epoch == current.selection_epoch
        && editor_service::task::TaskVersion::capture(
            result.info.as_ref(),
            result.task_generation,
            result.rule_revision,
        ) == task.input
        && result.selection_geometry.as_ref().is_none_or(|value| {
            current.info.as_ref().is_some_and(|info| {
                value.document_id == info.document_id
                    && value.computed_revision == info.revision
                    && value.resolution_mm.to_bits()
                        == info.manufacturing_precision.resolution_mm.to_bits()
                    && value.selected_count == current.selected.ordered.len()
            })
        })
        && task_reply_matches(task, current, result)
        && result
            .task_receipt
            .as_ref()
            .is_some_and(|r| r.state == editor_service::task::TaskState::Completed)
}
fn viewport_requires_rebase(scene: &display::Scene, camera: Camera) -> bool {
    // Complete geometry coverage does not waive the existing local-f32
    // precision envelope. Re-anchor through the normal viewport worker.
    scene
        .scalar(camera.center.x_mm - scene.anchor.x_mm)
        .is_err()
        || scene
            .scalar(camera.center.y_mm - scene.anchor.y_mm)
            .is_err()
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
        // At most one display request plus one user operation. A read-only
        // viewport build never disables or consumes the user's next command.
        let (tx, request) = mpsc::sync_channel::<(
            u64,
            rcam_diagnostics::Source,
            Action,
            editor_service::task::TaskContext,
        )>(2);
        let (reply, rx) = mpsc::sync_channel(1);
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let mut model = Model::default();
            // Dev-only native GUI smoke path (S4-B2 Final Closeout, task
            // §17): no Block Editor GUI ships this phase, so a synthetic
            // Block fixture is loaded this way instead of through the UI.
            #[cfg(feature = "internal-evidence")]
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
            while let Ok((id, source, action, task)) = request.recv() {
                let start = Instant::now();
                #[cfg(feature = "internal-evidence")]
                let measured_action = native_s5m1::action_label(&action);
                #[cfg(feature = "internal-evidence")]
                native_a2::worker_begin(&task, &model.view);
                rcam_diagnostics::with_source(source, || model.run_task(task, action));
                #[cfg(feature = "internal-evidence")]
                native_a2::worker_finished(id, &model.view);
                #[cfg(feature = "internal-evidence")]
                native_s5m1::worker_result(id, measured_action, start, &model.view);
                if start.elapsed().as_millis() > 100 {
                    rcam_diagnostics::runtime_event(
                        rcam_diagnostics::Level::Warn,
                        "gui.worker.slow",
                    );
                }
                #[cfg(feature = "internal-evidence")]
                native_a2::returning(id);
                if reply.send((id, model.view.clone())).is_err() {
                    break;
                }
                ctx.request_repaint();
                #[cfg(feature = "internal-evidence")]
                native_s5m1::gpu_event("worker-request-repaint", 1);
            }
        });
        let gpu = cc
            .wgpu_render_state
            .as_ref()
            .expect("eframe wgpu renderer required");
        gpu.device.set_device_lost_callback(|reason, _message| {
            if reason != eframe::wgpu::DeviceLostReason::Destroyed {
                rcam_diagnostics::render_exception(rcam_diagnostics::RenderException::DeviceLost);
            }
        });
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
        let shortcuts = shortcut_settings::Settings::load_background(
            &cc.egui_ctx,
            shortcut_store::path(),
            editor_core::command::Platform::current(),
            !prefs.shortcut_overrides.is_empty(),
        );
        let mut app = Self {
            block: Default::default(),
            operation_source: rcam_diagnostics::Source::System,
            diagnostic_export: None,
            tx,
            pending_task: None,
            viewport_task: None,
            geometry_task: None,
            geometry_context: None,
            rx,
            view: View::default(),
            busy: false,
            viewport_sequence: None,
            sequence: 0,
            request_failure_serial: 0,
            camera: Camera::default(),
            click_navigation: Default::default(),
            last_good: None,
            grid: Default::default(),
            grid_visual: Default::default(),
            object_snap: Default::default(),
            object_snap_runtime: Default::default(),
            draft_object_snap: Default::default(),
            spacing: "0.1".into(),
            tool: Default::default(),
            text: Default::default(),
            components: components_ui::UiState::default(),
            array: array_ui::Draft::default(),
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
            shortcuts,
            prefs,
            recovery_candidate,
            recovery_prompt_reported: None,
            recovery_attempted_identity: None,
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
            grip: None,
            bench: native_bench::NativeBench::from_env(gpu.device.clone()),
            #[cfg(feature = "internal-evidence")]
            s5m1: native_s5m1::Run::from_env(gpu.device.clone()),
            #[cfg(feature = "internal-evidence")]
            a2: native_a2::Run::from_env(),
            #[cfg(feature = "internal-evidence")]
            batch_drag: native_batch_drag::Run::from_env(gpu.device.clone()),
            #[cfg(feature = "internal-evidence")]
            i1: native_i1::Run::from_env(gpu.device.clone()),
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
        } else if matches!(&a, Action::ArrayApply(..)) {
            self.array.source
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
        if self.busy || (matches!(a, Action::Viewport(..)) && self.viewport_sequence.is_some()) {
            return;
        }
        // A background measurement must not supersede a pending viewport reply
        // or replace another measurement's task identity.
        if matches!(a, Action::SelectionCenters(..))
            && (self.viewport_sequence.is_some() || self.geometry_task.is_some())
        {
            return;
        }
        self.pending_project_error_title = match &a {
            Action::OpenProject(..) | Action::RestoreProject(..) => Some("无法打开工程"),
            Action::SaveProject(..) => Some("无法保存工程"),
            _ => None,
        };
        if !matches!(a, Action::ProbeDrag(..) | Action::SelectionCenters(..)) {
            self.drag = None;
        }
        #[cfg(feature = "internal-evidence")]
        let i1_action = native_i1::action_detail(&a);
        let previous_sequence = self.sequence;
        self.sequence += 1;
        if self.modal.is_some()
            && matches!(
                a,
                Action::ArrayApply(..)
                    | Action::BlockEdit(..)
                    | Action::Move(..)
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
        let geometry = matches!(a, Action::SelectionCenters(..));
        let geometry_context = if let Action::SelectionCenters(identity, _) = &a {
            Some(identity.clone())
        } else {
            None
        };
        let viewport = matches!(a, Action::Viewport(..));
        let task = editor_service::task::TaskContext::new(
            self.sequence,
            editor_service::task::TaskVersion::capture(
                self.view.info.as_ref(),
                self.view.task_generation,
                self.view.rule_revision,
            ),
        );
        match self.tx.try_send((self.sequence, source, a, task.clone())) {
            Ok(()) => {
                #[cfg(feature = "internal-evidence")]
                native_i1::accepted_action(i1_action, self.sequence);
                if geometry {
                    self.geometry_task = Some(task);
                    self.geometry_context = geometry_context;
                } else if viewport {
                    if let Some(old) = &self.geometry_task {
                        old.cancel_token.cancel();
                    }
                    self.viewport_sequence = Some(self.sequence);
                    self.viewport_task = Some(task);
                } else {
                    if let Some(old) = &self.geometry_task {
                        old.cancel_token.cancel();
                    }
                    if let Some(old) = &self.viewport_task {
                        old.cancel_token.cancel();
                    }
                    self.pending_task = Some(task);
                    self.busy = true;
                }
                self.ui_error = None;
            }
            Err(e) => {
                self.request_failure_serial = self.request_failure_serial.wrapping_add(1);
                self.sequence = previous_sequence;
                self.modal_pending = None;
                self.ui_error = Some(format!("后台任务不可用：{e}"));
            }
        }
    }
    fn route_shortcuts(&mut self, ctx: &egui::Context, text_focus: bool, modal_open: bool) {
        let presses = std::mem::take(&mut self.shortcuts.presses);
        if drag::shortcuts_allowed(
            text_focus || self.ime_event || self.ime_active,
            self.busy,
            modal_open
                || self.text.floating.is_some()
                || self.block.session.is_some()
                || self.shortcuts.popup_at_event
                || egui::Popup::is_any_open(ctx),
        ) && ctx.input(|i| i.focused)
        {
            // egui recalculates repeat from keys_down. Retain each original backend press
            // in event order; ownership is checked after this frame's UI takes focus.
            for (key, modifiers) in presses {
                if self.command_context_blocked() {
                    break;
                }
                let failure_serial = self.request_failure_serial;
                if let Some(key) = shortcut_settings::key_from_egui(key)
                    && let Resolution::Command(command) = ShortcutResolver::resolve(
                        &self.shortcuts.current.keymap,
                        &[
                            ShortcutContext::Global,
                            ShortcutContext::Canvas,
                            ShortcutContext::ObjectEdit,
                        ],
                        Shortcut::new(
                            shortcut_settings::logical(
                                modifiers,
                                editor_core::command::Platform::current(),
                            ),
                            key,
                        ),
                    )
                    && self.command_enabled(command)
                {
                    self.dispatch(command);
                    ctx.input_mut(|i| i.events.retain(|event| !matches!(event, egui::Event::Key { key: event_key, pressed: true, modifiers: event_modifiers, .. } if shortcut_settings::key_from_egui(*event_key) == Some(key) && *event_modifiers == modifiers)));
                    if self.command_context_blocked()
                        || self.request_failure_serial != failure_serial
                        || !ctx.input(|i| i.focused)
                    {
                        break;
                    }
                }
            }
        }
    }
    fn command_context_blocked(&self) -> bool {
        self.busy
            || self.shortcuts.loading
            || self.shortcuts.open
            || self.modal.is_some()
            || self.layer_dialog.is_some()
            || self.close_prompt
            || self.replace_project_path.is_some()
            || self.project_error.is_some()
            || self.recovery_candidate.is_some()
            || self.text.floating.is_some()
            || self.block.session.is_some()
            || self.view.error.as_ref().is_some_and(|e| {
                e.code == "CONFIRMATION_REQUIRED" && e.details.get("categories").is_some()
            })
    }
    fn command_state(&self, command: CommandId) -> crate::ui::command_widgets::CommandState {
        crate::ui::command_widgets::CommandState::enabled(self.command_enabled(command))
    }
    fn command_button(&self, ui: &mut egui::Ui, command: CommandId, label: &str) -> egui::Response {
        crate::ui::command_widgets::button_labeled(ui, command, label, self.command_state(command))
    }
    fn command_enabled(&self, command: CommandId) -> bool {
        self.command_enabled_for(
            command,
            self.layer.as_deref(),
            self.block.definition.as_deref(),
        )
    }
    fn command_enabled_for(
        &self,
        command: CommandId,
        layer: Option<&str>,
        definition: Option<&str>,
    ) -> bool {
        if command == command_ids::GRIP_CANCEL {
            return self.grip.is_some();
        }
        if self.command_context_blocked() {
            return false;
        }
        let doc = self.view.info.as_ref();
        let editable = self.usable() && drag::editable_selection(&self.view);
        match command {
            command_ids::FILE_NEW
            | command_ids::FILE_NEW_PROJECT
            | command_ids::FILE_OPEN_PROJECT
            | command_ids::FILE_IMPORT_GERBER
            | command_ids::LAYER_CREATE => true,
            command_ids::FILE_SAVE_PROJECT
            | command_ids::FILE_SAVE_PROJECT_AS
            | command_ids::FILE_CLOSE_PROJECT => doc.is_some(),
            command_ids::FILE_EXPORT_GERBER => self.usable() && layer.is_some(),
            command_ids::EDIT_UNDO => doc.is_some_and(|d| d.undo_entries > 0),
            command_ids::EDIT_REDO => doc.is_some_and(|d| d.redo_entries > 0),
            command_ids::EDIT_DUPLICATE
            | command_ids::EDIT_DELETE
            | command_ids::OBJECT_MOVE
            | command_ids::OBJECT_ROTATE
            | command_ids::OBJECT_MIRROR => editable,
            command_ids::OBJECT_ARRAY_RECTANGULAR => {
                self.usable() && array_ui::eligible(&self.view)
            }
            command_ids::OBJECT_ALIGN_LEFT
            | command_ids::OBJECT_ALIGN_RIGHT
            | command_ids::OBJECT_ALIGN_TOP
            | command_ids::OBJECT_ALIGN_BOTTOM
            | command_ids::OBJECT_ALIGN_HCENTER
            | command_ids::OBJECT_ALIGN_VCENTER => {
                self.usable() && state::arrangement_eligibility(&self.view).align
            }
            command_ids::OBJECT_DISTRIBUTE_HORIZONTAL | command_ids::OBJECT_DISTRIBUTE_VERTICAL => {
                self.usable() && state::arrangement_eligibility(&self.view).distribute
            }
            command_ids::VIEW_FIT => self.view.scene.is_some(),
            command_ids::VIEW_FIT_ACTIVE_LAYER
            | command_ids::LAYER_DELETE
            | command_ids::LAYER_SOLO => {
                layer.is_some_and(|id| self.view.layers.iter().any(|l| l.layer_id == id))
            }
            command_ids::VIEW_GRID_TOGGLE
            | command_ids::SNAP_TOGGLE
            | command_ids::TOOL_SELECT
            | command_ids::TOOL_MEASURE => true,
            command_ids::TOOL_TEXT => self.usable(),
            command_ids::BLOCK_CREATE => {
                self.usable() && block_ui::create_targets(&self.view).is_ok()
            }
            command_ids::BLOCK_EXPLODE | command_ids::BLOCK_TRANSFORM => {
                editable && self.selected_instance().is_some()
            }
            command_ids::BLOCK_PLACE => {
                doc.is_some()
                    && definition
                        .is_some_and(|id| self.view.block_definitions.iter().any(|d| d.id.0 == id))
                    && self
                        .view
                        .layers
                        .iter()
                        .any(|l| l.is_active && block_ui::target_ok(l))
            }
            command_ids::BLOCK_RENAME | command_ids::BLOCK_SELECT => {
                doc.is_some()
                    && definition
                        .is_some_and(|id| self.view.block_definitions.iter().any(|d| d.id.0 == id))
            }
            command_ids::BLOCK_DELETE => {
                doc.is_some()
                    && definition.is_some_and(|id| {
                        self.view.block_definitions.iter().any(|d| d.id.0 == id)
                            && self.view.block_counts.get(id).copied().unwrap_or(0) == 0
                    })
            }
            _ => false,
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
    fn command_entries(&mut self, ui: &mut egui::Ui, entries: &[(&str, CommandId)], close: bool) {
        for &(label, command) in entries {
            if self.command_button(ui, command, label).clicked() {
                self.dispatch(command);
                if close {
                    ui.close();
                }
            }
        }
    }
    fn object_buttons(&mut self, ui: &mut egui::Ui) {
        self.command_entries(
            ui,
            &[
                ("原位复制", command_ids::EDIT_DUPLICATE),
                ("删除对象", command_ids::EDIT_DELETE),
            ],
            false,
        );
    }
    fn arrangement_entries(&mut self, ui: &mut egui::Ui) {
        if self.view.selected.ordered.len() >= 2
            && let Some(anchor) = self.view.selected.primary()
        {
            ui.label(
                RichText::new(format!(
                    "锚点：{}（最后选中，保持不动）",
                    anchor.object.object_id
                ))
                .background_color(crate::ui::tokens::selection_highlight()),
            );
            ui.separator();
        }
        ui.menu_button("阵列", |ui| self.array_entries(ui));
        ui.menu_button("对齐", |ui| self.alignment_entries(ui));
        ui.menu_button("分布", |ui| self.distribution_entries(ui));
    }
    fn array_entries(&mut self, ui: &mut egui::Ui) {
        self.command_entries(
            ui,
            &[("矩形阵列…", command_ids::OBJECT_ARRAY_RECTANGULAR)],
            true,
        );
    }
    fn alignment_entries(&mut self, ui: &mut egui::Ui) {
        self.command_entries(
            ui,
            &[
                ("左对齐", command_ids::OBJECT_ALIGN_LEFT),
                ("右对齐", command_ids::OBJECT_ALIGN_RIGHT),
                ("顶端对齐", command_ids::OBJECT_ALIGN_TOP),
                ("底端对齐", command_ids::OBJECT_ALIGN_BOTTOM),
                ("水平居中", command_ids::OBJECT_ALIGN_HCENTER),
                ("垂直居中", command_ids::OBJECT_ALIGN_VCENTER),
            ],
            true,
        );
    }
    fn distribution_entries(&mut self, ui: &mut egui::Ui) {
        self.command_entries(
            ui,
            &[
                ("水平等距分布", command_ids::OBJECT_DISTRIBUTE_HORIZONTAL),
                ("垂直等距分布", command_ids::OBJECT_DISTRIBUTE_VERTICAL),
            ],
            true,
        );
    }
    fn transform_entries(&mut self, ui: &mut egui::Ui, close: bool) {
        self.command_entries(
            ui,
            &[
                ("移动…", command_ids::OBJECT_MOVE),
                ("旋转…", command_ids::OBJECT_ROTATE),
                ("镜像…", command_ids::OBJECT_MIRROR),
            ],
            close,
        );
        if ui
            .add_enabled(
                self.command_enabled(command_ids::OBJECT_MOVE),
                egui::Button::new("Flash 属性…"),
            )
            .clicked()
        {
            self.open_modal(ActiveModal::Flash);
            if close {
                ui.close();
            }
        }
    }
    fn tool_buttons(&mut self, ui: &mut egui::Ui, compact: bool) -> [egui::Response; 2] {
        [
            ("选择", command_ids::TOOL_SELECT, tools::ActiveTool::Select),
            (
                "测距",
                command_ids::TOOL_MEASURE,
                tools::ActiveTool::Measure,
            ),
        ]
        .map(|(label, command, tool)| {
            let mut state = self.command_state(command);
            state.checked = self.tool == tool;
            let response = if compact {
                crate::ui::command_widgets::compact_button(ui, command, label, state)
            } else {
                crate::ui::command_widgets::button_labeled(ui, command, label, state)
            };
            if response.clicked() {
                self.dispatch(command);
                if !compact {
                    ui.close();
                }
            }
            response
        })
    }
    fn history_buttons(&mut self, ui: &mut egui::Ui) {
        self.command_entries(
            ui,
            &[
                ("撤销", command_ids::EDIT_UNDO),
                ("重做", command_ids::EDIT_REDO),
            ],
            false,
        );
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
        if !self.command_enabled(command) {
            return false;
        }
        if shortcut_config::commands()
            .iter()
            .any(|c| c.id == command && c.category == editor_core::command::CommandCategory::File)
        {
            self.dispatch_file_command(command);
            return true;
        }
        if command == command_ids::OBJECT_ARRAY_RECTANGULAR {
            self.open_array();
            return true;
        }
        if self.block_command(command) {
            return true;
        }
        if let Some(action) = arrangement_action(command) {
            self.send(action);
            return true;
        }
        match command {
            command_ids::EDIT_UNDO => {
                self.send(Action::History(false));
                true
            }
            command_ids::EDIT_REDO => {
                self.send(Action::History(true));
                true
            }
            command_ids::EDIT_DUPLICATE => {
                self.send(Action::Duplicate);
                true
            }
            command_ids::EDIT_DELETE => {
                self.send(Action::Delete);
                true
            }
            command_ids::OBJECT_MOVE => {
                self.open_modal(ActiveModal::Move);
                true
            }
            command_ids::OBJECT_ROTATE => {
                self.open_modal(ActiveModal::Rotate);
                true
            }
            command_ids::OBJECT_MIRROR => {
                self.open_modal(ActiveModal::Mirror);
                true
            }
            command_ids::VIEW_FIT => {
                self.drag = None;
                self.fit = true;
                true
            }
            command_ids::VIEW_FIT_ACTIVE_LAYER => {
                if let Some(layer) = self.layer.clone() {
                    self.send(Action::FitLayer(layer));
                }
                true
            }
            command_ids::VIEW_GRID_TOGGLE => {
                self.grid.visible = !self.grid.visible;
                self.persist_project_view();
                true
            }
            command_ids::LAYER_CREATE => {
                self.create_empty_layer();
                true
            }
            command_ids::LAYER_DELETE => {
                if let Some(layer) = self.layer.clone() {
                    self.layer_dialog = Some(layer_panel::LayerDialog::DeletePending {
                        layer: layer.clone(),
                    });
                    self.send(Action::LayerSummary(layer));
                }
                true
            }
            command_ids::LAYER_SOLO => {
                if let Some(layer) = self.layer.clone() {
                    let solo = self
                        .view
                        .layers
                        .iter()
                        .find(|l| l.layer_id == layer)
                        .is_some_and(|l| l.is_solo);
                    self.send(Action::SetSoloLayer((!solo).then_some(layer)));
                }
                true
            }
            command_ids::TOOL_SELECT | command_ids::TOOL_MEASURE => {
                self.text.cancel();
                self.tool = if command == command_ids::TOOL_SELECT {
                    tools::ActiveTool::Select
                } else {
                    tools::ActiveTool::Measure
                };
                self.drag = None;
                self.measure.clear();
                true
            }
            command_ids::TOOL_TEXT => {
                self.open_modal(ActiveModal::Text);
                true
            }
            command_ids::GRIP_CANCEL => {
                if self.grip.take().is_some() {
                    rcam_diagnostics::runtime_event(
                        rcam_diagnostics::Level::Info,
                        "grip.cancel.esc",
                    );
                }
                true
            }
            command_ids::SNAP_TOGGLE => {
                self.object_snap.enabled = !self.object_snap.enabled;
                rcam_diagnostics::with_source(self.operation_source, || {
                    let revision = self
                        .view
                        .info
                        .as_ref()
                        .and_then(|i| i.revision.parse().ok());
                    let op = rcam_diagnostics::Operation::begin("snap.toggle", revision);
                    rcam_diagnostics::measurements(
                        rcam_diagnostics::Level::Info,
                        "snap.toggle",
                        &[("enabled", u64::from(self.object_snap.enabled))],
                    );
                    op.end(revision, None);
                });
                self.object_snap_runtime.reset();
                self.persist_project_view();
                true
            }
            _ => false,
        }
    }
}

fn arrangement_action(command: CommandId) -> Option<Action> {
    Some(match command {
        command_ids::OBJECT_ALIGN_LEFT => Action::Align(AlignmentMode::Left),
        command_ids::OBJECT_ALIGN_RIGHT => Action::Align(AlignmentMode::Right),
        command_ids::OBJECT_ALIGN_TOP => Action::Align(AlignmentMode::Top),
        command_ids::OBJECT_ALIGN_BOTTOM => Action::Align(AlignmentMode::Bottom),
        command_ids::OBJECT_ALIGN_HCENTER => Action::Align(AlignmentMode::HCenter),
        command_ids::OBJECT_ALIGN_VCENTER => Action::Align(AlignmentMode::VCenter),
        command_ids::OBJECT_DISTRIBUTE_HORIZONTAL => {
            Action::Distribute(DistributionAxis::Horizontal)
        }
        command_ids::OBJECT_DISTRIBUTE_VERTICAL => Action::Distribute(DistributionAxis::Vertical),
        _ => return None,
    })
}

impl eframe::App for EditorApp {
    fn on_exit(&mut self) {
        for task in [&self.pending_task, &self.viewport_task]
            .into_iter()
            .flatten()
        {
            task.cancel_token.cancel();
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        #[cfg(feature = "internal-evidence")]
        self.closeout_raw_input(raw);
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.s5m1.take() {
            run.input(self, ctx, raw);
            self.s5m1 = Some(run);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.a2.take() {
            run.input(self, ctx, raw);
            self.a2 = Some(run);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.batch_drag.take() {
            run.input(self, ctx, raw);
            self.batch_drag = Some(run);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.i1.take() {
            run.input(self, ctx, raw);
            self.i1 = Some(run);
        }
        // egui clears text focus on Escape before update; retain its event-time owner.
        self.text_input_at_event = ctx.wants_keyboard_input() || self.ime_active;
        self.shortcuts.popup_at_event = egui::Popup::is_any_open(ctx);
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
        self.shortcuts.raw_input(raw, self.ime_event);
        if let Some(mut bench) = self.bench.take() {
            bench.input(self, ctx, raw);
            self.bench = Some(bench);
        }
    }
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.shortcuts.poll();
        crate::ui::command_widgets::install_shortcuts(ctx, &self.shortcuts.current.config);
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

        let mut reply = self.rx.try_recv().ok();
        if let Some((id, view)) = &reply
            && self
                .geometry_task
                .as_ref()
                .is_some_and(|t| t.task_id == *id)
        {
            let task = self.geometry_task.take().unwrap();
            let accepted = self
                .geometry_context
                .as_deref()
                .is_some_and(|context| geometry_reply_matches(&task, &self.view, view, context));
            if accepted {
                self.view.selection_geometry = view.selection_geometry.clone();
                self.view.selection_geometry_identity = view.selection_geometry_identity.clone();
                self.view.selection_geometry_error = view.selection_geometry_error.clone();
            }
            self.geometry_context = None;
            #[cfg(feature = "internal-evidence")]
            native_a2::reply(*id, accepted);
            reply = None;
        }
        #[cfg(feature = "internal-evidence")]
        if let Some((id, view)) = &reply {
            native_a2::reply(
                *id,
                *id == self.sequence
                    && self
                        .pending_task
                        .as_ref()
                        .or(self.viewport_task.as_ref())
                        .is_none_or(|task| task_reply_matches(task, &self.view, view)),
            );
        }
        if let Some((id, view)) = &reply
            && *id == self.sequence
            && let Some(task) = self.pending_task.as_ref().or(self.viewport_task.as_ref())
            && !task_reply_matches(task, &self.view, view)
        {
            // Release only this request's busy state; never install a stale snapshot.
            self.ui_error = Some("后台结果身份已失效，结果未安装".into());
            self.busy = false;
            self.pending_task = None;
            self.viewport_sequence = None;
            self.viewport_task = None;
            self.modal_pending = None;
            reply = None;
        }
        if reply
            .as_ref()
            .is_some_and(|(id, _)| self.viewport_sequence == Some(*id))
        {
            self.viewport_sequence = None;
            self.viewport_task = None;
        }
        if let Some((id, view)) = reply
            && id == self.sequence
        {
            let changed = self.view.info.as_ref().map(|d| &d.document_id)
                != view.info.as_ref().map(|d| &d.document_id);
            self.view = view;
            self.accept_array_reply();
            self.accept_component_reply(changed);
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
            self.pending_task = None;
            if self.modal_pending == Some(id) {
                self.modal_pending = None;
                if self.view.error.is_none() {
                    self.modal = None;
                }
            }
            self.accept_text_reply();
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
                if self.grip.is_some() {
                    self.dispatch(command_ids::GRIP_CANCEL);
                }
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
        let modal_open = self.shortcuts.open
            || self.modal.is_some()
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
                !self.text_input_at_event
                    && !self.ime_event
                    && !self.ime_active
                    && i.key_pressed(egui::Key::Escape),
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
        if !modal_open
            && !text_focus
            && !self.ime_event
            && !self.ime_active
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.cancel_block();
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
        if self
            .block
            .session
            .as_ref()
            .is_some_and(|s| !s.valid(&self.view))
            || (self.block.session.is_some()
                && (modal_open
                    || self.tool != tools::ActiveTool::Block
                    || !ctx.input(|i| i.focused)))
        {
            self.cancel_block();
        }
        if self.block.session.is_some()
            && ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::PointerGone))
            })
        {
            self.cancel_block();
        }
        if cancel_drag || modal_open || self.display_error.is_some() {
            self.drag = None;
        }
        if self.grip.is_some()
            && (cancel_drag
                || modal_open
                || self.display_error.is_some()
                || self.tool != tools::ActiveTool::Select
                || self.grip.as_ref().is_some_and(|g| !g.valid(&self.view)))
        {
            self.grip = None;
            let reason = if modal_open {
                "grip.cancel.modal"
            } else if self.tool != tools::ActiveTool::Select {
                "grip.cancel.tool_change"
            } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                "grip.cancel.esc"
            } else if !ctx.input(|i| i.focused) {
                "grip.cancel.blur"
            } else if ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::PointerGone))
            }) {
                "grip.cancel.pointer_gone"
            } else {
                "grip.cancel.state_change"
            };
            rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, reason);
        }
        self.operation_source = rcam_diagnostics::Source::Menu;
        self.shortcuts.window(
            ctx,
            self.ime_event || self.ime_active,
            self.text_input_at_event,
        );
        self.component_window(ctx);
        if !self.busy
            && self.geometry_task.is_none()
            && self.viewport_sequence.is_none()
            && !self.view.selected.ordered.is_empty()
            && self.view.selection_geometry_identity
                != state::selection_geometry_identity(&self.view)
        {
            let identity = state::selection_geometry_identity(&self.view);
            let params = editor_service::SelectionCentersParams {
                groups: self.view.selected.groups(),
                semantics: editor_service::SelectionMaterialSemantics::SelectedLayerComposite,
            };
            self.send(Action::SelectionCenters(identity, params));
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if modal_open {
                ui.disable();
            }
            egui::MenuBar::new().ui(ui, |ui| {
                ui.strong("RCam");
                if ui.add_enabled(!self.busy && self.view.info.is_some(),egui::Button::new("PCB / PnP")).clicked(){self.components.open=true;}
                ui.separator();
                ui.menu_button("文件", |ui| {
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_NEW_PROJECT, self.command_state(command_ids::FILE_NEW_PROJECT)).clicked()
                    {
                        self.dispatch(command_ids::FILE_NEW_PROJECT);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_OPEN_PROJECT, self.command_state(command_ids::FILE_OPEN_PROJECT)).clicked() {
                        self.dispatch(command_ids::FILE_OPEN_PROJECT);
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
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_SAVE_PROJECT, self.command_state(command_ids::FILE_SAVE_PROJECT)).clicked() {
                        self.dispatch(command_ids::FILE_SAVE_PROJECT);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_SAVE_PROJECT_AS, self.command_state(command_ids::FILE_SAVE_PROJECT_AS)).clicked() {
                        self.dispatch(command_ids::FILE_SAVE_PROJECT_AS);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_CLOSE_PROJECT, self.command_state(command_ids::FILE_CLOSE_PROJECT)).clicked() {
                        self.dispatch(command_ids::FILE_CLOSE_PROJECT);
                        ui.close();
                    }
                    ui.separator();
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_IMPORT_GERBER, self.command_state(command_ids::FILE_IMPORT_GERBER),
                    )
                    .clicked()
                    {
                        self.dispatch(command_ids::FILE_IMPORT_GERBER);
                        ui.close();
                    }
                    if crate::ui::command_widgets::button(ui, command_ids::LAYER_CREATE, self.command_state(command_ids::LAYER_CREATE),
                    )
                    .clicked()
                    {
                        self.dispatch(command_ids::LAYER_CREATE);
                        ui.close();
                    }
                    ui.separator();
                    if crate::ui::command_widgets::button(ui, command_ids::FILE_EXPORT_GERBER, self.command_state(command_ids::FILE_EXPORT_GERBER),
                    )
                    .clicked()
                    {
                        self.dispatch(command_ids::FILE_EXPORT_GERBER);
                        ui.close();
                    }
                });
                ui.menu_button("编辑", |ui| {
                    self.history_buttons(ui);
                    self.object_buttons(ui);
                    self.block_entries(ui);
                    ui.separator();
                    self.transform_entries(ui, true);
                });
                ui.menu_button("排列", |ui| self.arrangement_entries(ui));
                ui.menu_button("插入", |ui| {
                    if crate::ui::command_widgets::button(ui, command_ids::TOOL_TEXT, self.command_state(command_ids::TOOL_TEXT),
                    )
                    .clicked()
                    {
                        self.dispatch(command_ids::TOOL_TEXT);
                        ui.close();
                    }
                });
                ui.menu_button("工具", |ui| { self.tool_buttons(ui, false); });
                ui.menu_button("图层", |ui| {
                    if crate::ui::command_widgets::button(ui, command_ids::LAYER_CREATE, self.command_state(command_ids::LAYER_CREATE),
                    )
                    .clicked()
                    {
                        self.dispatch(command_ids::LAYER_CREATE);
                        ui.close();
                    }
                    if self.command_button(ui, command_ids::FILE_IMPORT_GERBER, "导入 Gerber…")
                        .clicked()
                    {
                        self.dispatch(command_ids::FILE_IMPORT_GERBER);
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
                    if crate::ui::command_widgets::button(ui, command_ids::LAYER_DELETE, self.command_state(command_ids::LAYER_DELETE),
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
                    let mut grid_visible = self.grid.visible;
                    if crate::ui::command_widgets::checkbox(
                        ui,
                        command_ids::VIEW_GRID_TOGGLE,
                        &mut grid_visible,
                        self.command_enabled(command_ids::VIEW_GRID_TOGGLE),
                    ).changed() { self.dispatch(command_ids::VIEW_GRID_TOGGLE); }
                    if ui.button("网格 / 吸附设置…").clicked() {
                        self.open_modal(ActiveModal::Grid);
                        ui.close();
                    }
                    let mut object_snap_enabled = self.object_snap.enabled;
                    if crate::ui::command_widgets::checkbox(
                        ui,
                        command_ids::SNAP_TOGGLE,
                        &mut object_snap_enabled,
                        self.command_enabled(command_ids::SNAP_TOGGLE),
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
                    if crate::ui::command_widgets::button(ui, command_ids::VIEW_FIT, self.command_state(command_ids::VIEW_FIT)).clicked() {
                        self.dispatch(command_ids::VIEW_FIT);
                        ui.close();
                    }
                });
                ui.menu_button(if self.shortcuts.warning.is_some() { "设置 ⚠" } else { "设置" }, |ui| {
                    if ui.add_enabled(!self.busy && self.modal.is_none() && self.drag.is_none() && self.grip.is_none() && self.block.session.is_none() && self.text.floating.is_none(), egui::Button::new("快捷键…")).clicked() {
                        self.shortcuts.open = true; self.shortcuts.message = None; ui.close();
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
                                    let info = self.view.info.clone();
                                    let layers = self.view.layers.clone();
                                    let snapshot = self.view.snap_snapshot.clone();
                                    let unit = self.display_unit;
                                    let (tx, rx) = std::sync::mpsc::channel();
                                    self.diagnostic_export = Some(rx);
                                    std::thread::spawn(move || { rcam_diagnostics::with_source(rcam_diagnostics::Source::Menu, || {
                                        let context = info.as_ref().map(|info| editor_service::diagnostic_context(info, &layers, snapshot.as_deref(), unit)).unwrap_or_default();
                                        let revision = info.as_ref().and_then(|i| i.revision.parse().ok());
                                        // The exported ring contains the request; success is recorded after durable publication.
                                        let op = rcam_diagnostics::Operation::begin("diagnostics.export", revision);
                                        let result = runtime.export_with_context(&path, context);
                                        op.end(revision, result.as_ref().err().map(|_| "DIAGNOSTIC_EXPORT_FAILED"));
                                        let _ = tx.send(result.map_err(|_| "诊断包导出失败：请使用新文件名并检查写入权限".to_string())); }); });
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
                if crate::ui::command_widgets::compact_button(ui, command_ids::FILE_IMPORT_GERBER, "导入…", self.command_state(command_ids::FILE_IMPORT_GERBER)).clicked() {
                    self.dispatch(command_ids::FILE_IMPORT_GERBER);
                }
                if crate::ui::command_widgets::compact_button(ui, command_ids::VIEW_FIT, "适合窗口", self.command_state(command_ids::VIEW_FIT)).clicked() {
                    self.dispatch(command_ids::VIEW_FIT);
                }
                if crate::ui::command_widgets::compact_button(ui, command_ids::VIEW_FIT_ACTIVE_LAYER, "适合当前图层", self.command_state(command_ids::VIEW_FIT_ACTIVE_LAYER)).clicked() {
                    self.dispatch(command_ids::VIEW_FIT_ACTIVE_LAYER);
                }
                ui.separator();
                self.history_buttons(ui);
                ui.separator();
                ui.label(RichText::new("几何多选").color(Color32::from_rgb(100, 206, 183)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui::command_widgets::compact_button(ui, command_ids::FILE_EXPORT_GERBER, "导出图层…", self.command_state(command_ids::FILE_EXPORT_GERBER))
                .clicked()
                {
                        self.dispatch(command_ids::FILE_EXPORT_GERBER);
                    }
                    if self.busy {
                        ui.spinner();
                        ui.label("处理中…");
                        if let Some(task) = &self.pending_task {
                            use editor_service::task::{CancelOutcome, TaskState};
                            let cancel_state = task.cancel_token.state();
                            if matches!(cancel_state, TaskState::CancelRequested | TaskState::Cancelled) {
                                ui.label("正在取消…");
                            } else {
                                if cancel_state == TaskState::Committing { ui.label("已进入提交阶段，无法取消；等待实际结果"); }
                                let response = ui.button("取消任务");
                                #[cfg(feature = "internal-evidence")]
                                if let Some(run) = &mut self.a2 { run.cancel_rect = response.rect; run.cancel_state = Some(cancel_state); }
                                if response.clicked() {
                                let outcome = task.cancel_token.cancel();
                                #[cfg(feature = "internal-evidence")]
                                native_a2::cancel_clicked(task.task_id, outcome, ctx);
                                let message = match outcome {
                                    CancelOutcome::Requested | CancelOutcome::AlreadyCancelled => "已请求取消，等待后台释放资源",
                                    CancelOutcome::TooLate => "任务已进入提交阶段，等待实际结果",
                                };
                                self.toast = Some((message.into(), Instant::now()));
                                }
                            }
                        }
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
                    if self.view.selected.ordered.len() >= 2 {
                        let kind = if matches!(
                            o.object.origin,
                            editor_core::ObjectOrigin::GeneratedText { .. }
                        ) {
                            "文字组"
                        } else {
                            "对象"
                        };
                        ui.label(
                            RichText::new(format!(
                                "对齐锚点：{kind} {}（最后选中）",
                                o.object.object_id
                            ))
                            .background_color(crate::ui::tokens::selection_highlight()),
                        );
                    }
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
                    if self
                        .command_button(ui, command_ids::EDIT_UNDO, "撤销")
                        .clicked()
                    {
                        self.toast = None;
                        self.dispatch(command_ids::EDIT_UNDO);
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
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.block.library, false, "图层");
                    if ui
                        .selectable_value(&mut self.block.library, true, "Blocks")
                        .clicked()
                    {
                        rcam_diagnostics::runtime_event(
                            rcam_diagnostics::Level::Info,
                            "block.library.open",
                        );
                    }
                });
                if self.block.library {
                    self.block_library(ui);
                } else {
                    self.layer_panel(ui);
                }
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
                        ui.label("选择含锁定或不可编辑对象：整组编辑禁止（仅可查看）");
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
                        self.block_properties(ui);
                        ui.separator();
                        self.transform_entries(ui, false);
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
                let mut grid_visible = self.grid.visible;
                if crate::ui::command_widgets::checkbox(
                    ui,
                    command_ids::VIEW_GRID_TOGGLE,
                    &mut grid_visible,
                    self.command_enabled(command_ids::VIEW_GRID_TOGGLE),
                )
                .changed()
                {
                    self.dispatch(command_ids::VIEW_GRID_TOGGLE);
                }
                if ui.button("网格 / 吸附设置…").clicked() {
                    self.open_modal(ActiveModal::Grid);
                }
                let mut snap_state = self.command_state(command_ids::SNAP_TOGGLE);
                snap_state.checked = self.object_snap.enabled;
                if crate::ui::command_widgets::compact_button(
                    ui,
                    command_ids::SNAP_TOGGLE,
                    if self.object_snap.enabled {
                        "Object Snap ON"
                    } else {
                        "Object Snap OFF"
                    },
                    snap_state,
                )
                .clicked()
                {
                    self.dispatch(command_ids::SNAP_TOGGLE);
                }
                if ui.button("Object Snap 设置…").clicked() {
                    self.open_modal(ActiveModal::ObjectSnap);
                }
                self.unit_controls(ui);
                self.tool_buttons(ui, true);
                if crate::ui::command_widgets::compact_button(
                    ui,
                    command_ids::TOOL_TEXT,
                    "文本…",
                    self.command_state(command_ids::TOOL_TEXT),
                )
                .clicked()
                {
                    self.dispatch(command_ids::TOOL_TEXT);
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
                    self.grip = None;
                }
                self.canvas_rect = rect;
                if self.fit {
                    self.camera.fit(self.view.bounds, rect);
                    self.fit = false;
                }
                if !modal_open
                    && self.drag.is_none() && self.grip.is_none()
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
                if !modal_open && !text_focus && self.drag.is_none() && self.grip.is_none() {
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
                                    self.measure.click_snapped(resolution.point, resolution.kind);
                                    rcam_diagnostics::with_source(rcam_diagnostics::Source::Canvas, || {
                                        let revision = self.view.info.as_ref().and_then(|i| i.revision.parse().ok());
                                        rcam_diagnostics::Operation::begin("measure.point", revision).end(revision,None);
                                    });
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
                    }) && self.components.pick.is_none() && self.tool == tools::ActiveTool::Select
                        && !ctx.wants_keyboard_input()
                        && self.usable()
                        && !cancel_drag
                        && !modal_open
                        && rect.contains(press)
                    {
                        let feature = (selection::SelectionMode::from_modifiers(modifiers)==selection::SelectionMode::Replace).then(||grip::features(&self.view).ok().and_then(|features| grip::hit(&features, press, self.camera, rect, ctx.pixels_per_point()))).flatten();
                        if let Some(id) = feature {
                            self.grip = grip::Session::arm(&self.view, id);
                            if let Some(session) = &mut self.grip { session.pressed = Some(press); }
                            self.drag = None;
                            self.object_snap_runtime.reset();
                        } else {
                        self.drag = Some(drag::Gesture::arm(
                            &self.view,
                            press,
                            self.camera,
                            rect,
                            ctx.pixels_per_point(),
                            selection::SelectionMode::from_modifiers(modifiers),
                        ).with_navigation_epoch(self.click_navigation.observe(self.camera,rect,ctx.pixels_per_point())));
                        if self.drag.is_some() {
                            self.send(Action::ProbeDrag(
                                self.camera.world(press, rect),
                                self.camera.tolerance(ctx.pixels_per_point()),
                            ));
                        }
                        }
                    }
                }
                self.click_navigation.observe(self.camera,rect,ctx.pixels_per_point());
                if self.block.session.is_some() && !modal_open && !text_focus { self.block_canvas(ctx, &r, rect); }
                if self.tool == tools::ActiveTool::Select && !modal_open {
                    r.context_menu(|ui| {
                        self.operation_source = rcam_diagnostics::Source::Context;
                        ui.menu_button("排列 / 阵列", |ui| self.arrangement_entries(ui));
                        ui.separator();
                        self.block_entries(ui);
                    });
                }
                if !modal_open {self.component_canvas(ctx,&r,rect);}
                self.closeout_context_transition();
                // Tool/menu input above can change context in this same frame.
                // Recheck before release, rather than waiting for the next frame.
                if self.grip.is_some() && (self.tool != tools::ActiveTool::Select
                    || self.modal.is_some() || self.layer_dialog.is_some() || self.close_prompt) {
                    self.grip = None;
                    rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info,"grip.cancel.tool_change");
                }
                if let Some(grip) = &mut self.grip {
                    let pointer_position = ctx.input(|i| i.pointer.interact_pos());
                    let release_raw_target =
                        pointer_position.map(|position| self.camera.world(position, rect));
                    if let Some(position) = pointer_position
                        && (grip.moved || grip.pressed.is_some_and(|p| p.distance(position)*ctx.pixels_per_point() >= drag::THRESHOLD_PX)) {
                        let raw = self.camera.world(position, rect);
                        match self.object_snap_runtime.resolve(raw, &self.object_snap, self.grid, self.camera,
                            ctx.pixels_per_point(), self.view.snap_snapshot.as_deref(), &self.view.snap_index,
                            &self.view.layers, Some(&grip.excluded), ctx.input(|i| i.modifiers.alt)) {
                            Ok(resolution) => grip.update(resolution.point),
                            Err(error) => grip.preview = Err(error),
                        }
                    }
                    cursor_label = Some(format!("Grip {:?} · X {}  Y {} · Snap {:?}", grip.id,
                        self.display_unit.format_length(grip.target.x_mm, self.view.info.as_ref().map_or(0.0001, |d| d.manufacturing_precision.resolution_mm)),
                        self.display_unit.format_length(grip.target.y_mm, self.view.info.as_ref().map_or(0.0001, |d| d.manufacturing_precision.resolution_mm)),
                        self.object_snap_runtime.current.as_ref().and_then(|r| r.kind)));
                    if let Err(error) = &grip.preview && grip.moved { self.ui_error = Some(error.clone()); }
                    if ctx.input(|i| i.pointer.primary_released()) {
                        let session = self.grip.take().unwrap();
                        if let Some(probe) = &mut self.probe {
                            probe.record_grip_release(
                                &self.view,
                                &session,
                                release_raw_target,
                                self.object_snap_runtime.current.as_ref(),
                            );
                        }
                        let click = (!session.moved).then_some(session.pressed).flatten().map(|press| {
                            let mut context=selection::ClickContext::new(press,self.camera,rect,ctx.pixels_per_point());
                            context.navigation_epoch=self.click_navigation.observe(self.camera,rect,ctx.pixels_per_point());
                            Action::CanvasSelect(context,selection::SelectionMode::Replace)
                        });
                        if let Some(action) = session.release() {
                            let snap = self.object_snap_runtime.current.as_ref();
                            rcam_diagnostics::measurements(rcam_diagnostics::Level::Info,"grip.commit_target",&[("grid",u64::from(snap.is_some_and(|s|s.from_grid))),("object_snap",u64::from(snap.is_some_and(|s|s.kind.is_some())))]);
                            self.send(action);
                        }
                        else if let Some(action)=click {self.send(action);}
                        else { rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, "grip.cancel"); }
                    }
                }
                let drag_update = self.drag.as_ref().and_then(|drag| {
                    (!drag.released)
                        .then(|| ctx.input(|input| input.pointer.interact_pos()))
                        .flatten()
                });
                let (snapped_drag, drag_snap_error) = drag_update.as_ref().map_or(
                    (None, None),
                    |position| {
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
                        self.drag.as_ref().and_then(drag::Gesture::snap_exclusions),
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
                    if let Some(position) = drag_update {
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
                let outside = !self.view.render_coverage_complete && coverage.is_none_or(|b| {
                    lo.x_mm < b.min_x_mm
                        || lo.y_mm < b.min_y_mm
                        || hi.x_mm > b.max_x_mm
                        || hi.y_mm > b.max_y_mm
                });
                let rebase = self.view.scene.as_ref().is_some_and(|scene| viewport_requires_rebase(scene, self.camera));
                let needs_lod = self.view.info.is_some()
                    && (outside
                        || rebase
                        || ppm > self.view.render_ppm
                        || ppm < self.view.render_ppm / display::LOD_MAX_ZOOM_OUT);
                let attempted = self.view.display_attempt.is_some_and(|(b, scale)| {
                    display::covers_view(b, scale, lo, hi, ppm)
                });
                if needs_lod && !attempted && !self.busy && self.viewport_sequence.is_none() {
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
                            #[cfg(feature = "internal-evidence")]
                            if let Some(run) = &mut self.batch_drag { run.prepare(&prepared.stats); }
                            #[cfg(feature = "internal-evidence")]
                            if let Some(run) = &mut self.s5m1 {
                                run.prepare(&prepared.stats);
                            }
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
                                rcam_diagnostics::render_exception(rcam_diagnostics::RenderException::DisplayPrepareFailed);
                                if e.starts_with("RESOURCE_LIMIT:") { rcam_diagnostics::render_exception(rcam_diagnostics::RenderException::ResourceLimit); }
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
                    if !rendered { rcam_diagnostics::render_exception(rcam_diagnostics::RenderException::LastGoodFrameFallback); }
                    painter.add(egui_wgpu::Callback::new_paint_callback(
                        rect,
                        gpu::Callback {
                            painted: {
                                #[cfg(feature = "internal-evidence")]
                                let s5 = self.s5m1.as_ref().map(|r| (r.painted.clone(), r.frame_id))
                                    .or_else(|| self.batch_drag.as_ref().map(|r| (r.painted.clone(), r.frame_id)))
                                    .or_else(|| self.i1.as_ref().map(|r| (r.painted.clone(), r.frame_id)));
                                #[cfg(not(feature = "internal-evidence"))]
                                let s5 = None;
                                s5.or_else(|| self.bench.as_ref().map(|b| (b.painted.clone(), b.frame_id)))
                            },
                            index: last.index.clone(),
                            scene: last.scene.clone(),
                            selected: last.selected.clone(),
                            uniforms: last.uniforms,
                            format: self.format,
                        },
                    ));
                }
                if self.tool == tools::ActiveTool::Select && !modal_open {
                    match self.grip.as_ref().map_or_else(|| grip::features(&self.view), grip::Session::features) {
                        Ok(features) => {
                            let hover = ctx.input(|i| i.pointer.hover_pos()).and_then(|p| grip::hit(&features,p,self.camera,rect,ctx.pixels_per_point()));
                            grip::paint_features(&painter,&features,hover,self.grip.as_ref().map(|g|g.id),self.camera,rect,ctx.pixels_per_point());
                        }
                        Err(error) => self.ui_error = Some(error),
                    }
                    if let Some(grip) = &self.grip { grip.paint(&painter,self.camera,rect,ctx.pixels_per_point()); }
                }
                self.paint_component(&painter,rect,ctx.pixels_per_point());
                self.paint_candidates(&painter,rect,ctx.pixels_per_point());
                self.paint_block(&painter, rect, ctx.pixels_per_point());
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
                self.paint_array(&painter, rect, ctx.pixels_per_point());
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
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.batch_drag.take() {
            run.tick(self, ctx);
            self.batch_drag = Some(run);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.a2.take() {
            run.tick(self, ctx);
            self.a2 = Some(run);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.s5m1.take() {
            run.tick(self, ctx);
            self.s5m1 = Some(run);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut native) = self.components.native.take() {
            native.tick(self, ctx);
            self.components.native = Some(native);
        }
        #[cfg(feature = "internal-evidence")]
        if let Some(mut native) = self.components.candidates.native.take() {
            native.tick(self, ctx);
            self.components.candidates.native = Some(native);
        }
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
        self.operation_source = rcam_diagnostics::Source::Shortcut;
        self.route_shortcuts(
            ctx,
            text_focus || ctx.wants_keyboard_input(),
            modal_open
                || self.shortcuts.open
                || self.modal.is_some()
                || self.layer_dialog.is_some(),
        );
        self.operation_source = rcam_diagnostics::Source::Menu;
        // User input must claim the worker before an idle recovery write. In
        // particular, a same-frame press must not lose its ProbeDrag to busy.
        self.tick_recovery(now);
        #[cfg(feature = "internal-evidence")]
        if let Some(mut run) = self.i1.take() {
            run.tick(self, ctx);
            self.i1 = Some(run);
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
            {
                #[cfg(feature = "internal-evidence")]
                if let Some(dir) = shortcut_store::native_directory()
                    .or_else(native_s5m1::directory)
                    .or_else(native_a2::directory)
                    .or_else(native_batch_drag::directory)
                    .or_else(native_i1::directory)
                    .or_else(native_d2::directory)
                    .or_else(native_d1::directory)
                {
                    dir.join("logs")
                } else {
                    std::path::PathBuf::from(home).join("Library/Logs/RCam")
                }
                #[cfg(not(feature = "internal-evidence"))]
                std::path::PathBuf::from(home).join("Library/Logs/RCam")
            },
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

    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    let default_surface_error = wgpu_options.on_surface_error.clone();
    wgpu_options.on_surface_error = std::sync::Arc::new(move |error| {
        rcam_diagnostics::render_exception(rcam_diagnostics::RenderException::SurfaceError);
        default_surface_error(error)
    });
    let result = eframe::run_native(
        "RCam",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            wgpu_options,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280., 800.])
                .with_min_inner_size(
                    if cfg!(feature = "internal-evidence")
                        && std::env::var("RCAM_NATIVE_BENCH").ok().as_deref() == Some("s2b32")
                    {
                        [800., 400.]
                    } else {
                        [980., 620.]
                    },
                ),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    );
    if cfg!(feature = "internal-evidence")
        && std::env::var("RCAM_NATIVE_BENCH").ok().as_deref() == Some("s2b32")
    {
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

#[cfg(test)]
mod shortcut_rc1_regressions {
    use super::*;
    use editor_core::command::{Key, Modifiers, Platform};

    fn block_app() -> EditorApp {
        let mut project = block_ui::fixtures::big_project(1, 1);
        project.workspace.active_layer_id = Some("l1".into());
        let mut model = Model::default();
        model.run(Action::RestoreProject(
            rcam_project::encode_v1(&project).unwrap(),
        ));
        assert!(model.view.error.is_none());
        let mut app = modal::tests::app();
        app.view = model.view;
        app.block.definition = Some(app.view.block_definitions[0].id.0.clone());
        app.shortcuts.current = app
            .shortcuts
            .current
            .config
            .replace(
                command_ids::BLOCK_PLACE,
                vec![Shortcut::new(Modifiers::NONE, Key::F(8))],
                Platform::current(),
            )
            .unwrap();
        app
    }
    fn route(app: &mut EditorApp) {
        let mut raw = egui::RawInput {
            focused: true,
            events: [egui::Key::F8, egui::Key::F3]
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .to_vec(),
            ..Default::default()
        };
        app.shortcuts.raw_input(&mut raw, false);
        let _ = egui::Context::default().run(raw, |ctx| app.route_shortcuts(ctx, false, false));
        assert!(app.shortcuts.presses.is_empty());
    }
    #[test]
    fn block_placement_fences_same_frame_after_success_full_or_disconnected_queue() {
        for queue in 0..3 {
            let mut app = block_app();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            if queue == 1 {
                tx.send((
                    99,
                    rcam_diagnostics::Source::System,
                    Action::NewWorkspace,
                    editor_service::task::TaskContext::new(99, Default::default()),
                ))
                .unwrap();
            }
            let _receiver = (queue != 2).then_some(rx);
            app.tx = tx;
            let snap = app.object_snap.enabled;
            route(&mut app);
            assert!(app.block.session.is_some());
            assert_eq!(snap, app.object_snap.enabled);
            assert_eq!(app.busy, queue == 0);
            assert_eq!(app.ui_error.is_some(), queue != 0);
            assert!(!app.dispatch(command_ids::SNAP_TOGGLE));
            assert!(!app.dispatch(command_ids::TOOL_MEASURE));
        }
    }
    #[test]
    fn failed_enqueue_without_transient_context_also_stops_remaining_frame() {
        let mut app = block_app();
        app.shortcuts.current = app
            .shortcuts
            .current
            .config
            .replace(command_ids::BLOCK_PLACE, vec![], Platform::current())
            .unwrap();
        app.shortcuts.current = app
            .shortcuts
            .current
            .config
            .replace(
                command_ids::LAYER_CREATE,
                vec![Shortcut::new(Modifiers::NONE, Key::F(8))],
                Platform::current(),
            )
            .unwrap();
        let before = app.object_snap.enabled;
        route(&mut app); // fixture receiver is disconnected
        assert!(app.ui_error.is_some());
        assert!(!app.busy);
        assert!(app.block.session.is_none());
        assert_eq!(app.object_snap.enabled, before);
    }
    fn render_tools(
        app: &mut EditorApp,
        ctx: &egui::Context,
        compact: bool,
        events: Vec<egui::Event>,
    ) -> [egui::Response; 2] {
        let mut responses = None;
        let _ = ctx.run(
            egui::RawInput {
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    responses = Some(app.tool_buttons(ui, compact));
                });
            },
        );
        responses.unwrap()
    }
    #[test]
    fn actual_menu_and_toolbar_tools_share_busy_loading_modal_gate_and_handler() {
        for compact in [false, true] {
            for gate in 0..5 {
                let mut app = modal::tests::app();
                match gate {
                    0 => app.busy = true,
                    1 => app.shortcuts.loading = true,
                    2 => app.shortcuts.open = true,
                    3 => app.modal = Some(ActiveModal::Move),
                    _ => app.close_prompt = true,
                }
                let ctx = egui::Context::default();
                let responses = render_tools(&mut app, &ctx, compact, vec![]);
                assert!(responses.iter().all(|r| !r.enabled()));
                let pos = responses[1].rect.center();
                for pressed in [true, false] {
                    render_tools(
                        &mut app,
                        &ctx,
                        compact,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                }
                assert!(app.tool == tools::ActiveTool::Select);
                assert!(!app.dispatch(command_ids::TOOL_MEASURE));
            }
            let mut app = modal::tests::app();
            let ctx = egui::Context::default();
            let responses = render_tools(&mut app, &ctx, compact, vec![]);
            assert!(responses.iter().all(|r| r.enabled()));
            let pos = responses[1].rect.center();
            for pressed in [true, false] {
                render_tools(
                    &mut app,
                    &ctx,
                    compact,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(app.tool == tools::ActiveTool::Measure);
        }
    }
    fn texts(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.job.text.clone()),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, out);
                }
            }
            _ => {}
        }
    }
    #[test]
    fn actual_existing_menu_contents_show_changed_and_cleared_hints() {
        for (id, label) in [
            (command_ids::OBJECT_ARRAY_RECTANGULAR, "矩形阵列…"),
            (command_ids::OBJECT_ALIGN_LEFT, "左对齐"),
            (command_ids::OBJECT_ALIGN_RIGHT, "右对齐"),
            (command_ids::OBJECT_ALIGN_TOP, "顶端对齐"),
            (command_ids::OBJECT_ALIGN_BOTTOM, "底端对齐"),
            (command_ids::OBJECT_ALIGN_HCENTER, "水平居中"),
            (command_ids::OBJECT_ALIGN_VCENTER, "垂直居中"),
            (command_ids::OBJECT_DISTRIBUTE_HORIZONTAL, "水平等距分布"),
            (command_ids::OBJECT_DISTRIBUTE_VERTICAL, "垂直等距分布"),
            (command_ids::OBJECT_MOVE, "移动…"),
            (command_ids::OBJECT_ROTATE, "旋转…"),
            (command_ids::OBJECT_MIRROR, "镜像…"),
            (command_ids::TOOL_SELECT, "选择"),
            (command_ids::TOOL_MEASURE, "测距"),
            (command_ids::BLOCK_CREATE, "创建 Block…"),
            (command_ids::BLOCK_EXPLODE, "拆解 Block…"),
        ] {
            let mut app = modal::tests::app();
            for bindings in [vec![Shortcut::new(Modifiers::NONE, Key::F(8))], vec![]] {
                app.shortcuts.current = app
                    .shortcuts
                    .current
                    .config
                    .replace(id, bindings.clone(), Platform::current())
                    .unwrap();
                let ctx = egui::Context::default();
                crate::ui::command_widgets::install_shortcuts(&ctx, &app.shortcuts.current.config);
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.array_entries(ui);
                        app.alignment_entries(ui);
                        app.distribution_entries(ui);
                        app.transform_entries(ui, false);
                        app.tool_buttons(ui, false);
                        app.block_entries(ui);
                    });
                });
                let mut rendered = vec![];
                for shape in output.shapes {
                    texts(&shape.shape, &mut rendered);
                }
                let expected = if bindings.is_empty() {
                    label.to_string()
                } else {
                    format!("{label}  F8")
                };
                assert!(
                    rendered.contains(&expected),
                    "{id:?}: expected {expected}; got {rendered:?}"
                );
                if bindings.is_empty() {
                    assert!(!rendered.contains(&format!("{label}  F8")));
                }
            }
        }
    }
}

#[cfg(test)]
mod i1_tests;

#[cfg(test)]
mod selection_geometry_tests;
