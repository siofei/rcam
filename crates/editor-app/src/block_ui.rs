//! S4-C3 transient Block workflows. Manufacturing mutations belong to the service.
use crate::{
    EditorApp,
    modal::ActiveModal,
    state::{Action, Classifier, Model, View},
    tools::ActiveTool,
};
use editor_core::command::CommandDispatcher;
use editor_core::{
    MmPoint, RegionEdge, SemanticGeometry,
    block::BlockTransform,
    command::{CommandId, ids},
    workspace::DisplayClass,
};
use editor_service::*;
use eframe::egui::{self, Color32};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub document: String,
    pub revision: String,
    pub workspace: String,
}
impl Context {
    pub fn capture(view: &View) -> Option<Self> {
        view.info.as_ref().map(|i| Self {
            document: i.document_id.clone(),
            revision: i.revision.clone(),
            workspace: i.workspace_revision.clone(),
        })
    }
    pub fn valid(&self, view: &View) -> bool {
        Self::capture(view).as_ref() == Some(self)
    }
}
pub enum Edit {
    Create(CreateBlockDefinitionParams),
    Place(CreateBlockInstanceParams),
    Transform(UpdateBlockInstanceTransformParams),
    Rename(RenameBlockDefinitionParams),
    Explode(ExplodeBlockInstanceParams),
    Delete(BlockDefinitionIdParams),
}
pub struct Request {
    pub context: Context,
    pub edit: Edit,
}
#[derive(Clone)]
pub struct Preview {
    pub context: Context,
    pub definition: String,
    pub ppm: f64,
    pub paths: Vec<Vec<MmPoint>>,
    pub build_us: u64,
}
#[derive(Default)]
pub struct UiState {
    pub library: bool,
    pub definition: Option<String>,
    pub name: String,
    pub x: String,
    pub y: String,
    pub angle: String,
    pub mirror: bool,
    pub context: Option<Context>,
    pub session: Option<Session>,
}
pub struct Session {
    pub context: Context,
    pub layer: String,
    pub kind: SessionKind,
    pub point: Option<MmPoint>,
    pub preview: Option<Arc<Preview>>,
}
pub enum SessionKind {
    Create { name: String, objects: Vec<String> },
    Place { definition: String },
}

