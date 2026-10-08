//! Numeric array draft and worker-built, shared display-only source outlines.
use crate::{
    EditorApp,
    block_ui::Context,
    modal::ActiveModal,
    state::{Action, Classifier, Model, View},
};
use editor_core::{
    BoundsMm, MmPoint, SemanticGeometry,
    edit::{ArrayEstimate, RectangularArray},
};
use editor_service::{ArrayRectangularParams, ServiceError};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone, PartialEq)]
pub struct Request {
    pub context: Context,
    pub params: ArrayRectangularParams,
    pub ppm: f64,
}
impl Request {
    fn spec(&self) -> RectangularArray {
        RectangularArray {
            rows: self.params.rows,
            columns: self.params.columns,
            pitch_x_mm: self.params.pitch_x_mm,
            pitch_y_mm: self.params.pitch_y_mm,
        }
    }
}
#[derive(Clone)]
pub struct Preview {
    pub request: Request,
    pub estimate: ArrayEstimate,
    pub paths: Arc<Vec<Vec<MmPoint>>>,
    pub bounds: BoundsMm,
    pub simplified: bool,
    pub build_us: u64,
}
pub struct Draft {
    pub rows: String,
    pub columns: String,
    pub pitch_x: String,
    pub pitch_y: String,
    pub requested: Option<Request>,
    pub confirmed: Option<Request>,
    pub source: rcam_diagnostics::Source,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            rows: "2".into(),
            columns: "2".into(),
            pitch_x: "10".into(),
            pitch_y: "10".into(),
            requested: None,
            confirmed: None,
            source: rcam_diagnostics::Source::Menu,
        }
    }
}
fn fail(message: &str) -> ServiceError {
    ServiceError {
        code: "INVALID_ARGUMENT".into(),
        message: message.into(),
        details: serde_json::json!({}),
    }
}

