//! UI draft and owned terminal handling for the accumulating worker session.
use crate::{
    EditorApp, modal::ActiveModal, point_input, state::Action, unified_editor_worker as worker,
};
use editor_core::{ObjectOrigin, edit::MirrorAxis};
use editor_service::{DraftStep, SelectionEdit, SelectionGroup, UnifiedEditorHistory};
use eframe::egui;
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Move,
    Rotate,
    HorizontalMirror,
    VerticalMirror,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Base,
    Target,
}
#[derive(Clone)]
pub struct Target {
    pub text_group: Option<String>,
    pub label: String,
    pub group: SelectionGroup,
    pub enabled: bool,
}
pub struct Pending {
    pub task: u64,
    pub identity: Arc<()>,
    pub command: worker::Command,
}
pub struct Draft {
    pub identity: Arc<()>,
    pub context: point_input::Context,
    pub generation: u64,
    pub ready: bool,
    pub targets: Vec<Target>,
    pub mode: Mode,
    pub base: point_input::Draft,
    pub target: point_input::Draft,
    pub angle: String,
    pub dirty: bool,
    pub pending: Option<Pending>,
    pub reply: Option<Arc<worker::Reply>>,
    pub picking: Option<Pick>,
    pub confirm_cancel: bool,
    pub cancel_after_reply: bool,
    pub unknown: bool,
}
impl Draft {
    pub fn new(view: &crate::state::View) -> Self {
        let mut targets: Vec<Target> = Vec::new();
        for o in &view.selected.ordered {
            let text = match &o.object.origin {
                ObjectOrigin::GeneratedText { operation_id } => Some(operation_id.as_str()),
                _ => None,
            };
            let label = text.unwrap_or(&o.object.object_id).to_owned();
            if text.is_some()
                && let Some(t) = targets
                    .iter_mut()
                    .find(|t| t.group.layer_id == o.layer_id && t.text_group.as_deref() == text)
            {
                t.group.object_ids.push(o.object.object_id.clone());
            } else {
                targets.push(Target {
                    text_group: text.map(str::to_owned),
                    label,
                    group: SelectionGroup {
                        layer_id: o.layer_id.clone(),
                        object_ids: vec![o.object.object_id.clone()],
                    },
                    enabled: true,
                });
            }
        }
        Self {
            identity: Arc::new(()),
            context: point_input::Context::capture(view),
            generation: 0,
            ready: false,
            targets,
            mode: Mode::Move,
            base: Default::default(),
            target: Default::default(),
            angle: "0".into(),
            dirty: false,
            pending: None,
            reply: None,
            picking: None,
            confirm_cancel: false,
            cancel_after_reply: false,
            unknown: false,
        }
    }
    pub fn step(&self, unit: editor_core::units::DisplayUnit) -> Result<DraftStep, String> {
        let mut groups: Vec<SelectionGroup> = Vec::new();
        for t in self.targets.iter().filter(|t| t.enabled) {
            if let Some(g) = groups.iter_mut().find(|g| g.layer_id == t.group.layer_id) {
                g.object_ids.extend(t.group.object_ids.clone());
            } else {
                groups.push(t.group.clone());
            }
        }
        if groups.is_empty() {
            return Err("请选择本步作用对象".into());
        }
        let base = self.base.resolve(unit)?.world_mm;
        let operation = match self.mode {
            Mode::Move => {
                let target = self.target.resolve(unit)?.world_mm;
                SelectionEdit::Move {
                    dx_mm: target.x_mm - base.x_mm,
                    dy_mm: target.y_mm - base.y_mm,
                }
            }
            Mode::Rotate => SelectionEdit::Rotate {
                angle_deg: self
                    .angle
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|a| a.is_finite())
                    .ok_or("角度必须有限")?,
                pivot_mm: base,
            },
            Mode::HorizontalMirror => SelectionEdit::Mirror {
                axis: MirrorAxis::Horizontal {
                    coordinate_mm: base.y_mm,
                },
            },
            Mode::VerticalMirror => SelectionEdit::Mirror {
                axis: MirrorAxis::Vertical {
                    coordinate_mm: base.x_mm,
                },
            },
        };
        Ok(DraftStep { groups, operation })
    }
    fn clear_input(&mut self) {
        self.mode = Mode::Move;
        self.base = Default::default();
        self.target = Default::default();
        self.angle = "0".into();
        self.dirty = false;
        self.picking = None;
        for t in &mut self.targets {
            t.enabled = true;
        }
    }
}
fn record(response: &egui::Response, _tag: &'static str) {
    #[cfg(test)]
    geometry_probe::RECTS.with(|r| r.borrow_mut().push((_tag, response.rect)));
    #[cfg(feature = "internal-evidence")]
    crate::native_i1::widget(_tag, response);
    let _ = response;
}
fn button(ui: &mut egui::Ui, label: &'static str, width: f32) -> egui::Response {
    let r = ui.add_sized([width, 26.], egui::Button::new(label));
    record(&r, label);
    r
}
fn point_fields(
    ui: &mut egui::Ui,
    title: &str,
    d: &mut point_input::Draft,
    unit: editor_core::units::DisplayUnit,
) -> (bool, bool) {
    ui.label(title);
    let mut changed = false;
    for (name, value) in [("X", &mut d.x), ("Y", &mut d.y)] {
        ui.horizontal(|ui| {
            ui.add_sized(
                [66., 22.],
                egui::Label::new(format!("{name} {}", unit.suffix())).truncate(),
            );
            let r = ui.add_sized([144., 22.], egui::TextEdit::singleline(value));
            record(
                &r,
                if title == "目标点" {
                    if name == "X" {
                        "unified-target-X"
                    } else {
                        "unified-target-Y"
                    }
                } else if name == "X" {
                    "unified-base-X"
                } else {
                    "unified-base-Y"
                },
            );
            changed |= r.changed();
        });
    }
    (changed, button(ui, "画布拾取", 112.).clicked())
}
/// Stable content geometry; dispatch is returned so no widget mutates the worker.
pub fn controls(
    ui: &mut egui::Ui,
    d: &mut Draft,
    unit: editor_core::units::DisplayUnit,
    busy: bool,
) -> Option<worker::Command> {
    let enabled = d.ready && d.pending.is_none() && !d.unknown && !busy && !d.confirm_cancel;
    let mut command = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            for (mode, label) in [
                (Mode::Move, "移动"),
                (Mode::Rotate, "旋转"),
                (Mode::HorizontalMirror, "水平镜像"),
                (Mode::VerticalMirror, "垂直镜像"),
            ] {
                if ui.selectable_value(&mut d.mode, mode, label).changed() {
                    d.dirty = true;
                }
            }
        });
        let (changed, pick) = point_fields(ui, "基点 / 镜像轴", &mut d.base, unit);
        d.dirty |= changed;
        if pick {
            d.picking = Some(Pick::Base);
        }
        // Both regions stay allocated irrespective of the operation.
        ui.add_enabled_ui(d.mode == Mode::Move, |ui| {
            let (changed, pick) = point_fields(ui, "目标点", &mut d.target, unit);
            d.dirty |= changed;
            if pick {
                d.picking = Some(Pick::Target);
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([66., 22.], egui::Label::new("角度 °"));
            ui.add_enabled_ui(d.mode == Mode::Rotate, |ui| {
                d.dirty |= ui
                    .add_sized([144., 22.], egui::TextEdit::singleline(&mut d.angle))
                    .changed();
            });
        });
        ui.label("本步作用对象（文字按完整组）");
        crate::ui::modal_widgets::fixed_region(ui, "unified-targets", 80., |ui| {
            for (i, t) in d.targets.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.push_id(i, |ui| {
                        d.dirty |= ui.checkbox(&mut t.enabled, "").changed();
                    });
                    ui.add_sized(
                        [300., 20.],
                        egui::Label::new(format!("{} / {}", t.group.layer_id, t.label)).truncate(),
                    )
                    .on_hover_text(&t.label);
                });
            }
        });
        ui.horizontal(|ui| {
            if button(ui, "预览本步", 94.).clicked() {
                command = d.step(unit).ok().map(worker::Command::Preview);
            }
            if button(ui, "执行本步", 94.).clicked() {
                command = d.step(unit).ok().map(worker::Command::Execute);
            }
            if button(ui, "应用并结束", 94.).clicked() {
                command = if d.dirty {
                    d.step(unit).ok().map(|s| worker::Command::Apply(Some(s)))
                } else {
                    Some(worker::Command::Apply(None))
                };
            }
        });
        ui.horizontal(|ui| {
            for (kind, label) in [
                (UnifiedEditorHistory::Undo, "内部撤销"),
                (UnifiedEditorHistory::Redo, "内部重做"),
                (UnifiedEditorHistory::Reset, "重置入口"),
            ] {
                if button(ui, label, 94.).clicked() {
                    command = Some(worker::Command::History(kind));
                }
            }
        });
    });
    let status = if d.unknown {
        "结果身份未确认，已停止编辑；请保留工程并重新启动".into()
    } else if !d.ready {
        "正在准备编辑会话…".into()
    } else if let Some(r) = &d.reply {
        format!(
            "工作区{}；内部历史资源 {} KiB",
            if r.changed_work {
                "已改变"
            } else {
                "与入口相同"
            },
            r.resources.as_ref().map_or(0, |r| r.resident_bytes / 1024)
        )
    } else {
        String::new()
    };
    crate::ui::modal_widgets::status_slot(ui, &status, 24., d.unknown);
    let error = if d.dirty {
        d.step(unit).err().unwrap_or_default()
    } else {
        String::new()
    };
    crate::ui::modal_widgets::status_slot(ui, &error, 24., true);
    if d.confirm_cancel {
        ui.label("放弃本会话的全部工作？");
        ui.horizontal(|ui| {
            if ui.button("确认放弃").clicked() {
                d.cancel_after_reply = true;
                command = Some(worker::Command::Cancel);
            }
            if ui.button("继续编辑").clicked() {
                d.confirm_cancel = false;
            }
        });
    }
    command
}
impl EditorApp {
    pub(crate) fn open_unified_editor(&mut self) {
        if self.command_context_blocked()
            || self.selection_read_pending()
            || self.view.scene.is_none()
            || self.view.selected.ordered.is_empty()
        {
            return;
        }
        self.unified_editor = Some(Draft::new(&self.view));
        self.modal = Some(ActiveModal::UnifiedEditor);
        self.tool = crate::tools::ActiveTool::Select;
        self.drag = None;
        self.grip = None;
        self.fit = false;
        self.object_snap_runtime.clear_cache();
        self.ui_error = None;
        self.send_unified_editor(worker::Command::Begin);
    }
    pub(crate) fn send_unified_editor(&mut self, command: worker::Command) {
        if self.point_commit_blocked && !matches!(command, worker::Command::Cancel) {
            return;
        }
        let Some(d) = &self.unified_editor else {
            return;
        };
        if d.pending.is_some() || d.unknown || self.busy {
            return;
        }
        let ppp = if self.reported_ppp.is_finite() && self.reported_ppp > 0. {
            self.reported_ppp
        } else {
            1.
        };
        let request = worker::Request {
            draft: d.identity.clone(),
            request: Arc::new(()),
            generation: d.generation,
            context: d.context.clone(),
            command: command.clone(),
            render: worker::RenderInput {
                camera: self.camera,
                rect: self.canvas_rect,
                ppp,
                ppm: self.camera.scale * f64::from(ppp),
                retained_ui_bytes: self.last_good.as_ref().map_or(0, |last| {
                    last.scene.owned_bytes()
                        + last.selected.capacity() * size_of::<u32>()
                        + last.index.data.capacity() * size_of::<u32>()
                }),
            },
        };
        let identity = request.request.clone();
        let previous = self.task_serial;
        self.object_snap_runtime.clear_cache();
        self.prepare_work.clear();
        self.uniform_validation = Default::default();
        self.send(Action::UnifiedEditor(Box::new(request)));
        if self.task_serial != previous
            && self
                .pending_task
                .as_ref()
                .is_some_and(|t| t.task_id == self.task_serial)
        {
            self.unified_editor.as_mut().unwrap().pending = Some(Pending {
                task: self.task_serial,
                identity,
                command,
            });
        }
    }
    pub(crate) fn unified_editor_controls(&mut self, ui: &mut egui::Ui) {
        let Some(d) = &mut self.unified_editor else {
            return;
        };
        let command = controls(ui, d, self.display_unit, self.busy);
        let picking = d.picking.is_some();
        if picking {
            if !self.show_unified_work_for_pick(ui.ctx().pixels_per_point()) {
                return;
            }
            self.modal = None;
            self.object_snap_runtime.reset();
            return;
        }
        let command = command.or_else(|| {
            let d = self.unified_editor.as_ref()?;
            if self.dialog_enter(ui)
                && d.ready
                && d.pending.is_none()
                && !d.unknown
                && !d.confirm_cancel
            {
                if d.dirty {
                    d.step(self.display_unit)
                        .ok()
                        .map(|s| worker::Command::Apply(Some(s)))
                } else {
                    Some(worker::Command::Apply(None))
                }
            } else {
                None
            }
        });
        if let Some(command) = command {
            if matches!(command, worker::Command::Cancel) {
                self.confirm_unified_cancel();
            } else {
                self.send_unified_editor(command);
            }
        }
    }
    pub(crate) fn show_unified_work_for_pick(&mut self, ppp: f32) -> bool {
        let Some(work) = self
            .unified_editor
            .as_ref()
            .and_then(|d| d.reply.as_ref())
            .and_then(|r| r.work.clone())
        else {
            return false;
        };
        // Scene, bounds and coverage all describe the executed manufacturing work.
        // An unexecuted Preview may have been prepared at another display scale.
        let changed = self
            .view
            .scene
            .as_ref()
            .is_none_or(|scene| !Arc::ptr_eq(scene, &work.scene));
        self.view.scene = Some(work.scene.clone());
        self.view.bounds = work.bounds;
        self.view.render_ppm = work.scene.ppm;
        self.view.render_viewport = None;
        self.view.render_coverage_complete = true;
        self.view.display_attempt = None;
        if changed {
            self.selected_flags = Arc::new(crate::gpu::selection_flags(
                &work.scene,
                &self.view.selected.ids(),
            ));
        }
        let ppm = self.camera.scale * f64::from(ppp);
        if !ppm.is_finite() || ppm <= 0. || ppm > work.scene.ppm {
            self.unified_editor.as_mut().unwrap().picking = None;
            self.modal = Some(ActiveModal::UnifiedEditor);
            self.object_snap_runtime.reset();
            self.ui_error = Some("显示缩放已改变；请重置或执行本步后再拾取".into());
            return false;
        }
        true
    }
    pub(crate) fn request_unified_cancel(&mut self) {
        let Some(d) = &mut self.unified_editor else {
            return;
        };
        if d.picking.take().is_some() {
            self.modal = Some(ActiveModal::UnifiedEditor);
            self.object_snap_runtime.reset();
            return;
        }
        if d.reply.as_ref().is_some_and(|r| r.changed_work) || d.pending.is_some() {
            d.confirm_cancel = true;
            self.modal = Some(ActiveModal::UnifiedEditor);
        } else {
            self.confirm_unified_cancel();
        }
    }
    pub(crate) fn confirm_unified_cancel(&mut self) {
        let Some(d) = &mut self.unified_editor else {
            return;
        };
        if d.unknown {
            return;
        }
        d.cancel_after_reply = true;
        d.confirm_cancel = false;
        if d.pending.is_some() {
            if let Some(t) = &self.pending_task {
                t.cancel_token.cancel();
            }
            return;
        }
        if !d.ready {
            self.unified_editor = None;
            self.modal = None;
            return;
        }
        self.send_unified_editor(worker::Command::Cancel);
    }
    pub(crate) fn accept_unified_editor_reply(&mut self, id: u64) {
        let Some(d) = &mut self.unified_editor else {
            return;
        };
        let Some(p) = d.pending.as_ref() else {
            if d.cancel_after_reply && !self.busy {
                self.confirm_unified_cancel();
            }
            return;
        };
        if p.task != id {
            return;
        }
        let r =
            self.view.unified_editor.as_ref().filter(|r| {
                Arc::ptr_eq(&r.draft, &d.identity) && Arc::ptr_eq(&r.request, &p.identity)
            });
        let Some(r) = r else {
            // TaskReceipt was already authenticated by the common route. A pre-handler
            // cancellation/failure cannot have committed. Everything else stays closed.
            if self.view.task_receipt.as_ref().is_some_and(|r| {
                r.task_id == id
                    && matches!(
                        r.state,
                        editor_service::task::TaskState::Cancelled
                            | editor_service::task::TaskState::Failed
                    )
            }) {
                let begin = matches!(p.command, worker::Command::Begin);
                d.pending = None;
                if begin {
                    d.ready = false;
                }
                if d.cancel_after_reply {
                    self.confirm_unified_cancel();
                }
            } else {
                d.unknown = true;
                d.pending = None;
            }
            return;
        };
        let command = d.pending.take().unwrap().command;
        d.generation = r.generation;
        d.reply = Some(r.clone());
        d.ready = true;
        self.object_snap_runtime.clear_cache();
        if matches!(
            r.terminal,
            worker::Terminal::Changed | worker::Terminal::NoChange | worker::Terminal::Cancelled
        ) {
            self.unified_editor = None;
            self.modal = None;
            self.view.unified_editor = None;
            self.uniform_validation = Default::default();
            self.ui_error = None;
            return;
        }
        if r.terminal == worker::Terminal::Open
            && matches!(
                command,
                worker::Command::Execute(_) | worker::Command::History(_)
            )
        {
            d.clear_input();
        }
        if d.cancel_after_reply {
            self.confirm_unified_cancel();
        }
    }
    pub(crate) fn unified_editor_canvas(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) {
        if !self.arbitrate_point_input_frame(ctx) {
            return;
        }
        if self
            .unified_editor
            .as_ref()
            .is_some_and(|d| d.picking.is_some())
            && !self.show_unified_work_for_pick(ctx.pixels_per_point())
        {
            return;
        }
        let Some(d) = self.unified_editor.as_ref() else {
            return;
        };
        let Some(pick) = d.picking else {
            return;
        };
        if d.pending.is_some() || d.unknown || self.point_commit_blocked {
            return;
        }
        let Some(position) = response.hover_pos() else {
            return;
        };
        let live = d
            .reply
            .as_ref()
            .map_or(crate::unified_editor_resources::HOST_BYTES, |r| {
                r.host_peak_bytes
            });
        let result = self.object_snap_runtime.resolve_draft(
            self.camera.world(position, rect),
            &self.object_snap.contour(),
            self.grid,
            self.camera,
            ctx.pixels_per_point(),
            self.view.snap_snapshot.as_deref(),
            &self.view.snap_index,
            &self.view.layers,
            ctx.input(|i| i.modifiers.alt),
            live,
        );
        match result {
            Ok(value) => {
                self.object_snap_runtime.current = Some(value.clone());
                if response.clicked_by(egui::PointerButton::Primary) {
                    let d = self.unified_editor.as_mut().unwrap();
                    match pick {
                        Pick::Base => d.base.set(point_input::snapped(&value), self.display_unit),
                        Pick::Target => d
                            .target
                            .set(point_input::snapped(&value), self.display_unit),
                    }
                    d.dirty = true;
                    d.picking = None;
                    self.modal = Some(ActiveModal::UnifiedEditor);
                    self.object_snap_runtime.reset();
                }
            }
            Err(error) => self.ui_error = Some(error),
        }
    }
    pub(crate) fn paint_unified_reference(&self, painter: &egui::Painter, rect: egui::Rect) {
        let Some(d) = &self.unified_editor else {
            return;
        };
        let Some(reply) = &d.reply else {
            return;
        };
        for path in reply.reference.iter() {
            for edge in path.windows(2) {
                painter.line_segment(
                    [
                        self.camera.screen(edge[0], rect),
                        self.camera.screen(edge[1], rect),
                    ],
                    egui::Stroke::new(
                        0.7,
                        egui::Color32::from_rgba_unmultiplied(150, 180, 210, 65),
                    ),
                );
            }
        }
        if d.picking.is_some() {
            painter.text(
                rect.left_top() + egui::vec2(12., 12.),
                egui::Align2::LEFT_TOP,
                "拾取工作图形上的点；Alt 暂停吸附；Esc 返回",
                egui::FontId::proportional(13.),
                egui::Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod geometry_probe {
    std::thread_local! {pub static RECTS:std::cell::RefCell<Vec<(&'static str,eframe::egui::Rect)>>=const{std::cell::RefCell::new(Vec::new())};}
}