pub fn target_ok(layer: &LayerInfo) -> bool {
    layer.visible
        && layer.effective_visible
        && layer.selectable
        && !layer.locked
        && layer
            .classes
            .iter()
            .find(|c| c.class == DisplayClass::BlockInstance)
            .is_none_or(|c| c.visible && c.selectable && !c.locked)
}
fn fail(message: &str) -> ServiceError {
    ServiceError {
        code: "INVALID_ARGUMENT".into(),
        message: message.into(),
        details: serde_json::json!({}),
    }
}
pub fn create_targets(view: &View) -> Result<(String, Vec<String>), String> {
    let first = view.selected.primary().ok_or("请先选择同层普通对象")?;
    let classifier = Classifier::new(&view.layers, &view.apertures);
    if view.selected.ordered.iter().any(|o| {
        o.layer_id != first.layer_id
            || !classifier.selectable(o)
            || classifier.edit_refusal(o).is_some()
    }) {
        return Err("创建 Block 要求同一可编辑图层中的可见、可选对象".into());
    }
    if view
        .selected
        .ordered
        .iter()
        .any(|o| matches!(o.object.geometry, SemanticGeometry::BlockInstance { .. }))
    {
        return Err("不能创建嵌套 Block：选择中包含 Block Instance，请先显式拆解".into());
    }
    let snapshot = view.snap_snapshot.as_ref().ok_or("制造快照不可用")?;
    let layer = snapshot
        .layers
        .iter()
        .find(|l| l.id == first.layer_id)
        .ok_or("图层不存在")?;
    let positions: Vec<_> = layer
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| view.selected.contains(&layer.id, &o.object_id))
        .map(|(i, _)| i)
        .collect();
    if positions.windows(2).any(|p| p[1] != p[0] + 1) {
        return Err(
            "选择在曝光顺序中不连续；为保持 Dark/Clear 覆盖，请包含中间对象后再创建".into(),
        );
    }
    Ok((
        first.layer_id.clone(),
        view.selected
            .ordered
            .iter()
            .map(|o| o.object.object_id.clone())
            .collect(),
    ))
}
impl Session {
    pub fn valid(&self, view: &View) -> bool {
        self.context.valid(view)
            && match &self.kind {
                SessionKind::Create { objects, .. } => {
                    create_targets(view).is_ok_and(|(l, ids)| l == self.layer && ids == *objects)
                }
                SessionKind::Place { definition } => {
                    view.layers
                        .iter()
                        .any(|l| l.layer_id == self.layer && l.is_active && target_ok(l))
                        && view.block_definitions.iter().any(|d| &d.id.0 == definition)
                }
            }
    }
    pub fn request(self) -> Option<Request> {
        let point = self.point?;
        let edit = match self.kind {
            SessionKind::Create { name, objects } => Edit::Create(CreateBlockDefinitionParams {
                layer_id: self.layer,
                object_ids: objects,
                local_origin_mm: PivotMm {
                    x_mm: point.x_mm,
                    y_mm: point.y_mm,
                },
                name,
            }),
            SessionKind::Place { definition } => Edit::Place(CreateBlockInstanceParams {
                layer_id: self.layer,
                definition_id: definition,
                transform: BlockTransformParams {
                    translation_mm: PivotMm {
                        x_mm: point.x_mm,
                        y_mm: point.y_mm,
                    },
                    rotation_deg: 0.,
                    mirror: false,
                },
            }),
        };
        Some(Request {
            context: self.context,
            edit,
        })
    }
}
impl Model {
    pub fn block_edit(&mut self, request: Request) -> Result<(), ServiceError> {
        if !request.context.valid(&self.view) {
            return Err(fail("Block 操作期间工程或工作区已改变，请重试"));
        }
        let Context {
            document, revision, ..
        } = request.context;
        let mut selected = None;
        let (command, definition) = match &request.edit {
            Edit::Create(_) => ("blocks.create_definition", None),
            Edit::Place(p) => ("blocks.create_instance", Some(p.definition_id.clone())),
            Edit::Transform(_) => ("blocks.update_instance_transform", None),
            Edit::Rename(p) => ("blocks.rename_definition", Some(p.definition_id.clone())),
            Edit::Explode(_) => ("blocks.explode_instance", None),
            Edit::Delete(p) => ("blocks.delete_definition", Some(p.definition_id.clone())),
        };
        match request.edit {
            Edit::Create(p) => {
                let layer = p.layer_id.clone();
                let r = self
                    .service
                    .blocks_create_definition_from_objects(&document, &revision, p)?;
                selected = Some((layer, vec![r.instance_object_id]));
            }
            Edit::Place(p) => {
                let layer = p.layer_id.clone();
                let r = self
                    .service
                    .blocks_create_instance(&document, &revision, p)?;
                selected = Some((layer, vec![r.object_id]));
            }
            Edit::Transform(p) => {
                self.service
                    .blocks_update_instance_transform(&document, &revision, p)?;
            }
            Edit::Rename(p) => {
                self.service
                    .blocks_rename_definition(&document, &revision, p)?;
            }
            Edit::Explode(p) => {
                let layer = p.layer_id.clone();
                let removed = p.object_id.clone();
                let r = self
                    .service
                    .blocks_explode_instance(&document, &revision, p)?;
                // EditResult includes removed as well as inserted identities.
                selected = Some((
                    layer,
                    r.changed_object_ids
                        .into_iter()
                        .filter(|id| id != &removed)
                        .collect(),
                ));
            }
            Edit::Delete(p) => {
                self.service
                    .blocks_delete_definition(&document, &revision, p)?;
            }
        }
        if let Some((layer, ids)) = selected {
            self.view.selected.ordered = ids
                .into_iter()
                .map(|object_id| {
                    self.service.objects_get(
                        &document,
                        ObjectParams {
                            layer_id: layer.clone(),
                            object_id,
                        },
                    )
                })
                .collect::<Result<_, _>>()?;
        }
        self.refresh(true)?;
        self.view.message = "Block 操作完成（一次撤销）".into();
        let hash = definition
            .as_ref()
            .map(|id| editor_core::hash::sha256_hex(id.as_bytes()));
        rcam_diagnostics::identified_measurements(
            rcam_diagnostics::Level::Info,
            command,
            hash.as_deref(),
            None,
            &[("definition_count", self.view.block_definitions.len() as u64)],
        );
        Ok(())
    }
    pub fn block_select(&mut self, definition: &str) -> Result<(), ServiceError> {
        let snapshot = self
            .view
            .snap_snapshot
            .as_ref()
            .ok_or_else(|| fail("请先打开工程"))?;
        let classifier = Classifier::new(&self.view.layers, &self.view.apertures);
        self.view.selected.ordered = snapshot.layers.iter().flat_map(|l| l.objects.iter().filter_map(|o| {
            if matches!(&o.geometry, SemanticGeometry::BlockInstance { definition_id, .. } if definition_id.0 == definition) {
                let object = ObjectInfo { layer_id: l.id.clone(), object: o.clone() };
                classifier.selectable(&object).then_some(object)
            } else { None }
        })).collect();
        self.view.message = format!(
            "已选择 {} 个可见可选实例（跨层仅查看）",
            self.view.selected.ordered.len()
        );
        Ok(())
    }
    pub fn block_preview(
        &mut self,
        context: Context,
        definition: String,
        ppm: f64,
    ) -> Result<(), ServiceError> {
        if !context.valid(&self.view) {
            return Err(fail("放置预览已过期"));
        }
        if !ppm.is_finite() || ppm <= 0. {
            return Err(fail("无效显示比例"));
        }
        let start = std::time::Instant::now();
        let def = self
            .view
            .block_definitions
            .iter()
            .find(|d| d.id.0 == definition)
            .ok_or_else(|| fail("Block Definition 不存在"))?;
        let resolved = self
            .block_display_cache
            .resolve(
                def,
                &BlockTransform {
                    translation: MmPoint::new(0., 0.),
                    rotation_deg: 0.,
                    mirror: false,
                },
            )
            .map_err(|e| fail(&e))?;
        let paths = preview_paths(
            resolved.iter().map(|o| &o.geometry),
            &self.view.apertures,
            ppm,
        )?;
        self.view.block_cache_stats = self.block_display_cache.stats();
        self.view.block_preview = Some(Arc::new(Preview {
            context,
            definition,
            ppm,
            paths,
            build_us: start.elapsed().as_micros() as u64,
        }));
        Ok(())
    }
}
/// Shared display-only outlines, built on the worker and reused by Block/Array previews.
pub(crate) fn preview_paths<'a>(
    geometries: impl Iterator<Item = &'a SemanticGeometry>,
    apertures: &[editor_core::ApertureDefinition],
    ppm: f64,
) -> Result<Vec<Vec<MmPoint>>, ServiceError> {
    let mut paths = vec![];
    let mut total = 0usize;
    for geometry in geometries {
        for edge in editor_core::hit_test::display_boundary_edges(geometry, apertures)
            .map_err(|e| fail(&e.to_string()))?
        {
            let path = match edge {
                RegionEdge::Line { start, end } => vec![start, end],
                RegionEdge::Arc(a) => {
                    let sweep = a.sweep_radians().ok_or_else(|| fail("无效圆弧"))?;
                    let step = 4. * (0.25 / (2. * ppm * a.radius())).clamp(0., 1.).sqrt().asin();
                    let n = (sweep / step).ceil().max(1.);
                    if n > 100_000. {
                        return Err(fail("RESOURCE_LIMIT: Block 预览细分超限"));
                    }
                    let angle = (a.start.y_mm - a.center.y_mm).atan2(a.start.x_mm - a.center.x_mm);
                    let sign = if a.direction == editor_core::ArcDirection::Clockwise {
                        -1.
                    } else {
                        1.
                    };
                    (0..=n as usize)
                        .map(|i| {
                            let t = angle + sign * sweep * i as f64 / n;
                            MmPoint::new(
                                a.center.x_mm + a.radius() * t.cos(),
                                a.center.y_mm + a.radius() * t.sin(),
                            )
                        })
                        .collect()
                }
            };
            total += path.len();
            if total > 200_000 {
                return Err(fail("RESOURCE_LIMIT: Block 预览点数超限"));
            }
            paths.push(path);
        }
    }
    Ok(paths)
}
impl EditorApp {
    pub(crate) fn cancel_block(&mut self) {
        if self.block.session.take().is_some() {
            rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, "block.place.cancel");
        }
        if self.tool == ActiveTool::Block {
            self.tool = ActiveTool::Select;
        }
    }
    pub(crate) fn selected_instance(&self) -> Option<(String, String, String, BlockTransform)> {
        if self.view.selected.ordered.len() != 1 {
            return None;
        }
        let o = self.view.selected.primary()?;
        match &o.object.geometry {
            SemanticGeometry::BlockInstance {
                definition_id,
                transform,
            } => Some((
                o.layer_id.clone(),
                o.object.object_id.clone(),
                definition_id.0.clone(),
                *transform,
            )),
            _ => None,
        }
    }
    pub(crate) fn block_command(&mut self, command: CommandId) -> bool {
        if ![
            ids::BLOCK_CREATE,
            ids::BLOCK_PLACE,
            ids::BLOCK_RENAME,
            ids::BLOCK_EXPLODE,
            ids::BLOCK_DELETE,
            ids::BLOCK_SELECT,
            ids::BLOCK_TRANSFORM,
        ]
        .contains(&command)
        {
            return false;
        }
        if self.busy || self.modal.is_some() || self.close_prompt {
            return true;
        }
        let Some(context) = Context::capture(&self.view) else {
            return true;
        };
        self.cancel_block();
        self.block.context = Some(context.clone());
        match command {
            ids::BLOCK_CREATE => match create_targets(&self.view) {
                Ok(_) => {
                    self.block.name = "Block".into();
                    self.block.x = "0".into();
                    self.block.y = "0".into();
                    self.open_modal(ActiveModal::BlockCreate);
                }
                Err(e) => self.ui_error = Some(e),
            },
            ids::BLOCK_PLACE => {
                let Some(definition) = self.block.definition.clone() else {
                    return true;
                };
                let Some(layer) = self
                    .view
                    .layers
                    .iter()
                    .find(|l| l.is_active && target_ok(l))
                else {
                    self.ui_error = Some("请选择可见、可选、未锁定的活动图层及 Block 类别".into());
                    return true;
                };
                self.block_point_reference = MmPoint::new(0., 0.);
                self.block.session = Some(Session {
                    context: context.clone(),
                    layer: layer.layer_id.clone(),
                    kind: SessionKind::Place {
                        definition: definition.clone(),
                    },
                    point: None,
                    preview: None,
                });
                self.text.cancel();
                self.grip = None;
                self.drag = None;
                self.tool = ActiveTool::Block;
                self.object_snap_runtime.reset();
                self.send(Action::BlockPreview(
                    context,
                    definition,
                    self.camera.scale * f64::from(self.reported_ppp.max(1.)),
                ));
                rcam_diagnostics::runtime_event(rcam_diagnostics::Level::Info, "block.place.begin");
            }
            ids::BLOCK_RENAME => {
                if let Some(d) = self
                    .view
                    .block_definitions
                    .iter()
                    .find(|d| Some(&d.id.0) == self.block.definition.as_ref())
                {
                    self.block.name = d.name.clone();
                    self.open_modal(ActiveModal::BlockRename);
                }
            }
            ids::BLOCK_DELETE => {
                if let Some(id) = &self.block.definition {
                    let count = self.view.block_counts.get(id).copied().unwrap_or(0);
                    if count > 0 {
                        self.ui_error = Some(format!(
                            "此 Block 仍被 {count} 个实例引用，请先删除或 Explode 实例"
                        ));
                    } else {
                        self.open_modal(ActiveModal::BlockDelete);
                    }
                }
            }
            ids::BLOCK_SELECT => {
                if let Some(id) = &self.block.definition {
                    self.send(Action::BlockSelect(id.clone()));
                }
            }
            ids::BLOCK_EXPLODE => {
                if self.selected_instance().is_some() && crate::drag::editable_selection(&self.view)
                {
                    self.open_modal(ActiveModal::BlockExplode);
                }
            }
            ids::BLOCK_TRANSFORM => {
                if let Some((_, _, _, t)) = self.selected_instance() {
                    self.block.x = self.display_unit.input(t.translation.x_mm);
                    self.block.y = self.display_unit.input(t.translation.y_mm);
                    self.block.angle = t.rotation_deg.to_string();
                    self.block.mirror = t.mirror;
                    self.open_modal(ActiveModal::BlockTransform);
                }
            }
            _ => {}
        }
        true
    }
    pub(crate) fn block_entries(&mut self, ui: &mut egui::Ui) {
        self.command_entries(
            ui,
            &[
                ("创建 Block…", ids::BLOCK_CREATE),
                ("拆解 Block…", ids::BLOCK_EXPLODE),
            ],
            true,
        );
    }
    pub(crate) fn block_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("Block Library");
        if self
            .command_button(ui, ids::BLOCK_CREATE, "从选择创建 Block…")
            .clicked()
        {
            self.dispatch(ids::BLOCK_CREATE);
        }
        let count = self.view.block_definitions.len();
        if count == 0 {
            ui.label("No Blocks · 尚无块定义");
            return;
        }
        // Only visible row summaries are copied; never resolve Definition geometry here.
        egui::ScrollArea::vertical().show_rows(ui, 62., count, |ui, rows| {
            for i in rows {
                let d = &self.view.block_definitions[i];
                let (id, name, objects, revision) =
                    (d.id.0.clone(), d.name.clone(), d.objects.len(), d.revision);
                let instances = self.view.block_counts.get(&id).copied().unwrap_or(0);
                ui.push_id(&id, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 35.).max(40.);
                        let row = ui.add_sized(
                            [width, 20.],
                            egui::Button::new(egui::RichText::new(&name))
                                .selected(self.block.definition.as_ref() == Some(&id))
                                .truncate(),
                        );
                        if row.clicked() {
                            self.block.definition = Some(id.clone());
                        }
                        row.on_hover_text(format!("{name}\n{id}"));
                        ui.menu_button("⋯", |ui| {
                            for (label, command) in [
                                ("放置", ids::BLOCK_PLACE),
                                ("重命名…", ids::BLOCK_RENAME),
                                ("删除定义…", ids::BLOCK_DELETE),
                                ("选择实例", ids::BLOCK_SELECT),
                            ] {
                                if crate::ui::command_widgets::button_labeled(
                                    ui,
                                    command,
                                    label,
                                    crate::ui::command_widgets::CommandState::enabled(
                                        self.command_enabled_for(
                                            command,
                                            self.layer.as_deref(),
                                            Some(&id),
                                        ),
                                    ),
                                )
                                .clicked()
                                {
                                    self.block.definition = Some(id.clone());
                                    self.dispatch(command);
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.small(format!(
                        "{objects} objects · {instances} instances · rev {revision}"
                    ));
                    ui.separator();
                });
            }
        });
    }
    pub(crate) fn block_properties(&mut self, ui: &mut egui::Ui) {
        let Some((_, _, id, _)) = self.selected_instance() else {
            return;
        };
        if let Some(d) = self.view.block_definitions.iter().find(|d| d.id.0 == id) {
            ui.label(format!("Definition: {}", d.name));
            ui.small(format!(
                "{} objects · revision {}",
                d.objects.len(),
                d.revision
            ));
        }
        if self
            .command_button(ui, ids::BLOCK_TRANSFORM, "实例 X / Y / 角度 / 镜像…")
            .clicked()
        {
            self.dispatch(ids::BLOCK_TRANSFORM);
        }
    }
    pub(crate) fn block_modal(&mut self, ui: &mut egui::Ui, modal: ActiveModal) {
        let Some(context) = self.block.context.clone() else {
            return;
        };
        if !context.valid(&self.view) {
            ui.colored_label(Color32::LIGHT_RED, "工程已改变，请取消后重试");
            return;
        }
        match modal {
            ActiveModal::BlockCreate | ActiveModal::BlockRename => {
                ui.label("名称（1–128 字符，允许重名）");
                ui.text_edit_singleline(&mut self.block.name);
                if modal == ActiveModal::BlockCreate {
                    ui.label(format!(
                        "定义基点：X {} / Y {} {}",
                        self.block.x,
                        self.block.y,
                        self.display_unit.suffix()
                    ));
                    let origin = ui.button("基点：数值 / 拾取 / 双中心…");
                    #[cfg(feature = "internal-evidence")]
                    crate::native_i1::widget("block-origin", &origin);
                    if origin.clicked() {
                        let point = MmPoint::new(
                            self.display_unit.parse_length(&self.block.x).unwrap_or(0.),
                            self.display_unit.parse_length(&self.block.y).unwrap_or(0.),
                        );
                        self.open_point_adapter(crate::point_adapter::Adapter::BlockCreate, point);
                    }
                    let create = ui.button("以此基点创建 Block");
                    #[cfg(feature = "internal-evidence")]
                    crate::native_i1::widget("block-create", &create);
                    if create.clicked()
                        && let Ok((layer_id, object_ids)) = create_targets(&self.view)
                    {
                        match self.display_unit.parse_length(&self.block.x).and_then(|x| {
                            self.display_unit
                                .parse_length(&self.block.y)
                                .map(|y| MmPoint::new(x, y))
                        }) {
                            Ok(point) => self.send(Action::BlockEdit(Box::new(Request {
                                context: context.clone(),
                                edit: Edit::Create(CreateBlockDefinitionParams {
                                    layer_id,
                                    object_ids,
                                    local_origin_mm: PivotMm {
                                        x_mm: point.x_mm,
                                        y_mm: point.y_mm,
                                    },
                                    name: self.block.name.clone(),
                                }),
                            }))),
                            Err(error) => self.ui_error = Some(error),
                        }
                    }
                }
                let valid = !self.block.name.trim().is_empty()
                    && self.block.name.trim().chars().count() <= 128;
                if ui
                    .add_enabled(
                        valid,
                        egui::Button::new(if modal == ActiveModal::BlockCreate {
                            "拾取基点 →"
                        } else {
                            "应用名称"
                        }),
                    )
                    .clicked()
                    || (valid && self.dialog_enter(ui))
                {
                    if modal == ActiveModal::BlockCreate {
                        match create_targets(&self.view) {
                            Ok((layer, objects)) => {
                                self.block.session = Some(Session {
                                    context,
                                    layer,
                                    kind: SessionKind::Create {
                                        name: self.block.name.trim().into(),
                                        objects,
                                    },
                                    point: None,
                                    preview: None,
                                });
                                self.modal = None;
                                self.tool = ActiveTool::Block;
                                self.object_snap_runtime.reset();
                            }
                            Err(e) => self.ui_error = Some(e),
                        }
                    } else if let Some(id) = self.block.definition.clone() {
                        self.send(Action::BlockEdit(Box::new(Request {
                            context,
                            edit: Edit::Rename(RenameBlockDefinitionParams {
                                definition_id: id,
                                name: self.block.name.clone(),
                            }),
                        })));
                    }
                }
            }
            ActiveModal::BlockTransform => {
                ui.label(format!("X / Y ({})", self.display_unit.suffix()));
                ui.text_edit_singleline(&mut self.block.x);
                ui.text_edit_singleline(&mut self.block.y);
                ui.label("旋转 (°)");
                ui.text_edit_singleline(&mut self.block.angle);
                ui.checkbox(&mut self.block.mirror, "沿局部 Y 轴镜像（再旋转）");
                if ui.button("应用实例变换").clicked() || self.dialog_enter(ui) {
                    let parsed = self.display_unit.parse_length(&self.block.x).and_then(|x| {
                        self.display_unit
                            .parse_length(&self.block.y)
                            .map(|y| (x, y))
                    });
                    if let (Ok((x, y)), Ok(angle), Some((layer, object, _, _))) = (
                        parsed,
                        self.block.angle.trim().parse::<f64>(),
                        self.selected_instance(),
                    ) {
                        self.send(Action::BlockEdit(Box::new(Request {
                            context,
                            edit: Edit::Transform(UpdateBlockInstanceTransformParams {
                                layer_id: layer,
                                object_id: object,
                                transform: BlockTransformParams {
                                    translation_mm: PivotMm { x_mm: x, y_mm: y },
                                    rotation_deg: angle,
                                    mirror: self.block.mirror,
                                },
                            }),
                        })));
                    } else {
                        self.ui_error = Some("请输入有效 X、Y 与角度".into());
                    }
                }
            }
            ActiveModal::BlockExplode => {
                if let Some((layer, object, id, _)) = self.selected_instance() {
                    let n = self
                        .view
                        .block_definitions
                        .iter()
                        .find(|d| d.id.0 == id)
                        .map_or(0, |d| d.objects.len());
                    ui.label(format!(
                        "将此实例转成 {n} 个普通对象；Definition 保留。可撤销。"
                    ));
                    if ui.button("确认拆解").clicked() {
                        self.send(Action::BlockEdit(Box::new(Request {
                            context,
                            edit: Edit::Explode(ExplodeBlockInstanceParams {
                                layer_id: layer,
                                object_id: object,
                            }),
                        })));
                    }
                }
            }
            ActiveModal::BlockDelete => {
                ui.label("删除未被引用的定义及其中的几何；可撤销。");
                if ui.button("确认删除定义").clicked()
                    && let Some(id) = self.block.definition.clone()
                {
                    self.send(Action::BlockEdit(Box::new(Request {
                        context,
                        edit: Edit::Delete(BlockDefinitionIdParams { definition_id: id }),
                    })));
                }
            }
            _ => {}
        }
    }
    fn block_target(&mut self, raw: MmPoint, ppp: f32, alt: bool) -> Result<MmPoint, String> {
        self.object_snap_runtime
            .resolve(
                raw,
                &self.object_snap.contour(),
                self.grid,
                self.camera,
                ppp,
                self.view.snap_snapshot.as_deref(),
                &self.view.snap_index,
                &self.view.layers,
                None,
                alt,
            )
            .map(|r| r.point)
    }
    pub(crate) fn block_canvas(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) {
        let valid = self
            .block
            .session
            .as_ref()
            .is_some_and(|s| s.valid(&self.view));
        if !valid
            || self.tool != ActiveTool::Block
            || self.modal.is_some()
            || self.layer_dialog.is_some()
            || self.close_prompt
            || !ctx.input(|i| i.focused)
        {
            self.cancel_block();
            return;
        }
        if let Some(s) = &mut self.block.session
            && let SessionKind::Place { definition } = &s.kind
            && let Some(p) = &self.view.block_preview
            && p.context == s.context
            && &p.definition == definition
        {
            s.preview = Some(p.clone());
        }
        let ppm = self.camera.scale * f64::from(ctx.pixels_per_point());
        let rebuild = self.block.session.as_ref().and_then(|s| {
            let SessionKind::Place { definition } = &s.kind else {
                return None;
            };
            s.preview
                .as_ref()
                .filter(|p| ppm > p.ppm || ppm < p.ppm / 4.)
                .map(|_| (s.context.clone(), definition.clone()))
        });
        if !self.busy
            && let Some((context, definition)) = rebuild
        {
            if let Some(s) = &mut self.block.session {
                s.preview = None;
                s.point = None;
            }
            self.view.block_preview = None;
            self.send(Action::BlockPreview(context, definition, ppm));
            return;
        }
        let Some(pos) = response.hover_pos() else {
            return;
        };
        let raw = self.camera.world(pos, rect);
        let point =
            match self.block_target(raw, ctx.pixels_per_point(), ctx.input(|i| i.modifiers.alt)) {
                Ok(point) => point,
                Err(e) => {
                    self.ui_error = Some(e);
                    if let Some(s) = &mut self.block.session {
                        s.point = None;
                    }
                    return;
                }
            };
        if let Some(s) = &mut self.block.session {
            s.point = Some(if matches!(s.kind, SessionKind::Place { .. }) {
                MmPoint::new(
                    point.x_mm - self.block_point_reference.x_mm,
                    point.y_mm - self.block_point_reference.y_mm,
                )
            } else {
                point
            });
        }
        if response.clicked_by(egui::PointerButton::Primary) && self.usable() {
            let ready = self.block.session.as_ref().is_some_and(|s| {
                matches!(s.kind, SessionKind::Create { .. }) || s.preview.is_some()
            });
            if ready {
                let s = self.block.session.take().unwrap();
                self.tool = ActiveTool::Select;
                if let Some(request) = s.request() {
                    self.send(Action::BlockEdit(Box::new(request)));
                }
            }
        }
    }
    pub(crate) fn paint_block(&self, painter: &egui::Painter, rect: egui::Rect, ppp: f32) {
        let Some(s) = &self.block.session else {
            return;
        };
        let Some(point) = s.point else {
            return;
        };
        let screen = |p: MmPoint| {
            self.camera
                .screen(MmPoint::new(p.x_mm + point.x_mm, p.y_mm + point.y_mm), rect)
        };
        if let Some(preview) = &s.preview {
            for path in &preview.paths {
                painter.add(egui::Shape::line(
                    path.iter().copied().map(screen).collect(),
                    egui::Stroke::new(1.2 / ppp, Color32::LIGHT_GREEN),
                ));
            }
        }
        let p = self.camera.screen(point, rect);
        painter.circle_stroke(
            p,
            5. / ppp,
            egui::Stroke::new(1.5 / ppp, Color32::LIGHT_GREEN),
        );
        painter.text(
            p + egui::vec2(10., 10.),
            egui::Align2::LEFT_TOP,
            format!(
                "{} · {} · Esc 取消",
                if matches!(s.kind, SessionKind::Create { .. }) {
                    "拾取基点"
                } else {
                    "单次放置"
                },
                self.display_unit
                    .point_label(point, self.precision().resolution_mm)
            ),
            egui::FontId::proportional(12.),
            Color32::LIGHT_GREEN,
        );
    }
}

