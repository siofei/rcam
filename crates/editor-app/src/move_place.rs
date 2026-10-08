//! Click-placement owns input until one authoritative PointApply terminal reply.
//! A zero-delta preview is display data only; it never authorizes a moved result.
use crate::{
    EditorApp,
    modal::ActiveModal,
    point_input::{self, Point, Source},
    point_transform::{Mode, Pick, PickField, Preview, Request, Session},
    project_ui::Transition,
    state::{Action, View},
};
use editor_core::{MmPoint, command::ids};
use editor_service::{SelectionEdit, task::TaskContext};
use eframe::egui;
use std::sync::Arc;

pub(crate) enum Phase {
    Preparing,
    Following,
    Frozen,
    FinalPreview(Request),
    Ready(Request),
    Applying,
}
pub(crate) struct Placement {
    pub baseline: Option<Arc<Preview>>,
    pub phase: Phase,
    pub has_target: bool,
    pub frozen_ppm: Option<f64>,
    started_frame: Option<u64>,
    target_pressed: bool,
}
pub(crate) struct Pending {
    task: TaskContext,
    request: Request,
    apply: bool,
    cancelled: bool,
    after_stop: Option<Transition>,
}
impl Pending {
    pub fn new(task: TaskContext, request: Request, apply: bool) -> Self {
        Self {
            task,
            request,
            apply,
            cancelled: false,
            after_stop: None,
        }
    }
}
impl EditorApp {
    pub(crate) fn move_placing(&self) -> bool {
        self.point_transform
            .as_ref()
            .is_some_and(|s| s.placement.is_some())
    }
    pub(crate) fn start_move_place(&mut self) {
        if !self.command_enabled(ids::OBJECT_MOVE_PLACE) || self.selection_read_pending() {
            return;
        }
        self.cancel_canvas_probe();
        self.cancel_block();
        self.text.cancel();
        self.measure.clear();
        self.drag = None;
        self.grip = None;
        self.view.move_admission = None;
        self.tool = crate::tools::ActiveTool::Select;
        self.object_snap_runtime.reset();
        self.view.point_preview = None;
        self.view.error = None;
        self.ui_error = None;
        let mut session = Session::new(&self.view, Mode::Move, self.display_unit);
        // The worker's admitted complete manufacturing bounds supplies B. No
        // stale center or Session::new zero fallback can enter a final request.
        let request = Request {
            context: session.context.clone(),
            groups: session.groups.clone(),
            operation: SelectionEdit::Move {
                dx_mm: 0.,
                dy_mm: 0.,
            },
            ppm: self.camera.scale,
        };
        session.requested = Some(request.clone());
        session.placement = Some(Placement {
            baseline: None,
            phase: Phase::Preparing,
            has_target: false,
            frozen_ppm: None,
            started_frame: self.point_input_frame,
            target_pressed: false,
        });
        self.point_pick = Some(Pick {
            context: session.context.clone(),
            field: PickField::Target,
            saved: session.target.clone(),
            resume: ActiveModal::Move,
        });
        self.point_transform = Some(session);
        self.modal = None;
        self.modal_pending = None;
        self.send(Action::PointPreview(Box::new(request)));
        if self.move_place_task.is_none() {
            self.cancel_move_place();
        }
    }
    pub(crate) fn cancel_move_place(&mut self) {
        self.point_input_cancelled = true;
        self.point_commit_blocked = true;
        if let Some(pending) = &mut self.move_place_task {
            pending.cancelled = true;
            pending.task.cancel_token.cancel();
        }
        self.point_pick = None;
        self.point_transform = None;
        self.view.point_preview = None;
        self.modal = None;
        self.object_snap_runtime.reset();
    }
    pub(crate) fn defer_move_place_transition(&mut self, transition: Transition) -> bool {
        if let Some(pending) = &mut self.move_place_task {
            pending.after_stop.get_or_insert(transition);
            self.cancel_move_place();
            true
        } else {
            if self.move_placing() {
                self.cancel_move_place();
            }
            false
        }
    }
    fn release_move_place_task(&mut self) {
        self.busy = false;
        self.pending_task = None;
        self.modal_pending = None;
    }
    /// Consume retired read-only results. A legal late commit still installs
    /// its authoritative terminal View even after Esc; it cannot restart input.
    pub(crate) fn filter_move_place_reply(&mut self, id: u64, result: &View) -> bool {
        let Some(pending) = self
            .move_place_task
            .as_ref()
            .filter(|p| p.task.task_id == id)
        else {
            return false;
        };
        let identity = crate::task_reply_matches(&pending.task, &self.view, result)
            && pending.request.context.valid(&self.view)
            && pending.request.context.project_id.as_deref()
                == result.info.as_ref().map(|d| d.project_id.as_str())
            && result.info.as_ref().map(|d| &d.document_id)
                == pending.task.input.document_id.as_ref()
            && (pending.apply
                || (pending.request.context.valid(result)
                    && result
                        .task_receipt
                        .as_ref()
                        .is_some_and(|r| r.result_version == pending.task.input)));
        if !identity {
            let pending = self.move_place_task.take().unwrap();
            pending.task.cancel_token.cancel();
            self.cancel_move_place();
            self.release_move_place_task();
            self.ui_error = Some("点击放置结果身份未确认；结果未安装".into());
            self.view.error = Some(editor_service::ServiceError {
                code: "STALE_TASK".into(),
                message: "点击放置结果身份未确认；结果未安装".into(),
                details: serde_json::json!({}),
            });
            self.point_input_cancelled = true;
            self.point_commit_blocked = true;
            if pending.apply {
                self.canvas_selection_unconfirmed = true;
                self.view.blocked = self.ui_error.clone();
            }
            // Never execute a deferred transition on an unknown terminal state.
            return true;
        }
        if !pending.apply && (pending.cancelled || !self.move_placing()) {
            let pending = self.move_place_task.take().unwrap();
            self.release_move_place_task();
            self.view.point_preview = None;
            if let Some(transition) = pending.after_stop {
                self.begin_transition(transition);
            }
            return true;
        }
        false
    }
    pub(crate) fn complete_move_place_reply(&mut self, id: u64) {
        if self
            .move_place_task
            .as_ref()
            .is_none_or(|p| p.task.task_id != id)
        {
            return;
        }
        let pending = self.move_place_task.take().unwrap();
        let error = self
            .view
            .error
            .as_ref()
            .map(|e| format!("{} · {}", e.code, e.message));
        if pending.apply || pending.cancelled || error.is_some() || !self.move_placing() {
            self.cancel_move_place();
            if let Some(error) = error {
                self.ui_error = Some(error);
            }
        } else if let Some(preview) = self
            .view
            .point_preview
            .clone()
            .filter(|p| p.request == pending.request)
        {
            let session = self.point_transform.as_mut().unwrap();
            let placement = session.placement.as_mut().unwrap();
            match &placement.phase {
                Phase::Preparing | Phase::Frozen if placement.baseline.is_none() => {
                    let base = Point {
                        world_mm: preview.bounds.center(),
                        source: Source::BoundingCenter,
                    };
                    if !base.world_mm.is_valid_geometry() {
                        self.cancel_move_place();
                        self.ui_error = Some("点击放置制造基点无效".into());
                        return;
                    }
                    session.base.set(base.clone(), self.display_unit);
                    if !placement.has_target {
                        session.target.set(base, self.display_unit);
                    }
                    placement.baseline = Some(preview);
                    if matches!(placement.phase, Phase::Preparing) {
                        placement.phase = Phase::Following;
                    }
                }
                Phase::FinalPreview(request) if request == &pending.request => {
                    // Release the zero-delta template once the actual moved
                    // preview is accepted. The final Request remains frozen.
                    placement.baseline = Some(preview);
                    placement.phase = Phase::Ready(pending.request);
                }
                _ => {
                    self.cancel_move_place();
                    self.ui_error = Some("点击放置预览阶段不匹配；没有提交修改".into());
                }
            }
        } else {
            self.cancel_move_place();
            self.ui_error = Some("点击放置缺少匹配的完整预览；没有提交修改".into());
        }
        if let Some(transition) = pending.after_stop {
            self.begin_transition(transition);
        }
    }
    pub(crate) fn move_place_disconnected(&mut self) {
        if self.move_placing() || self.move_place_task.is_some() {
            let pending = self.move_place_task.take();
            if let Some(pending) = &pending {
                pending.task.cancel_token.cancel();
            }
            self.cancel_move_place();
            self.release_move_place_task();
            self.ui_error = Some("后台连接关闭，点击放置终态未确认".into());
            self.view.error = Some(editor_service::ServiceError {
                code: "WORKER_DISCONNECTED".into(),
                message: "后台连接关闭，点击放置终态未确认".into(),
                details: serde_json::json!({}),
            });
            if pending.is_some_and(|p| p.apply) {
                self.canvas_selection_unconfirmed = true;
                self.view.blocked = self.ui_error.clone();
            }
        }
    }
    pub(crate) fn tick_move_place(&mut self) {
        if self.busy || self.point_commit_blocked || self.point_input_cancelled {
            return;
        }
        let action = self.point_transform.as_ref().and_then(|s| {
            let placement = s.placement.as_ref()?;
            match &placement.phase {
                Phase::Frozen if placement.baseline.is_some() => Some(
                    s.request(
                        &self.view,
                        self.display_unit,
                        placement.frozen_ppm.unwrap_or(self.camera.scale),
                    )
                    .map(|r| (r, false)),
                ),
                Phase::Ready(request) => Some(Ok((request.clone(), true))),
                _ => None,
            }
        });
        let Some(action) = action else {
            return;
        };
        let (request, apply) = match action {
            Ok(value) => value,
            Err(error) => {
                self.cancel_move_place();
                self.ui_error = Some(error);
                return;
            }
        };
        self.send(if apply {
            Action::PointApply(Box::new(request.clone()))
        } else {
            Action::PointPreview(Box::new(request.clone()))
        });
        if self.move_place_task.is_none() {
            self.cancel_move_place();
            return;
        }
        let session = self.point_transform.as_mut().unwrap();
        session.requested = Some(request.clone());
        session.placement.as_mut().unwrap().phase = if apply {
            Phase::Applying
        } else {
            Phase::FinalPreview(request)
        };
    }
    pub(crate) fn move_place_canvas(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) {
        if response.secondary_clicked() {
            self.cancel_move_place();
            return;
        }
        if self.text_input_at_event
            || ctx.wants_keyboard_input()
            || self.ime_active
            || self.ime_event
        {
            if let Some(placement) = self
                .point_transform
                .as_mut()
                .and_then(|s| s.placement.as_mut())
            {
                placement.target_pressed = false;
            }
            return;
        }
        let owned_release = self
            .point_transform
            .as_mut()
            .and_then(|s| s.placement.as_mut())
            .and_then(|placement| {
                let accepts_press = placement.started_frame != Some(ctx.cumulative_frame_nr());
                ctx.input(|i| {
                    let mut click = None;
                    for event in &i.events {
                        if let egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers,
                        } = event
                        {
                            if *pressed {
                                placement.target_pressed = accepts_press && rect.contains(*pos);
                            } else {
                                if placement.target_pressed
                                    && rect.contains(*pos)
                                    && click.is_none()
                                {
                                    click = Some((*pos, modifiers.alt));
                                }
                                placement.target_pressed = false;
                            }
                        }
                    }
                    click
                })
            });
        let Some(session) = self.point_transform.as_ref() else {
            return;
        };
        if !session.context.valid(&self.view) {
            self.cancel_move_place();
            self.ui_error = Some("工程/选择/权限/精度已改变，请重新开始点击放置".into());
            return;
        }
        let following = session
            .placement
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Preparing | Phase::Following));
        if following {
            // A target click uses that release event's position and Alt state,
            // never a later hover or the frame's final modifier state.
            let click = response.clicked().then_some(owned_release).flatten();
            let position = click.or_else(|| {
                response
                    .hover_pos()
                    .map(|p| (p, ctx.input(|i| i.modifiers.alt)))
            });
            if let Some((pos, alt)) = position {
                let settings = self.object_snap.contour();
                match self.object_snap_runtime.resolve(
                    self.camera.world(pos, rect),
                    &settings,
                    self.grid,
                    self.camera,
                    ctx.pixels_per_point(),
                    self.view.snap_snapshot.as_deref(),
                    &self.view.snap_index,
                    &self.view.layers,
                    Some(&session.excluded),
                    alt,
                ) {
                    Ok(value) => {
                        let session = self.point_transform.as_mut().unwrap();
                        session
                            .target
                            .set(point_input::snapped(&value), self.display_unit);
                        let placement = session.placement.as_mut().unwrap();
                        placement.has_target = true;
                        if click.is_some() {
                            placement.phase = Phase::Frozen;
                            placement.frozen_ppm = Some(self.camera.scale);
                        }
                    }
                    Err(error) => {
                        self.ui_error = Some(error);
                        return;
                    }
                }
            }
        }
        self.tick_move_place();
        ctx.request_repaint();
    }
    pub(crate) fn paint_move_place(&self, painter: &egui::Painter, rect: egui::Rect, ppp: f32) {
        let Some(session) = self
            .point_transform
            .as_ref()
            .filter(|s| s.context.valid(&self.view))
        else {
            return;
        };
        let Some(placement) = &session.placement else {
            return;
        };
        let label = match placement.phase {
            Phase::Preparing => "移动 · 正在检查制造边界与容量 · Esc 取消",
            Phase::Following => {
                "移动 · 基点 B 为制造边界中心 · 点击目标提交 · Esc / 右键取消 · Alt 暂停吸附"
            }
            Phase::Frozen | Phase::FinalPreview(_) => "移动 · 目标已冻结，正在验证 · Esc 取消",
            Phase::Ready(_) | Phase::Applying => "移动 · 正在提交一次事务 · 取消以服务终态为准",
        };
        painter.text(
            rect.left_top() + egui::vec2(12., 12.),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(14.),
            egui::Color32::YELLOW,
        );
        let Some(preview) = &placement.baseline else {
            return;
        };
        let Ok(SelectionEdit::Move { dx_mm, dy_mm }) =
            session.operation(&self.view, self.display_unit)
        else {
            return;
        };
        let SelectionEdit::Move {
            dx_mm: original_x,
            dy_mm: original_y,
        } = preview.request.operation
        else {
            return;
        };
        let delta = (dx_mm - original_x, dy_mm - original_y);
        let translated = |p: MmPoint| {
            self.camera
                .screen(MmPoint::new(p.x_mm + delta.0, p.y_mm + delta.1), rect)
        };
        let stroke = egui::Stroke::new(1.2 / ppp, egui::Color32::LIGHT_GREEN);
        let base = session.base.value.world_mm;
        let b = self.camera.screen(base, rect);
        painter.circle_stroke(
            b,
            5. / ppp,
            egui::Stroke::new(1. / ppp, egui::Color32::YELLOW),
        );
        painter.text(
            b + egui::vec2(7., -7.),
            egui::Align2::LEFT_BOTTOM,
            "B",
            egui::FontId::proportional(12.),
            egui::Color32::YELLOW,
        );
        if preview.simplified {
            painter.rect_stroke(
                egui::Rect::from_two_pos(
                    translated(MmPoint::new(
                        preview.bounds.min_x_mm,
                        preview.bounds.min_y_mm,
                    )),
                    translated(MmPoint::new(
                        preview.bounds.max_x_mm,
                        preview.bounds.max_y_mm,
                    )),
                ),
                0.,
                stroke,
                egui::StrokeKind::Inside,
            );
        } else {
            for path in &preview.paths {
                for pair in path.windows(2) {
                    painter.line_segment([translated(pair[0]), translated(pair[1])], stroke);
                }
            }
        }
    }
}
