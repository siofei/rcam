use crate::display::Scene;
use editor_core::workspace::{DisplayClass, aperture_shape_map, classify_object};
use editor_core::{BoundsMm, MmPoint};
use editor_service::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Default)]
pub struct View {
    pub task_generation: u64,
    pub rule_revision: u64,
    pub task_receipt: Option<editor_service::task::TaskReceipt>,
    pub candidate_reply: Option<Arc<crate::candidates_ui::Reply>>,
    pub board: Option<Arc<editor_core::pnp::BoardState>>,
    pub component_indices: Arc<Vec<usize>>,
    pub component_query: Option<ComponentQuery>,
    pub pnp_preview: Option<Arc<crate::components_ui::PreviewReply>>,
    pub project_workspace: Option<rcam_project::WorkspaceProjectState>,
    pub text_reply: Option<Arc<crate::text_tool::Reply>>,
    pub metrics: Vec<MetricsItem>,
    pub metrics_error: Option<String>,
    pub info: Option<DocumentInfo>,
    pub layers: Vec<LayerInfo>,
    pub apertures: Vec<editor_core::ApertureDefinition>,
    pub block_cache_stats: (usize, usize),
    pub array_preview: Option<Arc<crate::array_ui::Preview>>,
    pub block_preview: Option<Arc<crate::block_ui::Preview>>,
    pub block_counts: std::collections::HashMap<String, usize>,
    pub block_definitions: Vec<editor_core::block::BlockDefinition>,
    /// Immutable manufacturing snapshot plus its object envelope index. Object
    /// Snap queries these lazily around the cursor; no global point list exists.
    pub snap_snapshot: Option<Arc<RenderSnapshot>>,
    pub snap_index: Arc<crate::world_index::WorldIndex>,
    pub selected: crate::selection::SelectionSet,
    pub bounds: Option<BoundsMm>,
    pub scene: Option<Arc<Scene>>,
    pub blocked: Option<String>,
    pub error: Option<ServiceError>,
    pub message: String,
    pub render_ppm: f64,
    pub render_viewport: Option<BoundsMm>,
    /// No manufacturing object was omitted by the viewport query. Panning a
    /// complete scene cannot expose uncached geometry; its LOD still applies.
    pub render_coverage_complete: bool,
    pub display_attempt: Option<(BoundsMm, f64)>,
    pub display_transient: Option<String>,
    pub drag_hit: bool,
    pub press_hit: Option<ObjectInfo>,
    /// Answer to the last `Action::LayerSummary`; drives the Delete Layer dialog.
    pub layer_summary: Option<LayerSummaryResult>,
    /// The last successful batch import, for the diagnostics summary.
    pub import: Option<ImportLayersResult>,
    /// The layer removed by the last `Action::RemoveLayer` (drives the Undo toast).
    pub removed: Option<RemoveLayerResult>,
    /// Bumped whenever a layer was created or imported; the UI fits/refocuses on it.
    pub structure_serial: u64,
    /// Answer to `Action::FitLayer`; the UI fits the camera to it once.
    pub focus_bounds: Option<BoundsMm>,
}

/// Class-aware view of the selection policy, built once per query.
pub struct Classifier<'a> {
    layers: &'a [LayerInfo],
    shapes: std::collections::HashMap<&'a str, &'a editor_core::ApertureShape>,
}
impl<'a> Classifier<'a> {
    pub fn new(layers: &'a [LayerInfo], apertures: &'a [editor_core::ApertureDefinition]) -> Self {
        Self {
            layers,
            shapes: aperture_shape_map(apertures),
        }
    }
    pub fn class(&self, object: &ObjectInfo) -> DisplayClass {
        classify_object(&object.object, &self.shapes)
    }
    fn style(&self, object: &ObjectInfo) -> Option<(&LayerInfo, Option<&ClassStyleInfo>)> {
        let layer = self.layers.iter().find(|l| l.layer_id == object.layer_id)?;
        let class = self.class(object);
        Some((layer, layer.classes.iter().find(|c| c.class == class)))
    }
    pub fn visible(&self, object: &ObjectInfo) -> bool {
        self.style(object)
            .is_some_and(|(l, c)| l.visible && l.effective_visible && c.is_none_or(|c| c.visible))
    }
    /// `effective_visible && layer.selectable && class.selectable`
    pub fn selectable(&self, object: &ObjectInfo) -> bool {
        self.visible(object)
            && self
                .style(object)
                .is_some_and(|(l, c)| l.selectable && c.is_none_or(|c| c.selectable))
    }
    /// Which policy refuses an edit of this object, if any.
    pub fn edit_refusal(&self, object: &ObjectInfo) -> Option<&'static str> {
        match self.style(object) {
            None => Some("NOT_FOUND"),
            Some((l, _)) if l.locked => Some("LAYER_LOCKED"),
            Some((_, Some(c))) if c.locked => Some("OBJECT_CLASS_LOCKED"),
            _ => None,
        }
    }
}

