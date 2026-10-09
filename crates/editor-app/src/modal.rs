//! One parameter dialog owns focus and draft state at a time.
use crate::{EditorApp, state::Action, tools};
use eframe::egui;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveModal {
    PointInput,
    Pnp,
    Array,
    Text,
    Move,
    Rotate,
    Mirror,
    Flash,
    Grid,
    ObjectSnap,
    Units,
    BlockCreate,
    BlockRename,
    BlockDelete,
    BlockExplode,
    BlockTransform,
}
impl ActiveModal {
    fn title(self) -> &'static str {
        match self {
            Self::PointInput => "共同点输入",
            Self::BlockCreate => "创建 Block",
            Self::BlockRename => "重命名 Block",
            Self::BlockDelete => "删除 Block 定义",
            Self::BlockExplode => "拆解 Block",
            Self::BlockTransform => "Block 实例属性",
            Self::Pnp => "导入 PnP — 明确映射与预览",
            Self::Array => "矩形阵列 / Rectangular Array",
            Self::Text => "插入文本",
            Self::Move => "数值移动",
            Self::Rotate => "旋转",
            Self::Mirror => "镜像",
            Self::Flash => "Flash 尺寸属性",
            Self::Grid => "网格 / 吸附设置",
            Self::ObjectSnap => "Object Snap",
            Self::Units => "单位 / 制造精度",
        }
    }
}
impl EditorApp {
    /// Decide cancellation before any widget can enqueue a manufacturing edit.
    /// The latch survives child Back and all later handlers in this egui frame.
    pub(crate) fn arbitrate_point_input_frame(&mut self, ctx: &egui::Context) -> bool {
        let frame = ctx.cumulative_frame_nr();
        if self.point_input_frame != Some(frame) {
            self.point_input_frame = Some(frame);
            self.point_input_cancelled = false;
            self.point_commit_blocked = false;
        }
        if self.point_input_cancelled {
            return false;
        }
        let (lost, escape, ime) = ctx.input(|i| {
            (
                !i.focused
                    || i.events.iter().any(|e| {
                        matches!(
                            e,
                            egui::Event::PointerGone | egui::Event::WindowFocused(false)
                        )
                    }),
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        }
                    )
                }),
                i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))),
            )
        });
        self.point_commit_blocked |= self.ime_active || self.ime_event || ime;
        let active = self.point_pick.is_some()
            || self.point_transform.is_some()
            || self.point_adapter.is_some();
        if active && lost && self.pause_move_place_on_pointer_gone(ctx) {
            return false;
        }
        if active && (lost || (escape && !self.point_commit_blocked)) {
            if self.frame_trace.is_some() {
                let reason = if lost {
                    ctx.input(|i| {
                        if !i.focused {
                            crate::frame_trace::MoveExitReason::FocusLost
                        } else if i
                            .events
                            .iter()
                            .any(|e| matches!(e, egui::Event::WindowFocused(false)))
                        {
                            crate::frame_trace::MoveExitReason::WindowFocusLost
                        } else {
                            crate::frame_trace::MoveExitReason::PointerGone
                        }
                    })
                } else {
                    crate::frame_trace::MoveExitReason::Escape
                };
                self.trace_move_exit(reason);
            }
            self.point_input_cancelled = true;
            self.point_commit_blocked = true;
            if let Some(task) = &self.pending_task {
                task.cancel_token.cancel();
            }
            if lost {
                self.point_pick = None;
                self.point_transform = None;
                self.point_adapter = None;
                self.view.point_preview = None;
                self.modal = None;
                self.object_snap_runtime.reset();
            } else if self.point_pick.is_some() {
                self.finish_point_pick(None);
            } else {
                self.cancel_modal();
            }
            // No Enter or pointer release may be interpreted by another handler.
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key { .. }
                            | egui::Event::PointerButton { .. }
                            | egui::Event::Text(_)
                    )
                });
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
            return false;
        }
        true
    }
    pub(crate) fn open_modal(&mut self, modal: ActiveModal) {
        if self.busy || self.selection_read_pending() || self.close_prompt || self.modal.is_some() {
            return;
        }
        self.trace_move_exit(crate::frame_trace::MoveExitReason::ModalReplaced);
        self.cancel_block();
        self.text.cancel();
        self.drag = None;
        if modal != ActiveModal::Units {
            self.measure.clear();
        }
        self.tool = if modal == ActiveModal::Units && self.tool == tools::ActiveTool::Measure {
            tools::ActiveTool::Measure
        } else if modal == ActiveModal::Text {
            tools::ActiveTool::Text
        } else {
            tools::ActiveTool::Select
        };
        self.ui_error = None;
        self.view.error = None;
        self.point_pick = None;
        self.point_adapter = None;
        self.point_transform = match modal {
            ActiveModal::Move => Some(crate::point_transform::Mode::Move),
            ActiveModal::Rotate => Some(crate::point_transform::Mode::Rotate),
            ActiveModal::Mirror => Some(crate::point_transform::Mode::HorizontalMirror),
            _ => None,
        }
        .map(|mode| crate::point_transform::Session::new(&self.view, mode, self.display_unit));
        self.view.point_preview = None;
        self.modal = Some(modal);
        self.modal_pending = None;
        self.spacing = if modal == ActiveModal::Units {
            (self.precision().resolution_mm * 1000.).to_string()
        } else {
            self.display_unit.input(self.grid.spacing_mm)
        };
        if modal == ActiveModal::Text {
            self.text.changed();
            if self.text.font.is_none()
                && let Some(path) = self.text.font_path.clone()
            {
                self.text.queue_font(path);
            }
        }
        self.draft_snap = self.grid.snap_enabled;
        self.draft_object_snap = self.object_snap.clone();
        self.dx = "0".into();
        self.dy = "0".into();
        self.size_aperture_id = None;
        self.sync_size_fields();
    }
    pub(crate) fn cancel_modal(&mut self) {
        if (self.point_transform.is_some() || self.point_adapter.is_some())
            && self.modal_pending.is_none()
            && let Some(task) = &self.pending_task
        {
            task.cancel_token.cancel();
        }
        if self.modal == Some(ActiveModal::PointInput)
            && let Some(session) = self.point_adapter.take()
        {
            self.modal = session.resume;
            self.point_pick = None;
            self.ui_error = None;
            return;
        }
        let keep_measure =
            self.modal == Some(ActiveModal::Units) && self.tool == tools::ActiveTool::Measure;
        self.trace_move_exit(crate::frame_trace::MoveExitReason::ModalCancelled);
        self.view.pnp_preview = None;
        self.array.requested = None;
        self.array.confirmed = None;
        self.view.array_preview = None;
        self.point_pick = None;
        self.point_transform = None;
        self.point_adapter = None;
        self.view.point_preview = None;
        self.modal = None;
        self.modal_pending = None;
        self.text.cancel();
        if !keep_measure {
            self.tool = tools::ActiveTool::Select;
        }
        self.ui_error = None;
        self.view.error = None;
    }
    pub(crate) fn dialog_enter(&self, ui: &egui::Ui) -> bool {
        !self.busy
            && !self.point_commit_blocked
            && !self.ime_active
            && !self.ime_event
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
    }
    pub(crate) fn parameter_modal(&mut self, ctx: &egui::Context) {
        if !self.arbitrate_point_input_frame(ctx) {
            return;
        }
        let Some(modal) = self.modal else {
            return;
        };
        let preferred = egui::vec2(
            if modal == ActiveModal::Pnp {
                760.
            } else {
                440.
            },
            match modal {
                ActiveModal::Array => 660.,
                ActiveModal::Grid => 260.,
                ActiveModal::BlockRename => 240.,
                ActiveModal::BlockDelete | ActiveModal::BlockExplode => 300.,
                ActiveModal::BlockCreate => 360.,
                ActiveModal::Move | ActiveModal::Text | ActiveModal::Pnp => 660.,
                _ => 520.,
            },
        );
        let response = crate::ui::modal_widgets::fixed_modal(
            ctx,
            egui::Id::new("manufacturing-parameters"),
            preferred,
            |ui| {
                ui.heading(modal.title());
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 160.).max(100.))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(
                            !self.busy
                                || (modal == ActiveModal::Text
                                    && self.modal_pending.is_none()
                                    && self.text.pending_apply.is_none()),
                            |ui| match modal {
                                ActiveModal::PointInput => self.point_adapter_modal(ui),
                                ActiveModal::BlockCreate
                                | ActiveModal::BlockRename
                                | ActiveModal::BlockDelete
                                | ActiveModal::BlockExplode
                                | ActiveModal::BlockTransform => self.block_modal(ui, modal),
                                ActiveModal::Pnp => self.pnp_modal(ui),
                                ActiveModal::Array => {
                                    self.array_modal(ui);
                                }
                                ActiveModal::Units => self.units_modal(ui),
                                ActiveModal::Text => self.text_controls(ui),
                                ActiveModal::Rotate | ActiveModal::Mirror => {
                                    self.transform_controls(ui)
                                }
                                ActiveModal::Flash => self.flash_size_controls(ui),
                                ActiveModal::Move => {
                                    self.unified_transform_controls(ui);
                                    ui.separator();
                                    ui.label("或使用现有数值位移");
                                    ui.label(format!("ΔX {}", self.display_unit.suffix()));
                                    ui.text_edit_singleline(&mut self.dx);
                                    ui.label(format!("ΔY {}", self.display_unit.suffix()));
                                    ui.text_edit_singleline(&mut self.dy);
                                    if ui.button("应用位移").clicked() || self.dialog_enter(ui)
                                    {
                                        self.send(Action::Move(self.dx.clone(), self.dy.clone()));
                                    }
                                }
                                ActiveModal::Grid => {
                                    ui.label(format!("网格步长 {}", self.display_unit.suffix()));
                                    ui.text_edit_singleline(&mut self.spacing);
                                    ui.checkbox(&mut self.draft_snap, "Grid Snap");
                                    ui.small("对象优先于网格；Alt 临时关闭吸附");
                                    if ui.button("应用设置").clicked() || self.dialog_enter(ui)
                                    {
                                        match self.display_unit.parse_length(&self.spacing) {
                                            Ok(v)
                                                if editor_core::grid::snap_scalar(0., v, 0.)
                                                    .is_ok() =>
                                            {
                                                self.grid.spacing_mm = v;
                                                self.grid.snap_enabled = self.draft_snap;
                                                self.persist_project_view();
                                                self.modal = None;
                                            }
                                            _ => {
                                                self.ui_error =
                                                    Some("网格步长必须是有限正数".into())
                                            }
                                        }
                                    }
                                }
                                ActiveModal::ObjectSnap => self.object_snap_controls(ui),
                            },
                        );
                        let errors = self
                            .ui_error
                            .iter()
                            .cloned()
                            .chain(
                                self.view
                                    .error
                                    .iter()
                                    .map(|e| format!("{}: {}", e.code, e.message)),
                            )
                            .collect::<Vec<_>>()
                            .join("\n");
                        crate::ui::modal_widgets::status_slot(ui, &errors, 56., true);
                    });
                crate::ui::modal_widgets::status_slot(
                    ui,
                    if self.busy { "正在处理…" } else { "" },
                    20.,
                    false,
                );
                if crate::ui::buttons::secondary_enabled(ui, "取消", self.modal_pending.is_none())
                    .clicked()
                {
                    self.cancel_modal();
                }
            },
        );
        if self.modal_pending.is_none()
            && !self.ime_active
            && !self.ime_event
            && response.should_close()
        {
            self.cancel_modal();
        }
    }

    fn object_snap_controls(&mut self, ui: &mut egui::Ui) {
        use editor_core::snap::SnapKind;
        ui.checkbox(&mut self.draft_object_snap.enabled, "启用 Object Snap");
        ui.separator();
        for (kind, label) in [
            (SnapKind::Endpoint, "Endpoint / 端点"),
            (SnapKind::Vertex, "Vertex / 顶点"),
            (SnapKind::Midpoint, "Midpoint / 中点"),
            (SnapKind::Center, "Center / 圆心"),
            (SnapKind::Quadrant, "Quadrant / 象限点"),
            (SnapKind::Intersection, "Intersection / 交点"),
            (SnapKind::Nearest, "Nearest / 最近点"),
        ] {
            let mut enabled = self.draft_object_snap.enabled_kinds.contains(&kind);
            if ui.checkbox(&mut enabled, label).changed() {
                self.draft_object_snap.set_kind(kind, enabled);
            }
        }
        ui.add(
            egui::Slider::new(&mut self.draft_object_snap.radius_px, 4.0..=20.0)
                .integer()
                .text("Snap Radius (physical px)"),
        );
        ui.checkbox(
            &mut self.draft_object_snap.manufacturing_boundary,
            "Manufacturing Boundary",
        );
        ui.checkbox(
            &mut self.draft_object_snap.original_path,
            "Original Path（高级）",
        );
        ui.small("对象捕捉优先于 Grid；按住 Alt 临时关闭全部捕捉。Nearest 默认关闭。");
        ui.horizontal(|ui| {
            if crate::ui::buttons::secondary(ui, "恢复默认").clicked() {
                self.draft_object_snap = Default::default();
            }
            if crate::ui::buttons::primary(ui, "应用", true).clicked() || self.dialog_enter(ui) {
                if self.draft_object_snap.enabled_kinds.is_empty()
                    || !(4.0..=20.0).contains(&self.draft_object_snap.radius_px)
                    || (!self.draft_object_snap.manufacturing_boundary
                        && !self.draft_object_snap.original_path)
                {
                    self.ui_error = Some(
                        "至少启用一种捕捉类型和一种几何来源；半径范围为 4–20 physical px".into(),
                    );
                } else {
                    self.object_snap = self.draft_object_snap.clone();
                    self.object_snap_runtime.reset();
                    self.persist_project_view();
                    self.modal = None;
                }
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn app() -> EditorApp {
        let (tx, _requests) = std::sync::mpsc::sync_channel(1);
        let (reply, rx) = std::sync::mpsc::sync_channel(1);
        EditorApp {
            routing: Default::default(),
            pending_task: None,
            canvas_read: None,
            canvas_selection_unconfirmed: false,
            gerber_import: None,
            viewport_task: None,
            geometry_task: None,
            geometry_context: None,
            block: Default::default(),
            diagnostic_export: None,
            operation_source: rcam_diagnostics::Source::System,
            tx,
            rx,
            // An idle synthetic worker stays connected until the app drops.
            // Disconnect tests explicitly replace rx with a closed channel.
            _fixture_reply: Some(reply),
            view: crate::state::View::default(),
            busy: false,
            viewport_sequence: None,
            sequence: 0,
            task_serial: 0,
            request_failure_serial: 0,
            camera: crate::camera::Camera::default(),
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
            components: crate::components_ui::UiState::default(),
            array: crate::array_ui::Draft::default(),
            point_adapter: None,
            array_point_base: None,
            block_point_reference: editor_core::MmPoint::new(0., 0.),
            point_transform: None,
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
            recent_colors: Vec::new(),
            prefs: crate::preferences::AppPreferences::default(),
            shortcuts: crate::shortcut_settings::Settings::load(
                None,
                editor_core::command::Platform::current(),
            ),
            recovery_candidate: None,
            recovery_prompt_reported: None,
            recovery_attempted_identity: None,
            recovery_ignore_confirm: false,
            last_dirty_identity: String::new(),
            dirty_since: std::time::Instant::now(),
            last_recovery_at: std::time::Instant::now(),
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
            allow_quit: false,
            format: egui_wgpu::wgpu::TextureFormat::Bgra8Unorm,
            adapter: String::new(),
            ui_error: None,
            last_title: String::new(),
            canvas_rect: egui::Rect::NOTHING,
            display_error: None,
            display_pending: false,
            drag: None,
            grip: None,
            bench: None,
            #[cfg(feature = "internal-evidence")]
            s5m1: None,
            #[cfg(feature = "internal-evidence")]
            a2: None,
            #[cfg(feature = "internal-evidence")]
            batch_drag: None,
            #[cfg(feature = "internal-evidence")]
            i1: None,
            #[cfg(feature = "internal-evidence")]
            pmix: None,
            probe: None,
            row_probes: Default::default(),
            layer_panel_rect: eframe::egui::Rect::NOTHING,
            selected_flags: Default::default(),
            uniform_validation: Default::default(),
            selection_presentation: Default::default(),
            prepare_work: Default::default(),
            timing: std::env::var_os("RCAM_RENDER_TIMING").is_some(),
            frame_trace: None,
            last_frame: std::time::Instant::now(),
            text_input_at_event: false,
            ime_active: false,
            reported_ppp: 0.,
        }
    }
    fn frame(app: &mut EditorApp, ctx: &egui::Context, key: Option<egui::Key>) {
        let mut raw = egui::RawInput::default();
        if let Some(key) = key {
            raw.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let _ = ctx.run(raw, |ctx| app.parameter_modal(ctx));
    }
    #[test]
    fn multiline_enter_does_not_apply_and_preview_busy_keeps_text_editable() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.open_modal(ActiveModal::Text);
        app.text.text = "abc".into();
        frame(&mut app, &ctx, None);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("manufacturing-text")));
        frame(&mut app, &ctx, Some(egui::Key::End));
        app.busy = true; // read-only preview in flight must not swallow input
        frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert!(app.text.text.contains('\n'));
        assert_eq!(app.modal, Some(ActiveModal::Text));
        assert!(app.text.pending_apply.is_none());
    }
    #[test]
    fn ime_commit_frame_does_not_submit_preedit_draft() {
        let mut app = app();
        app.tool = tools::ActiveTool::Text;
        app.text.catalog_requested = true;
        app.text.text = "zhong".into();
        app.text.changed();
        let now = std::time::Instant::now() + std::time::Duration::from_secs(1);
        app.ime_event = true;
        app.ime_active = false;
        app.tick_text(&egui::Context::default(), now);
        assert!(app.text.submitted.is_none());
        app.ime_event = false;
        app.tick_text(&egui::Context::default(), now);
        assert!(app.text.submitted.is_some());
    }
    #[test]
    fn context_change_cancels_floating_placement() {
        let mut app = app();
        app.tool = tools::ActiveTool::Text;
        app.text.floating = Some(editor_core::MmPoint { x_mm: 1., y_mm: 2. });
        app.text.context = Some(("old".into(), "0".into(), "0".into(), "layer".into()));
        app.tick_text(&egui::Context::default(), std::time::Instant::now());
        assert!(app.tool == tools::ActiveTool::Select);
        assert!(app.text.floating.is_none());
        assert!(app.text.preview.is_none());
    }
    #[test]
    fn exclusive_draft_cancel_invalid_enter_and_ime() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.open_modal(ActiveModal::Grid);
        app.open_modal(ActiveModal::Text);
        assert_eq!(app.modal, Some(ActiveModal::Grid));
        app.spacing = "0.25".into();
        frame(&mut app, &ctx, None);
        assert_eq!(app.grid.spacing_mm, 0.1);
        app.ime_active = true;
        frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert_eq!(app.grid.spacing_mm, 0.1);
        app.ime_active = false;
        app.ime_event = true;
        frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert_eq!(app.grid.spacing_mm, 0.1);
        app.ime_event = false;
        frame(&mut app, &ctx, Some(egui::Key::Escape));
        assert_eq!(app.modal, None);
        assert_eq!(app.grid.spacing_mm, 0.1);
        app.open_modal(ActiveModal::Grid);
        app.spacing = "NaN".into();
        frame(&mut app, &ctx, None);
        frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert_eq!(app.modal, Some(ActiveModal::Grid));
        assert!(app.ui_error.is_some());
        app.spacing = "0.25".into();
        frame(&mut app, &ctx, None);
        frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert_eq!(app.modal, None);
        assert_eq!(app.grid.spacing_mm, 0.25);
    }
    #[test]
    fn retained_text_and_four_units_preserve_exact_geometry_parameters() {
        let mut app = app();
        app.open_modal(ActiveModal::Text);
        app.text.text = "ABC\n123".into();
        app.text.height = "3.1234567".into();
        app.text.rotation = "37".into();
        app.text.placement = crate::text_tool::Placement::Absolute;
        app.text.x = "12.3456789".into();
        let original = app.text.params("layer").unwrap();
        app.cancel_modal();
        for _ in 0..10 {
            for unit in editor_core::units::DisplayUnit::ALL {
                app.open_modal(ActiveModal::Units);
                app.text.change_unit(unit).unwrap();
                app.display_unit = unit;
                app.cancel_modal();
                app.open_modal(ActiveModal::Text);
                assert_eq!(
                    serde_json::to_value(app.text.params("layer").unwrap()).unwrap(),
                    serde_json::to_value(&original).unwrap()
                );
                assert!(app.text.preview.is_none() && app.text.pending_apply.is_none());
                app.open_modal(ActiveModal::Units);
                assert_eq!(app.modal, Some(ActiveModal::Text));
                app.cancel_modal();
            }
        }
        app.text.height = "unfinished".into();
        let old_unit = app.text.display_unit;
        let different = if old_unit == editor_core::units::DisplayUnit::Inch {
            editor_core::units::DisplayUnit::Mil
        } else {
            editor_core::units::DisplayUnit::Inch
        };
        assert!(app.text.change_unit(different).is_err());
        assert_eq!(app.text.height, "unfinished");
        assert_eq!(app.text.display_unit, old_unit);
    }
    #[test]
    fn unit_settings_keep_measurement_and_do_not_reinterpret_active_modal() {
        let mut app = app();
        app.tool = tools::ActiveTool::Measure;
        app.measure.click(editor_core::MmPoint::new(0., 0.));
        app.measure.click(editor_core::MmPoint::new(3., 4.));
        for unit in editor_core::units::DisplayUnit::ALL {
            app.open_modal(ActiveModal::Units);
            app.text.change_unit(unit).unwrap();
            app.display_unit = unit;
            app.cancel_modal();
            assert!(app.tool == tools::ActiveTool::Measure);
            assert_eq!(app.measure.values(), Some((3., 4., 5.)));
            assert_eq!(app.measure.completed.len(), 1);
            assert!(
                app.measure
                    .label_with_resolution(unit, 0.0001)
                    .contains(&unit.format_length(5., 0.0001))
            );
            let action = app
                .length_action(Action::Move("1inch".into(), "-100um".into()))
                .unwrap();
            assert!(matches!(action, Action::Move(x,y) if x=="25.4" && y=="-0.1"));
        }
    }

    #[test]
    fn snap_toggle_dispatcher_is_the_single_state_mutation_path() {
        use editor_core::command::{CommandDispatcher, ids};

        let mut app = app();
        assert!(!app.object_snap.enabled);
        assert!(app.dispatch(ids::SNAP_TOGGLE));
        assert!(app.object_snap.enabled);
        assert!(app.dispatch(ids::SNAP_TOGGLE));
        assert!(!app.object_snap.enabled);
    }
}