#[cfg(test)]
#[path = "../../rcam-project/tests/common/mod.rs"]
pub(crate) mod fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c3_commands_and_virtualized_library_do_not_mutate_manufacturing() {
        let mut app = crate::modal::tests::app();
        let project = fixtures::big_project(1, 1);
        let mut model = Model::default();
        model.run(Action::RestoreProject(
            rcam_project::encode_v1(&project).unwrap(),
        ));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        app.view = model.view.clone();
        let template = app.view.block_definitions[0].clone();
        app.view.block_definitions = (0..1000)
            .map(|i| {
                let mut d = template.clone();
                d.id.0 = format!("definition-{i}");
                d.name = "长中文名称 μ — Block".repeat(8);
                d
            })
            .collect();
        let before = app.view.info.clone();
        let ctx = egui::Context::default();
        let mut shapes = 0;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(280., 600.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.block_library(ui));
            },
        );
        shapes += output.shapes.len();
        assert!(
            shapes < 300,
            "virtual list emitted {shapes} shapes for 1000 definitions"
        );
        assert_eq!(app.view.info, before);
        for id in [
            ids::BLOCK_CREATE,
            ids::BLOCK_PLACE,
            ids::BLOCK_RENAME,
            ids::BLOCK_DELETE,
            ids::BLOCK_EXPLODE,
        ] {
            assert!(
                editor_core::command::standard_commands()
                    .iter()
                    .any(|d| d.id == id)
            );
        }
    }

    #[test]
    fn c3_basepoint_and_placement_share_object_grid_alt_resolution() {
        let mut model = Model::default();
        model
            .open(
                &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/synthetic/s4c3/blocks.gbr"),
            )
            .unwrap();
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.camera.scale = 100.;
        app.object_snap.enabled = true;
        app.grid.snap_enabled = true;
        app.grid.spacing_mm = 0.1;
        let original = app.view.info.clone();
        for _workflow in ["basepoint", "placement"] {
            app.object_snap_runtime.reset();
            let raw = MmPoint::new(0.02, 0.01);
            assert_eq!(
                app.block_target(raw, 2., false).unwrap(),
                MmPoint::new(0., 0.)
            );
            assert_eq!(
                app.object_snap_runtime.current.as_ref().unwrap().kind,
                Some(editor_core::snap::SnapKind::Center)
            );
            assert_eq!(app.block_target(raw, 2., true).unwrap(), raw);
            // Alt clears the current candidate entirely; the returned target is raw.
            assert!(app.object_snap_runtime.current.is_none());
            app.object_snap_runtime.reset();
            let raw = MmPoint::new(3.021, 2.029);
            assert_eq!(
                app.block_target(raw, 2., false).unwrap(),
                MmPoint::new(3., 2.)
            );
            assert!(app.object_snap_runtime.current.as_ref().unwrap().from_grid);
            assert_eq!(app.block_target(raw, 2., true).unwrap(), raw);
            app.grid.snap_enabled = false;
            app.object_snap.enabled = false;
            assert_eq!(app.block_target(raw, 2., false).unwrap(), raw);
            app.grid.snap_enabled = true;
            app.object_snap.enabled = true;
        }
        assert_eq!(app.view.info, original);
    }

    #[test]
    #[ignore = "release timing: 400-object cached placement, 100 shared instances"]
    fn c3_placement_performance() {
        let project = fixtures::big_project(400, 100);
        let mut model = Model::default();
        model.run(Action::RestoreProject(
            rcam_project::encode_v1(&project).unwrap(),
        ));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        assert_eq!(model.view.block_definitions.len(), 1);
        assert_eq!(model.view.block_counts.values().sum::<usize>(), 100);
        let context = Context::capture(&model.view).unwrap();
        let definition = model.view.block_definitions[0].id.0.clone();
        model.run(Action::BlockPreview(
            context.clone(),
            definition.clone(),
            40.,
        ));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        let preview = model.view.block_preview.clone().unwrap();
        let cache = model.block_display_cache.stats();
        assert_eq!(cache, (1, 400));
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.camera.scale = 20.;
        app.block.session = Some(Session {
            context,
            layer: app.view.layers[0].layer_id.clone(),
            kind: SessionKind::Place { definition },
            point: Some(MmPoint::new(0., 0.)),
            preview: Some(preview.clone()),
        });
        let before = app.view.info.clone();
        let ctx = egui::Context::default();
        let mut timings = vec![];
        for i in 0..120 {
            app.block.session.as_mut().unwrap().point = Some(MmPoint::new(i as f64 / 10., 1.));
            let start = std::time::Instant::now();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.paint_block(ui.painter(), ui.max_rect(), 2.));
            });
            timings.push(start.elapsed().as_secs_f64() * 1000.);
        }
        timings.sort_by(f64::total_cmp);
        assert_eq!(app.view.info, before);
        assert!(Arc::ptr_eq(
            &preview,
            app.block
                .session
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
        ));
        println!(
            "S4C3_PREVIEW definition_objects=400 instance_refs=100 cache_entries={} cached_primitives={} paths={} build_us={} paint_p50_ms={} paint_p95_ms={} paint_max_ms={} frames=120",
            cache.0,
            cache.1,
            preview.paths.len(),
            preview.build_us,
            timings[60],
            timings[114],
            timings[119]
        );
    }
}