/// A layer can receive generated text: visible, not locked, and its
/// generated-text category neither hidden nor locked.
pub fn text_target_ok(layer: &LayerInfo) -> bool {
    layer.visible
        && layer.effective_visible
        && !layer.locked
        && layer
            .classes
            .iter()
            .find(|c| c.class == DisplayClass::GeneratedText)
            .is_none_or(|c| c.visible && !c.locked)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArrangementEligibility {
    pub align: bool,
    pub distribute: bool,
    pub logical_count: usize,
}

/// Menu gating only inspects the current selection and workspace policy. Bounds
/// remain the service's single manufacturing calculation when a command runs.
pub fn arrangement_eligibility(view: &View) -> ArrangementEligibility {
    let count = view.selected.ordered.len();
    if count < 2 || view.blocked.is_some() || view.scene.is_none() {
        return ArrangementEligibility::default();
    }
    let Some(primary) = view.selected.primary() else {
        return ArrangementEligibility::default();
    };
    let classifier = Classifier::new(&view.layers, &view.apertures);
    if !view.selected.ordered.iter().all(|object| {
        object.layer_id == primary.layer_id
            && classifier.selectable(object)
            && classifier.edit_refusal(object).is_none()
    }) {
        return ArrangementEligibility::default();
    }
    let mut selected_ids = std::collections::HashSet::with_capacity(count);
    let mut selected_text_counts = std::collections::HashMap::new();
    let mut selected_text_ops = std::collections::HashSet::new();
    for object in &view.selected.ordered {
        selected_ids.insert(object.object.object_id.as_str());
        if let editor_core::ObjectOrigin::GeneratedText { operation_id } = &object.object.origin {
            selected_text_ops.insert(operation_id.as_str());
            *selected_text_counts
                .entry(operation_id.as_str())
                .or_insert(0_usize) += 1;
        }
    }
    if selected_ids.len() != count {
        return ArrangementEligibility::default();
    }
    if !selected_text_ops.is_empty() {
        let Some(layer) = view.snap_snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .layers
                .iter()
                .find(|layer| layer.id == primary.layer_id)
        }) else {
            return ArrangementEligibility::default();
        };
        let mut group_totals = std::collections::HashMap::new();
        let mut selected_group_totals = std::collections::HashMap::new();
        for object in &layer.objects {
            if let editor_core::ObjectOrigin::GeneratedText { operation_id } = &object.origin
                && selected_text_ops.contains(operation_id.as_str())
            {
                *group_totals.entry(operation_id.as_str()).or_insert(0_usize) += 1;
                if selected_ids.contains(object.object_id.as_str()) {
                    *selected_group_totals
                        .entry(operation_id.as_str())
                        .or_insert(0_usize) += 1;
                }
            }
        }
        if selected_text_ops.iter().any(|operation_id| {
            group_totals.get(operation_id) != selected_text_counts.get(operation_id)
                || group_totals.get(operation_id) != selected_group_totals.get(operation_id)
        }) {
            return ArrangementEligibility::default();
        }
    }
    let text_member_count: usize = selected_text_counts.values().sum();
    let logical_count = count - text_member_count + selected_text_ops.len();
    ArrangementEligibility {
        align: logical_count >= 2,
        distribute: logical_count >= 3,
        logical_count,
    }
}

