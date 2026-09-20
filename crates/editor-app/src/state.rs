use crate::display::Scene;
use editor_core::{BoundsMm, MmPoint};
use editor_service::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Default)]
pub struct View {
    pub text_reply: Option<Arc<crate::text_tool::Reply>>,
    pub metrics: Vec<MetricsItem>,
    pub metrics_error: Option<String>,
    pub info: Option<DocumentInfo>,
    pub layers: Vec<LayerInfo>,
    pub apertures: Vec<editor_core::ApertureDefinition>,
    pub snap_points: Vec<crate::tools::SnapPoint>,
    pub selected: crate::selection::SelectionSet,
    pub bounds: Option<BoundsMm>,
    pub scene: Option<Arc<Scene>>,
    pub blocked: Option<String>,
    pub error: Option<ServiceError>,
    pub message: String,
    pub render_ppm: f64,
    pub render_viewport: Option<BoundsMm>,
    pub display_transient: Option<String>,
    pub drag_hit: bool,
    pub press_hit: Option<ObjectInfo>,
}
pub struct Model {
    pub service: ApplicationService,
    pub view: View,
    snapshot: Option<RenderSnapshot>,
    metrics_identity: String,
    world_index: crate::world_index::WorldIndex,
    viewport: Option<(MmPoint, BoundsMm)>,
    serial: u64,
    pub ppm: f64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PivotInput {
    SelectionCenter,
    WorldOrigin,
    Custom(String, String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorDirection {
    Horizontal,
    Vertical,
}
pub enum Action {
    Precision(ManufacturingPrecision),
    Open(PathBuf),
    TextFont(u64, PathBuf, u32),
    FontCatalog,
    SystemFont(u64, PathBuf, String),
    TextPreview(crate::text_tool::Request),
    TextCreate(crate::text_tool::Request),
    Select(MmPoint, f64, crate::selection::SelectionMode),
    SelectRect(BoundsMm, editor_core::hit_test::SelectRectMode),
    Move(String, String),
    SetFlashSize(String, Option<String>),
    Rotate(String, PivotInput),
    Mirror(MirrorDirection),
    ProbeDrag(MmPoint, f64),
    DragMove(Box<crate::drag::Drag>),
    Duplicate,
    Delete,
    History(bool),
    Layer(LayerUpdateParams),
    Save(PathBuf, String, Option<Vec<String>>),
    #[cfg(test)]
    Rebuild(f64),
    Viewport(MmPoint, BoundsMm, f64),
    Close(bool),
}
impl Default for Model {
    fn default() -> Self {
        Self {
            service: ApplicationService::new(),
            view: View::default(),
            snapshot: None,
            metrics_identity: String::new(),
            world_index: Default::default(),
            viewport: None,
            serial: 0,
            ppm: 20.,
        }
    }
}
fn error(code: &str, message: &str) -> ServiceError {
    ServiceError {
        code: code.into(),
        message: message.into(),
        details: serde_json::json!({}),
    }
}
fn finite(value: &str, name: &str) -> Result<f64, ServiceError> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| error("INVALID_ARGUMENT", &format!("{name} 必须是有限数值")))
}

pub fn selected_bounds(view: &View) -> Result<BoundsMm, ServiceError> {
    editor_core::geometries_bounds(
        view.selected
            .ordered
            .iter()
            .map(|object| &object.object.geometry),
        &view.apertures,
    )
    .map_err(|cause| error("VALIDATION_FAILED", &format!("选择集制造边界无效：{cause}")))?
    .ok_or_else(|| error("INVALID_ARGUMENT", "选择集没有可用的制造边界"))
}

pub fn selected_center(view: &View) -> Result<MmPoint, ServiceError> {
    Ok(selected_bounds(view)?.center())
}

impl Model {
    fn info(&self) -> Result<DocumentInfo, ServiceError> {
        self.view
            .info
            .clone()
            .ok_or_else(|| error("NOT_FOUND", "尚未打开文件"))
    }
    fn editable(&self) -> Result<(), ServiceError> {
        if self.view.blocked.is_some() || self.view.scene.is_none() {
            Err(error("VALIDATION_FAILED", "无法安全显示，编辑和导出已停止"))
        } else {
            Ok(())
        }
    }
    pub fn open(&mut self, path: &Path) -> Result<(), ServiceError> {
        if self.view.info.as_ref().is_some_and(|d| d.dirty) {
            return Err(error(
                "CONFIRMATION_REQUIRED",
                "当前文件有未保存修改。请先另存为，或通过文件菜单关闭并明确放弃修改。",
            ));
        }
        self.service.grant_file_access(path, false)?;
        let candidate = self.service.open(
            path.to_str()
                .ok_or_else(|| error("INVALID_ARGUMENT", "路径编码无效"))?,
        )?;
        let old_view = self.view.clone();
        let old_snapshot = self.snapshot.take();
        let old_ppm = self.ppm;
        let old_viewport = self.viewport.take();
        let old_index = std::mem::take(&mut self.world_index);
        self.ppm = 20.;
        self.view = View {
            info: Some(candidate),
            message: "文件已打开".into(),
            ..Default::default()
        };
        let prepared = self.refresh(true).and_then(|()| match &self.view.blocked {
            Some(reason) => Err(error("UNSUPPORTED_FEATURE", reason)),
            None => Ok(()),
        });
        if let Err(e) = prepared {
            if let Some(candidate) = &self.view.info {
                self.service
                    .close(&candidate.document_id, &candidate.revision, false)?;
            }
            self.view = old_view;
            self.snapshot = old_snapshot;
            self.ppm = old_ppm;
            self.viewport = old_viewport;
            self.world_index = old_index;
            return Err(e);
        }
        if let Some(old) = old_view.info {
            self.service.close(&old.document_id, &old.revision, false)?;
        }
        Ok(())
    }
    fn refresh(&mut self, geometry: bool) -> Result<(), ServiceError> {
        let id = self.info()?.document_id;
        self.view.info = Some(self.service.document_get(&id)?);
        self.view.layers = self.service.layers_list(&id)?;
        let mut bounds: Option<BoundsMm> = None;
        for l in &self.view.layers {
            if l.visible
                && let Some(b) = self
                    .service
                    .layer_bounds(
                        &id,
                        LayerBoundsParams {
                            layer_id: l.layer_id.clone(),
                        },
                    )?
                    .bounds
            {
                bounds = Some(match bounds {
                    None => b,
                    Some(a) => BoundsMm {
                        min_x_mm: a.min_x_mm.min(b.min_x_mm),
                        min_y_mm: a.min_y_mm.min(b.min_y_mm),
                        max_x_mm: a.max_x_mm.max(b.max_x_mm),
                        max_y_mm: a.max_y_mm.max(b.max_y_mm),
                    },
                });
            }
        }
        self.view.bounds = bounds;
        let mut selected = Vec::new();
        for o in &self.view.selected.ordered {
            if self
                .view
                .layers
                .iter()
                .any(|l| l.layer_id == o.layer_id && l.visible)
            {
                match self.service.objects_get(
                    &id,
                    ObjectParams {
                        layer_id: o.layer_id.clone(),
                        object_id: o.object.object_id.clone(),
                    },
                ) {
                    Ok(o) => selected.push(o),
                    Err(e) if e.code == "NOT_FOUND" => {}
                    Err(e) => return Err(e),
                }
            }
        }
        self.view.selected.ordered = selected;
        if geometry {
            let snapshot = self.service.render_snapshot(&id)?;
            self.view.apertures = snapshot.apertures.clone();
            self.world_index = crate::world_index::WorldIndex::build(&snapshot)
                .map_err(|e| error("VALIDATION_FAILED", &e))?;
            self.snapshot = Some(snapshot);
        }
        self.view.snap_points = self.snapshot.as_ref().map_or_else(Vec::new, |snapshot| {
            snap_points(snapshot, &self.view.layers)
        });
        self.rebuild();
        Ok(())
    }
    fn rebuild(&mut self) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        self.serial += 1;
        self.view.render_ppm = self.ppm;
        let anchor = self.view.bounds.map_or(MmPoint::new(0., 0.), |b| {
            MmPoint::new(
                b.min_x_mm + (b.max_x_mm - b.min_x_mm) / 2.,
                b.min_y_mm + (b.max_y_mm - b.min_y_mm) / 2.,
            )
        });
        let anchor = self.viewport.map_or(anchor, |(origin, _)| origin);
        let filtered = self
            .viewport
            .map(|(_, bounds)| self.world_index.query(snapshot, &self.view.layers, bounds));
        match Scene::build_cached(
            filtered.as_ref().unwrap_or(snapshot),
            &self.view.layers,
            anchor,
            self.ppm,
            self.serial,
            self.view.scene.as_deref(),
        ) {
            Ok(scene) => {
                self.view.scene = Some(Arc::new(scene));
                self.view.blocked = None;
                self.view.display_transient = None;
                self.view.render_viewport = self.viewport.map(|(_, b)| b);
            }
            Err(e) if e.starts_with("DISPLAY_PRECISION:") => {
                self.view.display_transient = Some(e);
                if let Some(scene) = &self.view.scene {
                    self.view.render_ppm = scene.ppm;
                }
            }
            Err(e) => {
                self.view.scene = None;
                self.view.blocked = Some(e);
            }
        }
        eprintln!(
            "display_rebuild={} document={} revision={} ppm={} blocked={:?}",
            self.serial, snapshot.document_id, snapshot.revision, self.ppm, self.view.blocked
        );
    }
    fn hit(&self, point: MmPoint, tolerance: f64) -> Result<Option<ObjectInfo>, ServiceError> {
        self.editable()?;
        let id = self.info()?.document_id;
        let hit = topmost_hit(&self.view.layers, |layer| {
            self.service
                .objects_hit_test(
                    &id,
                    HitTestParams {
                        layer_id: layer.into(),
                        point: HitTestPoint {
                            x_mm: point.x_mm,
                            y_mm: point.y_mm,
                        },
                        tolerance_mm: tolerance,
                    },
                )
                .map(|r| r.object_ids)
        })?;
        let selected = match hit {
            Some((layer_id, object_id)) => Some(self.service.objects_get(
                &id,
                ObjectParams {
                    layer_id,
                    object_id,
                },
            )?),
            None => None,
        };
        Ok(selected)
    }
    pub fn select(
        &mut self,
        point: MmPoint,
        tolerance: f64,
        mode: crate::selection::SelectionMode,
    ) -> Result<(), ServiceError> {
        let hit = self.hit(point, tolerance)?;
        if let Some(hit) = &hit
            && let editor_core::ObjectOrigin::Generated { operation_id } = &hit.object.origin
            && let Some(snapshot) = &self.snapshot
        {
            let ids:Vec<_>=snapshot.layers.iter().filter(|l|l.id==hit.layer_id).flat_map(|l|&l.objects)
                .filter(|o|matches!(&o.origin,editor_core::ObjectOrigin::Generated {operation_id:id} if id==operation_id))
                .map(|o|o.object_id.clone()).collect();
            if mode == crate::selection::SelectionMode::Replace {
                self.view.selected.ordered.clear();
            }
            for id in ids {
                let object = self.service.objects_get(
                    &self.info()?.document_id,
                    ObjectParams {
                        layer_id: hit.layer_id.clone(),
                        object_id: id,
                    },
                )?;
                self.view.selected.click(
                    Some(object),
                    if mode == crate::selection::SelectionMode::Remove {
                        mode
                    } else {
                        crate::selection::SelectionMode::Add
                    },
                );
            }
        } else {
            self.view.selected.click(hit, mode);
        }
        Ok(())
    }
    fn select_rect(
        &mut self,
        rect_mm: BoundsMm,
        mode: editor_core::hit_test::SelectRectMode,
    ) -> Result<(), ServiceError> {
        self.editable()?;
        let d = self.info()?;
        let mut selected = Vec::new();
        for l in self.view.layers.iter().filter(|l| l.visible) {
            let result = self.service.objects_select_rect(
                &d.document_id,
                SelectRectParams {
                    layer_id: l.layer_id.clone(),
                    rect_mm,
                    mode,
                },
            )?;
            for object_id in result.object_ids {
                selected.push(self.service.objects_get(
                    &d.document_id,
                    ObjectParams {
                        layer_id: l.layer_id.clone(),
                        object_id,
                    },
                )?);
            }
        }
        self.view.selected.ordered = selected;
        Ok(())
    }
    fn edit_targets(&self) -> Result<(String, Vec<String>), ServiceError> {
        let primary = self
            .view
            .selected
            .primary()
            .ok_or_else(|| error("NOT_FOUND", "请先选择对象"))?;
        for o in &self.view.selected.ordered {
            let layer = self
                .view
                .layers
                .iter()
                .find(|l| l.layer_id == o.layer_id)
                .ok_or_else(|| error("NOT_FOUND", "图层不存在"))?;
            if layer.locked {
                return Err(error("LAYER_LOCKED", "选择包含锁定层，整组操作已拒绝"));
            }
            if !layer.visible {
                return Err(error("INVALID_ARGUMENT", "选择包含隐藏层，整组操作已拒绝"));
            }
            if o.layer_id != primary.layer_id {
                return Err(error(
                    "UNSUPPORTED_FEATURE",
                    "本阶段只支持同层多对象编辑；跨层选择仅供查看，整组操作已拒绝",
                ));
            }
        }
        Ok((
            primary.layer_id.clone(),
            self.view
                .selected
                .ordered
                .iter()
                .map(|o| o.object.object_id.clone())
                .collect(),
        ))
    }
    pub fn numeric_move(&mut self, dx: &str, dy: &str) -> Result<(), ServiceError> {
        self.editable()?;
        let (dx, dy) = (finite(dx, "ΔX")?, finite(dy, "ΔY")?);
        if dx == 0. && dy == 0. {
            self.view.message = "位移为零，未提交修改".into();
            return Ok(());
        }
        let d = self.info()?;
        let (layer_id, object_ids) = self.edit_targets()?;
        self.service.objects_move(
            &d.document_id,
            &d.revision,
            MoveParams {
                layer_id,
                object_ids,
                dx_mm: dx,
                dy_mm: dy,
            },
        )?;
        self.view.message = "已移动所选对象".into();
        self.refresh(true)
    }
    pub fn numeric_rotate(&mut self, angle: &str, pivot: PivotInput) -> Result<(), ServiceError> {
        self.editable()?;
        let angle_deg = finite(angle, "旋转角度")?;
        let (layer_id, object_ids) = self.edit_targets()?;
        let pivot = match pivot {
            PivotInput::SelectionCenter => selected_center(&self.view)?,
            PivotInput::WorldOrigin => MmPoint::new(0., 0.),
            PivotInput::Custom(x, y) => {
                MmPoint::new(finite(&x, "Pivot X")?, finite(&y, "Pivot Y")?)
            }
        };
        let document = self.info()?;
        self.service.objects_rotate(
            &document.document_id,
            &document.revision,
            RotateParams {
                layer_id,
                object_ids,
                angle_deg,
                pivot_mm: PivotMm {
                    x_mm: pivot.x_mm,
                    y_mm: pivot.y_mm,
                },
            },
        )?;
        self.view.message = format!("已旋转 {angle_deg}°");
        self.refresh(true)
    }
    pub fn set_flash_size(
        &mut self,
        width: &str,
        height: Option<&str>,
    ) -> Result<(), ServiceError> {
        self.editable()?;
        let (layer_id, object_ids) = self.edit_targets()?;
        let document = self.info()?;
        self.service.objects_set_properties(
            &document.document_id,
            &document.revision,
            SetPropertiesParams {
                layer_id,
                object_ids,
                width_mm: finite(width, "宽度/直径")?,
                height_mm: height.map(|value| finite(value, "高度")).transpose()?,
            },
        )?;
        self.view.message = "已通过写时复制修改 Flash 尺寸".into();
        self.refresh(true)
    }
    pub fn mirror_selection(&mut self, direction: MirrorDirection) -> Result<(), ServiceError> {
        self.editable()?;
        let (layer_id, object_ids) = self.edit_targets()?;
        let center = selected_center(&self.view)?;
        let (axis, label) = match direction {
            MirrorDirection::Horizontal => (
                MirrorAxis::Horizontal {
                    coordinate_mm: center.y_mm,
                },
                "水平轴",
            ),
            MirrorDirection::Vertical => (
                MirrorAxis::Vertical {
                    coordinate_mm: center.x_mm,
                },
                "垂直轴",
            ),
        };
        let document = self.info()?;
        self.service.objects_mirror(
            &document.document_id,
            &document.revision,
            MirrorParams {
                layer_id,
                object_ids,
                axis,
            },
        )?;
        self.view.message = format!("已关于{label}镜像");
        self.refresh(true)
    }
    pub fn save(
        &mut self,
        path: &Path,
        layer: String,
        categories: Option<Vec<String>>,
    ) -> Result<(), ServiceError> {
        self.editable()?;
        let d = self.info()?;
        self.service.grant_file_access(
            path.parent()
                .ok_or_else(|| error("INVALID_ARGUMENT", "缺少输出目录"))?,
            true,
        )?;
        let metadata_policy = match categories {
            None => MetadataPolicy {
                mode: "require_confirmation".into(),
                categories: None,
            },
            Some(c) => MetadataPolicy {
                mode: "drop_listed".into(),
                categories: Some(c),
            },
        };
        let result = self.service.export_layer(
            &d.document_id,
            &d.revision,
            ExportParams {
                layer_id: layer.clone(),
                path: path.to_string_lossy().into(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy,
            },
        );
        match result {
            Ok(r) => {
                self.view.info = Some(self.service.document_get(&d.document_id)?);
                self.view.message = format!("已另存为 {}", r.path);
                Ok(())
            }
            Err(mut e) => {
                e.details["gui_layer_id"] = serde_json::json!(layer);
                e.details["gui_target_path"] = serde_json::json!(path);
                e.details["gui_document_id"] = serde_json::json!(d.document_id);
                e.details["gui_revision"] = serde_json::json!(d.revision);
                Err(e)
            }
        }
    }
    fn refresh_metrics(&mut self) {
        let identity = format!(
            "{:?}:{:?}",
            self.view
                .info
                .as_ref()
                .map(|d| (&d.document_id, &d.revision)),
            self.view.selected.ids()
        );
        if self.metrics_identity == identity {
            return;
        }
        self.metrics_identity = identity;
        self.view.metrics.clear();
        self.view.metrics_error = None;
        let Some(d) = &self.view.info else {
            return;
        };
        let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
        for o in &self.view.selected.ordered {
            groups
                .entry(o.layer_id.clone())
                .or_default()
                .push(o.object.object_id.clone());
        }
        for (layer_id, object_ids) in groups {
            match self.service.objects_metrics(
                &d.document_id,
                MetricsParams {
                    layer_id,
                    object_ids,
                },
            ) {
                Ok(result) => self.view.metrics.extend(result.items),
                Err(e) => {
                    self.view.metrics.clear();
                    self.view.metrics_error = Some(format!("{}: {}", e.code, e.message));
                    break;
                }
            }
        }
    }
    pub fn run(&mut self, action: Action) {
        self.view.error = None;
        self.view.text_reply = None;
        let result = (|| match action {
            Action::Open(path) => self.open(&path),
            Action::FontCatalog => {
                self.view.text_reply = Some(Arc::new(crate::text_tool::Reply::Catalog(
                    crate::font_catalog::installed_fonts().map(Arc::new),
                )));
                Ok(())
            }
            Action::SystemFont(generation, path, postscript) => {
                let result = self.service.grant_file_access(&path, false).and_then(|()| {
                    self.service
                        .font_inspect_named(&path.to_string_lossy(), &postscript)
                });
                self.view.text_reply = Some(Arc::new(crate::text_tool::Reply::Font {
                    generation,
                    result,
                }));
                Ok(())
            }
            Action::TextFont(generation, path, face) => {
                let result = self
                    .service
                    .grant_file_access(&path, false)
                    .and_then(|()| self.service.font_inspect(&path.to_string_lossy(), face));
                self.view.text_reply = Some(Arc::new(crate::text_tool::Reply::Font {
                    generation,
                    result,
                }));
                Ok(())
            }
            Action::TextPreview(request) => {
                let start = std::time::Instant::now();
                let result = self
                    .service
                    .text_preview(&request.document, &request.revision, request.params.clone())
                    .map(Arc::new);
                self.view.text_reply = Some(Arc::new(crate::text_tool::Reply::Preview {
                    request: Box::new(request),
                    result,
                    finished: std::time::Instant::now(),
                    worker_ms: start.elapsed().as_secs_f64() * 1000.,
                }));
                Ok(())
            }
            Action::TextCreate(request) => {
                self.editable()?;
                let result = self.service.text_create(
                    &request.document,
                    &request.revision,
                    request.params.clone(),
                )?;
                self.view.selected.ordered = result
                    .generated_object_ids
                    .into_iter()
                    .map(|id| {
                        self.service.objects_get(
                            &request.document,
                            ObjectParams {
                                layer_id: request.params.layer_id.clone(),
                                object_id: id,
                            },
                        )
                    })
                    .collect::<Result<_, _>>()?;
                self.view.message =
                    "文字已创建并整组选中；普通 Gerber 不保存文字原文/字体组".into();
                self.refresh(true)
            }
            Action::Select(p, t, mode) => self.select(p, t, mode),
            Action::SelectRect(r, m) => self.select_rect(r, m),
            Action::Move(dx, dy) => self.numeric_move(&dx, &dy),
            Action::SetFlashSize(width, height) => self.set_flash_size(&width, height.as_deref()),
            Action::Rotate(angle, pivot) => self.numeric_rotate(&angle, pivot),
            Action::Mirror(direction) => self.mirror_selection(direction),
            Action::Precision(precision) => {
                let d = self
                    .view
                    .info
                    .as_ref()
                    .ok_or_else(|| error("NOT_FOUND", "请先打开文件"))?;
                self.service
                    .set_manufacturing_precision(&d.document_id, &d.revision, precision)?;
                self.view.message = "制造导出策略已更新；现有几何未改变".into();
                self.refresh(false)
            }
            Action::ProbeDrag(p, tolerance_mm) => {
                self.view.drag_hit = false;
                self.view.press_hit = self.hit(p, tolerance_mm)?;
                if crate::drag::editable_selection(&self.view) {
                    let d = self.info()?;
                    let layer_id = self.view.selected.primary().unwrap().layer_id.clone();
                    let hits = self.service.objects_hit_test(
                        &d.document_id,
                        HitTestParams {
                            layer_id: layer_id.clone(),
                            point: HitTestPoint {
                                x_mm: p.x_mm,
                                y_mm: p.y_mm,
                            },
                            tolerance_mm,
                        },
                    )?;
                    self.view.drag_hit = hits
                        .object_ids
                        .iter()
                        .any(|id| self.view.selected.contains(&layer_id, id));
                }
                Ok(())
            }
            Action::DragMove(drag) => {
                self.editable()?;
                self.service.objects_move(
                    &drag.document,
                    &drag.revision,
                    MoveParams {
                        layer_id: drag.layer,
                        object_ids: drag.objects,
                        dx_mm: drag.delta.x_mm,
                        dy_mm: drag.delta.y_mm,
                    },
                )?;
                self.view.message = "已拖动所选对象".into();
                self.refresh(true)
            }
            Action::Duplicate | Action::Delete => {
                self.editable()?;
                let d = self.info()?;
                let (layer_id, object_ids) = self.edit_targets()?;
                if matches!(action, Action::Duplicate) {
                    let result = self.service.objects_duplicate(
                        &d.document_id,
                        &d.revision,
                        DuplicateParams {
                            layer_id: layer_id.clone(),
                            object_ids,
                            dx_mm: 0.,
                            dy_mm: 0.,
                        },
                    )?;
                    let mut selected = Vec::new();
                    for object_id in result.changed_object_ids {
                        selected.push(self.service.objects_get(
                            &d.document_id,
                            ObjectParams {
                                layer_id: layer_id.clone(),
                                object_id,
                            },
                        )?);
                    }
                    self.view.selected.ordered = selected;
                    self.view.message = "已原位复制，可拖动副本".into();
                } else {
                    self.service.objects_delete(
                        &d.document_id,
                        &d.revision,
                        DeleteParams {
                            layer_id,
                            object_ids,
                        },
                    )?;
                    self.view.selected.ordered.clear();
                    self.view.message = "已删除对象".into();
                }
                self.refresh(true)
            }
            Action::History(redo) => {
                let d = self.info()?;
                if redo {
                    self.service.history_redo(&d.document_id, &d.revision)?;
                } else {
                    self.service.history_undo(&d.document_id, &d.revision)?;
                }
                self.view.message = if redo { "已重做" } else { "已撤销" }.into();
                self.refresh(true)
            }
            Action::Layer(p) => {
                let d = self.info()?;
                self.service.layer_update(&d.document_id, &d.revision, p)?;
                self.refresh(false)
            }
            Action::Save(path, layer, c) => self.save(&path, layer, c),
            Action::Viewport(origin, bounds, ppm) => {
                self.viewport = Some((origin, bounds));
                self.ppm = ppm;
                self.rebuild();
                Ok(())
            }
            #[cfg(test)]
            Action::Rebuild(ppm) => {
                self.ppm = ppm;
                self.rebuild();
                Ok(())
            }
            Action::Close(discard) => {
                let d = self.info()?;
                self.service.close(&d.document_id, &d.revision, discard)?;
                self.view = View::default();
                self.snapshot = None;
                self.viewport = None;
                Ok(())
            }
        })();
        if let Err(e) = result {
            eprintln!("service_error {} {} {}", e.code, e.message, e.details);
            self.view.error = Some(e);
        }
        self.refresh_metrics();
        if let Some(d) = &self.view.info {
            eprintln!(
                "state document={} revision={} workspace={} dirty={} undo={} redo={} selected={:?} saved={:?}",
                d.document_id,
                d.revision,
                d.workspace_revision,
                d.dirty,
                d.undo_entries,
                d.redo_entries,
                self.view.selected.ids(),
                d.last_saved_path
            );
        }
    }
}

fn snap_points(snapshot: &RenderSnapshot, layers: &[LayerInfo]) -> Vec<crate::tools::SnapPoint> {
    use crate::tools::{SnapKind, SnapPoint};
    let mut points = Vec::new();
    let mut push = |point, object_id: &str, kind| {
        points.push(SnapPoint {
            point,
            object_id: object_id.into(),
            kind,
        });
    };
    for layer in &snapshot.layers {
        if !layers
            .iter()
            .any(|workspace| workspace.layer_id == layer.id && workspace.visible)
        {
            continue;
        }
        for object in &layer.objects {
            let id = object.object_id.as_str();
            match &object.geometry {
                editor_core::SemanticGeometry::Flash { center, .. } => {
                    push(*center, id, SnapKind::Center)
                }
                editor_core::SemanticGeometry::Line { start, end, .. }
                | editor_core::SemanticGeometry::RectangularSweep { start, end, .. } => {
                    push(*start, id, SnapKind::Endpoint);
                    push(*end, id, SnapKind::Endpoint);
                }
                editor_core::SemanticGeometry::Arc { path, .. } => {
                    push(path.start, id, SnapKind::Endpoint);
                    push(path.end, id, SnapKind::Endpoint);
                    push(path.center, id, SnapKind::Center);
                }
                editor_core::SemanticGeometry::Region { contours } => {
                    for edge in contours.iter().flat_map(|contour| &contour.edges) {
                        match edge {
                            editor_core::RegionEdge::Line { start, end } => {
                                push(*start, id, SnapKind::Endpoint);
                                push(*end, id, SnapKind::Endpoint);
                            }
                            editor_core::RegionEdge::Arc(path) => {
                                push(path.start, id, SnapKind::Endpoint);
                                push(path.end, id, SnapKind::Endpoint);
                                push(path.center, id, SnapKind::Center);
                            }
                        }
                    }
                }
            }
        }
    }
    points
}

/// UI policy only. The closure must call the exact service query.
pub fn topmost_hit(
    layers: &[LayerInfo],
    mut query: impl FnMut(&str) -> Result<Vec<String>, ServiceError>,
) -> Result<Option<(String, String)>, ServiceError> {
    for layer in layers.iter().rev().filter(|l| l.visible) {
        if let Some(id) = query(&layer.layer_id)?.last() {
            return Ok(Some((layer.layer_id.clone(), id.clone())));
        }
    }
    Ok(None)
}
