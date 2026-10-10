//! Complete document UI state is exchanged only at a safe frame boundary.
use crate::*;

// Adjustable first-slice protection: every record retains service/Undo,
// UI/CPU caches and widget state. Eight bounds admission without promising a
// measured total-memory budget or a permanent/user-requested product limit.
pub(crate) const MAX_SESSIONS: usize = 8;
struct RestorePending {
    dir: std::path::PathBuf,
    metadata: recovery::RecoveryMetadata,
    accepted: Option<(session::Owner, u64)>,
}
enum Change {
    Activate { slot: u64, close: bool },
    Create(Box<Action>),
}
#[derive(Default)]
pub(crate) struct Tabs {
    pub(crate) parked: Vec<UiSessionState>,
    pending: Option<Change>,
    last_slot: u64,
    pub(crate) quitting: bool,
    quit_owner: Option<session::Owner>,
    quit_task: Option<u64>,
    pub(crate) closed: bool,
    pub(crate) retired_close: bool,
    input_barrier: bool,
    held_buttons: [bool; 5],
    pub(crate) block_document_input: bool,
    recovery_cursor: usize,
    disconnected: bool,
    restore: Option<RestorePending>,
}
impl Tabs {
    pub(crate) fn begin_quit(&mut self, owner: session::Owner) {
        if self.quit_owner.as_ref() != Some(&owner) {
            self.quit_task = None;
        }
        self.quitting = true;
        self.quit_owner = Some(owner);
    }
    pub(crate) fn accepted_quit(&mut self, owner: session::Owner, task: u64) {
        if self.quitting && self.quit_owner.as_ref() == Some(&owner) {
            self.quit_task = Some(task);
        }
    }
    pub(crate) fn quit_matches(&self, owner: &session::Owner, task: u64) -> bool {
        self.quitting && self.quit_owner.as_ref() == Some(owner) && self.quit_task == Some(task)
    }
    pub(crate) fn cancel_quit(&mut self, owner: &session::Owner) {
        if self.quit_owner.as_ref() == Some(owner) {
            self.quitting = false;
            self.quit_owner = None;
            self.quit_task = None;
        }
    }
    pub(crate) fn change_pending(&self) -> bool {
        self.pending.is_some() || self.closed
    }
}

