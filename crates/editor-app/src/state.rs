use crate::display::Scene;
use editor_core::{BoundsMm, MmPoint};
use editor_service::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Default)]
pub struct View {
    pub info: Option<DocumentInfo>,
    pub layers: Vec<LayerInfo>,
    pub apertures: Vec<editor_core::ApertureDefinition>,
    pub selected: Option<ObjectInfo>,
    pub bounds: Option<BoundsMm>,
    pub scene: Option<Arc<Scene>>,
    pub blocked: Option<String>,
    pub error: Option<ServiceError>,
    pub message: String,
    pub render_ppm: f64,
    pub drag_hit: bool,
}
pub struct Model {
    pub service: ApplicationService,
    pub view: View,
    snapshot: Option<RenderSnapshot>,
    serial: u64,
    pub ppm: f64,
}
pub enum Action {
    Open(PathBuf),
    Select(MmPoint, f64),
    Move(String, String),
    ProbeDrag(MmPoint, f64),
    DragMove(crate::drag::Drag),
    Duplicate,
    Delete,
    History(bool),
    Layer(LayerUpdateParams),
    Save(PathBuf, String, Option<Vec<String>>),
    Rebuild(f64),
    Close(bool),
}
impl Default for Model {
    fn default() -> Self {
        Self {
            service: ApplicationService::new(),
            view: View::default(),
            snapshot: None,
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
        if let Some(o) = &self.view.selected {
            self.view.selected = if self
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
                    Ok(o) => Some(o),
                    Err(e) if e.code == "NOT_FOUND" => None,
                    Err(e) => return Err(e),
                }
            } else {
                None
            };
        }
        if geometry {
            let snapshot = self.service.render_snapshot(&id)?;
            self.view.apertures = snapshot.apertures.clone();
            self.snapshot = Some(snapshot);
        }
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
        match Scene::build(snapshot, &self.view.layers, anchor, self.ppm, self.serial) {
            Ok(scene) => {
                self.view.scene = Some(Arc::new(scene));
                self.view.blocked = None;
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
    pub fn select(&mut self, point: MmPoint, tolerance: f64) -> Result<(), ServiceError> {
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
        self.view.selected = selected;
        Ok(())
    }
    pub fn numeric_move(&mut self, dx: &str, dy: &str) -> Result<(), ServiceError> {
        self.editable()?;
        let parse = |s: &str| {
            s.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| error("INVALID_ARGUMENT", "请输入有限的毫米数值"))
        };
        let (dx, dy) = (parse(dx)?, parse(dy)?);
        if dx == 0. && dy == 0. {
            self.view.message = "位移为零，未提交修改".into();
            return Ok(());
        }
        let d = self.info()?;
        let selected = self
            .view
            .selected
            .as_ref()
            .ok_or_else(|| error("NOT_FOUND", "请先选择一个对象"))?;
        self.service.objects_move(
            &d.document_id,
            &d.revision,
            MoveParams {
                layer_id: selected.layer_id.clone(),
                object_ids: vec![selected.object.object_id.clone()],
                dx_mm: dx,
                dy_mm: dy,
            },
        )?;
        self.view.message = format!("已移动 ΔX {dx} / ΔY {dy} mm");
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
    pub fn run(&mut self, action: Action) {
        self.view.error = None;
        let result = (|| match action {
            Action::Open(path) => self.open(&path),
            Action::Select(p, t) => self.select(p, t),
            Action::Move(dx, dy) => self.numeric_move(&dx, &dy),
            Action::ProbeDrag(p, tolerance_mm) => {
                self.view.drag_hit = false;
                self.editable()?;
                let d = self.info()?;
                if crate::drag::editable_selection(&self.view) {
                    let o = self.view.selected.as_ref().unwrap();
                    self.view.drag_hit = self
                        .service
                        .objects_hit_test(
                            &d.document_id,
                            HitTestParams {
                                layer_id: o.layer_id.clone(),
                                point: HitTestPoint {
                                    x_mm: p.x_mm,
                                    y_mm: p.y_mm,
                                },
                                tolerance_mm,
                            },
                        )?
                        .object_ids
                        .contains(&o.object.object_id);
                    if !self.view.drag_hit {
                        self.select(p, tolerance_mm)?;
                    }
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
                        object_ids: vec![drag.object],
                        dx_mm: drag.delta.x_mm,
                        dy_mm: drag.delta.y_mm,
                    },
                )?;
                self.view.message =
                    format!("已拖动 ΔX {} / ΔY {} mm", drag.delta.x_mm, drag.delta.y_mm);
                self.refresh(true)
            }
            Action::Duplicate | Action::Delete => {
                self.editable()?;
                let d = self.info()?;
                let o = self
                    .view
                    .selected
                    .clone()
                    .ok_or_else(|| error("NOT_FOUND", "请先选择一个对象"))?;
                if matches!(action, Action::Duplicate) {
                    let result = self.service.objects_duplicate(
                        &d.document_id,
                        &d.revision,
                        DuplicateParams {
                            layer_id: o.layer_id.clone(),
                            object_ids: vec![o.object.object_id],
                            dx_mm: 0.,
                            dy_mm: 0.,
                        },
                    )?;
                    self.view.selected = Some(self.service.objects_get(
                        &d.document_id,
                        ObjectParams {
                            layer_id: o.layer_id,
                            object_id: result.changed_object_ids[0].clone(),
                        },
                    )?);
                    self.view.message = "已原位复制，可拖动副本".into();
                } else {
                    self.service.objects_delete(
                        &d.document_id,
                        &d.revision,
                        DeleteParams {
                            layer_id: o.layer_id,
                            object_ids: vec![o.object.object_id],
                        },
                    )?;
                    self.view.selected = None;
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
                Ok(())
            }
        })();
        if let Err(e) = result {
            eprintln!("service_error {} {} {}", e.code, e.message, e.details);
            self.view.error = Some(e);
        }
        if let Some(d) = &self.view.info {
            eprintln!(
                "state document={} revision={} workspace={} dirty={} undo={} redo={} selected={:?} saved={:?}",
                d.document_id,
                d.revision,
                d.workspace_revision,
                d.dirty,
                d.undo_entries,
                d.redo_entries,
                self.view.selected.as_ref().map(|o| &o.object.object_id),
                d.last_saved_path
            );
        }
    }
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