pub fn eligible(view: &View) -> bool {
    let Some(first) = view.selected.primary() else {
        return false;
    };
    let classifier = Classifier::new(&view.layers, &view.apertures);
    if view.blocked.is_some()
        || !view.selected.ordered.iter().all(|o| {
            o.layer_id == first.layer_id
                && classifier.selectable(o)
                && classifier.edit_refusal(o).is_none()
        })
    {
        return false;
    }
    let Some(layer) = view
        .snap_snapshot
        .as_ref()
        .and_then(|s| s.layers.iter().find(|l| l.id == first.layer_id))
    else {
        return false;
    };
    let ids: std::collections::HashSet<_> = view
        .selected
        .ordered
        .iter()
        .map(|o| o.object.object_id.as_str())
        .collect();
    let indices: Vec<_> = layer
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| ids.contains(o.object_id.as_str()))
        .map(|(i, _)| i)
        .collect();
    if indices.len() != view.selected.ordered.len() || indices.windows(2).any(|w| w[1] != w[0] + 1)
    {
        return false;
    }
    let ops: std::collections::HashSet<_> = view
        .selected
        .ordered
        .iter()
        .filter_map(|o| match &o.object.origin {
            editor_core::ObjectOrigin::GeneratedText { operation_id } => Some(operation_id),
            _ => None,
        })
        .collect();
    !layer.objects.iter().any(|o| matches!(&o.origin, editor_core::ObjectOrigin::GeneratedText { operation_id } if ops.contains(operation_id) && !ids.contains(o.object_id.as_str())))
}
impl Model {
    pub fn array_preview(&mut self, request: Request) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) {
            return Err(fail("阵列预览已过期"));
        }
        let start = std::time::Instant::now();
        let estimate = self.service.estimate_array_rectangular(
            &request.context.document,
            &request.context.revision,
            &request.params,
        )?;
        let bounds = crate::state::selected_bounds(&self.view)?;
        let previous = self.view.array_preview.as_ref().filter(|p| {
            p.request.context == request.context
                && p.request.params.object_ids == request.params.object_ids
                && p.request.ppm == request.ppm
        });
        let paths = if let Some(previous) = previous {
            previous.paths.clone()
        } else if source_outline_cost(&self.view) > 10_000 {
            // Bound expansion before resolving/cloning shared Block geometry.
            Arc::new(vec![])
        } else {
            let mut geometries = vec![];
            for selected in &self.view.selected.ordered {
                match &selected.object.geometry {
                    SemanticGeometry::BlockInstance {
                        definition_id,
                        transform,
                    } => {
                        let definition = self
                            .view
                            .block_definitions
                            .iter()
                            .find(|d| d.id == *definition_id)
                            .ok_or_else(|| fail("Block 不存在"))?;
                        geometries.extend(
                            self.block_display_cache
                                .resolve(definition, transform)
                                .map_err(|e| fail(&e))?
                                .into_iter()
                                .map(|o| o.geometry),
                        );
                    }
                    geometry => geometries.push(geometry.clone()),
                }
            }
            // Complex display outlines may simplify, independently of commit capability.
            Arc::new(
                crate::block_ui::preview_paths(
                    geometries.iter(),
                    &self.view.apertures,
                    request.ppm,
                )
                .unwrap_or_default(),
            )
        };
        let points: usize = paths.iter().map(Vec::len).sum();
        let simplified = paths.is_empty()
            || estimate.cell_count > 1000
            || points.saturating_mul(estimate.cell_count) > 200_000;
        if simplified {
            static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            rcam_diagnostics::rate_limited(
                &LAST,
                rcam_diagnostics::Level::Debug,
                "array.preview.simplified",
                &[("cell_count", estimate.cell_count as u64)],
            );
        }
        self.view.block_cache_stats = self.block_display_cache.stats();
        self.view.array_preview = Some(Arc::new(Preview {
            request,
            estimate,
            paths,
            bounds,
            simplified,
            build_us: start.elapsed().as_micros() as u64,
        }));
        Ok(())
    }
    pub fn array_apply(&mut self, request: Request) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) {
            return Err(fail("阵列参数已过期，请重新打开弹窗"));
        }
        let result = self.service.objects_array_rectangular(
            &request.context.document,
            &request.context.revision,
            request.params.clone(),
        )?;
        self.view.array_preview = None;
        self.view.message = format!(
            "已创建 {} 个副本（{} × {} 阵列）；原选择保持",
            result.changed_object_ids.len(),
            request.params.rows,
            request.params.columns
        );
        if !result.changed_object_ids.is_empty() {
            self.refresh(true)?;
        }
        Ok(())
    }
}
pub(crate) fn source_outline_cost(view: &View) -> usize {
    use editor_core::block::BlockObjectGeometry;
    let edges = |contours: &[editor_core::RegionContour]| {
        contours
            .iter()
            .fold(1usize, |n, c| n.saturating_add(c.edges.len()))
    };
    view.selected.ordered.iter().fold(0usize, |n, selected| {
        n.saturating_add(match &selected.object.geometry {
            SemanticGeometry::Region { contours } => edges(contours),
            SemanticGeometry::BlockInstance { definition_id, .. } => view
                .block_definitions
                .iter()
                .find(|d| d.id == *definition_id)
                .map(|d| {
                    d.objects.iter().fold(0usize, |n, o| {
                        n.saturating_add(match &o.geometry {
                            BlockObjectGeometry::Region { contours } => edges(contours),
                            _ => 1,
                        })
                    })
                })
                .unwrap_or(usize::MAX),
            _ => 1,
        })
    })
}
impl EditorApp {
    pub(crate) fn accept_array_reply(&mut self) {
        if self.modal != Some(ActiveModal::Array)
            || self.view.array_preview.as_ref().is_some_and(|preview| {
                self.array.requested.as_ref() != Some(&preview.request)
                    || !preview.request.context.valid(&self.view)
            })
        {
            self.view.array_preview = None;
        }
    }
    pub(crate) fn open_array(&mut self) {
        if !self.usable() || !eligible(&self.view) {
            return;
        }
        let source = self.operation_source;
        self.open_modal(ActiveModal::Array);
        if self.modal != Some(ActiveModal::Array) {
            return;
        }
        self.array_point_base = None;
        self.array = Draft {
            source,
            pitch_x: self.display_unit.input(10.),
            pitch_y: self.display_unit.input(10.),
            ..Draft::default()
        };
        self.view.array_preview = None;
    }
    fn array_request(&self) -> Result<Request, String> {
        if !eligible(&self.view) {
            return Err("选择必须是同层连续曝光区间，并包含完整文字组".into());
        }
        let spec = RectangularArray {
            rows: self.array.rows.parse().map_err(|_| "Rows 必须为正整数")?,
            columns: self
                .array
                .columns
                .parse()
                .map_err(|_| "Columns 必须为正整数")?,
            pitch_x_mm: self.display_unit.parse_length(&self.array.pitch_x)?,
            pitch_y_mm: self.display_unit.parse_length(&self.array.pitch_y)?,
        };
        let cells = spec.cell_count().map_err(
            |_| "行列必须 ≥1 且乘积不能溢出；重复轴的 Pitch 不可为零且必须有限，偏移必须有效",
        )?;
        if self
            .view
            .selected
            .ordered
            .len()
            .checked_mul(cells - 1)
            .is_none()
        {
            return Err("预计新增对象数溢出".into());
        }
        Ok(Request {
            context: Context::capture(&self.view).ok_or("没有工程")?,
            params: ArrayRectangularParams {
                layer_id: self.view.selected.primary().unwrap().layer_id.clone(),
                object_ids: self
                    .view
                    .selected
                    .ordered
                    .iter()
                    .map(|o| o.object.object_id.clone())
                    .collect(),
                rows: spec.rows,
                columns: spec.columns,
                pitch_x_mm: spec.pitch_x_mm,
                pitch_y_mm: spec.pitch_y_mm,
            },
            ppm: self.camera.scale,
        })
    }
    fn array_can_apply(&self, request: &Request) -> bool {
        let Ok(cells) = request.spec().cell_count() else {
            return false;
        };
        !self.busy
            && request.context.valid(&self.view)
            && self
                .view
                .array_preview
                .as_ref()
                .is_some_and(|p| p.request == *request)
            && (cells <= editor_core::edit::ARRAY_CELL_WARNING_THRESHOLD
                || self.array.confirmed.as_ref() == Some(request))
    }
    pub(crate) fn array_modal(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let suffix = self.display_unit.suffix();
        let mut changed = false;
        for (index, (label, value)) in [
            ("Rows / 行数".to_owned(), &mut self.array.rows),
            ("Columns / 列数".to_owned(), &mut self.array.columns),
            (format!("X Pitch ({suffix})"), &mut self.array.pitch_x),
            (format!("Y Pitch ({suffix})"), &mut self.array.pitch_y),
        ]
        .into_iter()
        .enumerate()
        {
            ui.horizontal(|ui| {
                ui.add_sized(egui::vec2(132., 20.), egui::Label::new(&label).truncate())
                    .on_hover_text(label);
                let width = ui.available_width().clamp(20., 240.);
                changed |= ui
                    .add_sized(
                        egui::vec2(width, 20.),
                        egui::TextEdit::singleline(value)
                            .id(egui::Id::new(("array-input", index)))
                            .desired_width(width),
                    )
                    .changed();
            });
        }
        if changed {
            self.array.confirmed = None;
        }
        crate::ui::modal_widgets::status_slot(
            ui,
            "Pitch 为格子原点间距；负 X 向左，负 Y 向下。原对象保持不动。",
            38.,
            false,
        );
        ui.horizontal(|ui| {
            if ui.button("Pitch 基点 B…").clicked() {
                self.open_point_adapter(
                    crate::point_adapter::Adapter::ArrayBase,
                    self.array_point_base.unwrap_or(MmPoint::new(0., 0.)),
                );
            }
            if ui
                .add_enabled(
                    self.array_point_base.is_some(),
                    egui::Button::new("Pitch 目标 T…"),
                )
                .clicked()
            {
                self.open_point_adapter(
                    crate::point_adapter::Adapter::ArrayTarget,
                    self.array_point_base.unwrap(),
                );
            }
        });
        let block = self
            .view
            .selected
            .ordered
            .iter()
            .all(|o| matches!(o.object.geometry, SemanticGeometry::BlockInstance { .. }));
        crate::ui::modal_widgets::status_slot(
            ui,
            if block {
                "Block 阵列：创建共享定义的轻量实例引用"
            } else {
                ""
            },
            22.,
            false,
        );
        let request = self.array_request();
        let mut large = false;
        let mut summary = String::new();
        let mut advice = String::new();
        let notice;
        let mut warning = false;
        match &request {
            Err(error) => {
                notice = error.clone();
                warning = true;
            }
            Ok(request) => {
                let cells = request.spec().cell_count().unwrap();
                let count = request.params.object_ids.len();
                let created = (cells - 1) * count; // Checked in array_request.
                large = cells > editor_core::edit::ARRAY_CELL_WARNING_THRESHOLD;
                summary = format!(
                    "总格数 {cells}（含原始位置）· 新格子 {}\n源对象 {count} · 新增对象 {created}",
                    cells - 1
                );
                if created > 5000 {
                    advice = "大型拼板建议先创建 Block；格数不等于新增对象数。".into();
                }
                if self.array.requested.as_ref() != Some(request) && !self.busy {
                    self.array.requested = Some(request.clone());
                    self.view.error = None;
                    self.send(Action::ArrayPreview(Box::new(request.clone())));
                }
                notice = if let Some(preview) = self
                    .view
                    .array_preview
                    .as_ref()
                    .filter(|p| p.request == *request)
                {
                    format!(
                        "预估历史/提交峰值预算 {:.1} MiB{}",
                        preview.estimate.history_bytes as f64 / (1024. * 1024.),
                        if preview.simplified {
                            "\n预览仅显示整个阵列范围"
                        } else {
                            ""
                        }
                    )
                } else if let Some(error) = self
                    .view
                    .error
                    .as_ref()
                    .filter(|e| e.code == "RESOURCE_LIMIT")
                {
                    if let (Some(required), Some(limit)) = (
                        error.details["required_history_bytes"].as_u64(),
                        error.details["max_history_bytes"].as_u64(),
                    ) {
                        format!(
                            "预估历史/提交峰值预算 {:.1} MiB；当前预算 {:.1} MiB\n资源预检未通过，未创建任何对象。",
                            required as f64 / (1024. * 1024.),
                            limit as f64 / (1024. * 1024.)
                        )
                    } else {
                        error.message.clone()
                    }
                } else {
                    "正在检查资源预算与预览…".into()
                };
            }
        }
        crate::ui::modal_widgets::status_slot(ui, &summary, 42., false);
        crate::ui::modal_widgets::status_slot(ui, &advice, 38., false);
        crate::ui::modal_widgets::status_slot(
            ui,
            if large {
                "总格数超过 500000；确认后可继续，仍检查实际资源预算。"
            } else {
                ""
            },
            40.,
            large,
        );
        let mut confirmed = request
            .as_ref()
            .ok()
            .is_some_and(|r| self.array.confirmed.as_ref() == Some(r));
        if ui
            .add_enabled(
                large && !self.busy,
                egui::Checkbox::new(&mut confirmed, "确认继续超过 500000 格的阵列"),
            )
            .changed()
        {
            self.array.confirmed = request.as_ref().ok().filter(|_| confirmed).cloned();
        }
        crate::ui::modal_widgets::status_slot(ui, &notice, 50., warning);
        let enabled = request.as_ref().is_ok_and(|r| self.array_can_apply(r));
        let apply = crate::ui::buttons::primary(ui, "Apply / 应用阵列", enabled);
        if enabled && (apply.clicked() || self.dialog_enter(ui)) {
            self.send(Action::ArrayApply(Box::new(request.unwrap())));
        }
        apply
    }
    pub(crate) fn paint_array(&self, painter: &egui::Painter, rect: egui::Rect, ppp: f32) {
        if self.modal != Some(ActiveModal::Array) {
            return;
        }
        let Some(preview) = &self.view.array_preview else {
            return;
        };
        if self.array_request().ok().as_ref() != Some(&preview.request) {
            return;
        }
        let spec = preview.request.spec();
        let stroke = egui::Stroke::new(1.2 / ppp, egui::Color32::LIGHT_GREEN.gamma_multiply(0.65));
        if preview.simplified {
            let Ok(offset) = spec.offset(preview.estimate.cell_count - 1) else {
                return;
            };
            let b = preview.bounds;
            let min = MmPoint::new(
                b.min_x_mm + offset.x_mm.min(0.),
                b.min_y_mm + offset.y_mm.min(0.),
            );
            let max = MmPoint::new(
                b.max_x_mm + offset.x_mm.max(0.),
                b.max_y_mm + offset.y_mm.max(0.),
            );
            painter.rect_stroke(
                egui::Rect::from_two_pos(
                    self.camera.screen(min, rect),
                    self.camera.screen(max, rect),
                ),
                0.,
                stroke,
                egui::StrokeKind::Inside,
            );
            return;
        }
        for cell in 1..preview.estimate.cell_count {
            let Ok(offset) = spec.offset(cell) else {
                continue;
            };
            let screen = |p: MmPoint| {
                self.camera.screen(
                    MmPoint::new(p.x_mm + offset.x_mm, p.y_mm + offset.y_mm),
                    rect,
                )
            };
            for path in preview.paths.iter() {
                painter.add(egui::Shape::line(
                    path.iter().copied().map(screen).collect(),
                    stroke,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_ui::fixtures;
    use editor_core::command::{CommandDispatcher, ids};
    fn model() -> Model {
        let mut model = Model::default();
        model.run(Action::RestoreProject(
            rcam_project::encode_v1(&fixtures::big_project(400, 1)).unwrap(),
        ));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        let definition = model.view.block_definitions[0].id.0.clone();
        model.run(Action::BlockSelect(definition));
        assert_eq!(model.view.selected.ordered.len(), 1);
        model
    }
    #[test]
    fn array_modal_units_preview_cancel_commit_parity_and_selection() {
        let mut model = model();
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.fit = false;
        assert!(app.dispatch(ids::OBJECT_ARRAY_RECTANGULAR));
        assert_eq!(app.modal, Some(ActiveModal::Array));
        let before = model.view.info.clone();
        let original = model.view.selected.clone();
        for unit in [
            crate::tools::DisplayUnit::Millimeter,
            crate::tools::DisplayUnit::Inch,
            crate::tools::DisplayUnit::Mil,
            crate::tools::DisplayUnit::Micrometer,
        ] {
            app.display_unit = unit;
            app.array.rows = "3".into();
            app.array.columns = "4".into();
            app.array.pitch_x = unit.input(-12.7);
            app.array.pitch_y = unit.input(25.4);
            let request = app.array_request().unwrap();
            assert!((request.params.pitch_x_mm + 12.7).abs() < 1e-10);
            assert!((request.params.pitch_y_mm - 25.4).abs() < 1e-10);
            model.array_preview(request.clone()).unwrap();
            let preview = model.view.array_preview.clone().unwrap();
            assert_eq!(model.view.info, before);
            assert_eq!(preview.estimate.created_object_count, 11);
            assert_eq!(model.block_display_cache.stats(), (1, 400));
            app.view = model.view.clone();
            let ctx = egui::Context::default();
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.paint_array(ui.painter(), ui.max_rect(), 2.));
            });
            assert!(!out.shapes.is_empty());
        }
        app.cancel_modal();
        assert_eq!(app.view.info, before);
        assert!(app.view.array_preview.is_none());
        assert_eq!(app.modal, None);
        // A worker reply arriving after Escape must not restore a cancelled preview.
        app.view.array_preview = model.view.array_preview.clone();
        app.accept_array_reply();
        assert!(app.view.array_preview.is_none());
        app.open_array();
        app.array.rows = "3".into();
        app.array.columns = "4".into();
        let request = app.array_request().unwrap();
        model.array_preview(request.clone()).unwrap();
        let preview = model.view.array_preview.clone().unwrap();
        let source = original.primary().unwrap().object.geometry.clone();
        model.array_apply(request.clone()).unwrap();
        assert_eq!(model.view.selected.ordered.len(), 1);
        assert_eq!(
            model.view.selected.primary().unwrap().object.object_id,
            original.primary().unwrap().object.object_id
        );
        let snapshot = model.view.snap_snapshot.as_ref().unwrap();
        let SemanticGeometry::BlockInstance {
            transform: before, ..
        } = source
        else {
            panic!()
        };
        for (cell, o) in snapshot.layers[0].objects.iter().enumerate() {
            let SemanticGeometry::BlockInstance { transform, .. } = o.geometry else {
                panic!()
            };
            let offset = preview.request.spec().offset(cell).unwrap();
            assert_eq!(
                transform.translation,
                MmPoint::new(
                    before.translation.x_mm + offset.x_mm,
                    before.translation.y_mm + offset.y_mm
                )
            );
        }
        assert!(model.array_apply(request).is_err());
    }
    #[test]
    fn array_large_preview_simplifies_without_changing_commit_capability_and_reuses_paths() {
        let mut model = model();
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.modal = Some(ActiveModal::Array);
        app.array.rows = "10".into();
        app.array.columns = "10".into();
        let request = app.array_request().unwrap();
        model.array_preview(request.clone()).unwrap();
        let first = model.view.array_preview.clone().unwrap();
        let mut next = request;
        next.params.rows = 100;
        next.params.columns = 100;
        model.array_preview(next).unwrap();
        let second = model.view.array_preview.as_ref().unwrap();
        assert!(second.simplified);
        assert_eq!(second.estimate.created_object_count, 9999);
        assert!(Arc::ptr_eq(&first.paths, &second.paths));
        assert_eq!(model.view.info.as_ref().unwrap().undo_entries, 0);
    }
    #[test]
    fn array_preview_bounds_expansion_before_resolving_many_block_instances() {
        let mut model = Model::default();
        model.run(Action::RestoreProject(
            rcam_project::encode_v1(&fixtures::big_project(400, 100)).unwrap(),
        ));
        let definition = model.view.block_definitions[0].id.0.clone();
        model.run(Action::BlockSelect(definition));
        assert_eq!(model.view.selected.ordered.len(), 100);
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.modal = Some(ActiveModal::Array);
        app.array.rows = "1".into();
        app.array.columns = "2".into();
        let request = app.array_request().unwrap();
        let before = model.block_display_cache.stats();
        model.array_preview(request).unwrap();
        let preview = model.view.array_preview.as_ref().unwrap();
        assert!(preview.simplified);
        assert!(preview.paths.is_empty());
        assert_eq!(preview.estimate.created_object_count, 100);
        assert_eq!(model.block_display_cache.stats(), before);
    }
    #[test]
    #[ignore = "release preview timing with 400 shared Block primitives"]
    fn performance_array_preview_shared_block_400_10x10() {
        let mut model = model();
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.modal = Some(ActiveModal::Array);
        app.array.rows = "10".into();
        app.array.columns = "10".into();
        let request = app.array_request().unwrap();
        model.array_preview(request.clone()).unwrap();
        let build = model.view.array_preview.as_ref().unwrap().build_us;
        app.view = model.view.clone();
        let ctx = egui::Context::default();
        let mut times = vec![];
        for _ in 0..60 {
            let t = std::time::Instant::now();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.paint_array(ui.painter(), ui.max_rect(), 2.));
                },
            );
            times.push(t.elapsed().as_micros());
        }
        times.sort_unstable();
        let mut update = request;
        update.params.pitch_x_mm = 11.;
        model.array_preview(update).unwrap();
        println!(
            "S4C5_PREVIEW build_us={build} update_us={} paint_p50_us={} paint_p95_us={} simplified={} cache={:?}",
            model.view.array_preview.as_ref().unwrap().build_us,
            times[30],
            times[57],
            app.view.array_preview.as_ref().unwrap().simplified,
            model.block_display_cache.stats()
        );
    }
    #[test]
    fn array_inputs_count_validation_and_pending_keep_real_layout_stable() {
        let view = model().view;
        for viewport in [egui::vec2(980., 760.), egui::vec2(320., 420.)] {
            let ctx = egui::Context::default();
            let mut app = crate::modal::tests::app();
            app.view = view.clone();
            app.fit = false;
            app.modal = Some(ActiveModal::Array);
            let mut baseline = None;
            for (rows, columns, pitch) in [
                ("2", "2", "10"),
                ("1", "500000", "10"),
                ("1", "500001", "10"),
                ("", "2", "10"),
                ("999999999999999999999999999999", "2", "10"),
                ("2", "2", "-"),
                ("2", "2", "0"),
                ("2", "2", ".3"),
            ] {
                app.array.rows = rows.into();
                app.array.columns = columns.into();
                app.array.pitch_x = pitch.into();
                app.array.requested = app.array_request().ok(); // No worker in layout-only test.
                let mut measured = None;
                for _ in 0..3 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            let response = crate::ui::modal_widgets::fixed_modal(
                                ctx,
                                egui::Id::new("array-layout-regression"),
                                egui::vec2(440., 660.),
                                |ui| app.array_modal(ui),
                            );
                            let fields = (0usize..4)
                                .map(|index| {
                                    ctx.read_response(egui::Id::new(("array-input", index)))
                                        .unwrap()
                                        .rect
                                })
                                .collect::<Vec<_>>();
                            measured = Some((response.response.rect, fields, response.inner.rect));
                        },
                    );
                }
                let measured = measured.unwrap();
                if let Some(previous) = &baseline {
                    assert_eq!(
                        &measured, previous,
                        "viewport={viewport:?} rows={rows} columns={columns} pitch={pitch}"
                    );
                } else {
                    baseline = Some(measured);
                }
            }
        }
    }

    #[test]
    fn large_array_confirmation_is_request_bound_and_preview_work_is_bounded() {
        let mut model = model();
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.fit = false;
        app.modal = Some(ActiveModal::Array);
        let small = app.array_request().unwrap();
        model.array_preview(small).unwrap();
        let base = model.view.array_preview.as_ref().unwrap().as_ref().clone();
        for cells in [500_000, 500_001] {
            app.array.rows = "1".into();
            app.array.columns = cells.to_string();
            let request = app.array_request().unwrap();
            let mut preview = base.clone();
            preview.request = request.clone();
            preview.estimate.cell_count = cells;
            preview.simplified = true;
            // Readiness fixture isolates UI consent from the separate real resource preflight.
            app.view.array_preview = Some(Arc::new(preview));
            app.array.confirmed = None;
            assert_eq!(app.array_can_apply(&request), cells == 500_000);
            app.array.confirmed = Some(request.clone());
            assert!(app.array_can_apply(&request));
            let ctx = egui::Context::default();
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.paint_array(ui.painter(), ui.max_rect(), 2.));
            });
            assert!(
                out.shapes.len() < 20,
                "large preview must not emit a shape per cell"
            );
            app.array.columns = (cells + 1).to_string();
            let changed = app.array_request().unwrap();
            let mut preview = base.clone();
            preview.request = changed.clone();
            app.view.array_preview = Some(Arc::new(preview));
            assert!(!app.array_can_apply(&changed));
        }
        app.cancel_modal();
        assert!(app.array.confirmed.is_none());
        assert!(app.view.array_preview.is_none());
    }
    #[test]
    fn new_array_request_clears_previous_resource_demand_while_pending() {
        let m = model();
        let mut app = crate::modal::tests::app();
        app.view = m.view.clone();
        app.modal = Some(ActiveModal::Array);
        let (tx, _requests) = std::sync::mpsc::sync_channel(8);
        app.tx = tx;
        app.array.requested = Some(app.array_request().unwrap());
        app.array.columns = "3".into();
        let current = app.array_request().unwrap();
        app.view.error = Some(ServiceError {
            code: "RESOURCE_LIMIT".into(),
            message: "old request".into(),
            details: serde_json::json!({"required_history_bytes":999999999,"max_history_bytes":67108864}),
        });
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.array_modal(ui);
            });
        });
        assert!(app.view.error.is_none());
        assert!(app.array.requested.as_ref() == Some(&current));
        assert!(app.busy);
        assert!(!app.array_can_apply(&current));
    }
}