impl EditorApp {
    pub(crate) fn tab_input_barrier_active(&self) -> bool {
        self.tabs.input_barrier
    }
    pub(crate) fn restore_pending(&self) -> bool {
        self.tabs.restore.is_some()
    }
    pub(crate) fn request_restore(
        &mut self,
        dir: std::path::PathBuf,
        metadata: recovery::RecoveryMetadata,
    ) {
        if self.tabs.restore.is_some() {
            return;
        }
        self.tabs.restore = Some(RestorePending {
            dir: dir.clone(),
            metadata: metadata.clone(),
            accepted: None,
        });
        self.request_new_session(Action::RestoreSnapshot(dir, metadata));
        if self.tabs.pending.is_none() {
            self.tabs.restore = None;
        }
    }
    pub(crate) fn finish_restore(&mut self, owner: &session::Owner, task: u64, succeeded: bool) {
        if !self.tabs.restore.as_ref().is_some_and(|p| {
            p.accepted
                .as_ref()
                .is_some_and(|(o, id)| o == owner && *id == task)
        }) {
            return;
        }
        let pending = self.tabs.restore.take().unwrap();
        if succeeded {
            self.recovery_source = Some((pending.dir, pending.metadata));
            self.refresh_recovery_candidate();
        }
    }
    pub(crate) fn refresh_recovery_candidate(&mut self) {
        if let Some(dir) = recovery::directory() {
            self.refresh_recovery_candidate_in(&dir);
        } else {
            self.recovery_candidate = None;
        }
    }
    fn refresh_recovery_candidate_in(&mut self, dir: &std::path::Path) {
        self.recovery_candidate = recovery::discover(dir).into_iter().find(|candidate| {
            let source_matches =
                |source: &Option<(std::path::PathBuf, recovery::RecoveryMetadata)>| {
                    source.as_ref().is_some_and(|(_, metadata)| {
                        metadata.project_id == candidate.project_id
                            && metadata.session_key == candidate.session_key
                            && metadata.snapshot_hash == candidate.snapshot_hash
                    })
                };
            !source_matches(&self.recovery_source)
                && !self
                    .tabs
                    .parked
                    .iter()
                    .any(|state| source_matches(&state.recovery_source))
                && !self.view.info.as_ref().is_some_and(|info| {
                    info.project_id == candidate.project_id
                        && candidate.session_key.as_deref()
                            == Some(self.routing.recovery_key().as_str())
                })
                && !self.tabs.parked.iter().any(|state| {
                    state.view.info.as_ref().is_some_and(|info| {
                        info.project_id == candidate.project_id
                            && candidate.session_key.as_deref()
                                == Some(state.routing.recovery_key().as_str())
                    })
                })
        });
        self.recovery_ignore_confirm = false;
        self.recovery_prompt_reported = None;
    }
    pub(crate) fn clean_restored_source(&mut self) {
        if let Some((dir, metadata)) = self.recovery_source.take() {
            // Preserve a changed newer recovery record; it was not this tab's source.
            let _ = recovery::remove_candidate(&dir, &metadata);
        }
    }
    fn tab_interaction_blocked(&self) -> bool {
        self.tabs.disconnected
            || self.tabs.closed
            || self.shortcuts.open
            || self.shortcuts.recording
            || self.modal.is_some()
            || self.modal_pending.is_some()
            || self.transition.is_some()
            || self.close_prompt
            || self.waiting_save
            || self.replace_project_path.is_some()
            || self.project_error.is_some()
            || (self.recovery_candidate.is_some() && self.tabs.restore.is_none())
            || self.ime_active
            || self.ime_event
            || self.drag.is_some()
            || self.grip.is_some()
            || self.point_transform.is_some()
            || self.point_adapter.is_some()
            || self.point_pick.is_some()
            || self.move_place_task.is_some()
            || self.text.floating.is_some()
            || self.block.session.is_some()
            || self.unified_editor.is_some()
            || self.canvas_selection_unconfirmed
            || self.view.blocked.is_some()
            || self.gerber_import.as_ref().is_some_and(|q| q.active())
            || self.bench.is_some()
            || self.probe.is_some()
    }
    pub(crate) fn request_new_session(&mut self, action: Action) {
        if self.tabs.pending.is_some()
            || self.tab_interaction_blocked()
            || self.busy
            || self.pending_task.is_some()
            || self.tabs.quitting
        {
            self.ui_error = Some("请先完成当前工程任务、交互或确认，再打开新页签".into());
            return;
        }
        if self.tabs.parked.len() + 1 >= MAX_SESSIONS {
            self.ui_error = Some(format!(
                "已达到当前工程页签数量限制（{MAX_SESSIONS}），请先关闭一个工程"
            ));
            return;
        }
        if !matches!(
            action,
            Action::NewWorkspace
                | Action::OpenProject(..)
                | Action::RestoreProject(..)
                | Action::RestoreSnapshot(..)
        ) {
            self.ui_error = Some("新页签仅接纳新建、打开或恢复工程".into());
            return;
        }
        self.tabs.pending = Some(Change::Create(Box::new(action)));
        self.cancel_tab_reads();
    }
    fn request_tab(&mut self, slot: u64, close: bool) {
        if self.tabs.pending.is_some()
            || self.tabs.quitting
            || self.tab_interaction_blocked()
            || self.busy
            || self.pending_task.is_some()
        {
            self.ui_error = Some("请先完成当前工程任务、交互或确认，再切换页签".into());
            return;
        }
        if slot == self.routing.owner().slot() {
            if close {
                self.close(false);
            }
            return;
        }
        if !self
            .tabs
            .parked
            .iter()
            .any(|s| s.routing.owner().slot() == slot)
        {
            return;
        }
        self.tabs.pending = Some(Change::Activate { slot, close });
        self.cancel_tab_reads();
    }
    fn cancel_tab_reads(&self) {
        for task in [
            self.viewport_task.as_ref(),
            self.geometry_task.as_ref(),
            self.canvas_read.as_ref().map(|r| &r.task),
        ]
        .into_iter()
        .flatten()
        {
            task.cancel_token.cancel();
        }
    }
    fn exchange_tab_memory(&mut self, ctx: &egui::Context) {
        egui::Popup::close_all(ctx);
        ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
            std::mem::swap(memory, &mut self.document_memory);
        });
    }
    fn activate_state(&mut self, mut state: UiSessionState, ctx: &egui::Context) -> UiSessionState {
        let options = ctx.memory(|m| m.options.clone());
        let fonts = ctx.fonts(|fonts| fonts.definitions().clone());
        self.exchange_tab_memory(ctx);
        state.exchange(self);
        self.exchange_tab_memory(ctx);
        ctx.memory_mut(|m| m.options = options);
        ctx.set_fonts(fonts);
        self.tabs.input_barrier = true;
        self.text_input_at_event = false;
        self.ime_active = false;
        self.ime_event = false;
        self.point_input_frame = None;
        self.click_navigation.invalidate();
        self.view.click_cycle = None;
        ctx.request_repaint();
        state
    }
    pub(crate) fn advance_tab_change(&mut self, ctx: &egui::Context) -> bool {
        if self.tabs.closed {
            if self.tabs.retired_close
                && self
                    .tabs
                    .parked
                    .last()
                    .is_some_and(|state| !state.routing.idle())
            {
                // A parked recovery terminal must settle before Quit starts the
                // next document's Close. Keep painting while that owner waits.
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
                return true;
            }
            self.tabs.closed = false;
            if !self.tabs.retired_close {
                // The last tab keeps an empty registered placeholder, while all
                // closed document state/resources and widget memory are dropped.
                let routing = std::mem::take(&mut self.routing);
                drop(self.activate_state(UiSessionState::fresh(routing), ctx));
                return true;
            }
            if let Some(state) = self.tabs.parked.pop() {
                drop(self.activate_state(state, ctx));
                if self.tabs.quitting {
                    self.close(true);
                }
            } else if self.tabs.quitting {
                self.allow_quit = true;
            }
            return true;
        }
        if self.tabs.pending.is_none() {
            return false;
        }
        if egui::Popup::is_any_open(ctx)
            || ctx.input(|input| {
                !input.focused
                    || input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Ime(_)))
            })
        {
            // Do not suppress all input while waiting for a popup/IME/focus
            // boundary: that could prevent the popup from ever dismissing.
            // Cancel only this tab intent and leave the original document live.
            self.tabs.pending = None;
            self.tabs.restore = None;
            self.ui_error = Some("请先关闭菜单或完成输入，再切换页签".into());
            ctx.request_repaint();
            return true;
        }
        if self.tab_interaction_blocked() {
            self.tabs.pending = None;
            self.tabs.restore = None;
            self.ui_error = Some("工程状态发生变化，页签切换已取消".into());
            return true;
        }
        if !self.routing.idle()
            || self.busy
            || self.pending_task.is_some()
            || self.canvas_read.is_some()
            || self.viewport_task.is_some()
            || self.geometry_task.is_some()
        {
            self.cancel_tab_reads();
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
            return true;
        }
        let change = self.tabs.pending.take().unwrap();
        match change {
            Change::Create(action) => {
                let Some(slot) = self
                    .tabs
                    .last_slot
                    .max(self.routing.owner().slot())
                    .checked_add(1)
                else {
                    self.ui_error = Some("工程页签身份已耗尽".into());
                    return true;
                };
                self.tabs.last_slot = slot;
                let state = UiSessionState::fresh(self.routing.fork(slot));
                let old = self.activate_state(state, ctx);
                self.send(*action);
                if self.busy {
                    if let Some(pending) = &mut self.tabs.restore {
                        pending.accepted = self
                            .pending_task
                            .as_ref()
                            .map(|task| (self.routing.owner(), task.task_id));
                    }
                    self.tabs.parked.push(old);
                } else {
                    let cause = self.ui_error.take();
                    drop(self.activate_state(old, ctx));
                    self.ui_error = cause;
                    self.tabs.restore = None;
                }
            }
            Change::Activate { slot, close } => {
                let Some(index) = self
                    .tabs
                    .parked
                    .iter()
                    .position(|s| s.routing.owner().slot() == slot)
                else {
                    return true;
                };
                if !self.tabs.parked[index].routing.idle() {
                    self.tabs.pending = Some(Change::Activate { slot, close });
                    ctx.request_repaint_after(std::time::Duration::from_millis(20));
                    return true;
                }
                let state = self.tabs.parked.remove(index);
                let old = self.activate_state(state, ctx);
                self.tabs.parked.insert(index, old);
                if close {
                    self.close(false);
                }
            }
        }
        true
    }
    pub(crate) fn show_session_tabs(&mut self, ctx: &egui::Context) -> bool {
        let active = self.routing.owner().slot();
        let mut labels = self
            .tabs
            .parked
            .iter()
            .map(|s| (s.routing.owner().slot(), tab_label(&s.view)))
            .collect::<Vec<_>>();
        labels.push((active, tab_label(&self.view)));
        labels.sort_by_key(|(slot, _)| *slot);
        let mut request = None;
        let mut create = false;
        let mut recovery = false;
        egui::TopBottomPanel::top("project-tabs")
            .exact_height(32.)
            .show(ctx, |ui| {
                if self.tabs.input_barrier {
                    ui.disable();
                }
                egui::ScrollArea::horizontal()
                    .id_salt("project-tabs-scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (slot, title) in labels {
                                ui.push_id(slot, |ui| {
                                    if ui
                                        .selectable_label(slot == active, &title)
                                        .on_hover_text(&title)
                                        .clicked()
                                    {
                                        request = Some((slot, false));
                                    }
                                    if ui.small_button("×").on_hover_text("关闭此工程").clicked()
                                    {
                                        request = Some((slot, true));
                                    }
                                });
                            }
                            create = ui.small_button("+").on_hover_text("新建工程页签").clicked();
                            recovery = ui
                                .small_button("恢复副本…")
                                .on_hover_text("查看保留在本机的恢复副本")
                                .clicked();
                            if self.tabs.pending.is_some() || self.tabs.closed {
                                ui.spinner();
                                ui.label("正在等待工程任务确认");
                                if self.tabs.pending.is_some()
                                    && ui.small_button("取消切换").clicked()
                                {
                                    // Keep the original task/receipt lane alive until its
                                    // terminal. Cancelling this intent never clears it.
                                    self.tabs.pending = None;
                                    self.tabs.restore = None;
                                }
                            }
                        });
                    });
            });
        if let Some((slot, close)) = request {
            self.request_tab(slot, close);
        }
        if create {
            self.request_new_session(Action::NewWorkspace);
        }
        if recovery && !self.tabs.change_pending() && !self.tabs.quitting {
            self.refresh_recovery_candidate();
            if self.recovery_candidate.is_none() {
                self.ui_error = Some("没有待恢复的副本".into());
            }
        }
        self.tabs.pending.is_some()
    }
    pub(crate) fn enforce_tab_input_barrier(&mut self, ctx: &egui::Context) {
        let blocked = self.tabs.input_barrier || self.tabs.change_pending();
        self.tabs.block_document_input = blocked;
        if !blocked {
            return;
        }
        let held = ctx.input(|i| {
            for (index, button) in [
                egui::PointerButton::Primary,
                egui::PointerButton::Secondary,
                egui::PointerButton::Middle,
                egui::PointerButton::Extra1,
                egui::PointerButton::Extra2,
            ]
            .into_iter()
            .enumerate()
            {
                self.tabs.held_buttons[index] |= i.pointer.button_down(button);
                for event in &i.raw.events {
                    match event {
                        egui::Event::PointerButton {
                            button: actual,
                            pressed,
                            ..
                        } if *actual == button => self.tabs.held_buttons[index] = *pressed,
                        egui::Event::PointerGone => self.tabs.held_buttons[index] = false,
                        _ => {}
                    }
                }
            }
            !i.keys_down.is_empty()
                || i.modifiers != egui::Modifiers::NONE
                || self.tabs.held_buttons.iter().any(|down| *down)
        });
        ctx.input_mut(|i| {
            i.events.clear();
            i.raw.events.clear();
            i.raw.dropped_files.clear();
            // egui derives these before update. Clearing events alone cannot
            // stop a release, scroll or pan from the previous document.
            i.pointer = Default::default();
            i.raw_scroll_delta = egui::Vec2::ZERO;
            i.smooth_scroll_delta = egui::Vec2::ZERO;
        });
        self.shortcuts.presses.clear();
        self.point_input_cancelled = true;
        self.point_commit_blocked = true;
        // The release-only frame stays blocked; the next frame may accept fresh input.
        if !held && self.tabs.pending.is_none() {
            self.tabs.input_barrier = false;
        }
    }
    pub(crate) fn receive_session_reply(&mut self, envelope: session::Reply, now: Instant) {
        if envelope.2.slot() == self.routing.owner().slot() {
            self.receive_owned_reply(envelope, now);
        } else if let Some(index) = self
            .tabs
            .parked
            .iter()
            .position(|s| s.routing.owner().slot() == envelope.2.slot())
        {
            let mut state = self.tabs.parked.remove(index);
            state.exchange(self);
            self.receive_owned_reply(envelope, now);
            state.exchange(self);
            self.tabs.parked.insert(index, state);
        }
    }
    pub(crate) fn tick_parked_recovery(&mut self, now: Instant, ctx: &egui::Context) {
        let count = self.tabs.parked.len();
        if count == 0 || self.tabs.disconnected || self.tabs.quitting {
            return;
        }
        let index = self.tabs.recovery_cursor % count;
        self.tabs.recovery_cursor = (index + 1) % count;
        let mut state = self.tabs.parked.remove(index);
        state.exchange(self);
        self.tick_recovery(now);
        state.exchange(self);
        self.tabs.parked.insert(index, state);
        if self
            .tabs
            .parked
            .iter()
            .any(|s| s.view.info.as_ref().is_some_and(|i| i.project_dirty))
        {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
    pub(crate) fn disconnect_parked_sessions(&mut self) {
        self.tabs.disconnected = true;
        self.tabs.restore = None;
        self.tabs.pending = None;
        self.tabs.quitting = false;
        for index in 0..self.tabs.parked.len() {
            let mut state = self.tabs.parked.remove(index);
            state.exchange(self);
            if let Some(id) = self.pending_task.as_ref().map(|t| t.task_id) {
                self.reject_owned_reply(
                    id,
                    editor_service::ServiceError {
                        code: "WORKER_DISCONNECTED".into(),
                        message: "后台连接已关闭，工程任务终态未确认".into(),
                        details: serde_json::json!({}),
                    },
                );
            }
            self.routing.disconnect();
            self.pending_recovery_identity = None;
            self.pending_recovery_task = None;
            state.exchange(self);
            self.tabs.parked.insert(index, state);
        }
    }
}
fn tab_label(view: &View) -> String {
    view.info.as_ref().map_or_else(
        || "未打开工程".into(),
        |info| {
            let name = info
                .project_path
                .as_ref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map_or_else(
                    || "Untitled".into(),
                    |name| name.to_string_lossy().into_owned(),
                );
            format!("{name}{}", if info.project_dirty { " *" } else { "" })
        },
    )
}

pub(crate) struct UiSessionState {
    components: components_ui::UiState,
    block: block_ui::UiState,
    routing: session::SessionRouting,
    pending_task: Option<editor_service::task::TaskContext>,
    canvas_read: Option<canvas_read::Pending>,
    canvas_selection_unconfirmed: bool,
    gerber_import: Option<gerber_import::ImportQueue>,
    viewport_task: Option<editor_service::task::TaskContext>,
    geometry_task: Option<editor_service::task::TaskContext>,
    geometry_context: Option<String>,
    view: View,
    busy: bool,
    viewport_sequence: Option<u64>,
    sequence: u64,
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
    point_adapter: Option<point_adapter::Session>,
    array_point_base: Option<editor_core::MmPoint>,
    block_point_reference: editor_core::MmPoint,
    point_transform: Option<point_transform::Session>,
    unified_editor: Option<crate::unified_editor_ui::Draft>,
    point_pick: Option<point_transform::Pick>,
    move_place_task: Option<move_place::Pending>,
    point_input_frame: Option<u64>,
    point_input_cancelled: bool,
    point_commit_blocked: bool,
    modal: Option<ActiveModal>,
    modal_pending: Option<u64>,
    draft_snap: bool,
    ime_event: bool,
    measure: tools::MeasureState,
    fit: bool,
    dx: String,
    dy: String,
    size_aperture_id: Option<String>,
    size_width: String,
    size_height: String,
    display_unit: tools::DisplayUnit,
    layer: Option<String>,
    layer_dialog: Option<layer_panel::LayerDialog>,
    layer_dialog_close_on_success: bool,
    pending_summary: Option<editor_service::LayerSummaryResult>,
    recovery_source: Option<(std::path::PathBuf, recovery::RecoveryMetadata)>,
    recovery_attempted_identity: Option<String>,
    last_dirty_identity: String,
    dirty_since: Instant,
    last_recovery_at: Instant,
    last_recovered_identity: String,
    pending_recovery_identity: Option<String>,
    pending_recovery_task: Option<(session::Owner, u64)>,
    toast: Option<(String, Instant)>,
    last_structure_serial: u64,
    transition: Option<project_ui::Transition>,
    close_prompt: bool,
    waiting_save: bool,
    replace_project_path: Option<std::path::PathBuf>,
    pending_project_error_title: Option<&'static str>,
    project_error: Option<(String, String)>,
    quit_after_close: bool,
    ui_error: Option<String>,
    canvas_rect: egui::Rect,
    display_error: Option<String>,
    display_pending: bool,
    drag: Option<drag::Gesture>,
    grip: Option<grip::Session>,
    selected_flags: std::sync::Arc<Vec<u32>>,
    uniform_validation: UniformValidationCache,
    selection_presentation: selection_presentation::Cache,
    prepare_work: gpu::PrepareWorkCache,
    text_input_at_event: bool,
    ime_active: bool,
    document_memory: egui::Memory,
}
impl UiSessionState {
    #[cfg(test)]
    pub(crate) fn view(&self) -> &View {
        &self.view
    }
    fn fresh(routing: session::SessionRouting) -> Self {
        Self {
            components: components_ui::UiState::default(),
            block: Default::default(),
            routing,
            pending_task: None,
            canvas_read: None,
            canvas_selection_unconfirmed: false,
            gerber_import: None,
            viewport_task: None,
            geometry_task: None,
            geometry_context: None,
            view: View::default(),
            busy: false,
            viewport_sequence: None,
            sequence: 0,
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
            array: array_ui::Draft::default(),
            point_adapter: None,
            array_point_base: None,
            block_point_reference: editor_core::MmPoint::new(0., 0.),
            point_transform: None,
            unified_editor: None,
            point_pick: None,
            move_place_task: None,
            point_input_frame: None,
            point_input_cancelled: false,
            point_commit_blocked: false,
            modal: None,
            modal_pending: None,
            draft_snap: false,
            ime_event: false,
            measure: Default::default(),
            fit: false,
            dx: "0".into(),
            dy: "0".into(),
            size_aperture_id: None,
            size_width: String::new(),
            size_height: String::new(),
            display_unit: Default::default(),
            layer: None,
            layer_dialog: None,
            layer_dialog_close_on_success: false,
            pending_summary: None,
            recovery_source: None,
            recovery_attempted_identity: None,
            last_dirty_identity: String::new(),
            dirty_since: Instant::now(),
            last_recovery_at: Instant::now() - std::time::Duration::from_secs(60),
            last_recovered_identity: String::new(),
            pending_recovery_identity: None,
            pending_recovery_task: None,
            toast: None,
            last_structure_serial: 0,
            transition: None,
            close_prompt: false,
            waiting_save: false,
            replace_project_path: None,
            pending_project_error_title: None,
            project_error: None,
            quit_after_close: false,
            ui_error: None,
            canvas_rect: egui::Rect::NOTHING,
            display_error: None,
            display_pending: false,
            drag: None,
            grip: None,
            selected_flags: Default::default(),
            uniform_validation: Default::default(),
            selection_presentation: Default::default(),
            prepare_work: Default::default(),
            text_input_at_event: false,
            ime_active: false,
            document_memory: Default::default(),
        }
    }
    fn exchange(&mut self, app: &mut EditorApp) {
        // Deliberately exhaustive: any new app field must be classified.
        let EditorApp {
            components: _,
            block: _,
            operation_source: _,
            diagnostic_export: _,
            tx: _,
            routing: _,
            pending_task: _,
            canvas_read: _,
            canvas_selection_unconfirmed: _,
            gerber_import: _,
            viewport_task: _,
            geometry_task: _,
            geometry_context: _,
            rx: _,
            #[cfg(test)]
                _fixture_reply: _,
            view: _,
            busy: _,
            viewport_sequence: _,
            sequence: _,
            task_serial: _,
            request_failure_serial: _,
            camera: _,
            click_navigation: _,
            last_good: _,
            grid: _,
            grid_visual: _,
            object_snap: _,
            object_snap_runtime: _,
            draft_object_snap: _,
            spacing: _,
            tool: _,
            text: _,
            array: _,
            point_adapter: _,
            array_point_base: _,
            block_point_reference: _,
            point_transform: _,
            unified_editor: _,
            point_pick: _,
            move_place_task: _,
            point_input_frame: _,
            point_input_cancelled: _,
            point_commit_blocked: _,
            modal: _,
            modal_pending: _,
            draft_snap: _,
            ime_event: _,
            measure: _,
            fit: _,
            dx: _,
            dy: _,
            size_aperture_id: _,
            size_width: _,
            size_height: _,
            display_unit: _,
            layer: _,
            layer_dialog: _,
            layer_dialog_close_on_success: _,
            pending_summary: _,
            recent_colors: _,
            prefs: _,
            shortcuts: _,
            recovery_candidate: _,
            recovery_prompt_reported: _,
            recovery_source: _,
            recovery_attempted_identity: _,
            recovery_ignore_confirm: _,
            last_dirty_identity: _,
            dirty_since: _,
            last_recovery_at: _,
            last_recovered_identity: _,
            pending_recovery_identity: _,
            pending_recovery_task: _,
            toast: _,
            last_structure_serial: _,
            transition: _,
            close_prompt: _,
            waiting_save: _,
            replace_project_path: _,
            pending_project_error_title: _,
            project_error: _,
            quit_after_close: _,
            allow_quit: _,
            format: _,
            adapter: _,
            ui_error: _,
            last_title: _,
            canvas_rect: _,
            display_error: _,
            display_pending: _,
            drag: _,
            grip: _,
            bench: _,
            #[cfg(feature = "internal-evidence")]
                s5m1: _,
            #[cfg(feature = "internal-evidence")]
                a2: _,
            #[cfg(feature = "internal-evidence")]
                batch_drag: _,
            #[cfg(feature = "internal-evidence")]
                i1: _,
            #[cfg(feature = "internal-evidence")]
                pmix: _,
            probe: _,
            row_probes: _,
            layer_panel_rect: _,
            timing: _,
            frame_trace: _,
            selected_flags: _,
            uniform_validation: _,
            selection_presentation: _,
            prepare_work: _,
            last_frame: _,
            text_input_at_event: _,
            ime_active: _,
            reported_ppp: _,
            tabs: _,
            document_memory: _,
        } = app;
        std::mem::swap(&mut self.components, &mut app.components);
        std::mem::swap(&mut self.block, &mut app.block);
        std::mem::swap(&mut self.routing, &mut app.routing);
        std::mem::swap(&mut self.pending_task, &mut app.pending_task);
        std::mem::swap(&mut self.canvas_read, &mut app.canvas_read);
        std::mem::swap(
            &mut self.canvas_selection_unconfirmed,
            &mut app.canvas_selection_unconfirmed,
        );
        std::mem::swap(&mut self.gerber_import, &mut app.gerber_import);
        std::mem::swap(&mut self.viewport_task, &mut app.viewport_task);
        std::mem::swap(&mut self.geometry_task, &mut app.geometry_task);
        std::mem::swap(&mut self.geometry_context, &mut app.geometry_context);
        std::mem::swap(&mut self.view, &mut app.view);
        std::mem::swap(&mut self.busy, &mut app.busy);
        std::mem::swap(&mut self.viewport_sequence, &mut app.viewport_sequence);
        std::mem::swap(&mut self.sequence, &mut app.sequence);
        std::mem::swap(&mut self.camera, &mut app.camera);
        std::mem::swap(&mut self.click_navigation, &mut app.click_navigation);
        std::mem::swap(&mut self.last_good, &mut app.last_good);
        std::mem::swap(&mut self.grid, &mut app.grid);
        std::mem::swap(&mut self.grid_visual, &mut app.grid_visual);
        std::mem::swap(&mut self.object_snap, &mut app.object_snap);
        std::mem::swap(&mut self.object_snap_runtime, &mut app.object_snap_runtime);
        std::mem::swap(&mut self.draft_object_snap, &mut app.draft_object_snap);
        std::mem::swap(&mut self.spacing, &mut app.spacing);
        std::mem::swap(&mut self.tool, &mut app.tool);
        std::mem::swap(&mut self.text, &mut app.text);
        std::mem::swap(&mut self.array, &mut app.array);
        std::mem::swap(&mut self.point_adapter, &mut app.point_adapter);
        std::mem::swap(&mut self.array_point_base, &mut app.array_point_base);
        std::mem::swap(
            &mut self.block_point_reference,
            &mut app.block_point_reference,
        );
        std::mem::swap(&mut self.point_transform, &mut app.point_transform);
        std::mem::swap(&mut self.unified_editor, &mut app.unified_editor);
        std::mem::swap(&mut self.point_pick, &mut app.point_pick);
        std::mem::swap(&mut self.move_place_task, &mut app.move_place_task);
        std::mem::swap(&mut self.point_input_frame, &mut app.point_input_frame);
        std::mem::swap(
            &mut self.point_input_cancelled,
            &mut app.point_input_cancelled,
        );
        std::mem::swap(
            &mut self.point_commit_blocked,
            &mut app.point_commit_blocked,
        );
        std::mem::swap(&mut self.modal, &mut app.modal);
        std::mem::swap(&mut self.modal_pending, &mut app.modal_pending);
        std::mem::swap(&mut self.draft_snap, &mut app.draft_snap);
        std::mem::swap(&mut self.ime_event, &mut app.ime_event);
        std::mem::swap(&mut self.measure, &mut app.measure);
        std::mem::swap(&mut self.fit, &mut app.fit);
        std::mem::swap(&mut self.dx, &mut app.dx);
        std::mem::swap(&mut self.dy, &mut app.dy);
        std::mem::swap(&mut self.size_aperture_id, &mut app.size_aperture_id);
        std::mem::swap(&mut self.size_width, &mut app.size_width);
        std::mem::swap(&mut self.size_height, &mut app.size_height);
        std::mem::swap(&mut self.display_unit, &mut app.display_unit);
        std::mem::swap(&mut self.layer, &mut app.layer);
        std::mem::swap(&mut self.layer_dialog, &mut app.layer_dialog);
        std::mem::swap(
            &mut self.layer_dialog_close_on_success,
            &mut app.layer_dialog_close_on_success,
        );
        std::mem::swap(&mut self.pending_summary, &mut app.pending_summary);
        std::mem::swap(&mut self.recovery_source, &mut app.recovery_source);
        std::mem::swap(
            &mut self.recovery_attempted_identity,
            &mut app.recovery_attempted_identity,
        );
        std::mem::swap(&mut self.last_dirty_identity, &mut app.last_dirty_identity);
        std::mem::swap(&mut self.dirty_since, &mut app.dirty_since);
        std::mem::swap(&mut self.last_recovery_at, &mut app.last_recovery_at);
        std::mem::swap(
            &mut self.last_recovered_identity,
            &mut app.last_recovered_identity,
        );
        std::mem::swap(
            &mut self.pending_recovery_identity,
            &mut app.pending_recovery_identity,
        );
        std::mem::swap(
            &mut self.pending_recovery_task,
            &mut app.pending_recovery_task,
        );
        std::mem::swap(&mut self.toast, &mut app.toast);
        std::mem::swap(
            &mut self.last_structure_serial,
            &mut app.last_structure_serial,
        );
        std::mem::swap(&mut self.transition, &mut app.transition);
        std::mem::swap(&mut self.close_prompt, &mut app.close_prompt);
        std::mem::swap(&mut self.waiting_save, &mut app.waiting_save);
        std::mem::swap(
            &mut self.replace_project_path,
            &mut app.replace_project_path,
        );
        std::mem::swap(
            &mut self.pending_project_error_title,
            &mut app.pending_project_error_title,
        );
        std::mem::swap(&mut self.project_error, &mut app.project_error);
        std::mem::swap(&mut self.quit_after_close, &mut app.quit_after_close);
        std::mem::swap(&mut self.ui_error, &mut app.ui_error);
        std::mem::swap(&mut self.canvas_rect, &mut app.canvas_rect);
        std::mem::swap(&mut self.display_error, &mut app.display_error);
        std::mem::swap(&mut self.display_pending, &mut app.display_pending);
        std::mem::swap(&mut self.drag, &mut app.drag);
        std::mem::swap(&mut self.grip, &mut app.grip);
        std::mem::swap(&mut self.selected_flags, &mut app.selected_flags);
        std::mem::swap(&mut self.uniform_validation, &mut app.uniform_validation);
        std::mem::swap(
            &mut self.selection_presentation,
            &mut app.selection_presentation,
        );
        std::mem::swap(&mut self.prepare_work, &mut app.prepare_work);
        std::mem::swap(&mut self.text_input_at_event, &mut app.text_input_at_event);
        std::mem::swap(&mut self.ime_active, &mut app.ime_active);
        std::mem::swap(&mut self.document_memory, &mut app.document_memory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    struct Run {
        app: EditorApp,
        ctx: egui::Context,
        requests: mpsc::Receiver<session::Request>,
        host: session::WorkerHost,
        dir: PathBuf,
    }
    impl Run {
        fn new() -> Self {
            let mut app = modal::tests::app();
            let (tx, requests) = mpsc::sync_channel(16);
            app.tx = tx;
            let host = session::WorkerHost::new(app.routing.owner(), Model::default());
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |_| {});
            let dir = std::env::temp_dir().join(format!(
                "rcam-tabs-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            let mut run = Self {
                app,
                ctx,
                requests,
                host,
                dir,
            };
            run.action(Action::NewWorkspace);
            run
        }
        fn execute(&mut self) -> session::Reply {
            let (id, _, mut action, task, route) = self.requests.try_recv().unwrap();
            // Only synthetic test output is written by this harness.
            if matches!(action, Action::RecoveryWrite(..)) {
                action = Action::RecoveryWrite(self.dir.join("recovery"));
            }
            self.host.route_action(&route, &action).unwrap();
            self.host.model.run_task(task, action);
            let reply = self.host.finish(&route).unwrap();
            (id, self.host.model.view.clone(), reply)
        }
        fn settle(&mut self) -> session::Reply {
            let reply = self.execute();
            self.app
                .receive_session_reply(reply.clone(), Instant::now());
            reply
        }
        fn action(&mut self, action: Action) {
            self.app.send(action);
            self.settle();
            assert!(self.app.view.error.is_none(), "{:?}", self.app.view.error);
        }
        fn advance(&mut self) {
            let _ = self.ctx.run(Default::default(), |ctx| {
                self.app.advance_tab_change(ctx);
            });
        }
        fn create(&mut self) {
            self.app.request_new_session(Action::NewWorkspace);
            self.advance();
            self.settle();
            assert!(self.app.view.error.is_none());
        }
        fn activate(&mut self, slot: u64) {
            self.app.request_tab(slot, false);
            self.advance();
            assert_eq!(self.app.routing.owner().slot(), slot);
        }
        fn import(&mut self, x: i32) {
            let path = self.dir.join(format!("{x}.gbr"));
            std::fs::write(
                &path,
                format!("%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.4*%\nD10*\nX{x}000000Y0D03*\nM02*\n"),
            )
            .unwrap();
            self.action(Action::ImportGerbers(vec![path]));
            self.action(Action::SelectAll);
        }
    }
    impl Drop for Run {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    #[test]
    fn unadmitted_quit_does_not_lock_tabs_and_pending_inactive_recovery_settles_before_next_close()
    {
        let mut full = Run::new();
        let info = full.app.view.info.clone();
        let (tx, _full) = mpsc::sync_channel(0);
        full.app.tx = tx;
        full.app.close(true);
        assert!(!full.app.tabs.quitting);
        assert!(!full.app.quit_after_close);
        assert_eq!(full.app.view.info, info);
        assert!(!full.app.busy);
        assert!(full.app.ui_error.is_some());

        let mut save = Run::new();
        save.import(10);
        save.action(Action::SaveProject(
            Some(save.dir.join("saved.rcam")),
            false,
            None,
        ));
        save.action(Action::Move("1".into(), "0".into()));
        save.app.close(true);
        assert!(save.app.close_prompt);
        let (tx, _full) = mpsc::sync_channel(0);
        save.app.tx = tx;
        assert!(!save.app.save_project(false));
        assert!(!save.app.tabs.quitting);
        assert!(!save.app.waiting_save);
        assert!(save.app.transition.is_none());
        assert!(save.app.view.info.as_ref().unwrap().project_dirty);

        let mut run = Run::new();
        run.import(10);
        run.create();
        let now = Instant::now();
        run.app.tabs.parked[0].last_dirty_identity.clear();
        run.app.tick_parked_recovery(now, &run.ctx);
        run.app.tabs.parked[0].dirty_since = now - std::time::Duration::from_secs(40);
        run.app.tabs.parked[0].last_recovery_at = now - std::time::Duration::from_secs(80);
        run.app.tick_parked_recovery(now, &run.ctx);
        let delayed = run.execute(); // actual immutable snapshot, terminal delivery held
        let closing = run.app.routing.owner().slot();
        run.app.close(true);
        run.settle();
        run.advance();
        assert!(run.app.tabs.closed);
        assert_eq!(run.app.routing.owner().slot(), closing);
        assert!(!run.app.allow_quit);
        run.app.receive_session_reply(delayed, now);
        run.advance();
        assert!(!run.app.tabs.closed);
        assert_ne!(run.app.routing.owner().slot(), closing);
        assert!(run.app.close_prompt);
        assert!(run.app.tabs.quitting);
        run.app.cancel_transition();
        assert!(!run.app.tabs.quitting);
    }
    #[test]
    fn unadmitted_save_for_quit_preserves_dirty_document_and_releases_exit_intent() {
        let mut run = Run::new();
        run.import(10);
        run.action(Action::SaveProject(
            Some(run.dir.join("saved.rcam")),
            false,
            None,
        ));
        run.action(Action::Move("1".into(), "0".into()));
        let info = run.app.view.info.clone();
        run.app.close(true);
        assert!(run.app.close_prompt);
        let (tx, _full) = mpsc::sync_channel(0);
        run.app.tx = tx;
        assert!(!run.app.save_project(false));
        assert!(!run.app.tabs.quitting);
        assert!(run.app.transition.is_none());
        assert_eq!(run.app.view.info, info);
        assert!(run.app.view.info.as_ref().unwrap().project_dirty);
        assert!(!run.app.allow_quit);
    }
    #[test]
    fn replacement_button_queue_rejection_releases_quit_and_reopens_close_choice() {
        for quit in [false, true] {
            let mut run = Run::new();
            run.import(10);
            let info = run.app.view.info.clone();
            run.app.close(quit);
            run.app.waiting_save = true;
            run.app.close_prompt = false;
            run.app.replace_project_path = Some(run.dir.join("existing.rcam"));
            let original = b"existing file must remain untouched";
            std::fs::write(run.app.replace_project_path.as_ref().unwrap(), original).unwrap();
            let (tx, _full) = mpsc::sync_channel(0);
            run.app.tx = tx;
            let frame = |events| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                events,
                ..Default::default()
            };
            for _ in 0..3 {
                let _ = run
                    .ctx
                    .run(frame(vec![]), |ctx| run.app.project_prompts(ctx));
            }
            let pos = run
                .ctx
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new("project-replace-confirm-rect"))
                })
                .unwrap()
                .center();
            for pressed in [true, false] {
                let _ = run.ctx.run(
                    frame(vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]),
                    |ctx| run.app.project_prompts(ctx),
                );
            }
            assert!(run.app.replace_project_path.is_none());
            assert!(!run.app.waiting_save);
            assert!(!run.app.busy);
            assert!(!run.app.allow_quit);
            assert_eq!(run.app.view.info, info);
            assert_eq!(
                std::fs::read(run.dir.join("existing.rcam")).unwrap(),
                original
            );
            assert!(run.app.ui_error.is_some());
            if quit {
                assert!(!run.app.tabs.quitting);
                assert!(run.app.transition.is_none());
            } else {
                assert!(run.app.close_prompt);
                assert!(matches!(
                    run.app.transition,
                    Some(project_ui::Transition::Close)
                ));
                run.app.cancel_transition();
                assert!(run.app.routing.idle());
            }
        }
    }

    #[test]
    fn replacement_waits_for_actual_recovery_terminal_then_admits_save() {
        for quit in [false, true] {
            let mut run = Run::new();
            run.import(10);
            run.app.close(quit);
            run.app.waiting_save = true;
            run.app.close_prompt = false;
            let path = run.dir.join("existing.rcam");
            std::fs::write(&path, b"existing file").unwrap();
            run.app.replace_project_path = Some(path.clone());
            run.app
                .send(Action::RecoveryWrite(run.dir.join("recovery")));
            assert!(run.app.busy);
            let recovery_task = run.app.pending_task.as_ref().unwrap().task_id;
            let reply = run.execute(); // Hold the authentic Recovery terminal.
            let click_replace = |run: &mut Run| {
                let frame = |events| egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events,
                    ..Default::default()
                };
                for _ in 0..3 {
                    let _ = run
                        .ctx
                        .run(frame(vec![]), |ctx| run.app.project_prompts(ctx));
                }
                let pos = run
                    .ctx
                    .data(|data| {
                        data.get_temp::<egui::Rect>(egui::Id::new("project-replace-confirm-rect"))
                    })
                    .unwrap()
                    .center();
                for pressed in [true, false] {
                    let _ = run.ctx.run(
                        frame(vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ]),
                        |ctx| run.app.project_prompts(ctx),
                    );
                }
            };
            click_replace(&mut run);
            assert_eq!(run.app.replace_project_path.as_ref(), Some(&path));
            assert!(run.app.waiting_save);
            assert_eq!(
                run.app.pending_task.as_ref().unwrap().task_id,
                recovery_task
            );
            assert!(run.requests.try_recv().is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"existing file");
            run.app.receive_session_reply(reply, Instant::now());
            assert!(!run.app.busy);
            click_replace(&mut run);
            assert!(run.app.replace_project_path.is_none());
            assert!(run.app.busy);
            assert_ne!(
                run.app.pending_task.as_ref().unwrap().task_id,
                recovery_task
            );
            run.settle();
            assert!(run.app.view.error.is_none());
            assert!(!run.app.view.info.as_ref().unwrap().project_dirty);
            run.app.saved_for_transition();
            assert!(!run.app.waiting_save);
            assert!(run.app.busy); // The real successful Save admits original Close.
            run.settle();
            run.advance();
            assert!(run.app.view.info.is_none());
            assert_eq!(run.app.allow_quit, quit);
        }
    }

    #[test]
    fn held_release_scroll_zoom_and_shortcuts_cannot_cross_document_boundary() {
        let mut run = Run::new();
        run.app.camera.center = editor_core::MmPoint::new(7., 9.);
        run.app.camera.scale = 42.;
        run.app.fit = false;
        let camera = run.app.camera;
        run.app.tabs.input_barrier = true;
        let pos = egui::pos2(300., 200.);
        let pressed = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(50., 70.),
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Zoom(2.),
            ],
            ..Default::default()
        };
        let out = run.ctx.run(pressed, |ctx| {
            run.app.observed_update_body(ctx, &None);
            assert!(run.app.tabs.block_document_input);
            assert!(!ctx.input(|i| i.pointer.any_down() || i.pointer.any_released()));
            assert_eq!(ctx.input(|i| i.smooth_scroll_delta), egui::Vec2::ZERO);
        });
        assert!(!out.shapes.is_empty());
        assert_eq!(run.app.camera.center, camera.center);
        assert_eq!(run.app.camera.scale, camera.scale);
        let _ = run.ctx.run(Default::default(), |ctx| {
            run.app.enforce_tab_input_barrier(ctx);
            assert!(
                run.app.tabs.input_barrier,
                "held button survives the cleared PointerState"
            );
        });
        let released = egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = run.ctx.run(released, |ctx| {
            run.app.enforce_tab_input_barrier(ctx);
            assert!(
                run.app.tabs.block_document_input,
                "release-only frame remains blocked"
            );
            assert!(!ctx.input(|i| i.pointer.any_released()));
        });
        let _ = run.ctx.run(Default::default(), |ctx| {
            run.app.enforce_tab_input_barrier(ctx);
            assert!(!run.app.tabs.block_document_input);
        });
    }
    #[test]
    fn changed_apply_cancelled_geometry_and_tab_reentry_release_task_and_input_fences() {
        let mut run = Run::new();
        run.import(10);
        let a = run.app.routing.owner().slot();
        run.create();
        run.import(20);
        let b = run.app.routing.owner().slot();
        let b_info = run.app.view.info.clone();
        run.activate(a);
        for _ in 0..2 {
            let _ = run.ctx.run(Default::default(), |ctx| {
                run.app.arbitrate_point_input_frame(ctx);
                run.app.enforce_tab_input_barrier(ctx)
            });
        }
        run.app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        run.app.open_unified_editor();
        run.settle();
        let entry = run
            .host
            .model
            .service
            .render_snapshot(&run.app.view.info.as_ref().unwrap().document_id)
            .unwrap();
        let step = editor_service::DraftStep {
            groups: run.app.view.selected.groups(),
            operation: editor_service::SelectionEdit::Move {
                dx_mm: 1.,
                dy_mm: 0.,
            },
        };
        run.app
            .send_unified_editor(crate::unified_editor_worker::Command::Apply(Some(step)));
        run.settle();
        assert!(run.app.unified_editor.is_none() && !run.app.busy);
        let a_info = run.app.view.info.clone();
        run.app.send(Action::SelectionCenters(
            crate::state::selection_geometry_identity(&run.app.view),
            editor_service::SelectionCentersParams {
                groups: run.app.view.selected.groups(),
                semantics: editor_service::SelectionMaterialSemantics::SelectedLayerComposite,
            },
        ));
        assert!(run.app.geometry_task.is_some());
        run.app.request_tab(b, false);
        run.advance();
        assert_eq!(run.app.routing.owner().slot(), a);
        assert!(run.app.tabs.change_pending());
        run.settle();
        assert!(run.app.geometry_task.is_none() && run.app.routing.idle());
        run.advance();
        assert_eq!(run.app.routing.owner().slot(), b);
        assert_eq!(run.app.view.info, b_info);
        assert!(!run.app.tabs.change_pending());
        for _ in 0..2 {
            let _ = run.ctx.run(Default::default(), |ctx| {
                run.app.arbitrate_point_input_frame(ctx);
                run.app.enforce_tab_input_barrier(ctx)
            });
        }
        assert!(!run.app.tabs.input_barrier && !run.app.tabs.block_document_input);
        assert!(!run.app.busy && run.app.pending_task.is_none());
        run.activate(a);
        assert_eq!(run.app.view.info, a_info);
        for _ in 0..2 {
            let _ = run.ctx.run(Default::default(), |ctx| {
                run.app.arbitrate_point_input_frame(ctx);
                run.app.enforce_tab_input_barrier(ctx)
            });
        }
        run.app.send(Action::SelectionCenters(
            crate::state::selection_geometry_identity(&run.app.view),
            editor_service::SelectionCentersParams {
                groups: run.app.view.selected.groups(),
                semantics: editor_service::SelectionMaterialSemantics::SelectedLayerComposite,
            },
        ));
        let reply = run.execute();
        assert!(reply.1.error.is_none());
        assert_eq!(reply.1.selection_epoch, run.app.view.selection_epoch);
        run.app.receive_session_reply(reply, Instant::now());
        assert!(run.app.view.selection_geometry.is_some());
        run.action(Action::History(false));
        let undone = run
            .host
            .model
            .service
            .render_snapshot(&run.app.view.info.as_ref().unwrap().document_id)
            .unwrap();
        assert_eq!(undone.layers, entry.layers);
        assert_eq!(undone.apertures, entry.apertures);
        run.activate(b);
        assert_eq!(run.app.view.info, b_info);
    }
    #[test]
    fn waiting_read_keeps_document_and_tabs_painted_and_cancel_keeps_receipt_lane() {
        let mut run = Run::new();
        let a = run.app.routing.owner().slot();
        run.create();
        run.app.send(Action::Viewport(
            editor_core::MmPoint::new(0., 0.),
            editor_core::BoundsMm {
                min_x_mm: -1.,
                min_y_mm: -1.,
                max_x_mm: 1.,
                max_y_mm: 1.,
            },
            20.,
        ));
        let task = run.app.viewport_task.as_ref().unwrap().task_id;
        run.app.request_tab(a, false);
        let output = run.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            },
            |ctx| run.app.observed_update_body(ctx, &None),
        );
        assert!(run.app.tabs.change_pending());
        assert!(
            output.shapes.len() > 10,
            "waiting must still paint the whole document chrome"
        );
        assert!(run.app.canvas_rect.is_positive());
        assert_eq!(run.app.viewport_task.as_ref().unwrap().task_id, task);
        run.app.tabs.pending = None;
        run.settle();
        assert!(run.app.viewport_task.is_none());
        assert!(run.app.routing.idle());
    }
    #[test]
    fn cap_retirement_failed_open_and_empty_placeholder_preserve_live_records() {
        let mut run = Run::new();
        let first = run.app.routing.owner().slot();
        for _ in 1..MAX_SESSIONS {
            run.create();
        }
        assert_eq!(run.app.tabs.parked.len(), MAX_SESSIONS - 1);
        let last = run.app.tabs.last_slot;
        run.app.request_new_session(Action::NewWorkspace);
        assert!(run.app.tabs.pending.is_none());
        assert_eq!(run.app.tabs.last_slot, last);
        run.app.close(false);
        run.settle();
        run.advance();
        assert_eq!(run.app.tabs.parked.len(), MAX_SESSIONS - 2);
        run.app
            .request_new_session(Action::OpenProject(run.dir.join("missing.rcam"), false));
        run.advance();
        let failed_slot = run.app.routing.owner().slot();
        run.settle();
        assert_eq!(run.app.view.error.as_ref().unwrap().code, "IO_ERROR");
        assert!(failed_slot > last);
        run.app.project_error = None;
        run.app.view.error = None;
        run.app.close(false); // the registered empty failed-open tab also retires
        run.settle();
        run.advance();
        assert!(run.app.routing.owner().slot() != failed_slot);
        while !run.app.tabs.parked.is_empty() {
            run.app.close(false);
            run.settle();
            run.advance();
        }
        run.app.dx = "old draft".into();
        let old_owner = run.app.routing.owner();
        run.app.close(false);
        run.settle();
        run.advance();
        assert!(run.app.view.info.is_none());
        assert_eq!(run.app.dx, "0");
        assert_eq!(run.app.routing.owner().slot(), old_owner.slot());
        assert!(run.app.routing.idle());
        run.app.dx = "empty placeholder draft".into();
        let memory_id = egui::Id::new("empty-placeholder-cache");
        run.ctx.data_mut(|data| data.insert_temp(memory_id, 42_u64));
        run.app.close(false);
        run.settle();
        run.advance();
        assert_eq!(run.app.dx, "0");
        assert!(
            run.ctx
                .data(|data| data.get_temp::<u64>(memory_id))
                .is_none()
        );
        assert_eq!(run.app.routing.owner().slot(), old_owner.slot());
        assert!(run.app.routing.idle());
        run.create();
        run.activate(old_owner.slot());
        assert!(run.app.view.info.is_none());
        assert!(first > 0);
    }
    #[test]
    fn inactive_rejection_does_not_cancel_active_quit_and_failed_close_stops_exit() {
        let mut run = Run::new();
        run.import(10);
        let now = Instant::now();
        run.create();
        run.app.tabs.parked[0].last_dirty_identity.clear();
        run.app.tick_parked_recovery(now, &run.ctx);
        run.app.tabs.parked[0].dirty_since = now - std::time::Duration::from_secs(40);
        run.app.tabs.parked[0].last_recovery_at = now - std::time::Duration::from_secs(80);
        run.app.tick_parked_recovery(now, &run.ctx);
        let (id, _, action, task, route) = run.requests.try_recv().unwrap();
        run.host.route_action(&route, &action).unwrap();
        // Authentic owner, but a malformed receipt must fail closed only that document.
        run.host
            .model
            .run_task(task, Action::RecoveryWrite(run.dir.join("recovery")));
        let reply_route = run.host.finish(&route).unwrap();
        let mut view = run.host.model.view.clone();
        view.task_receipt.as_mut().unwrap().task_id += 1000;
        run.app.close(true);
        let active = run.app.routing.owner();
        run.app.receive_session_reply((id, view, reply_route), now);
        assert!(run.app.tabs.quitting);
        assert_eq!(run.app.tabs.quit_owner, Some(active));
        run.settle();
        run.advance();
        assert!(run.app.view.blocked.is_some());
        assert!(!run.app.allow_quit);
        run.app.cancel_transition();
        assert!(!run.app.tabs.quitting);

        let mut failed = Run::new();
        failed.import(10);
        let owner = failed.app.routing.owner();
        failed.app.tabs.begin_quit(owner.clone());
        failed.app.routing.retire_on_close(true);
        failed.app.quit_after_close = true;
        failed.app.send(Action::Close(false)); // real dirty service refuses without discard
        let task = failed.app.pending_task.as_ref().unwrap().task_id;
        assert!(failed.app.tabs.quit_matches(&owner, task));
        failed.settle();
        assert!(failed.app.view.error.is_some());
        assert!(failed.app.view.info.is_some());
        assert!(!failed.app.tabs.quitting);
        assert!(!failed.app.allow_quit);
    }
    #[test]
    fn restore_admission_queue_failure_worker_failure_and_next_candidate_are_retryable() {
        let mut run = Run::new();
        run.import(10);
        run.action(Action::RecoveryWrite(run.dir.join("recovery")));
        let dir = run.dir.join("recovery");
        let candidate = recovery::discover(&dir).pop().unwrap();
        let before = run.app.view.info.clone();
        run.app.recovery_candidate = Some(candidate.clone());
        let (tx, _full) = mpsc::sync_channel(0);
        let old_tx = std::mem::replace(&mut run.app.tx, tx);
        run.app.request_restore(dir.clone(), candidate.clone());
        run.advance();
        assert!(!run.app.restore_pending());
        assert_eq!(
            run.app.recovery_candidate.as_ref().unwrap().snapshot_hash,
            candidate.snapshot_hash
        );
        assert_eq!(run.app.view.info, before);
        run.app.tx = old_tx;
        let mut invalid = candidate.clone();
        invalid.snapshot_hash = editor_core::hash::sha256_hex(b"invalid");
        run.app.request_restore(dir.clone(), invalid);
        run.advance();
        run.settle();
        assert!(run.app.view.error.is_some());
        assert!(run.app.recovery_candidate.is_some());
        assert_eq!(recovery::discover(&dir).len(), 1);
        run.app.project_error = None;
        run.app.view.error = None;
        run.app.request_restore(dir.clone(), candidate.clone());
        run.advance();
        run.settle();
        assert!(run.app.view.error.is_none());
        assert!(run.app.view.info.as_ref().unwrap().project_dirty);
        assert!(run.app.view.info.as_ref().unwrap().project_path.is_none());
        assert!(run.app.recovery_source.is_some());
        // Add a second valid independent persisted record, then skip the already
        // admitted source and show the next candidate without restarting.
        let info = run.app.view.info.clone().unwrap();
        let bytes = run
            .host
            .model
            .service
            .project_recovery_bytes(&info.document_id)
            .unwrap();
        let other_key = editor_core::hash::sha256_hex(b"separate persisted session");
        recovery::write_scoped(&dir, &info, &bytes, Some(&other_key)).unwrap();
        run.app.refresh_recovery_candidate_in(&dir);
        assert_eq!(
            run.app
                .recovery_candidate
                .as_ref()
                .unwrap()
                .session_key
                .as_deref(),
            Some(other_key.as_str())
        );
        assert_eq!(recovery::discover(&dir).len(), 2);
    }
    #[test]
    fn real_production_worker_thread_routes_save_reopen_alias_and_undo() {
        let mut app = modal::tests::app();
        let (tx, request) = mpsc::sync_channel(2);
        let (reply, rx) = mpsc::sync_channel(1);
        app.tx = tx;
        app.rx = rx;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |_| {});
        let worker_ctx = ctx.clone();
        let host = session::WorkerHost::new(app.routing.owner(), Model::default());
        let worker =
            std::thread::spawn(move || session::run_worker(host, request, reply, worker_ctx));
        let settle = |app: &mut EditorApp| {
            let envelope = app
                .rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("bounded production worker terminal");
            app.receive_session_reply(envelope, Instant::now());
        };
        let action = |app: &mut EditorApp, action| {
            app.send(action);
            settle(app);
        };
        let dir =
            std::env::temp_dir().join(format!("rcam-real-tabs-thread-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gbr = dir.join("synthetic.gbr");
        std::fs::write(
            &gbr,
            b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.4*%\nD10*\nX10000000Y0D03*\nM02*\n",
        )
        .unwrap();
        action(&mut app, Action::NewWorkspace);
        action(&mut app, Action::ImportGerbers(vec![gbr]));
        action(&mut app, Action::SelectAll);
        let geometry = app.view.selected.ordered[0].object.geometry.clone();
        let a = app.routing.owner().slot();
        let path_a = dir.join("A.rcam");
        action(
            &mut app,
            Action::SaveProject(Some(path_a.clone()), false, None),
        );
        assert!(app.view.error.is_none());
        let original = std::fs::read(&path_a).unwrap();
        app.request_new_session(Action::OpenProject(path_a.clone(), false));
        let _ = ctx.run(Default::default(), |ctx| {
            app.advance_tab_change(ctx);
        });
        settle(&mut app);
        assert_eq!(app.view.error.as_ref().unwrap().code, "CONFLICT");
        app.project_error = None;
        app.view.error = None;
        app.request_new_session(Action::NewWorkspace);
        let _ = ctx.run(Default::default(), |ctx| {
            app.advance_tab_change(ctx);
        });
        settle(&mut app);
        action(&mut app, Action::CreateEmptyLayer(Some("B".into())));
        action(
            &mut app,
            Action::SaveProject(Some(path_a.clone()), true, None),
        );
        assert_eq!(app.view.error.as_ref().unwrap().code, "CONFLICT");
        assert_eq!(std::fs::read(&path_a).unwrap(), original);
        app.project_error = None;
        app.view.error = None;
        let path_b = dir.join("B.rcam");
        action(
            &mut app,
            Action::SaveProject(Some(path_b.clone()), false, None),
        );
        assert!(app.view.error.is_none());
        app.request_tab(a, false);
        let _ = ctx.run(Default::default(), |ctx| {
            app.advance_tab_change(ctx);
        });
        action(&mut app, Action::Move("1.25".into(), "0".into()));
        assert_ne!(app.view.selected.ordered[0].object.geometry, geometry);
        action(&mut app, Action::History(false));
        assert_eq!(app.view.selected.ordered[0].object.geometry, geometry);
        action(&mut app, Action::SaveProject(None, false, None));
        assert!(app.view.error.is_none());
        assert_eq!(std::fs::read(&path_a).unwrap(), original);
        app.close(false);
        settle(&mut app);
        let _ = ctx.run(Default::default(), |ctx| {
            app.advance_tab_change(ctx);
        });
        app.request_new_session(Action::OpenProject(path_a, false));
        let _ = ctx.run(Default::default(), |ctx| {
            app.advance_tab_change(ctx);
        });
        settle(&mut app);
        assert!(app.view.error.is_none());
        action(&mut app, Action::SelectAll);
        assert_eq!(app.view.selected.ordered[0].object.geometry, geometry);
        drop(app);
        worker.join().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn real_owned_records_keep_selection_geometry_undo_dirty_camera_drafts_and_memory() {
        let mut run = Run::new();
        run.import(10);
        let a_slot = run.app.routing.owner().slot();
        let a_document = run.app.view.info.as_ref().unwrap().document_id.clone();
        let a_ids = run
            .app
            .view
            .selected
            .ids()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let a_scene = run.app.view.scene.clone().unwrap();
        let a_flags = run.app.selected_flags.clone();
        let mut fonts = run.ctx.fonts(|fonts| fonts.definitions().clone());
        let font = fonts.font_data.values().next().unwrap().clone();
        fonts
            .font_data
            .insert("synthetic-global-font-alias".into(), font);
        run.ctx.set_fonts(fonts);
        run.ctx
            .options_mut(|options| options.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let _ = run.ctx.run(Default::default(), |_| {});
        run.app.dx = "123.456".into();
        run.app.text.text = "工程甲草稿".into();
        run.app.array.pitch_x = "8.125".into();
        run.app.camera.center = editor_core::MmPoint::new(7., 9.);
        run.app.camera.scale = 42.;
        let widget = egui::Id::new("shared-widget");
        run.ctx
            .data_mut(|data| data.insert_temp(widget, "A".to_owned()));
        run.create();
        let b_slot = run.app.routing.owner().slot();
        assert_eq!(run.ctx.options(|options| options.max_passes.get()), 1);
        assert!(run.ctx.fonts(|fonts| {
            fonts
                .definitions()
                .font_data
                .contains_key("synthetic-global-font-alias")
        }));
        run.import(20);
        assert_eq!(a_ids, run.app.view.selected.ids());
        assert_ne!(a_scene.serial, run.app.view.scene.as_ref().unwrap().serial);
        assert!(run.host.model.service.document_get(&a_document).is_ok());
        run.app.dx = "-2".into();
        run.app.text.text = "工程乙草稿".into();
        run.ctx
            .data_mut(|data| data.insert_temp(widget, "B".to_owned()));
        let b_revision = run.app.view.info.as_ref().unwrap().revision.clone();
        run.activate(a_slot);
        assert_eq!(run.app.dx, "123.456");
        assert_eq!(run.app.text.text, "工程甲草稿");
        assert_eq!(run.app.array.pitch_x, "8.125");
        assert_eq!(run.app.camera.center, editor_core::MmPoint::new(7., 9.));
        assert_eq!(run.app.camera.scale, 42.);
        assert!(std::sync::Arc::ptr_eq(
            run.app.view.scene.as_ref().unwrap(),
            &a_scene
        ));
        assert!(std::sync::Arc::ptr_eq(&run.app.selected_flags, &a_flags));
        assert_eq!(
            run.ctx.data(|data| data.get_temp::<String>(widget)),
            Some("A".into())
        );
        let before = run.app.view.selected.ordered[0].object.geometry.clone();
        run.action(Action::Move("1.25".into(), "0".into()));
        assert_ne!(run.app.view.selected.ordered[0].object.geometry, before);
        run.action(Action::History(false));
        assert_eq!(run.app.view.selected.ordered[0].object.geometry, before);
        run.activate(b_slot);
        assert_eq!(run.app.dx, "-2");
        assert_eq!(run.app.text.text, "工程乙草稿");
        assert_eq!(run.app.view.info.as_ref().unwrap().revision, b_revision);
        assert_eq!(
            run.ctx.data(|data| data.get_temp::<String>(widget)),
            Some("B".into())
        );
        assert!(run.app.view.info.as_ref().unwrap().project_dirty);
    }
    #[test]
    fn late_inactive_recovery_terminal_updates_only_its_owner_and_duplicate_is_inert() {
        let mut run = Run::new();
        run.import(10);
        let a_slot = run.app.routing.owner().slot();
        run.create();
        let b_slot = run.app.routing.owner().slot();
        let b_info = run.app.view.info.clone();
        let now = Instant::now();
        run.app.tabs.parked[0].last_dirty_identity = String::new();
        run.app.tick_parked_recovery(now, &run.ctx);
        run.app.tabs.parked[0].dirty_since = now - std::time::Duration::from_secs(40);
        run.app.tabs.parked[0].last_recovery_at = now - std::time::Duration::from_secs(80);
        run.app.tick_parked_recovery(now, &run.ctx);
        assert!(run.app.tabs.parked[0].pending_recovery_task.is_some());
        assert_eq!(run.app.routing.owner().slot(), b_slot);
        assert!(!run.app.busy);
        let reply = run.settle();
        assert!(run.app.tabs.parked[0].pending_recovery_task.is_none());
        assert!(!run.app.tabs.parked[0].last_recovered_identity.is_empty());
        assert_eq!(run.app.routing.owner().slot(), b_slot);
        assert_eq!(run.app.view.info, b_info);
        let recovered = run.app.tabs.parked[0].last_recovered_identity.clone();
        run.app.receive_session_reply(reply, now);
        assert_eq!(run.app.tabs.parked[0].last_recovered_identity, recovered);
        assert_eq!(run.app.view.info, b_info);
        run.activate(a_slot);
        assert!(run.app.view.info.as_ref().unwrap().project_dirty);
    }
    #[test]
    fn queued_switch_waits_for_read_terminal_and_write_ime_modal_block() {
        let mut run = Run::new();
        let a_slot = run.app.routing.owner().slot();
        run.create();
        let b_slot = run.app.routing.owner().slot();
        run.app.send(Action::Viewport(
            editor_core::MmPoint::new(0., 0.),
            editor_core::BoundsMm {
                min_x_mm: -10.,
                min_y_mm: -10.,
                max_x_mm: 10.,
                max_y_mm: 10.,
            },
            20.,
        ));
        assert!(run.app.viewport_task.is_some());
        run.app.request_tab(a_slot, false);
        run.advance();
        assert_eq!(run.app.routing.owner().slot(), b_slot);
        let late = run.settle();
        run.advance();
        assert_eq!(run.app.routing.owner().slot(), a_slot);
        let info = run.app.view.info.clone();
        run.app.receive_session_reply(late, Instant::now());
        assert_eq!(run.app.view.info, info);
        run.app.ime_active = true;
        run.app.request_tab(b_slot, false);
        assert!(run.app.tabs.pending.is_none());
        run.app.ime_active = false;
        run.app.modal = Some(ActiveModal::Grid);
        run.app.request_tab(b_slot, false);
        assert!(run.app.tabs.pending.is_none());
        run.app.modal = None;
        run.app.send(Action::CreateEmptyLayer(Some("A".into())));
        run.app.request_tab(b_slot, false);
        assert!(run.app.tabs.pending.is_none());
        run.settle();
        run.activate(b_slot);
        let before = run.app.view.info.clone();
        run.app.request_tab(a_slot, false);
        egui::Popup::open_id(&run.ctx, egui::Id::new("synthetic-tab-popup"));
        run.advance();
        assert!(!run.app.tabs.change_pending());
        assert_eq!(run.app.view.info, before);
        egui::Popup::close_all(&run.ctx);
        run.activate(a_slot);
    }
    #[test]
    fn new_queue_failure_restores_original_state_and_never_reuses_slot() {
        let mut run = Run::new();
        let info = run.app.view.info.clone();
        let owner = run.app.routing.owner();
        let (tx, _full) = mpsc::sync_channel(0);
        run.app.tx = tx;
        run.app.request_new_session(Action::NewWorkspace);
        run.advance();
        assert_eq!(run.app.view.info, info);
        assert_eq!(run.app.routing.owner(), owner);
        assert!(run.app.tabs.parked.is_empty());
        assert_eq!(run.app.tabs.last_slot, 2);
        let (tx, requests) = mpsc::sync_channel(1);
        run.app.tx = tx;
        run.requests = requests;
        run.create();
        assert_eq!(run.app.routing.owner().slot(), 3);
    }
    #[test]
    fn close_dirty_tab_and_quit_all_cancel_preserve_other_documents() {
        let mut run = Run::new();
        run.import(10);
        let a_document = run.app.view.info.as_ref().unwrap().document_id.clone();
        let a_slot = run.app.routing.owner().slot();
        run.create();
        let b_document = run.app.view.info.as_ref().unwrap().document_id.clone();
        run.app.close(true);
        run.settle();
        run.advance();
        assert_eq!(run.app.routing.owner().slot(), a_slot);
        assert!(run.app.close_prompt);
        assert!(run.host.model.service.document_get(&b_document).is_err());
        assert!(run.host.model.service.document_get(&a_document).is_ok());
        run.app.cancel_transition();
        assert!(!run.app.tabs.quitting);
        assert!(!run.app.allow_quit);
        assert!(run.app.view.info.as_ref().unwrap().project_dirty);
    }
}
