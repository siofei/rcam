//! Common point modal for existing spatial-tool reference/target inputs.
use crate::{
    EditorApp,
    modal::ActiveModal,
    point_input::{self, Context, Draft, Point},
    point_transform::{Pick, PickField},
    state::{Action, Model},
};
use editor_core::MmPoint;
use eframe::egui;
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq)]
pub enum Adapter {
    TextReference,
    Measure,
    BoardWorld(usize),
    ArrayBase,
    ArrayTarget,
    BlockCreate,
    BlockLocal(String),
    BlockTarget,
    Grip(editor_core::grip::GripFeatureId),
}
impl Adapter {
    fn label(&self) -> &'static str {
        match self {
            Self::TextReference => "文字相对参考点",
            Self::Measure => "测距端点",
            Self::BoardWorld(_) => "配准世界坐标",
            Self::ArrayBase => "阵列 Pitch 基点 B",
            Self::ArrayTarget => "阵列 Pitch 目标 T",
            Self::BlockCreate => "Block 定义原点",
            Self::BlockLocal(_) => "Block 定义局部参考点",
            Self::BlockTarget => "Block 世界放置目标",
            Self::Grip(_) => "Grip 目标点",
        }
    }
}
pub struct Session {
    pub context: Context,
    pub target: Adapter,
    pub draft: Draft,
    pub resume: Option<ActiveModal>,
    pub local_requested: bool,
    pub grip_preview: Option<crate::grip::Session>,
    pub block_translation: Option<MmPoint>,
    pub excluded: std::collections::HashSet<String>,
}
#[derive(Clone)]
pub struct DefinitionReply {
    pub context: Context,
    pub definition: String,
    pub result: editor_service::SelectionCentersResult,
}
impl Model {
    pub fn definition_centers(
        &mut self,
        context: Context,
        definition: String,
    ) -> Result<(), editor_service::ServiceError> {
        if !context.valid(&self.view) {
            return Err(editor_service::ServiceError {
                code: "REVISION_CONFLICT".into(),
                message: "定义中心上下文已过期".into(),
                details: serde_json::json!({}),
            });
        }
        let info = self.info()?;
        let token = self.active_cancel.clone();
        let result = self.service.geometry_block_definition_centers_cancellable(
            &info.document_id,
            &info.revision,
            &definition,
            || token.as_ref().is_some_and(|c| c.checkpoint().is_err()),
        )?;
        self.view.definition_centers = Some(Arc::new(DefinitionReply {
            context,
            definition,
            result,
        }));
        Ok(())
    }
}
impl EditorApp {
    pub(crate) fn open_point_adapter(&mut self, target: Adapter, point: MmPoint) {
        if self.busy {
            return;
        }
        let resume = self.modal.take();
        let mut draft = Draft::default();
        draft.set(
            Point {
                world_mm: point,
                source: point_input::Source::Numeric,
            },
            self.display_unit,
        );
        self.point_adapter = Some(Session {
            context: Context::capture(&self.view),
            target,
            draft,
            resume,
            local_requested: false,
            grip_preview: None,
            block_translation: None,
            excluded: self
                .view
                .selected
                .ids()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        });
        self.point_pick = None;
        self.drag = None;
        self.grip = None;
        self.modal = Some(ActiveModal::PointInput);
        self.object_snap_runtime.reset();
    }
    pub(crate) fn point_adapter_modal(&mut self, ui: &mut egui::Ui) {
        if !self.arbitrate_point_input_frame(ui.ctx()) {
            return;
        }
        let Some(mut session) = self.point_adapter.take() else {
            return;
        };
        if !session.context.valid(&self.view) {
            ui.label("点输入上下文已改变，请取消重试");
            self.point_adapter = Some(session);
            return;
        }
        ui.label(session.target.label());
        let centers = if let Adapter::BlockLocal(definition) = &session.target {
            ui.label("定义局部坐标；此参考点映射到放置目标，不修改定义");
            if !session.local_requested && !self.busy {
                session.local_requested = true;
                self.send(Action::DefinitionCenters(
                    session.context.clone(),
                    definition.clone(),
                ));
            }
            if let Some(reply) = &self.view.definition_centers
                && reply.context == session.context
                && reply.definition == *definition
            {
                (
                    point_input::from_centers(&reply.result, false),
                    point_input::from_centers(&reply.result, true),
                )
            } else {
                (
                    Err("正在计算定义局部中心".into()),
                    Err("正在计算定义局部中心".into()),
                )
            }
        } else {
            ui.small("中心仅计算选中对象；各层按原 Dark/Clear 合成，跨层按面积加权");
            (
                point_input::center(&self.view, false),
                point_input::center(&self.view, true),
            )
        };
        let pick = matches!(
            point_input::controls_with_centers(
                ui,
                &mut session.draft,
                self.display_unit,
                centers.0,
                centers.1
            ),
            point_input::Event::Pick
        );
        let point = session.draft.resolve(self.display_unit);
        let preview = point
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| self.preview_adapter_point(&mut session, p.world_mm));
        if let Err(error) = &preview {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
        let apply = ui.add_enabled(
            preview.is_ok() && !self.busy && !self.point_commit_blocked,
            egui::Button::new("使用此点"),
        );
        #[cfg(feature = "internal-evidence")]
        crate::native_i1::widget("adapter-apply", &apply);
        if apply.clicked() || (preview.is_ok() && self.dialog_enter(ui)) {
            if let Ok(point) = point
                && let Err(error) = self.commit_adapter_point(&session, point.world_mm)
            {
                self.ui_error = Some(error);
                self.point_adapter = Some(session);
                return;
            }
            self.modal = session.resume;
            return;
        }
        if pick {
            self.point_pick = Some(Pick {
                context: session.context.clone(),
                field: PickField::Adapter,
                saved: session.draft.clone(),
                resume: ActiveModal::PointInput,
            });
            self.modal = None;
            self.object_snap_runtime.reset();
        }
        self.point_adapter = Some(session);
    }
    pub(crate) fn preview_adapter_point(
        &mut self,
        session: &mut Session,
        point: MmPoint,
    ) -> Result<(), String> {
        session.block_translation = None;
        if !session.context.valid(&self.view) || !point.is_valid_geometry() {
            session.grip_preview = None;
            return Err("点输入上下文或坐标无效".into());
        }
        match &session.target {
            Adapter::Grip(id) => {
                if session
                    .grip_preview
                    .as_ref()
                    .is_none_or(|g| !g.valid(&self.view) || g.id != *id)
                {
                    session.grip_preview = Some(
                        crate::grip::Session::arm(&self.view, *id).ok_or("Grip 上下文不可用")?,
                    );
                }
                let grip = session.grip_preview.as_mut().unwrap();
                grip.update(point);
                grip.preview.as_ref().map(|_| ()).map_err(Clone::clone)
            }
            Adapter::BlockTarget => {
                let s = self
                    .block
                    .session
                    .as_mut()
                    .filter(|s| s.valid(&self.view))
                    .ok_or("Block 上下文不可用")?;
                if let crate::block_ui::SessionKind::Place { definition } = &s.kind
                    && let Some(p) = &self.view.block_preview
                    && p.context == s.context
                    && &p.definition == definition
                {
                    s.preview = Some(p.clone());
                }
                if s.preview.is_none() {
                    return Err("正在准备 Block 预览，请稍后重试".into());
                }
                session.block_translation = Some(MmPoint::new(
                    point.x_mm - self.block_point_reference.x_mm,
                    point.y_mm - self.block_point_reference.y_mm,
                ));
                Ok(())
            }
            Adapter::ArrayTarget if self.array_point_base.is_none() => {
                Err("请先设置阵列 Pitch 基点 B".into())
            }
            _ => Ok(()),
        }
    }
    fn commit_adapter_point(&mut self, session: &Session, point: MmPoint) -> Result<(), String> {
        if !session.context.valid(&self.view) {
            return Err("点输入已过期".into());
        }
        if session.target == Adapter::Measure {
            let source = session.draft.resolve(self.display_unit)?.source;
            let kind = if let point_input::Source::Feature(kind) = source {
                Some(kind)
            } else {
                None
            };
            self.tool = crate::tools::ActiveTool::Measure;
            self.measure.click_snapped(point, kind);
            return Ok(());
        }
        if matches!(session.target, Adapter::Grip(_)) {
            let grip = session
                .grip_preview
                .as_ref()
                .filter(|g| g.target == point && g.valid(&self.view) && g.preview.is_ok())
                .ok_or("Grip 预览与目标不一致")?;
            if let Some(action) = grip.clone().release() {
                self.send(action);
            }
            Ok(())
        } else {
            if matches!(session.target, Adapter::BlockTarget)
                && session.block_translation
                    != Some(MmPoint::new(
                        point.x_mm - self.block_point_reference.x_mm,
                        point.y_mm - self.block_point_reference.y_mm,
                    ))
            {
                return Err("Block 预览与目标不一致".into());
            }
            self.apply_adapter_point(&session.target, point)
        }
    }
    pub(crate) fn paint_adapter_point(&self, painter: &egui::Painter, rect: egui::Rect, ppp: f32) {
        let Some(s) = &self.point_adapter else {
            return;
        };
        if !s.context.valid(&self.view) {
            return;
        }
        let Ok(point) = s.draft.resolve(self.display_unit) else {
            return;
        };
        if let Some(g) = &s.grip_preview
            && g.target == point.world_mm
        {
            g.paint(painter, self.camera, rect, ppp);
        }
        if let Some(t) = s.block_translation
            && t == MmPoint::new(
                point.world_mm.x_mm - self.block_point_reference.x_mm,
                point.world_mm.y_mm - self.block_point_reference.y_mm,
            )
            && let Some(preview) = self.block.session.as_ref().and_then(|s| s.preview.as_ref())
        {
            for path in &preview.paths {
                for pair in path.windows(2) {
                    painter.line_segment(
                        [
                            self.camera.screen(
                                MmPoint::new(pair[0].x_mm + t.x_mm, pair[0].y_mm + t.y_mm),
                                rect,
                            ),
                            self.camera.screen(
                                MmPoint::new(pair[1].x_mm + t.x_mm, pair[1].y_mm + t.y_mm),
                                rect,
                            ),
                        ],
                        egui::Stroke::new(1.2 / ppp, egui::Color32::LIGHT_GREEN),
                    );
                }
            }
        }
    }
    pub(crate) fn apply_adapter_point(
        &mut self,
        target: &Adapter,
        point: MmPoint,
    ) -> Result<(), String> {
        if !point.is_valid_geometry() {
            return Err("无效制造坐标".into());
        }
        let x = self.display_unit.input(point.x_mm);
        let y = self.display_unit.input(point.y_mm);
        match target {
            Adapter::TextReference => {
                self.text.rx = x;
                self.text.ry = y;
                self.text.has_reference = true;
                self.text.pick_reference = false;
                self.text.changed();
            }
            Adapter::Measure => {
                self.tool = crate::tools::ActiveTool::Measure;
                self.measure.click_snapped(point, None);
            }
            Adapter::BoardWorld(i) => {
                self.components.world_points[*i] = [point.x_mm, point.y_mm];
                self.components.registration_confirmed = false;
            }
            Adapter::ArrayBase => self.array_point_base = Some(point),
            Adapter::ArrayTarget => {
                let base = self.array_point_base.ok_or("请先设置阵列 Pitch 基点 B")?;
                self.array.pitch_x = self.display_unit.input(point.x_mm - base.x_mm);
                self.array.pitch_y = self.display_unit.input(point.y_mm - base.y_mm);
                self.array.requested = None;
            }
            Adapter::BlockCreate => {
                self.block.x = x;
                self.block.y = y;
            }
            Adapter::BlockLocal(_) => {
                self.block_point_reference = point;
            }
            Adapter::BlockTarget => {
                if self
                    .block
                    .session
                    .as_ref()
                    .is_none_or(|s| s.preview.is_none() || !s.valid(&self.view))
                {
                    return Err("正在准备 Block 预览，请稍后重试".into());
                }
                let mut s = self.block.session.take().unwrap();
                s.point = Some(MmPoint::new(
                    point.x_mm - self.block_point_reference.x_mm,
                    point.y_mm - self.block_point_reference.y_mm,
                ));
                if let Some(request) = s.request() {
                    self.tool = crate::tools::ActiveTool::Select;
                    self.send(Action::BlockEdit(Box::new(request)));
                }
            }
            Adapter::Grip(id) => {
                let mut grip =
                    crate::grip::Session::arm(&self.view, *id).ok_or("Grip 上下文不可用")?;
                grip.update(point);
                if let Some(action) = grip.release() {
                    self.send(action);
                }
            }
        }
        Ok(())
    }
}