fn operation_id(origin: &editor_core::ObjectOrigin) -> Option<&str> {
    match origin {
        editor_core::ObjectOrigin::Generated { operation_id }
        | editor_core::ObjectOrigin::GeneratedText { operation_id } => Some(operation_id),
        editor_core::ObjectOrigin::Imported { .. } => None,
    }
}
pub struct Model {
    active_cancel: Option<editor_service::task::CancellationToken>,
    pub service: ApplicationService,
    pub view: View,
    snapshot: Option<Arc<RenderSnapshot>>,
    metrics_identity: String,
    world_index: Arc<crate::world_index::WorldIndex>,
    viewport: Option<(MmPoint, BoundsMm)>,
    serial: u64,
    pub ppm: f64,
    pub(crate) block_display_cache: crate::block_display::BlockDisplayCache,
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
    PnpPreview(crate::components_ui::PreviewRequest),
    PnpImport(crate::block_ui::Context, ImportPnpParams),
    BoardRegistration(
        crate::block_ui::Context,
        editor_core::pnp::RegistrationInput,
    ),
    ComponentSearch(crate::block_ui::Context, ComponentQuery),
    CandidateQuery(crate::candidates_ui::Request),
    CandidateSelect(crate::candidates_ui::Request, bool),
    ArrayPreview(Box<crate::array_ui::Request>),
    ArrayApply(Box<crate::array_ui::Request>),
    BlockEdit(Box<crate::block_ui::Request>),
    BlockPreview(crate::block_ui::Context, String, f64),
    BlockSelect(String),
    Precision(ManufacturingPrecision),
    Open(PathBuf),
    OpenProject(PathBuf, bool),
    SaveProject(Option<PathBuf>, bool, Option<rcam_project::CameraState>),
    ProjectWorkspace(rcam_project::WorkspaceProjectState),
    RestoreProject(Vec<u8>),
    RecoveryWrite(PathBuf),
    /// Start an empty Workspace (refuses while there are unexported edits).
    NewWorkspace,
    /// Same, after the user explicitly agreed to lose unexported edits.
    DiscardNewWorkspace,
    /// Batch import: all files or none; every file becomes its own layer.
    ImportGerbers(Vec<PathBuf>),
    CreateEmptyLayer(Option<String>),
    /// Read-only: content summary that decides which Delete Layer dialog to show.
    LayerSummary(String),
    RemoveLayer(String, bool),
    ReorderLayers(Vec<String>),
    SetActiveLayer(Option<String>),
    SetSoloLayer(Option<String>),
    /// Show or hide every layer in one workspace revision ("show all" also ends Solo).
    SetAllLayersVisible(bool),
    ResetLayerColors,
    /// Ask for the bounds of one layer's visible objects (Fit to layer).
    FitLayer(String),
    TextFont(u64, PathBuf, u32),
    FontCatalog,
    SystemFont(u64, PathBuf, String),
    TextPreview(crate::text_tool::Request),
    TextCreate(crate::text_tool::Request),
    Select(MmPoint, f64, crate::selection::SelectionMode),
    SelectRect(BoundsMm, editor_core::hit_test::SelectRectMode),
    Move(String, String),
    Align(AlignmentMode),
    Distribute(DistributionAxis),
    SetFlashSize(String, Option<String>),
    Rotate(String, PivotInput),
    Mirror(MirrorDirection),
    ProbeDrag(MmPoint, f64),
    DragMove(Box<crate::drag::Drag>),
    GripEdit(Box<crate::grip::Session>),
    Duplicate,
    Delete,
    History(bool),
    Layer(LayerUpdateParams),
    Save(PathBuf, String, Option<Vec<String>>),
    SaveWithPrecision(PathBuf, String, Option<Vec<String>>, f64),
    #[cfg(test)]
    Rebuild(f64),
    Viewport(MmPoint, BoundsMm, f64),
    Close(bool),
}
impl Default for Model {
    fn default() -> Self {
        Self {
            active_cancel: None,
            service: ApplicationService::new(),
            view: View::default(),
            snapshot: None,
            metrics_identity: String::new(),
            world_index: Default::default(),
            viewport: None,
            serial: 0,
            ppm: 20.,
            block_display_cache: Default::default(),
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
    editor_core::geometries_bounds_with_blocks(
        view.selected
            .ordered
            .iter()
            .map(|object| &object.object.geometry),
        &view.apertures,
        &view.block_definitions,
    )
    .map_err(|cause| error("VALIDATION_FAILED", &format!("选择集制造边界无效：{cause}")))?
    .ok_or_else(|| error("INVALID_ARGUMENT", "选择集没有可用的制造边界"))
}

pub fn selected_center(view: &View) -> Result<MmPoint, ServiceError> {
    Ok(selected_bounds(view)?.center())
}

impl Model {
    /// Dev-only synthetic Block fixture loader (S4-B2 Final Closeout native
    /// GUI smoke, task §17): no Block Editor GUI ships this phase, so this
    /// is the "internal/dev synthetic loading path" the closeout task
    /// explicitly allows for native verification — it must never be called
    /// from the normal app startup path or claimed as a Block Editor.
    /// Imports a small Flash+Line+Arc+Region fixture, captures it as one
    /// `BlockDefinition`, and places 5 instances at 0°/90°/37°/Mirror/
    /// Mirror+90°, matching `docs` review's `sample.rcam` fixture shape.
    #[cfg(feature = "internal-evidence")]
    pub(crate) fn autoload_block_fixture(&mut self) -> Result<(), ServiceError> {
        const MIXED: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX2000000Y2000000D03*\nX0Y0D02*\nG01X2000000Y0D01*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nG36*\nX10000000Y10000000D02*\nG01X12000000Y10000000D01*\nX12000000Y12000000D01*\nX10000000Y12000000D01*\nX10000000Y10000000D01*\nG37*\nM02*\n";
        let dir = std::env::temp_dir().join(format!(
            "rcam-native-smoke-{}-{}",
            std::process::id(),
            self.serial
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|e| error("IO_ERROR", &format!("autoload scratch dir: {e}")))?;
        let path = dir.join("block-fixture.gbr");
        std::fs::write(&path, MIXED)
            .map_err(|e| error("IO_ERROR", &format!("autoload fixture write: {e}")))?;
        self.open(&path)?;
        let doc_id = self.info()?.document_id;
        let layer_id = self.view.layers[0].layer_id.clone();
        let object_ids: Vec<String> = self
            .service
            .objects_query(
                &doc_id,
                QueryParams {
                    layer_id: layer_id.clone(),
                    geometry_type: None,
                    region_mm: None,
                    relation: None,
                    limit: Some(1000),
                    cursor: None,
                },
            )?
            .objects
            .into_iter()
            .map(|o| o.object.object_id)
            .collect();
        // `self.service.blocks_*` is called directly (no Block Editor GUI
        // exists to drive through `Action`/`self.run`), so `self.view.info`
        // does not advance between calls — the revision must be re-read
        // from the service itself each time, not from the cached view.
        let revision = self.service.document_get(&doc_id)?.revision;
        let create = self.service.blocks_create_definition_from_objects(
            &doc_id,
            &revision,
            CreateBlockDefinitionParams {
                layer_id: layer_id.clone(),
                object_ids,
                local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
                name: "native-smoke-opening".into(),
            },
        )?;
        for (translation, rotation_deg, mirror) in [
            (10., 0., false),
            (20., 90., false),
            (30., 37., false),
            (40., 0., true),
            (50., 90., true),
        ] {
            let revision = self.service.document_get(&doc_id)?.revision;
            self.service.blocks_create_instance(
                &doc_id,
                &revision,
                CreateBlockInstanceParams {
                    layer_id: layer_id.clone(),
                    definition_id: create.definition_id.clone(),
                    transform: BlockTransformParams {
                        translation_mm: PivotMm {
                            x_mm: translation,
                            y_mm: 0.,
                        },
                        rotation_deg,
                        mirror,
                    },
                },
            )?;
        }
        self.refresh(true)
    }
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
                    .close(&candidate.document_id, &candidate.revision, true)?;
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
        let warnings = self
            .view
            .layers
            .iter()
            .filter(|layer| {
                layer
                    .import_diagnostics
                    .iter()
                    .any(|line| line.starts_with("兼容导入："))
            })
            .count();
        if warnings > 0 {
            self.view.message =
                format!("文件已打开；⚠ {warnings} 个图层含兼容解释，请查看图层设置");
        }
        Ok(())
    }
    pub fn open_project(&mut self, path: &Path, discard: bool) -> Result<(), ServiceError> {
        if !discard && self.view.info.as_ref().is_some_and(|d| d.project_dirty) {
            return Err(error("CONFIRMATION_REQUIRED", "当前工程有未保存修改"));
        }
        self.service.grant_file_access(path, false)?;
        let candidate = self.service.project_open(
            path.to_str()
                .ok_or_else(|| error("INVALID_ARGUMENT", "路径编码无效"))?,
        )?;
        self.install_project(candidate, discard)
    }
    fn install_project(
        &mut self,
        candidate: DocumentInfo,
        discard: bool,
    ) -> Result<(), ServiceError> {
        let old_view = self.view.clone();
        let old_snapshot = self.snapshot.take();
        let old_ppm = self.ppm;
        let old_viewport = self.viewport.take();
        let old_index = std::mem::take(&mut self.world_index);
        self.ppm = 20.;
        self.view = View {
            info: Some(candidate),
            message: "工程已打开".into(),
            ..Default::default()
        };
        let prepared = self.refresh(true).and_then(|()| match &self.view.blocked {
            Some(reason) => Err(error("UNSUPPORTED_FEATURE", reason)),
            None => Ok(()),
        });
        if let Err(e) = prepared {
            if let Some(candidate) = &self.view.info {
                self.service
                    .close(&candidate.document_id, &candidate.revision, true)?;
            }
            self.view = old_view;
            self.snapshot = old_snapshot;
            self.ppm = old_ppm;
            self.viewport = old_viewport;
            self.world_index = old_index;
            return Err(e);
        }
        if let Some(old) = old_view.info {
            self.service
                .close(&old.document_id, &old.revision, discard)?;
        }
        Ok(())
    }
    pub fn restore_project(&mut self, bytes: &[u8]) -> Result<(), ServiceError> {
        let candidate = self.service.project_restore(bytes)?;
        self.install_project(candidate, true)
    }
    pub fn save_project(
        &mut self,
        path: Option<&Path>,
        replace: bool,
        camera: Option<rcam_project::CameraState>,
    ) -> Result<(), ServiceError> {
        let d = self.info()?;
        let grant = path
            .map(Path::to_path_buf)
            .or_else(|| d.project_path.as_ref().map(PathBuf::from));
        if let Some(path) = grant.as_deref() {
            self.service.grant_file_access(
                path.parent()
                    .ok_or_else(|| error("INVALID_ARGUMENT", "缺少输出目录"))?,
                true,
            )?;
        }
        self.service.project_save_with_camera(
            &d.document_id,
            &d.revision,
            path.map(|p| p.to_string_lossy().into_owned()).as_deref(),
            replace,
            camera,
        )?;
        self.view.message = "工程已保存".into();
        self.refresh(false)
    }
    fn workspace_revision(&self) -> Result<(String, String, String), ServiceError> {
        let d = self.info()?;
        Ok((d.document_id, d.revision, d.workspace_revision))
    }
    /// Replace the Project with an empty one after the caller handles dirty state.
    pub fn new_workspace(&mut self, discard: bool) -> Result<(), ServiceError> {
        if !discard && self.view.info.as_ref().is_some_and(|d| d.project_dirty) {
            return Err(error(
                "CONFIRMATION_REQUIRED",
                "当前工程有未保存修改。请先保存 .rcam，或明确放弃修改。",
            ));
        }
        let fresh = self.service.document_new()?;
        if let Some(old) = self.view.info.take() {
            self.service.close(&old.document_id, &old.revision, true)?;
        }
        self.snapshot = None;
        self.viewport = None;
        self.world_index = Default::default();
        self.ppm = 20.;
        self.view = View {
            info: Some(fresh),
            message: "已新建空工作区".into(),
            ..Default::default()
        };
        self.refresh(true)
    }
    /// Batch import. Files are all read, parsed and validated before any layer
    /// exists; one failure leaves the Workspace untouched.
    pub fn import_gerbers(&mut self, paths: &[PathBuf]) -> Result<(), ServiceError> {
        if paths.is_empty() {
            return Ok(());
        }
        let creating = self.view.info.is_none();
        let mut names = Vec::new();
        for path in paths {
            self.service.grant_file_access(path, false)?;
            names.push(
                path.to_str()
                    .ok_or_else(|| error("INVALID_ARGUMENT", "路径编码无效"))?
                    .to_string(),
            );
        }
        if creating {
            self.new_workspace(true)?;
        }
        let (document_id, revision, _) = self.workspace_revision()?;
        let cancel = self.active_cancel.as_ref();
        let result = self.service.import_gerber_layers_with_cancel(
            &document_id,
            &revision,
            ImportGerberLayersParams { paths: names },
            cancel,
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if creating {
                    self.service.close(&document_id, &revision, true)?;
                    self.view = View::default();
                    self.snapshot = None;
                    self.world_index = Default::default();
                    self.viewport = None;
                }
                return Err(error);
            }
        };
        let diagnostics: usize = result.layers.iter().map(|l| l.diagnostics.len()).sum();
        let compatibility_layers = result
            .layers
            .iter()
            .filter(|layer| {
                layer
                    .diagnostics
                    .iter()
                    .any(|line| line.starts_with("兼容导入："))
            })
            .count();
        self.view.message = if compatibility_layers > 0 {
            format!(
                "已导入 {} 个图层；⚠ {compatibility_layers} 个含兼容几何，请查看图层设置与导出确认",
                result.layers.len()
            )
        } else if diagnostics == 0 {
            format!("已导入 {} 个图层（可整体撤销）", result.layers.len())
        } else {
            format!(
                "已导入 {} 个图层，{diagnostics} 条解析提示（见图层设置）",
                result.layers.len()
            )
        };
        self.view.import = Some(result);
        self.view.structure_serial += 1;
        self.refresh(true)
    }
    pub fn create_empty_layer(&mut self, name: Option<String>) -> Result<(), ServiceError> {
        let (document_id, revision, _) = self.workspace_revision()?;
        let result = self.service.create_empty_layer(
            &document_id,
            &revision,
            CreateEmptyLayerParams { display_name: name },
        )?;
        self.view.message = format!("已新建空图层“{}”（可撤销）", result.display_name);
        self.view.structure_serial += 1;
        self.refresh(true)
    }
    pub fn layer_summary(&mut self, layer_id: String) -> Result<(), ServiceError> {
        let (document_id, ..) = self.workspace_revision()?;
        self.view.layer_summary = Some(
            self.service
                .layer_summary(&document_id, LayerSummaryParams { layer_id })?,
        );
        Ok(())
    }
    pub fn remove_layer(
        &mut self,
        layer_id: String,
        allow_non_empty: bool,
    ) -> Result<(), ServiceError> {
        let (document_id, revision, _) = self.workspace_revision()?;
        let result = self.service.remove_layer(
            &document_id,
            &revision,
            RemoveLayerParams {
                layer_id,
                allow_non_empty,
            },
        )?;
        self.view.message = format!("已删除图层“{}”（可撤销）", result.display_name);
        self.view.removed = Some(result);
        self.view.structure_serial += 1;
        self.refresh(true)
    }
    fn workspace_only<T>(
        &mut self,
        geometry: bool,
        call: impl FnOnce(&mut ApplicationService, &str, &str, String) -> Result<T, ServiceError>,
    ) -> Result<(), ServiceError> {
        let (document_id, revision, workspace) = self.workspace_revision()?;
        call(&mut self.service, &document_id, &revision, workspace)?;
        self.refresh(geometry)
    }
    pub(crate) fn refresh(&mut self, geometry: bool) -> Result<(), ServiceError> {
        let timing = std::env::var_os("RCAM_EDIT_TIMING").is_some();
        let mut phase = std::time::Instant::now();
        let id = self.info()?.document_id;
        self.view.info = Some(self.service.document_get(&id)?);
        if self
            .view
            .candidate_reply
            .as_ref()
            .is_some_and(|r| !r.request.context.valid(&self.view))
        {
            self.view.candidate_reply = None;
        }
        let board = self.service.board_state(&id)?;
        let changed = match (&board, &self.view.board) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            let q = ComponentQuery {
                revision: self.view.info.as_ref().unwrap().revision.clone(),
                query: String::new(),
                mode: RefdesMatch::Prefix,
                side: None,
                footprint: None,
                offset: 0,
                limit: 500,
            };
            self.view.component_indices = self.service.component_indices(&id, &q)?;
            self.view.component_query = Some(q);
        }
        self.view.board = board;
        self.view.project_workspace = Some(self.service.project_workspace(&id)?);
        let layers = self.service.layers_list(&id)?;
        let styles_changed = layers.len() != self.view.layers.len()
            || layers.iter().zip(&self.view.layers).any(|(a, b)| {
                a.layer_id != b.layer_id
                    || a.visible != b.visible
                    || a.effective_visible != b.effective_visible
                    || a.base_color != b.base_color
                    || a.color_mode != b.color_mode
                    || a.display_mode != b.display_mode
                    || a.classes.len() != b.classes.len()
                    || a.classes.iter().zip(&b.classes).any(|(a, b)| {
                        a.class != b.class
                            || a.visible != b.visible
                            || a.effective_color != b.effective_color
                    })
            });
        let display_changed = geometry || styles_changed;
        let mut delta = None;
        if timing {
            eprintln!(
                "EDIT_PHASE info_ms={}",
                phase.elapsed().as_secs_f64() * 1000.
            );
        }
        phase = std::time::Instant::now();
        self.view.layers = layers;
        if display_changed && !geometry {
            self.view.bounds = self.service.visible_bounds(&id)?.bounds;
        }
        if timing {
            eprintln!(
                "EDIT_PHASE bounds_ms={}",
                phase.elapsed().as_secs_f64() * 1000.
            );
        }
        phase = std::time::Instant::now();
        if geometry {
            if self.snapshot.as_ref().is_none_or(|s| s.document_id != id) {
                self.block_display_cache = Default::default();
                self.view.block_preview = None;
            }
            let snapshot = Arc::new(self.service.render_snapshot(&id)?);
            delta = self
                .snapshot
                .as_deref()
                .and_then(|old| crate::world_index::changed_objects(&snapshot, old));
            self.view.apertures = snapshot.apertures.clone();
            self.view.block_definitions = snapshot.block_definitions.clone();
            self.view.block_counts.clear();
            for object in snapshot.layers.iter().flat_map(|l| &l.objects) {
                if let editor_core::SemanticGeometry::BlockInstance { definition_id, .. } =
                    &object.geometry
                {
                    *self
                        .view
                        .block_counts
                        .entry(definition_id.0.clone())
                        .or_default() += 1;
                }
            }
            if timing {
                eprintln!(
                    "EDIT_PHASE snapshot_ms={}",
                    phase.elapsed().as_secs_f64() * 1000.
                );
            }
            phase = std::time::Instant::now();
            self.world_index = Arc::new(
                match &delta {
                    Some(changed) => self.world_index.update(&snapshot, changed),
                    None => crate::world_index::WorldIndex::build(&snapshot),
                }
                .map_err(|e| error("VALIDATION_FAILED", &e))?,
            );
            self.view.bounds = self
                .world_index
                .visible_bounds(&snapshot, &self.view.layers);
            self.view.snap_index = self.world_index.clone();
            self.view.snap_snapshot = Some(snapshot.clone());
            self.snapshot = Some(snapshot);
        }
        if timing {
            eprintln!(
                "EDIT_PHASE world_index_ms={}",
                phase.elapsed().as_secs_f64() * 1000.
            );
        }
        phase = std::time::Instant::now();
        // Hidden objects leave selection. Visible non-selectable objects retain
        // identity for inspection; edit_targets and Grip independently refuse edits.
        let previous = std::mem::take(&mut self.view.selected.ordered);
        let requested: std::collections::HashSet<_> = previous
            .iter()
            .map(|o| (o.layer_id.as_str(), o.object.object_id.as_str()))
            .collect();
        let mut found = std::collections::HashMap::with_capacity(previous.len());
        if !requested.is_empty()
            && let Some(snapshot) = &self.snapshot
        {
            for layer in &snapshot.layers {
                for object in &layer.objects {
                    if requested.contains(&(layer.id.as_str(), object.object_id.as_str())) {
                        found.insert((layer.id.as_str(), object.object_id.as_str()), object);
                    }
                }
            }
        }
        let mut selected: Vec<_> = previous
            .iter()
            .filter_map(|o| {
                found
                    .get(&(o.layer_id.as_str(), o.object.object_id.as_str()))
                    .map(|object| ObjectInfo {
                        layer_id: o.layer_id.clone(),
                        object: (*object).clone(),
                    })
            })
            .collect();
        let classifier = Classifier::new(&self.view.layers, &self.view.apertures);
        selected.retain(|o| classifier.visible(o));
        self.view.selected.ordered = selected;
        if display_changed {
            self.rebuild_changed(if styles_changed {
                None
            } else {
                delta.as_deref()
            });
        }
        if timing {
            eprintln!(
                "EDIT_PHASE selection_scene_ms={}",
                phase.elapsed().as_secs_f64() * 1000.
            );
        }
        Ok(())
    }
    fn rebuild(&mut self) {
        self.rebuild_changed(None);
    }
    fn rebuild_changed(&mut self, delta: Option<&[(usize, usize)]>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        self.serial += 1;
        #[cfg(feature = "internal-evidence")]
        crate::native_s5m1::count_rebuild(delta.is_some());
        self.view.render_ppm = self.ppm;
        // Coverage records attempted work, including a resource refusal. The UI
        // retries only when the camera/LOD changes, never on every error reply.
        self.view.render_viewport = self.viewport.map(|(_, b)| b);
        self.view.display_attempt = self.viewport.map(|(_, b)| (b, self.ppm));
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
        let coverage_complete = filtered.as_ref().is_none_or(|subset| {
            subset
                .layers
                .iter()
                .zip(&snapshot.layers)
                .all(|(a, b)| a.id == b.id && a.objects.len() == b.objects.len())
        });
        self.view.render_coverage_complete = false;
        let patched = delta.and_then(|changed| {
            self.view.scene.as_deref().and_then(|previous| {
                Scene::patch(
                    previous,
                    filtered.as_ref().unwrap_or(snapshot),
                    snapshot,
                    changed,
                    &self.view.layers,
                    anchor,
                    self.ppm,
                    self.serial,
                )
            })
        });
        let scene = patched.unwrap_or_else(|| {
            #[cfg(feature = "internal-evidence")]
            crate::native_s5m1::gpu_event("geometry-full-build-call", 1);
            Scene::build_cached_with_cancel(
                filtered.as_ref().unwrap_or(snapshot),
                &self.view.layers,
                anchor,
                self.ppm,
                self.serial,
                self.view.scene.as_deref(),
                &mut self.block_display_cache,
                self.active_cancel.as_ref(),
            )
        });
        match scene {
            Ok(scene) => {
                self.view.render_coverage_complete = coverage_complete;
                self.view.block_cache_stats = self.block_display_cache.stats();
                self.view.scene = Some(Arc::new(scene));
                self.view.blocked = None;
                self.view.display_transient = None;
            }
            Err(e) if e.starts_with("DISPLAY_PRECISION:") => {
                rcam_diagnostics::render_exception(
                    rcam_diagnostics::RenderException::DisplayPrepareFailed,
                );
                self.view.display_transient = Some(e);
                if let Some(scene) = &self.view.scene {
                    self.view.render_ppm = scene.ppm;
                }
            }
            Err(e) => {
                rcam_diagnostics::render_exception(
                    rcam_diagnostics::RenderException::DisplayPrepareFailed,
                );
                if e.starts_with("RESOURCE_LIMIT:") {
                    rcam_diagnostics::render_exception(
                        rcam_diagnostics::RenderException::ResourceLimit,
                    );
                }
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
        #[cfg(feature = "internal-evidence")]
        let _measure = crate::native_s5m1::HitTimer::start();
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
                        selectable_only: true,
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
            && let Some(operation_id) = operation_id(&hit.object.origin)
            && let Some(snapshot) = &self.snapshot
        {
            let ids: Vec<_> = snapshot
                .layers
                .iter()
                .filter(|l| l.id == hit.layer_id)
                .flat_map(|l| &l.objects)
                .filter(|o| self::operation_id(&o.origin) == Some(operation_id))
                .map(|o| o.object_id.clone())
                .collect();
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
        let snapshot = match &self.snapshot {
            Some(snapshot)
                if snapshot.document_id == d.document_id && snapshot.revision == d.revision =>
            {
                snapshot.clone()
            }
            _ => Arc::new(self.service.render_snapshot(&d.document_id)?),
        };
        let mut selected = Vec::new();
        for l in self
            .view
            .layers
            .iter()
            .filter(|l| l.visible && l.effective_visible && l.selectable)
        {
            let result = self.service.objects_select_rect_with_cancel(
                &d.document_id,
                SelectRectParams {
                    layer_id: l.layer_id.clone(),
                    rect_mm,
                    mode,
                    selectable_only: true,
                },
                self.active_cancel.as_ref(),
            )?;
            if result.object_ids.is_empty() {
                continue;
            }
            let layer = snapshot
                .layers
                .iter()
                .find(|layer| layer.id == l.layer_id)
                .ok_or_else(|| error("NOT_FOUND", "框选快照缺少图层"))?;
            let objects: std::collections::HashMap<_, _> = layer
                .objects
                .iter()
                .map(|object| (object.object_id.as_str(), object))
                .collect();
            for object_id in result.object_ids {
                if let Some(cancel) = &self.active_cancel {
                    cancel.checkpoint()?;
                }
                let object = objects
                    .get(object_id.as_str())
                    .ok_or_else(|| error("NOT_FOUND", "框选快照缺少对象"))?;
                selected.push(ObjectInfo {
                    layer_id: l.layer_id.clone(),
                    object: (*object).clone(),
                });
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
        let classifier = Classifier::new(&self.view.layers, &self.view.apertures);
        for o in &self.view.selected.ordered {
            match classifier.edit_refusal(o) {
                Some("LAYER_LOCKED") => {
                    return Err(error("LAYER_LOCKED", "选择包含锁定层，整组操作已拒绝"));
                }
                Some("OBJECT_CLASS_LOCKED") => {
                    return Err(error(
                        "OBJECT_CLASS_LOCKED",
                        "选择包含已锁定的对象类别，整组操作已拒绝",
                    ));
                }
                Some(_) => return Err(error("NOT_FOUND", "图层不存在")),
                None => {}
            }
            if !classifier.selectable(o) {
                return Err(error(
                    "INVALID_ARGUMENT",
                    "选择包含隐藏或不可选对象，整组操作已拒绝",
                ));
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
    fn arrangement_targets(
        &self,
        minimum: usize,
    ) -> Result<(String, Vec<String>, String), ServiceError> {
        if self.view.selected.ordered.len() < minimum {
            return Err(error("INVALID_ARGUMENT", "所选对象数量不足"));
        }
        let anchor = self
            .view
            .selected
            .primary()
            .ok_or_else(|| error("INVALID_ARGUMENT", "请先选择对象"))?;
        Ok((
            anchor.layer_id.clone(),
            self.view
                .selected
                .ordered
                .iter()
                .map(|object| object.object.object_id.clone())
                .collect(),
            anchor.object.object_id.clone(),
        ))
    }
    pub fn align_selection(&mut self, mode: AlignmentMode) -> Result<(), ServiceError> {
        self.editable()?;
        let document = self.info()?;
        let (layer_id, object_ids, anchor_object_id) = self.arrangement_targets(2)?;
        let result = self.service.objects_align(
            &document.document_id,
            &document.revision,
            AlignParams {
                layer_id,
                object_ids,
                anchor_object_id,
                mode,
            },
        )?;
        if result.changed_object_ids.is_empty() {
            self.view.message = "对象已对齐；没有位置变化".into();
            return Ok(());
        }
        self.view.message = format!(
            "已对齐 {} 个对象（锚点保持不动）",
            result.changed_object_ids.len()
        );
        self.refresh(true)
    }
    pub fn distribute_selection(&mut self, axis: DistributionAxis) -> Result<(), ServiceError> {
        self.editable()?;
        if self.view.selected.ordered.len() < 3 {
            return Err(error("INVALID_ARGUMENT", "等距分布至少需要 3 个对象"));
        }
        let document = self.info()?;
        let (layer_id, object_ids, _) = self.arrangement_targets(3)?;
        let result = self.service.objects_distribute(
            &document.document_id,
            &document.revision,
            DistributeParams {
                layer_id,
                object_ids,
                axis,
            },
        )?;
        if result.changed_object_ids.is_empty() {
            self.view.message = "对象已经等距；没有位置变化".into();
            return Ok(());
        }
        self.view.message = format!("已等距分布 {} 个对象", result.changed_object_ids.len());
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
        self.save_with_precision(path, layer, categories, None)
    }

    pub fn save_with_precision(
        &mut self,
        path: &Path,
        layer: String,
        categories: Option<Vec<String>>,
        compatibility_precision_override_mm: Option<f64>,
    ) -> Result<(), ServiceError> {
        self.editable()?;
        let d = self.info()?;
        self.service.grant_file_access(
            path.parent()
                .ok_or_else(|| error("INVALID_ARGUMENT", "缺少输出目录"))?,
            true,
        )?;
        let metadata_policy = match categories.clone() {
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
                compatibility_precision_override_mm,
            },
        );
        match result {
            Ok(r) => {
                self.view.info = Some(self.service.document_get(&d.document_id)?);
                self.view.message =
                    format!("已导出 {}（导出不改变工作区，也不建立文件关联）", r.path);
                Ok(())
            }
            Err(mut e) => {
                e.details["gui_layer_id"] = serde_json::json!(layer);
                e.details["gui_target_path"] = serde_json::json!(path);
                e.details["gui_document_id"] = serde_json::json!(d.document_id);
                e.details["gui_revision"] = serde_json::json!(d.revision);
                e.details["gui_confirmed_categories"] = serde_json::json!(categories);
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
    pub(crate) fn task_version(&self) -> Result<editor_service::task::TaskVersion, ServiceError> {
        // Query the owner of manufacturing state, not a potentially stale GUI snapshot.
        let info = self
            .view
            .info
            .as_ref()
            .map(|d| self.service.document_get(&d.document_id))
            .transpose()?;
        Ok(editor_service::task::TaskVersion::capture(
            info.as_ref(),
            self.view.task_generation,
            self.view.rule_revision,
        ))
    }
    pub(crate) fn run_task(&mut self, task: editor_service::task::TaskContext, action: Action) {
        use editor_service::task::{TaskReceipt, TaskState};
        let before = self.view.clone();
        let old_viewport = self.viewport;
        let old_ppm = self.ppm;
        let readonly = matches!(
            &action,
            Action::Viewport(..)
                | Action::Select(..)
                | Action::SelectRect(..)
                | Action::ProbeDrag(..)
                | Action::FitLayer(..)
                | Action::LayerSummary(..)
                | Action::CandidateQuery(..)
                | Action::CandidateSelect(..)
                | Action::ComponentSearch(..)
                | Action::ArrayPreview(..)
                | Action::BlockPreview(..)
                | Action::BlockSelect(..)
                | Action::TextPreview(..)
                | Action::TextFont(..)
                | Action::SystemFont(..)
                | Action::FontCatalog
                | Action::PnpPreview(..)
        );
        let cancellable_import = matches!(&action, Action::ImportGerbers(..));
        let result = (|| {
            task.cancel_token.start()?;
            task.validate(&self.task_version()?)?;
            if !readonly && !cancellable_import {
                task.cancel_token.begin_commit()?;
                #[cfg(feature = "internal-evidence")]
                crate::native_a2::committing(&task);
            }
            self.active_cancel = Some(task.cancel_token.clone());
            self.run(action);
            if (readonly || (cancellable_import && self.view.error.is_none()))
                && task.cancel_token.state() != TaskState::Committing
            {
                task.cancel_token.begin_commit()?;
            }
            Ok::<(), ServiceError>(())
        })();
        self.active_cancel = None;
        if let Err(error) = result {
            if readonly && task.cancel_token.checkpoint().is_err() {
                self.view = before.clone();
                self.viewport = old_viewport;
                self.ppm = old_ppm;
                self.block_display_cache = Default::default();
                self.metrics_identity.clear();
            }
            self.view.error = Some(error);
        }
        let changed = before.info.as_ref().map(|d| &d.document_id)
            != self.view.info.as_ref().map(|d| &d.document_id);
        self.view.task_generation = before.task_generation + u64::from(changed);
        self.view.rule_revision = before.rule_revision;
        let result_version = self.task_version().unwrap_or_default();
        drop(before); // Release the task rollback snapshot before publishing terminal state.
        task.cancel_token.finish(self.view.error.is_none());
        self.view.task_receipt = Some(TaskReceipt {
            task_id: task.task_id,
            input: task.input,
            result_version,
            state: task.cancel_token.state(),
        });
    }
    pub fn run(&mut self, action: Action) {
        let edit_timing = std::env::var_os("RCAM_EDIT_TIMING").is_some();
        let edit_label = match &action {
            Action::Move(..) => Some("move"),
            Action::History(false) => Some("undo"),
            Action::History(true) => Some("redo"),
            Action::Rotate(..) => Some("rotate"),
            Action::Mirror(..) => Some("mirror"),
            Action::Duplicate => Some("duplicate"),
            Action::Delete => Some("delete"),
            Action::SetFlashSize(..) => Some("properties"),
            _ => None,
        };
        let edit_started = std::time::Instant::now();
        self.view.error = None;
        self.view.text_reply = None;
        self.view.layer_summary = None;
        self.view.import = None;
        self.view.removed = None;
        self.view.focus_bounds = None;
        let result = (|| match action {
            Action::PnpPreview(request) => self.pnp_preview(request),
            Action::PnpImport(context, params) => self.pnp_import(context, params),
            Action::BoardRegistration(context, input) => self.registration_apply(context, input),
            Action::ComponentSearch(context, query) => self.component_search(context, query),
            Action::CandidateQuery(request) => self.candidate_query(request),
            Action::CandidateSelect(request, add) => self.candidate_select(request, add),
            Action::BlockEdit(request) => self.block_edit(*request),
            Action::ArrayPreview(request) => self.array_preview(*request),
            Action::ArrayApply(request) => self.array_apply(*request),
            Action::BlockPreview(context, id, ppm) => self.block_preview(context, id, ppm),
            Action::BlockSelect(id) => self.block_select(&id),
            Action::Open(path) => self.open(&path),
            Action::OpenProject(path, discard) => self.open_project(&path, discard),
            Action::SaveProject(path, replace, camera) => {
                self.save_project(path.as_deref(), replace, camera)
            }
            Action::ProjectWorkspace(settings) => {
                let id = self.info()?.document_id;
                self.service.project_set_workspace(&id, settings)?;
                self.refresh(false)
            }
            Action::RestoreProject(bytes) => {
                let result = self.restore_project(&bytes);
                crate::recovery::event(if result.is_ok() {
                    "recovery.restore_success"
                } else {
                    "recovery.restore_failed"
                });
                result
            }
            Action::RecoveryWrite(dir) => {
                let d = self.info()?;
                if d.project_dirty {
                    let bytes = self.service.project_recovery_bytes(&d.document_id)?;
                    crate::recovery::write(&dir, &d, &bytes)
                        .map_err(|e| error("IO_ERROR", &e.to_string()))?;
                }
                Ok(())
            }
            Action::NewWorkspace => self.new_workspace(false),
            Action::DiscardNewWorkspace => self.new_workspace(true),
            Action::ImportGerbers(paths) => self.import_gerbers(&paths),
            Action::CreateEmptyLayer(name) => self.create_empty_layer(name),
            Action::LayerSummary(layer_id) => self.layer_summary(layer_id),
            Action::RemoveLayer(layer_id, allow) => self.remove_layer(layer_id, allow),
            Action::ReorderLayers(layer_ids) => {
                self.workspace_only(true, |svc, doc, rev, workspace| {
                    svc.layers_reorder(
                        doc,
                        rev,
                        ReorderLayersParams {
                            expected_workspace_revision: workspace,
                            layer_ids,
                        },
                    )
                })
            }
            Action::SetActiveLayer(layer_id) => {
                self.workspace_only(false, |svc, doc, rev, workspace| {
                    svc.layers_set_active(
                        doc,
                        rev,
                        SetActiveLayerParams {
                            expected_workspace_revision: workspace,
                            layer_id,
                        },
                    )
                })
            }
            Action::SetSoloLayer(layer_id) => {
                self.workspace_only(false, |svc, doc, rev, workspace| {
                    svc.layers_set_solo(
                        doc,
                        rev,
                        SetSoloLayerParams {
                            expected_workspace_revision: workspace,
                            layer_id,
                        },
                    )
                })
            }
            Action::SetAllLayersVisible(visible) => {
                let updates: Vec<LayerPatch> = self
                    .view
                    .layers
                    .iter()
                    .map(|l| LayerPatch {
                        layer_id: l.layer_id.clone(),
                        visible: Some(visible),
                        ..Default::default()
                    })
                    .collect();
                let end_solo = visible && self.view.layers.iter().any(|l| l.is_solo);
                self.workspace_only(false, |svc, doc, rev, workspace| {
                    let mut workspace = workspace;
                    let info = svc.layers_update_many(
                        doc,
                        rev,
                        UpdateLayersParams {
                            expected_workspace_revision: workspace.clone(),
                            updates,
                        },
                    )?;
                    workspace = info.workspace_revision;
                    if end_solo {
                        svc.layers_set_solo(
                            doc,
                            rev,
                            SetSoloLayerParams {
                                expected_workspace_revision: workspace,
                                layer_id: None,
                            },
                        )?;
                    }
                    Ok(())
                })
            }
            Action::FitLayer(layer_id) => {
                let d = self.info()?;
                let bounds = self
                    .service
                    .layer_bounds(&d.document_id, LayerBoundsParams { layer_id })?
                    .bounds;
                if bounds.is_none() {
                    self.view.message = "该图层没有可缩放的内容".into();
                }
                self.view.focus_bounds = bounds;
                Ok(())
            }
            Action::ResetLayerColors => self.workspace_only(false, |svc, doc, rev, workspace| {
                svc.layers_reset_colors(
                    doc,
                    rev,
                    ResetLayerColorsParams {
                        expected_workspace_revision: workspace,
                    },
                )
            }),
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
            Action::Align(mode) => self.align_selection(mode),
            Action::Distribute(axis) => self.distribute_selection(axis),
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
                            selectable_only: true,
                        },
                    )?;
                    self.view.drag_hit = hits
                        .object_ids
                        .iter()
                        .any(|id| self.view.selected.contains(&layer_id, id));
                }
                Ok(())
            }
            Action::GripEdit(grip) => {
                self.editable()?;
                self.service.objects_grip_edit(
                    &grip.document,
                    &grip.revision,
                    GripEditParams {
                        layer_id: grip.layer,
                        object_id: grip.object.object_id,
                        grip_id: grip.id,
                        target_mm: grip.target,
                    },
                )?;
                self.view.message = "已编辑控制点".into();
                self.refresh(true)
            }
            Action::DragMove(drag) => {
                self.editable()?;
                let (layer, objects) = self.edit_targets()?;
                if layer != drag.layer || objects != drag.objects {
                    return Err(error("INVALID_ARGUMENT", "拖动期间选择已改变"));
                }
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
            Action::SaveWithPrecision(path, layer, c, q) => {
                self.save_with_precision(&path, layer, c, Some(q))
            }
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
                self.world_index = Default::default();
                self.block_display_cache = Default::default();
                self.metrics_identity.clear();
                Ok(())
            }
        })();
        if let Err(e) = result {
            eprintln!("service_error {} {} {}", e.code, e.message, e.details);
            self.view.error = Some(e);
        }
        if self
            .active_cancel
            .as_ref()
            .is_none_or(|c| c.checkpoint().is_ok())
        {
            self.refresh_metrics();
        }
        if edit_timing && let Some(label) = edit_label {
            eprintln!(
                "EDIT_ACTION operation={label} elapsed_ms={} failed={}",
                edit_started.elapsed().as_secs_f64() * 1000.,
                self.view.error.is_some()
            );
        }
        if let Some(d) = &self.view.info {
            eprintln!(
                "state document={} revision={} workspace={} dirty={} undo={} redo={} selected_count={} selected_first64={:?} layers={}",
                d.document_id,
                d.revision,
                d.workspace_revision,
                d.dirty,
                d.undo_entries,
                d.redo_entries,
                self.view.selected.ordered.len(),
                self.view
                    .selected
                    .ordered
                    .iter()
                    .take(64)
                    .map(|object| object.object.object_id.as_str())
                    .collect::<Vec<_>>(),
                d.layer_ids.len()
            );
        }
    }
}

/// UI policy only. The closure must call the exact service query.
pub fn topmost_hit(
    layers: &[LayerInfo],
    mut query: impl FnMut(&str) -> Result<Vec<String>, ServiceError>,
) -> Result<Option<(String, String)>, ServiceError> {
    // The panel lists layers top first, which is also hit-test priority.
    for layer in layers
        .iter()
        .filter(|l| l.visible && l.effective_visible && l.selectable)
    {
        if let Some(id) = query(&layer.layer_id)?.last() {
            return Ok(Some((layer.layer_id.clone(), id.clone())));
        }
    }
    Ok(None)
}
