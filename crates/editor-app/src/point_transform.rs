//! Resolved selection transform and worker-built display preview.
use crate::{
    EditorApp,
    modal::ActiveModal,
    point_input::{self, Context, Draft, Point},
    state::{Action, Model, View},
};
use editor_core::{MmPoint, SemanticGeometry, edit::MirrorAxis};
use editor_service::{SelectionEdit, SelectionGroup, ServiceError};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Move,
    Copy,
    Rotate,
    HorizontalMirror,
    VerticalMirror,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub context: Context,
    pub groups: Arc<Vec<SelectionGroup>>,
    pub operation: SelectionEdit,
    pub ppm: f64,
}
#[derive(Clone)]
pub struct Preview {
    pub request: Request,
    pub paths: Vec<Vec<MmPoint>>,
    pub bounds: editor_core::BoundsMm,
    pub simplified: bool,
}
pub struct Session {
    pub context: Context,
    pub groups: Arc<Vec<SelectionGroup>>,
    pub mode: Mode,
    pub base: Draft,
    pub target: Draft,
    pub angle: String,
    pub requested: Option<Request>,
    pub excluded: std::collections::HashSet<String>,
}
impl Session {
    pub fn new(view: &View, mode: Mode, unit: editor_core::units::DisplayUnit) -> Self {
        let mut base = Draft::default();
        if let Ok(point) = point_input::center(view, false) {
            base.set(point, unit);
        }
        Self {
            context: Context::capture(view),
            groups: Arc::new(view.selected.groups()),
            mode,
            base,
            target: Draft::default(),
            angle: "90".into(),
            requested: None,
            excluded: view.selected.ids().into_iter().map(str::to_owned).collect(),
        }
    }
    pub fn operation(
        &self,
        view: &View,
        unit: editor_core::units::DisplayUnit,
    ) -> Result<SelectionEdit, String> {
        if !self.context.valid(view) {
            return Err("工程/选择/权限/精度已改变，请重新开始".into());
        }
        let base = self.base.resolve(unit)?.world_mm;
        let operation = match self.mode {
            Mode::Move | Mode::Copy => {
                let t = self.target.resolve(unit)?.world_mm;
                let (dx_mm, dy_mm) = (t.x_mm - base.x_mm, t.y_mm - base.y_mm);
                if self.mode == Mode::Copy {
                    SelectionEdit::Duplicate { dx_mm, dy_mm }
                } else {
                    SelectionEdit::Move { dx_mm, dy_mm }
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
        Ok(operation)
    }
    pub fn request(
        &self,
        view: &View,
        unit: editor_core::units::DisplayUnit,
        ppm: f64,
    ) -> Result<Request, String> {
        Ok(Request {
            context: self.context.clone(),
            groups: self.groups.clone(),
            operation: self.operation(view, unit)?,
            ppm,
        })
    }
}
fn fail(message: &str) -> ServiceError {
    ServiceError {
        code: "INVALID_ARGUMENT".into(),
        message: message.into(),
        details: serde_json::json!({}),
    }
}
fn resource(message: &str) -> ServiceError {
    ServiceError {
        code: "RESOURCE_LIMIT".into(),
        message: message.into(),
        details: serde_json::json!({"temporary_budget_bytes":128*1024*1024,"time_budget_ms":2000}),
    }
}
fn preview_source_cost(
    view: &View,
    check: &mut impl FnMut() -> Result<(), ServiceError>,
) -> Result<usize, ServiceError> {
    use editor_core::{ApertureShape, MacroPrimitive, RegionContour, block::BlockObjectGeometry};
    const MAX: usize = 128 * 1024;
    fn edges(
        contours: &[RegionContour],
        check: &mut impl FnMut() -> Result<(), ServiceError>,
    ) -> Result<usize, ServiceError> {
        let mut n = 1usize;
        for c in contours {
            check()?;
            n = n.saturating_add(c.edges.len());
            if n > MAX {
                return Err(resource("基点预览源轮廓超限"));
            }
        }
        Ok(n)
    }
    let mut apertures = std::collections::HashMap::new();
    if view.apertures.len() > MAX {
        return Err(resource("基点预览光圈查找数据超限"));
    }
    for a in &view.apertures {
        check()?;
        let mut n = 1usize;
        if let ApertureShape::Macro { primitives } = &a.shape {
            for p in primitives {
                check()?;
                n = n.saturating_add(match p {
                    MacroPrimitive::Outline { points, .. } => points.len(),
                    _ => 4,
                });
            }
        }
        apertures.insert(a.id.as_str(), n);
    }
    let shape = |id: &str| apertures.get(id).copied().ok_or_else(|| fail("光圈不存在"));
    let mut blocks = std::collections::HashMap::new();
    let mut total = view.apertures.len();
    for o in &view.selected.ordered {
        check()?;
        let cost = match &o.object.geometry {
            SemanticGeometry::Region { contours } => edges(contours, check)?,
            SemanticGeometry::Flash { aperture_id, .. } => shape(aperture_id)?,
            SemanticGeometry::BlockInstance { definition_id, .. } => {
                if let Some(n) = blocks.get(&definition_id.0) {
                    *n
                } else {
                    let d = view
                        .block_definitions
                        .iter()
                        .find(|d| d.id == *definition_id)
                        .ok_or_else(|| fail("Block 不存在"))?;
                    let mut n = 1usize;
                    for o in &d.objects {
                        check()?;
                        n = n.saturating_add(match &o.geometry {
                            BlockObjectGeometry::Region { contours } => edges(contours, check)?,
                            BlockObjectGeometry::Flash { aperture_id, .. } => shape(aperture_id)?,
                            _ => 1,
                        });
                        if n > MAX {
                            return Err(resource("基点预览 Block 源数据超限"));
                        }
                    }
                    blocks.insert(definition_id.0.clone(), n);
                    n
                }
            }
            _ => 1,
        };
        total = total.saturating_add(cost);
        if total > MAX {
            return Err(resource("基点预览临时数据超限；没有提交修改"));
        }
    }
    Ok(total)
}
impl Model {
    pub fn point_preview(&mut self, request: Request) -> Result<(), ServiceError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let token = self.active_cancel.clone();
        let mut check = || {
            if let Some(cancel) = &token {
                cancel.checkpoint()?;
            }
            if std::time::Instant::now() >= deadline {
                return Err(resource("基点预览超过 2 秒总预算"));
            }
            Ok(())
        };
        check()?;
        if !request.context.valid(&self.view) || *request.groups != self.view.selected.groups() {
            return Err(fail("基点预览已过期"));
        }
        let cost = preview_source_cost(&self.view, &mut check)?;
        let simplified = cost > 5000;
        let mut transformed = Vec::with_capacity(self.view.selected.ordered.len());
        for selected in &self.view.selected.ordered {
            check()?;
            transformed.push(
                request
                    .operation
                    .preview_geometry(&selected.object.geometry)
                    .map_err(|e| fail(&format!("变换预览：{e:?}")))?,
            );
        }
        check()?;
        // One batch shares aperture and Block-orientation lookups; the former
        // per-object helper rebuilt every aperture table for every selected item.
        let bounds = editor_core::geometries_bounds_with_blocks(
            transformed.iter(),
            &self.view.apertures,
            &self.view.block_definitions,
        )
        .map_err(|e| fail(&e.to_string()))?
        .ok_or_else(|| fail("没有可预览的制造边界"))?;
        check()?;
        let mut geometries = Vec::new();
        if !simplified {
            for geometry in transformed {
                check()?;
                match geometry {
                    SemanticGeometry::BlockInstance {
                        definition_id,
                        transform,
                    } => {
                        let d = self
                            .view
                            .block_definitions
                            .iter()
                            .find(|d| d.id == definition_id)
                            .ok_or_else(|| fail("Block 不存在"))?;
                        geometries.extend(
                            self.block_display_cache
                                .resolve(d, &transform)
                                .map_err(|e| fail(&e))?
                                .into_iter()
                                .map(|o| o.geometry),
                        );
                    }
                    g => geometries.push(g),
                }
            }
        }
        let paths = if simplified {
            vec![]
        } else {
            crate::block_ui::preview_paths(geometries.iter(), &self.view.apertures, request.ppm)?
        };
        check()?;
        self.view.point_preview = Some(Arc::new(Preview {
            request,
            paths,
            bounds,
            simplified,
        }));
        Ok(())
    }
    pub fn point_apply(&mut self, request: Request) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) || *request.groups != self.view.selected.groups() {
            return Err(fail("基点变换已过期"));
        }
        self.editable()?;
        if *request.groups != self.edit_groups()? {
            return Err(fail("编辑权限或选择已改变"));
        }
        let info = self.info()?;
        let groups = self.submit_selection(
            &info.document_id,
            &info.revision,
            (*request.groups).clone(),
            request.operation,
        )?;
        let mut selected = Vec::new();
        for group in groups {
            for object_id in group.object_ids {
                selected.push(self.service.objects_get(
                    &info.document_id,
                    editor_service::ObjectParams {
                        layer_id: group.layer_id.clone(),
                        object_id,
                    },
                )?);
            }
        }
        self.view.selected.ordered = selected.into();
        self.refresh(true)?;
        self.view.message = "已通过共同基点提交一次原子变换".into();
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub enum PickField {
    Base,
    Target,
    Adapter,
}
pub struct Pick {
    pub context: Context,
    pub field: PickField,
    pub saved: Draft,
    pub resume: ActiveModal,
}
impl EditorApp {
    pub(crate) fn unified_transform_controls(&mut self, ui: &mut egui::Ui) {
        if !self.arbitrate_point_input_frame(ui.ctx()) {
            return;
        }
        let Some(mut session) = self.point_transform.take() else {
            ui.label("变换上下文已失效，请重新打开");
            return;
        };
        ui.label("共同世界基点 B");
        let base_pick = matches!(
            point_input::controls_tagged(ui, &mut session.base, &self.view, self.display_unit, "B"),
            point_input::Event::Pick
        );
        let mut target_pick = false;
        match session.mode {
            Mode::Move | Mode::Copy => {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut session.mode, Mode::Move, "移动");
                    let _copy = ui.selectable_value(&mut session.mode, Mode::Copy, "复制到目标");
                    #[cfg(feature = "internal-evidence")]
                    crate::native_i1::widget("transform-copy", &_copy);
                });
                ui.label("目标 T（Δ = T − B）");
                target_pick = matches!(
                    point_input::controls_tagged(
                        ui,
                        &mut session.target,
                        &self.view,
                        self.display_unit,
                        "T"
                    ),
                    point_input::Event::Pick
                );
            }
            Mode::Rotate => {
                ui.label("逆时针角度 °");
                let _angle = ui.text_edit_singleline(&mut session.angle);
                #[cfg(feature = "internal-evidence")]
                crate::native_i1::widget("transform-angle", &_angle);
            }
            Mode::HorizontalMirror | Mode::VerticalMirror => {
                ui.radio_value(&mut session.mode, Mode::HorizontalMirror, "水平轴穿过 B");
                let _vertical =
                    ui.radio_value(&mut session.mode, Mode::VerticalMirror, "垂直轴穿过 B");
                #[cfg(feature = "internal-evidence")]
                crate::native_i1::widget("transform-vertical", &_vertical);
            }
        }
        let operation = session.operation(&self.view, self.display_unit);
        let mut ready = false;
        let mut apply_request = None;
        let mut notice = String::new();
        if let Ok(operation) = &operation {
            let changed = session
                .requested
                .as_ref()
                .is_none_or(|r| r.operation != *operation || r.ppm != self.camera.scale);
            if !self.busy
                && changed
                && let Ok(request) =
                    session.request(&self.view, self.display_unit, self.camera.scale)
            {
                session.requested = Some(request.clone());
                self.send(Action::PointPreview(Box::new(request)));
            }
            let request = session
                .requested
                .as_ref()
                .filter(|r| r.operation == *operation && r.ppm == self.camera.scale);
            ready = request.is_some_and(|r| {
                self.view
                    .point_preview
                    .as_ref()
                    .is_some_and(|p| p.request == *r)
            });
            if request.is_some_and(|r| {
                self.view
                    .point_preview
                    .as_ref()
                    .is_some_and(|p| p.request == *r && p.simplified)
            }) {
                notice = "大型选区预览已简化为制造边界框".into();
            }
            apply_request = request.cloned();
        } else if let Err(error) = &operation {
            notice = error.clone();
        }
        crate::ui::modal_widgets::status_slot(ui, &notice, 38., operation.is_err());
        let apply = ui.add_enabled(
            ready && !self.busy && !self.point_commit_blocked,
            egui::Button::new("应用基点变换"),
        );
        #[cfg(test)]
        crate::ui::modal_widgets::record_control(ui, "transform-apply-rect", &apply);
        #[cfg(feature = "internal-evidence")]
        crate::native_i1::widget("transform-apply", &apply);
        if (apply.clicked() || (ready && self.dialog_enter(ui)))
            && let Some(request) = apply_request
        {
            self.send(Action::PointApply(Box::new(request)));
        }
        if base_pick || target_pick {
            let field = if base_pick {
                PickField::Base
            } else {
                PickField::Target
            };
            let saved = if base_pick {
                session.base.clone()
            } else {
                session.target.clone()
            };
            if let Some(resume) = self.modal.take() {
                self.point_pick = Some(Pick {
                    context: session.context.clone(),
                    field,
                    saved,
                    resume,
                });
                self.drag = None;
                self.grip = None;
                self.object_snap_runtime.reset();
            }
        }
        self.point_transform = Some(session);
    }
    pub(crate) fn finish_point_pick(&mut self, point: Option<Point>) {
        let Some(pick) = self.point_pick.take() else {
            return;
        };
        if !pick.context.valid(&self.view) {
            self.point_transform = None;
            return;
        }
        let draft = match pick.field {
            PickField::Base => self.point_transform.as_mut().map(|s| &mut s.base),
            PickField::Target => self.point_transform.as_mut().map(|s| &mut s.target),
            PickField::Adapter => self.point_adapter.as_mut().map(|s| &mut s.draft),
        };
        if let Some(draft) = draft {
            *draft = pick.saved;
            if let Some(point) = point {
                draft.set(point, self.display_unit);
            }
        }
        self.modal = Some(pick.resume);
        self.object_snap_runtime.reset();
    }
    pub(crate) fn point_canvas(
        &mut self,
        ctx: &egui::Context,
        r: &egui::Response,
        rect: egui::Rect,
    ) {
        if !self.arbitrate_point_input_frame(ctx) {
            return;
        }
        let Some(pick) = &self.point_pick else {
            return;
        };
        if !pick.context.valid(&self.view) {
            self.point_pick = None;
            self.point_transform = None;
            return;
        }
        let text = self.text_input_at_event
            || ctx.wants_keyboard_input()
            || self.ime_event
            || self.ime_active;
        if !ctx.input(|i| i.focused)
            || ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::PointerGone))
            })
            || (!text && ctx.input(|i| i.key_pressed(egui::Key::Escape)))
            || r.secondary_clicked()
        {
            self.finish_point_pick(None);
            return;
        }
        if text {
            return;
        }
        if let Some(pos) = r.hover_pos() {
            let excluded = match pick.field {
                PickField::Target => self.point_transform.as_ref().map(|s| &s.excluded),
                PickField::Adapter => self.point_adapter.as_ref().and_then(|s| {
                    matches!(
                        s.target,
                        crate::point_adapter::Adapter::Grip(_)
                            | crate::point_adapter::Adapter::ArrayTarget
                    )
                    .then_some(&s.excluded)
                }),
                PickField::Base => None,
            };
            let settings = self.object_snap.contour();
            let resolution = if let Some(crate::point_adapter::Session {
                target: crate::point_adapter::Adapter::BlockLocal(id),
                ..
            }) = &self.point_adapter
            {
                self.view
                    .block_definitions
                    .iter()
                    .find(|d| d.id.0 == *id)
                    .ok_or_else(|| "定义已不存在".to_string())
                    .and_then(|d| {
                        self.object_snap_runtime.resolve_definition(
                            self.camera.world(pos, rect),
                            d,
                            &self.view.apertures,
                            self.camera,
                            ctx.pixels_per_point(),
                            self.grid,
                            ctx.input(|i| i.modifiers.alt),
                        )
                    })
            } else {
                self.object_snap_runtime.resolve(
                    self.camera.world(pos, rect),
                    &settings,
                    self.grid,
                    self.camera,
                    ctx.pixels_per_point(),
                    self.view.snap_snapshot.as_deref(),
                    &self.view.snap_index,
                    &self.view.layers,
                    excluded,
                    ctx.input(|i| i.modifiers.alt),
                )
            };
            match resolution {
                Ok(value) => {
                    if matches!(
                        self.point_pick.as_ref().map(|p| p.field),
                        Some(PickField::Adapter)
                    ) && let Some(mut adapter) = self.point_adapter.take()
                    {
                        adapter
                            .draft
                            .set(point_input::snapped(&value), self.display_unit);
                        if let Err(error) = self.preview_adapter_point(&mut adapter, value.point) {
                            self.ui_error = Some(error);
                        }
                        self.point_adapter = Some(adapter);
                    }
                    if matches!(
                        self.point_pick.as_ref().map(|p| p.field),
                        Some(PickField::Target)
                    ) {
                        if let Some(s) = &mut self.point_transform {
                            s.target
                                .set(point_input::snapped(&value), self.display_unit);
                        }
                        let request = self.point_transform.as_ref().and_then(|s| {
                            s.request(&self.view, self.display_unit, self.camera.scale)
                                .ok()
                        });
                        if !self.busy
                            && let Some(request) = request
                            && self
                                .point_transform
                                .as_ref()
                                .is_some_and(|s| s.requested.as_ref() != Some(&request))
                        {
                            if let Some(s) = &mut self.point_transform {
                                s.requested = Some(request.clone());
                            }
                            self.send(Action::PointPreview(Box::new(request)));
                        }
                    }
                    if r.clicked() {
                        self.finish_point_pick(Some(point_input::snapped(&value)));
                    }
                }
                Err(error) => self.ui_error = Some(error),
            }
        }
    }
    pub(crate) fn paint_point_transform(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        ppp: f32,
    ) {
        if let Some(pick) = &self.point_pick {
            let label = match pick.field {
                PickField::Base => "拾取基点 B",
                PickField::Target => "拾取目标 T",
                PickField::Adapter => "拾取工具参考点 / 目标点",
            };
            painter.text(
                rect.left_top() + egui::vec2(12., 12.),
                egui::Align2::LEFT_TOP,
                format!("{label} · 点击确认 · Esc / 右键返回 · Alt 暂停吸附"),
                egui::FontId::proportional(14.),
                egui::Color32::YELLOW,
            );
        }
        if self.point_pick.is_some()
            && self
                .point_adapter
                .as_ref()
                .is_some_and(|s| matches!(s.target, crate::point_adapter::Adapter::BlockLocal(_)))
            && let Some(preview) = &self.view.block_preview
        {
            for path in &preview.paths {
                for pair in path.windows(2) {
                    painter.line_segment(
                        [
                            self.camera.screen(pair[0], rect),
                            self.camera.screen(pair[1], rect),
                        ],
                        egui::Stroke::new(1. / ppp, egui::Color32::LIGHT_BLUE),
                    );
                }
            }
        }
        let Some(session) = &self.point_transform else {
            return;
        };
        if !session.context.valid(&self.view) {
            return;
        }
        if let Ok(point) = session.base.resolve(self.display_unit) {
            let p = self.camera.screen(point.world_mm, rect);
            painter.circle_stroke(
                p,
                5. / ppp,
                egui::Stroke::new(1. / ppp, egui::Color32::YELLOW),
            );
            painter.text(
                p + egui::vec2(7., -7.),
                egui::Align2::LEFT_BOTTOM,
                "B",
                egui::FontId::proportional(12.),
                egui::Color32::YELLOW,
            );
        }
        let Some(preview) = &self.view.point_preview else {
            return;
        };
        if session
            .operation(&self.view, self.display_unit)
            .ok()
            .as_ref()
            != Some(&preview.request.operation)
            || preview.request.ppm != self.camera.scale
        {
            return;
        }
        let stroke = egui::Stroke::new(1.2 / ppp, egui::Color32::LIGHT_GREEN);
        if preview.simplified {
            painter.rect_stroke(
                egui::Rect::from_two_pos(
                    self.camera.screen(
                        MmPoint::new(preview.bounds.min_x_mm, preview.bounds.min_y_mm),
                        rect,
                    ),
                    self.camera.screen(
                        MmPoint::new(preview.bounds.max_x_mm, preview.bounds.max_y_mm),
                        rect,
                    ),
                ),
                0.,
                stroke,
                egui::StrokeKind::Inside,
            );
        } else {
            for path in &preview.paths {
                for pair in path.windows(2) {
                    painter.line_segment(
                        [
                            self.camera.screen(pair[0], rect),
                            self.camera.screen(pair[1], rect),
                        ],
                        stroke,
                    );
                }
            }
        }
    }
}
