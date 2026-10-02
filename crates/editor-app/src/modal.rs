//! One parameter dialog owns focus and draft state at a time.
use crate::{EditorApp, PivotMode, state::Action, tools};
use eframe::egui;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveModal {
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
    pub(crate) fn open_modal(&mut self, modal: ActiveModal) {
        if self.busy || self.close_prompt || self.modal.is_some() {
            return;
        }
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
        self.modal = Some(modal);
        self.modal_pending = None;
        self.mirror_direction = crate::state::MirrorDirection::Horizontal;
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
        self.angle = "90".into();
        self.pivot_mode = PivotMode::SelectionCenter;
        self.pivot_x = "0".into();
        self.pivot_y = "0".into();
        self.size_aperture_id = None;
        self.sync_size_fields();
    }
    pub(crate) fn cancel_modal(&mut self) {
        let keep_measure =
            self.modal == Some(ActiveModal::Units) && self.tool == tools::ActiveTool::Measure;
        self.view.pnp_preview = None;
        self.array.requested = None;
        self.view.array_preview = None;
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
            && !self.ime_active
            && !self.ime_event
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
    }
    pub(crate) fn parameter_modal(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal else {
            return;
        };
        let response =
            egui::Modal::new(egui::Id::new("manufacturing-parameters")).show(ctx, |ui| {
                ui.set_width(crate::ui::tokens::modal_width(
                    ctx,
                    if modal == ActiveModal::Pnp {
                        760.
                    } else {
                        440.
                    },
                    180.,
                ));
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
                                ActiveModal::BlockCreate
                                | ActiveModal::BlockRename
                                | ActiveModal::BlockDelete
                                | ActiveModal::BlockExplode
                                | ActiveModal::BlockTransform => self.block_modal(ui, modal),
                                ActiveModal::Pnp => self.pnp_modal(ui),
                                ActiveModal::Array => self.array_modal(ui),
                                ActiveModal::Units => self.units_modal(ui),
                                ActiveModal::Text => self.text_controls(ui),
                                ActiveModal::Rotate | ActiveModal::Mirror => {
                                    self.transform_controls(ui)
                                }
                                ActiveModal::Flash => self.flash_size_controls(ui),
                                ActiveModal::Move => {
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
                        if let Some(error) = &self.ui_error {
                            ui.colored_label(egui::Color32::YELLOW, error);
                        }
                        if let Some(error) = &self.view.error {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                format!("{}: {}", error.code, error.message),
                            );
                        }
                    });
                if self.busy {
                    ui.spinner();
                }
                if crate::ui::buttons::secondary_enabled(ui, "取消", self.modal_pending.is_none())
                    .clicked()
                {
                    self.cancel_modal();
                }
            });
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
        let (_reply, rx) = std::sync::mpsc::sync_channel(1);
        EditorApp {
            pending_task: None,
            viewport_task: None,
            block: Default::default(),
            diagnostic_export: None,
            operation_source: rcam_diagnostics::Source::System,
            tx,
            rx,
            view: crate::state::View::default(),
            busy: false,
            viewport_sequence: None,
            sequence: 0,
            request_failure_serial: 0,
            camera: crate::camera::Camera::default(),
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
            modal: None,
            modal_pending: None,
            draft_snap: false,
            ime_event: false,
            measure: Default::default(),
            fit: false,
            dx: "0".into(),
            dy: "0".into(),
            angle: "90".into(),
            pivot_mode: crate::PivotMode::SelectionCenter,
            mirror_direction: crate::state::MirrorDirection::Horizontal,
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
            probe: None,
            row_probes: Default::default(),
            layer_panel_rect: eframe::egui::Rect::NOTHING,
            selected_flags: Default::default(),
            timing: std::env::var_os("RCAM_RENDER_TIMING").is_some(),
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
