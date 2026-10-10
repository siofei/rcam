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
    pub unified_editor: Option<Arc<crate::unified_editor_worker::Reply>>,
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
    pub selection_epoch: u64,
    pub selection_geometry: Option<Arc<SelectionCentersResult>>,
    pub selection_geometry_identity: String,
    pub selection_geometry_error: Option<String>,
    pub metrics: crate::shared_snapshot::SnapshotVec<MetricsItem>,
    pub metrics_error: Option<String>,
    pub info: Option<DocumentInfo>,
    pub layers: crate::shared_snapshot::SnapshotVec<LayerInfo>,
    pub apertures: crate::shared_snapshot::SnapshotVec<editor_core::ApertureDefinition>,
    pub block_cache_stats: (usize, usize),
    pub definition_centers: Option<Arc<crate::point_adapter::DefinitionReply>>,
    pub point_preview: Option<Arc<crate::point_transform::Preview>>,
    pub array_preview: Option<Arc<crate::array_ui::Preview>>,
    pub block_preview: Option<Arc<crate::block_ui::Preview>>,
    pub block_counts: std::collections::HashMap<String, usize>,
    pub block_definitions: Vec<editor_core::block::BlockDefinition>,
    /// Immutable manufacturing snapshot plus its object envelope index. Object
    /// Snap queries these lazily around the cursor; no global point list exists.
    pub snap_snapshot: Option<Arc<RenderSnapshot>>,
    pub snap_index: Arc<crate::world_index::WorldIndex>,
    pub selected: crate::selection::SelectionSet,
    pub click_cycle: Option<crate::selection::ClickCycle>,
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
    pub move_admission: Option<Arc<crate::drag::MoveAdmission>>,
    pub drag_hit: bool,
    pub press_hit: Option<ObjectInfo>,
    /// Answer to the last `Action::LayerSummary`; drives the Delete Layer dialog.
    pub layer_summary: Option<LayerSummaryResult>,
    /// The last successful batch import, for the diagnostics summary.
    pub import: Option<ImportLayersResult>,
    /// The worker request whose import committed, including a later refresh failure.
    pub import_committed_task: Option<u64>,
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
    /// Incoming edit-selection policy. Inspection Replace/Remove keeps the
    /// separate selectable policy; Add and SelectAll cannot introduce locks.
    pub fn editable_object(&self, layer_id: &str, object: &editor_core::SemanticObject) -> bool {
        let Some(layer) = self.layers.iter().find(|l| l.layer_id == layer_id) else {
            return false;
        };
        let class = classify_object(object, &self.shapes);
        layer.visible
            && layer.effective_visible
            && layer.selectable
            && !layer.locked
            && layer
                .classes
                .iter()
                .find(|c| c.class == class)
                .is_none_or(|c| c.visible && c.selectable && !c.locked)
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
        editor_core::ObjectOrigin::GeneratedText { operation_id } => Some(operation_id),
        editor_core::ObjectOrigin::Imported { .. }
        | editor_core::ObjectOrigin::Generated { .. } => None,
    }
}
pub struct Model {
    pub(crate) recovery_key: Option<String>,
    recovery_reservation: Option<String>,
    pub(crate) reserved_paths: Vec<PathBuf>,
    pub(crate) active_cancel: Option<editor_service::task::CancellationToken>,
    pub service: ApplicationService,
    pub view: View,
    pub(crate) snapshot: Option<Arc<RenderSnapshot>>,
    pub(crate) metrics_identity: String,
    pub(crate) world_index: Arc<crate::world_index::WorldIndex>,
    pub(crate) viewport: Option<(MmPoint, BoundsMm)>,
    pub(crate) serial: u64,
    pub ppm: f64,
    pub(crate) block_display_cache: crate::block_display::BlockDisplayCache,
    pub(crate) unified_editor: Option<crate::unified_editor_worker::WorkerSession>,
}

/// Document-owned worker state. The service, executing token and scene serial
/// stay in the host when a state is moved or a replacement is rolled back.
pub(crate) struct ModelSessionState {
    recovery_key: Option<String>,
    recovery_reservation: Option<String>,
    view: View,
    snapshot: Option<Arc<RenderSnapshot>>,
    metrics_identity: String,
    world_index: Arc<crate::world_index::WorldIndex>,
    viewport: Option<(MmPoint, BoundsMm)>,
    ppm: f64,
    block_display_cache: crate::block_display::BlockDisplayCache,
    unified_editor: Option<crate::unified_editor_worker::WorkerSession>,
}
impl ModelSessionState {
    pub(crate) fn empty() -> Self {
        Self {
            unified_editor: None,
            recovery_key: None,
            recovery_reservation: None,
            view: View::default(),
            snapshot: None,
            metrics_identity: String::new(),
            world_index: Default::default(),
            viewport: None,
            ppm: 20.,
            block_display_cache: Default::default(),
        }
    }
    pub(crate) fn view(&self) -> &View {
        &self.view
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Legacy worker actions retained for native/regression callers.
pub enum PivotInput {
    SelectionCenter,
    WorldOrigin,
    Custom(String, String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Legacy worker actions retained for native/regression callers.
pub enum MirrorDirection {
    Horizontal,
    Vertical,
}
pub enum Action {
    UnifiedEditor(Box<crate::unified_editor_worker::Request>),
    DefinitionCenters(crate::point_input::Context, String),
    PointPreview(Box<crate::point_transform::Request>),
    PointApply(Box<crate::point_transform::Request>),
    SelectionCenters(String, SelectionCentersParams),
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
    #[allow(dead_code)] // Original byte restore path retained for internal fixtures.
    RestoreProject(Vec<u8>),
    RestoreSnapshot(PathBuf, crate::recovery::RecoveryMetadata),
    RecoveryWrite(PathBuf),
    /// Start an empty Workspace (refuses while there are unexported edits).
    NewWorkspace,
    /// Same, after the user explicitly agreed to lose unexported edits.
    #[allow(dead_code)] // Explicit replacement retained for internal evidence.
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
    CanvasSelect(
        crate::selection::ClickContext,
        crate::selection::SelectionMode,
    ),
    SelectRect(BoundsMm, editor_core::hit_test::SelectRectMode),
    CanvasSelectRect(
        BoundsMm,
        editor_core::hit_test::SelectRectMode,
        crate::selection::SelectionMode,
    ),
    SelectAll,
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
            unified_editor: None,
            recovery_key: None,
            recovery_reservation: None,
            reserved_paths: Vec::new(),
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

pub(crate) fn selection_geometry_identity(view: &View) -> String {
    format!(
        "{:?}:{:?}:{:?}",
        editor_service::task::TaskVersion::capture(
            view.info.as_ref(),
            view.task_generation,
            view.rule_revision
        ),
        view.selection_epoch,
        "selected-material-v1"
    )
}

pub fn selected_center(view: &View) -> Result<MmPoint, ServiceError> {
    Ok(selected_bounds(view)?.center())
}

impl Model {
    pub(crate) fn take_session_state(&mut self) -> ModelSessionState {
        ModelSessionState {
            unified_editor: self.unified_editor.take(),
            recovery_key: self.recovery_key.take(),
            recovery_reservation: self.recovery_reservation.take(),
            view: std::mem::take(&mut self.view),
            snapshot: self.snapshot.take(),
            metrics_identity: std::mem::take(&mut self.metrics_identity),
            world_index: std::mem::take(&mut self.world_index),
            viewport: self.viewport.take(),
            ppm: std::mem::replace(&mut self.ppm, 20.),
            block_display_cache: std::mem::take(&mut self.block_display_cache),
        }
    }

    pub(crate) fn install_session_state(&mut self, state: ModelSessionState) {
        self.unified_editor = state.unified_editor;
        self.recovery_key = state.recovery_key;
        self.recovery_reservation = state.recovery_reservation;
        self.view = state.view;
        self.snapshot = state.snapshot;
        self.metrics_identity = state.metrics_identity;
        self.world_index = state.world_index;
        self.viewport = state.viewport;
        self.ppm = state.ppm;
        self.block_display_cache = state.block_display_cache;
    }

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
    pub(crate) fn info(&self) -> Result<DocumentInfo, ServiceError> {
        self.view
            .info
            .clone()
            .ok_or_else(|| error("NOT_FOUND", "尚未打开文件"))
    }
    pub(crate) fn editable(&self) -> Result<(), ServiceError> {
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
        let old = self.take_session_state();
        self.view = View {
            info: Some(candidate),
            message: "文件已打开".into(),
            ..Default::default()
        };
        let prepared = self.refresh(true).and_then(|()| match &self.view.blocked {
            Some(reason) => Err(error("UNSUPPORTED_FEATURE", reason)),
            None => Ok(()),
        });
        if let Err(cause) = prepared {
            return self.rollback_candidate(old, cause);
        }
        if let Some(info) = &old.view.info
            && let Err(cause) = self.service.close(&info.document_id, &info.revision, false)
        {
            return self.rollback_candidate(old, cause);
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
        crate::session_files::check_reserved(path, &self.reserved_paths)?;
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
        self.install_project_prepared(candidate, discard, |model| {
            model
                .refresh(true)
                .and_then(|()| match &model.view.blocked {
                    Some(reason) => Err(error("UNSUPPORTED_FEATURE", reason)),
                    None => Ok(()),
                })
        })
    }
    fn install_project_prepared(
        &mut self,
        candidate: DocumentInfo,
        discard: bool,
        prepare: impl FnOnce(&mut Self) -> Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let old = self.take_session_state();
        self.view = View {
            info: Some(candidate),
            message: "工程已打开".into(),
            ..Default::default()
        };
        let prepared = prepare(self);
        if let Err(cause) = prepared {
            return self.rollback_candidate(old, cause);
        }
        if let Some(info) = &old.view.info
            && let Err(cause) = self
                .service
                .close(&info.document_id, &info.revision, discard)
        {
            return self.rollback_candidate(old, cause);
        }
        Ok(())
    }
    fn rollback_candidate(
        &mut self,
        old: ModelSessionState,
        cause: ServiceError,
    ) -> Result<(), ServiceError> {
        let candidate = self.view.info.clone();
        self.install_session_state(old);
        if let Some(candidate) = candidate {
            self.service
                .close(&candidate.document_id, &candidate.revision, true)?;
        }
        Err(cause)
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
            crate::session_files::check_reserved(path, &self.reserved_paths)?;
        }
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
        if let Some(old) = &self.view.info
            && let Err(cause) = self.service.close(&old.document_id, &old.revision, true)
        {
            self.service
                .close(&fresh.document_id, &fresh.revision, true)?;
            return Err(cause);
        }
        // Retiring the old service record succeeded; no old derived state may
        // survive the replacement, even if the new display refresh fails.
        drop(self.take_session_state());
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
        self.view.layers = layers.into();
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
            self.view.apertures = snapshot.apertures.clone().into();
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
        self.view.selected.ordered = selected.into();
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
        let Some(serial) = self.serial.checked_add(1) else {
            self.view.scene = None;
            self.view.blocked = Some("RESOURCE_LIMIT: display scene identity exhausted".into());
            return;
        };
        self.serial = serial;
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
        self.hit_with_policy(point, tolerance, false)
    }
    fn hit_with_policy(
        &self,
        point: MmPoint,
        tolerance: f64,
        editable_only: bool,
    ) -> Result<Option<ObjectInfo>, ServiceError> {
        #[cfg(feature = "internal-evidence")]
        let _pmix_measure = crate::native_pmix::HitTimer::start(point, tolerance);
        #[cfg(feature = "internal-evidence")]
        let _measure = crate::native_s5m1::HitTimer::start();
        self.editable()?;
        let id = self.info()?.document_id;
        let classifier =
            editable_only.then(|| Classifier::new(&self.view.layers, &self.view.apertures));
        let hit = topmost_hit(&self.view.layers, |layer| {
            let mut ids = self
                .service
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
                .map(|r| r.object_ids)?;
            if editable_only && !ids.is_empty() {
                let source = self
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.layers.iter().find(|l| l.id == layer))
                    .ok_or_else(|| error("NOT_FOUND", "点选快照缺少图层"))?;
                let wanted: std::collections::HashSet<_> = ids.iter().map(String::as_str).collect();
                let mut allowed = std::collections::HashSet::new();
                for (index, object) in source.objects.iter().enumerate() {
                    if index % 256 == 0
                        && let Some(cancel) = &self.active_cancel
                    {
                        cancel.checkpoint()?;
                    }
                    if wanted.contains(object.object_id.as_str())
                        && classifier
                            .as_ref()
                            .is_some_and(|c| c.editable_object(layer, object))
                    {
                        allowed.insert(object.object_id.clone());
                    }
                }
                ids.retain(|id| allowed.contains(id));
            }
            Ok(ids)
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
        let hit = self.hit_with_policy(
            point,
            tolerance,
            mode == crate::selection::SelectionMode::Add,
        )?;
        self.select_hit(hit, mode)?;
        self.view.click_cycle = None;
        Ok(())
    }
    fn select_hit(
        &mut self,
        hit: Option<ObjectInfo>,
        mode: crate::selection::SelectionMode,
    ) -> Result<(), ServiceError> {
        if let Some(hit) = &hit
            && let Some(operation_id) = operation_id(&hit.object.origin)
            && let Some(snapshot) = &self.snapshot
        {
            let mut objects = Vec::new();
            for layer in snapshot.layers.iter().filter(|l| l.id == hit.layer_id) {
                for (index, object) in layer.objects.iter().enumerate() {
                    if index % 256 == 0
                        && let Some(cancel) = &self.active_cancel
                    {
                        cancel.checkpoint()?;
                    }
                    if self::operation_id(&object.origin) == Some(operation_id) {
                        objects.push((layer.id.as_str(), object));
                    }
                }
            }
            self.view.selected.apply_checked(objects, mode, || {
                self.active_cancel
                    .as_ref()
                    .map_or(Ok(()), |c| c.checkpoint())
            })?;
        } else {
            self.view.selected.apply_checked(
                hit.as_ref().map(|o| (o.layer_id.as_str(), &o.object)),
                mode,
                || {
                    self.active_cancel
                        .as_ref()
                        .map_or(Ok(()), |c| c.checkpoint())
                },
            )?;
        }
        Ok(())
    }
    fn canvas_select(
        &mut self,
        context: crate::selection::ClickContext,
        mode: crate::selection::SelectionMode,
    ) -> Result<(), ServiceError> {
        #[cfg(feature = "internal-evidence")]
        let _pmix_measure = crate::native_pmix::HitTimer::start(context.world, context.tolerance);
        #[cfg(feature = "internal-evidence")]
        let _measure = crate::native_s5m1::HitTimer::start();
        self.editable()?;
        let document = self.info()?;
        let classifier = (mode == crate::selection::SelectionMode::Add)
            .then(|| Classifier::new(&self.view.layers, &self.view.apertures));
        let mut candidates = Vec::new();
        let mut logical = std::collections::HashMap::new();
        let mut order = 0usize;
        for layer in self
            .view
            .layers
            .iter()
            .filter(|l| l.visible && l.effective_visible && l.selectable)
        {
            if let Some(cancel) = &self.active_cancel {
                cancel.checkpoint()?;
            }
            let hits = self.service.objects_hit_test_scored_with_cancel(
                &document.document_id,
                HitTestParams {
                    layer_id: layer.layer_id.clone(),
                    point: HitTestPoint {
                        x_mm: context.world.x_mm,
                        y_mm: context.world.y_mm,
                    },
                    tolerance_mm: context.tolerance,
                    selectable_only: true,
                },
                self.active_cancel.as_ref(),
            )?;
            if hits.hits.is_empty() {
                continue;
            }
            let wanted: std::collections::HashSet<_> =
                hits.hits.iter().map(|hit| hit.object_id.as_str()).collect();
            let source = self
                .snapshot
                .as_ref()
                .and_then(|s| s.layers.iter().find(|l| l.id == layer.layer_id))
                .ok_or_else(|| error("NOT_FOUND", "点选快照缺少图层"))?;
            let mut found = std::collections::HashMap::with_capacity(wanted.len());
            for (index, object) in source.objects.iter().enumerate() {
                if index % 256 == 0
                    && let Some(cancel) = &self.active_cancel
                {
                    cancel.checkpoint()?;
                }
                if wanted.contains(object.object_id.as_str()) {
                    found.insert(object.object_id.as_str(), object);
                }
            }
            for hit in hits.hits.iter().rev() {
                if let Some(cancel) = &self.active_cancel {
                    cancel.checkpoint()?;
                }
                let object = found
                    .get(hit.object_id.as_str())
                    .ok_or_else(|| error("NOT_FOUND", "点选快照缺少对象"))?;
                if classifier
                    .as_ref()
                    .is_some_and(|c| !c.editable_object(&layer.layer_id, object))
                {
                    continue;
                }
                // GeneratedText members share the same layer and class policy,
                // so this eligibility cannot split a logical text operation.
                crate::selection::merge_click_candidate(
                    &mut candidates,
                    &mut logical,
                    &layer.layer_id,
                    object,
                    hit,
                    order,
                );
                order = order.saturating_add(1);
                if candidates.len() > editor_core::edit::MAX_MOVE_OBJECTS {
                    return Err(error("RESOURCE_LIMIT", "点选候选过多"));
                }
            }
        }
        let mut previous = self.view.click_cycle.as_ref().filter(|c| {
            c.context.same_place(&context)
                && c.document == document.document_id
                && c.revision == document.revision
                && c.workspace == document.workspace_revision
                && c.logical_candidates.len() == logical.len()
        });
        if let Some(cycle) = previous {
            for (index, key) in logical.keys().enumerate() {
                if index % 256 == 0
                    && let Some(cancel) = &self.active_cancel
                {
                    cancel.checkpoint()?;
                }
                if !cycle.logical_candidates.contains(key) {
                    previous = None;
                    break;
                }
            }
        }
        // Keep the original screen anchor and ranked representatives. Scores
        // may cross under small pointer motion without changing the candidate set.
        let (candidates, logical_candidates, index, anchor) = if let Some(cycle) = previous {
            (
                cycle.candidates.clone(),
                cycle.logical_candidates.clone(),
                if mode == crate::selection::SelectionMode::Replace {
                    (cycle.index + 1) % cycle.candidates.len().max(1)
                } else {
                    cycle.index
                },
                cycle.context.clone(),
            )
        } else {
            crate::selection::rank_click_candidates(&mut candidates);
            let candidates: crate::shared_snapshot::SnapshotVec<_> = candidates
                .into_iter()
                .map(|candidate| candidate.representative)
                .collect();
            (
                candidates,
                Arc::new(logical.into_keys().collect()),
                0,
                context.clone(),
            )
        };
        let hit = candidates
            .get(index)
            .map(|(layer_id, object_id)| {
                self.service.objects_get(
                    &document.document_id,
                    ObjectParams {
                        layer_id: layer_id.clone(),
                        object_id: object_id.clone(),
                    },
                )
            })
            .transpose()?;
        if let Some(cancel) = &self.active_cancel {
            cancel.checkpoint()?;
        }
        self.select_hit(hit, mode)?;
        self.view.click_cycle =
            if candidates.is_empty() || mode != crate::selection::SelectionMode::Replace {
                None
            } else {
                Some(crate::selection::ClickCycle {
                    context: anchor,
                    document: document.document_id,
                    revision: document.revision,
                    workspace: document.workspace_revision,
                    candidates,
                    logical_candidates,
                    index,
                })
            };
        Ok(())
    }
    fn select_rect(
        &mut self,
        rect_mm: BoundsMm,
        mode: editor_core::hit_test::SelectRectMode,
        selection_mode: crate::selection::SelectionMode,
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
        let classifier = (selection_mode == crate::selection::SelectionMode::Add)
            .then(|| Classifier::new(&self.view.layers, &self.view.apertures));
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
            let wanted: std::collections::HashSet<_> =
                result.object_ids.iter().map(String::as_str).collect();
            let mut objects = std::collections::HashMap::with_capacity(wanted.len());
            for (index, object) in layer.objects.iter().enumerate() {
                if index % 256 == 0
                    && let Some(cancel) = &self.active_cancel
                {
                    cancel.checkpoint()?;
                }
                if wanted.contains(object.object_id.as_str()) {
                    objects.insert(object.object_id.as_str(), object);
                }
            }
            for object_id in result.object_ids {
                if let Some(cancel) = &self.active_cancel {
                    cancel.checkpoint()?;
                }
                let object = objects
                    .get(object_id.as_str())
                    .ok_or_else(|| error("NOT_FOUND", "框选快照缺少对象"))?;
                if classifier
                    .as_ref()
                    .is_none_or(|c| c.editable_object(&l.layer_id, object))
                {
                    selected.push((l.layer_id.as_str(), *object));
                }
            }
        }
        self.view
            .selected
            .apply_checked(selected, selection_mode, || {
                self.active_cancel
                    .as_ref()
                    .map_or(Ok(()), |c| c.checkpoint())
            })?;
        Ok(())
    }
    fn select_all(&mut self) -> Result<(), ServiceError> {
        self.editable()?;
        let document = self.info()?;
        let snapshot = self
            .snapshot
            .as_ref()
            .filter(|s| s.document_id == document.document_id && s.revision == document.revision)
            .ok_or_else(|| error("STALE_REVISION", "全选快照未确认"))?;
        let classifier = Classifier::new(&self.view.layers, &self.view.apertures);
        let mut selected = Vec::new();
        // Panel order and original exposure order; no viewport/renderer subset.
        for layer in &self.view.layers {
            let source = snapshot
                .layers
                .iter()
                .find(|l| l.id == layer.layer_id)
                .ok_or_else(|| error("NOT_FOUND", "全选快照缺少图层"))?;
            for (index, object) in source.objects.iter().enumerate() {
                if index % 256 == 0
                    && let Some(cancel) = &self.active_cancel
                {
                    cancel.checkpoint()?;
                }
                if classifier.editable_object(&source.id, object) {
                    selected.push((source.id.as_str(), object));
                }
            }
        }
        if let Some(cancel) = &self.active_cancel {
            cancel.checkpoint()?;
        }
        self.view.selected.apply_checked(
            selected,
            crate::selection::SelectionMode::Replace,
            || {
                self.active_cancel
                    .as_ref()
                    .map_or(Ok(()), |c| c.checkpoint())
            },
        )?;
        Ok(())
    }
    pub(crate) fn edit_groups(&self) -> Result<Vec<SelectionGroup>, ServiceError> {
        self.view
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
        }
        Ok(self.view.selected.groups())
    }
    fn edit_targets(&self) -> Result<(String, Vec<String>), ServiceError> {
        let mut groups = self.edit_groups()?;
        if groups.len() != 1 {
            return Err(error(
                "CROSS_LAYER_EDIT_UNSUPPORTED",
                "此操作要求同层对象，整组操作已拒绝",
            ));
        }
        let group = groups.remove(0);
        Ok((group.layer_id, group.object_ids))
    }
    pub(crate) fn submit_selection(
        &mut self,
        document: &str,
        revision: &str,
        groups: Vec<SelectionGroup>,
        operation: SelectionEdit,
    ) -> Result<Vec<SelectionGroup>, ServiceError> {
        if groups.len() != 1 {
            return self
                .service
                .objects_edit_selection(
                    document,
                    revision,
                    EditSelectionParams { groups, operation },
                )
                .map(|r| r.groups);
        }
        // Preserve the accepted single-layer command/cache fast path.
        let group = &groups[0];
        let layer_id = group.layer_id.clone();
        let object_ids = group.object_ids.clone();
        let result = match operation {
            SelectionEdit::Move { dx_mm, dy_mm } => self.service.objects_move(
                document,
                revision,
                MoveParams {
                    layer_id: layer_id.clone(),
                    object_ids,
                    dx_mm,
                    dy_mm,
                },
            ),
            SelectionEdit::Rotate {
                angle_deg,
                pivot_mm,
            } => self.service.objects_rotate(
                document,
                revision,
                RotateParams {
                    layer_id: layer_id.clone(),
                    object_ids,
                    angle_deg,
                    pivot_mm: PivotMm {
                        x_mm: pivot_mm.x_mm,
                        y_mm: pivot_mm.y_mm,
                    },
                },
            ),
            SelectionEdit::Mirror { axis } => self.service.objects_mirror(
                document,
                revision,
                MirrorParams {
                    layer_id: layer_id.clone(),
                    object_ids,
                    axis,
                },
            ),
            SelectionEdit::Duplicate { dx_mm, dy_mm } => self.service.objects_duplicate(
                document,
                revision,
                DuplicateParams {
                    layer_id: layer_id.clone(),
                    object_ids,
                    dx_mm,
                    dy_mm,
                },
            ),
            SelectionEdit::Delete => self.service.objects_delete(
                document,
                revision,
                DeleteParams {
                    layer_id: layer_id.clone(),
                    object_ids,
                },
            ),
        }?;
        Ok(vec![SelectionGroup {
            layer_id,
            object_ids: result.changed_object_ids,
        }])
    }
    pub fn numeric_move(&mut self, dx: &str, dy: &str) -> Result<(), ServiceError> {
        self.editable()?;
        let (dx, dy) = (finite(dx, "ΔX")?, finite(dy, "ΔY")?);
        if dx == 0. && dy == 0. {
            self.view.message = "位移为零，未提交修改".into();
            return Ok(());
        }
        let d = self.info()?;
        let groups = self.edit_groups()?;
        self.submit_selection(
            &d.document_id,
            &d.revision,
            groups,
            SelectionEdit::Move {
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
        let groups = self.edit_groups()?;
        let pivot = match pivot {
            PivotInput::SelectionCenter => selected_center(&self.view)?,
            PivotInput::WorldOrigin => MmPoint::new(0., 0.),
            PivotInput::Custom(x, y) => {
                MmPoint::new(finite(&x, "Pivot X")?, finite(&y, "Pivot Y")?)
            }
        };
        let document = self.info()?;
        self.submit_selection(
            &document.document_id,
            &document.revision,
            groups,
            SelectionEdit::Rotate {
                angle_deg,
                pivot_mm: pivot,
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
        let groups = self.edit_groups()?;
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
        self.submit_selection(
            &document.document_id,
            &document.revision,
            groups,
            SelectionEdit::Mirror { axis },
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
    pub(crate) fn refresh_metrics(&mut self) {
        let identity = format!(
            "{:?}:{:?}:{:?}",
            editor_service::task::TaskVersion::capture(
                self.view.info.as_ref(),
                self.view.task_generation,
                self.view.rule_revision
            ),
            self.view.selected.ids(),
            self.view.selected.primary().map(|o| &o.object.object_id)
        );
        if self.metrics_identity == identity {
            return;
        }
        self.metrics_identity = identity;
        self.view.selection_epoch = self.view.selection_epoch.wrapping_add(1);
        self.view.move_admission = None;
        self.view.selection_geometry = None;
        self.view.selection_geometry_identity.clear();
        self.view.selection_geometry_error = None;
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
        self.view.import = None;
        self.view.import_committed_task = None;
        let old_viewport = self.viewport;
        let old_ppm = self.ppm;
        let readonly = matches!(
            &action,
            Action::SelectionCenters(..)
                | Action::PointPreview(..)
                | Action::DefinitionCenters(..)
                | Action::Viewport(..)
                | Action::Select(..)
                | Action::CanvasSelect(..)
                | Action::SelectRect(..)
                | Action::CanvasSelectRect(..)
                | Action::SelectAll
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
        // Read cancellation restores View, including its published epoch. Its
        // metrics key must be restored with it, or the next read advances the
        // worker epoch without a corresponding full View publication to App.
        let old_metrics_identity = readonly.then(|| self.metrics_identity.clone());
        let cancellable_import = matches!(&action, Action::ImportGerbers(..));
        let cancellable_draft = matches!(&action, Action::UnifiedEditor(..));
        let result = (|| {
            task.cancel_token.start()?;
            task.validate(&self.task_version()?)?;
            if !readonly && !cancellable_import && !cancellable_draft {
                task.cancel_token.begin_commit()?;
                #[cfg(feature = "internal-evidence")]
                crate::native_a2::committing(&task);
            }
            self.active_cancel = Some(task.cancel_token.clone());
            self.run(action);
            if cancellable_import && self.view.import.is_some() {
                self.view.import_committed_task = Some(task.task_id);
            }
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
                if let Some(identity) = old_metrics_identity {
                    self.metrics_identity = identity;
                }
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
        let probe_only = self.unified_editor.is_some()
            || matches!(&action, Action::ProbeDrag(..) | Action::UnifiedEditor(..));
        let reset_cycle = !matches!(
            &action,
            Action::SelectionCenters(..)
                | Action::PointPreview(..)
                | Action::DefinitionCenters(..)
                | Action::CanvasSelect(..)
                | Action::ProbeDrag(..)
                | Action::Viewport(..)
                | Action::RecoveryWrite(..)
        );
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
        self.view.import_committed_task = None;
        self.view.removed = None;
        self.view.focus_bounds = None;
        if self.unified_editor.is_some()
            && !matches!(
                &action,
                Action::UnifiedEditor(..) | Action::RecoveryWrite(..)
            )
        {
            self.view.error = Some(error("INVALID_ARGUMENT", "请先结束当前编辑会话"));
            return;
        }
        let result = (|| match action {
            Action::UnifiedEditor(request) => self.unified_editor_action(*request),
            Action::DefinitionCenters(context, definition) => {
                self.definition_centers(context, definition)
            }
            Action::PointPreview(request) => self.point_preview(*request),
            Action::PointApply(request) => self.point_apply(*request),
            Action::SelectionCenters(identity, params) => {
                if identity != selection_geometry_identity(&self.view)
                    || params.groups != self.view.selected.groups()
                {
                    return Err(error("STALE_TASK", "选择几何请求上下文已失效"));
                }
                let d = self.info()?;
                let token = self.active_cancel.clone();
                let result = self.service.geometry_selection_centers_cancellable(
                    &d.document_id,
                    &d.revision,
                    params,
                    || token.as_ref().is_some_and(|t| t.checkpoint().is_err()),
                );
                self.view.selection_geometry_identity = identity;
                match result {
                    Ok(value) => {
                        self.view.selection_geometry = Some(Arc::new(value));
                        self.view.selection_geometry_error = None;
                        Ok(())
                    }
                    Err(e) => {
                        self.view.selection_geometry = None;
                        self.view.selection_geometry_error =
                            Some(format!("{}: {}", e.code, e.message));
                        if e.code == "CANCELLED" {
                            Err(e)
                        } else {
                            Ok(())
                        }
                    }
                }
            }
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
            Action::RestoreSnapshot(dir, metadata) => {
                let bytes = crate::recovery::load(&dir, &metadata)
                    .map_err(|_| error("IO_ERROR", "恢复快照损坏或无法读取；原记录已保留"))?;
                let result = self.restore_project(&bytes);
                crate::recovery::event(if result.is_ok() {
                    "recovery.restore_success"
                } else {
                    "recovery.restore_failed"
                });
                result
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
                    if let Some(key) = self.recovery_key.as_ref()
                        && self.recovery_reservation.as_ref() != Some(key)
                    {
                        crate::recovery::reserve_record(&dir, &d.project_id, key).map_err(
                            |_| error("IO_ERROR", "恢复记录身份无法独占；现有恢复记录已保留"),
                        )?;
                        self.recovery_reservation = Some(key.clone());
                    }
                    let bytes = self.service.project_recovery_bytes(&d.document_id)?;
                    crate::recovery::write_scoped(&dir, &d, &bytes, self.recovery_key.as_deref())
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
            Action::CanvasSelect(context, mode) => self.canvas_select(context, mode),
            Action::SelectRect(r, m) => {
                self.select_rect(r, m, crate::selection::SelectionMode::Replace)
            }
            Action::CanvasSelectRect(r, m, selection_mode) => {
                self.select_rect(r, m, selection_mode)
            }
            Action::SelectAll => self.select_all(),
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
                self.view.move_admission = None;
                self.view.drag_hit = false;
                self.view.press_hit = self.hit(p, tolerance_mm)?;
                if crate::drag::editable_selection(&self.view) {
                    let d = self.info()?;
                    for group in self.view.selected.groups() {
                        let hits = self.service.objects_hit_test(
                            &d.document_id,
                            HitTestParams {
                                layer_id: group.layer_id.clone(),
                                point: HitTestPoint {
                                    x_mm: p.x_mm,
                                    y_mm: p.y_mm,
                                },
                                tolerance_mm,
                                selectable_only: true,
                            },
                        )?;
                        if hits
                            .object_ids
                            .iter()
                            .any(|id| self.view.selected.contains(&group.layer_id, id))
                        {
                            self.view.drag_hit = true;
                            break;
                        }
                    }
                }
                if self.view.drag_hit {
                    if let Some(cancel) = &self.active_cancel {
                        cancel.checkpoint()?;
                    }
                    let d = self.info()?;
                    let result = self.edit_groups().and_then(|groups| {
                        self.service.selection_move_demand_with_cancel(
                            &d.document_id,
                            &d.revision,
                            &groups,
                            self.active_cancel.as_ref(),
                        )
                    });
                    if let Some(cancel) = &self.active_cancel {
                        cancel.checkpoint()?;
                    }
                    self.view.move_admission = Some(Arc::new(crate::drag::MoveAdmission::new(
                        &self.view, result,
                    )));
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
                let groups = self.edit_groups()?;
                if groups != drag.groups
                    || self
                        .view
                        .selected
                        .ids()
                        .iter()
                        .copied()
                        .ne(drag.objects.iter().map(String::as_str))
                {
                    return Err(error("INVALID_ARGUMENT", "拖动期间选择已改变"));
                }
                self.submit_selection(
                    &drag.document,
                    &drag.revision,
                    groups,
                    SelectionEdit::Move {
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
                let groups = self.edit_groups()?;
                let duplicate = matches!(action, Action::Duplicate);
                let groups = self.submit_selection(
                    &d.document_id,
                    &d.revision,
                    groups,
                    if duplicate {
                        SelectionEdit::Duplicate {
                            dx_mm: 0.,
                            dy_mm: 0.,
                        }
                    } else {
                        SelectionEdit::Delete
                    },
                )?;
                let mut selected = Vec::new();
                if duplicate {
                    for group in groups {
                        for object_id in group.object_ids {
                            selected.push(self.service.objects_get(
                                &d.document_id,
                                ObjectParams {
                                    layer_id: group.layer_id.clone(),
                                    object_id,
                                },
                            )?);
                        }
                    }
                }
                self.view.selected.ordered = selected.into();
                self.view.message = if duplicate {
                    "已原位复制，可拖动副本"
                } else {
                    "已删除对象"
                }
                .into();
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
                if let Some(d) = &self.view.info {
                    self.service.close(&d.document_id, &d.revision, discard)?;
                }
                self.view = View::default();
                self.snapshot = None;
                self.viewport = None;
                self.world_index = Default::default();
                self.block_display_cache = Default::default();
                self.metrics_identity.clear();
                Ok(())
            }
        })();
        if result.is_ok() && reset_cycle {
            self.view.click_cycle = None;
        }
        if let Err(e) = result {
            eprintln!("service_error {} {} {}", e.code, e.message, e.details);
            self.view.error = Some(e);
        }
        if !probe_only
            && self
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

#[cfg(test)]
pub(crate) mod session_tests {
    use super::*;
    use editor_core::block::{BlockObjectGeometry, BlockTransform};

    pub(crate) fn fixture() -> Model {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rcam-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.gbr");
        std::fs::write(&path, b"%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*X2000000Y2000000D03*X0Y0D02*G01X2000000Y0D01*G75*X1000000Y0D02*G03X0Y1000000I-1000000J0D01*M02*").unwrap();
        let mut model = Model::default();
        model.open(&path).unwrap();
        model.run(Action::SelectAll);
        let (layer, objects) = crate::block_ui::create_targets(&model.view).unwrap();
        let context = crate::block_ui::Context::capture(&model.view).unwrap();
        model.run(Action::BlockEdit(Box::new(crate::block_ui::Request {
            context,
            edit: crate::block_ui::Edit::Create(CreateBlockDefinitionParams {
                layer_id: layer,
                object_ids: objects,
                local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
                name: "session fixture".into(),
            }),
        })));
        assert!(model.view.error.is_none());
        assert!(
            model.view.block_definitions[0]
                .objects
                .iter()
                .any(|o| matches!(o.geometry, BlockObjectGeometry::Flash { .. }))
        );
        assert!(
            model.view.block_definitions[0]
                .objects
                .iter()
                .any(|o| matches!(o.geometry, BlockObjectGeometry::Arc { .. }))
        );
        assert!(model.block_display_cache.stats().0 > 0);
        std::fs::remove_dir_all(dir).unwrap();
        model
    }
    pub(crate) fn variant_bytes(model: &Model) -> Vec<u8> {
        let id = &model.view.info.as_ref().unwrap().document_id;
        let mut project = model.service.project_snapshot(id).unwrap();
        for object in &mut project.block_definitions[0].objects {
            if let BlockObjectGeometry::Flash { center, .. } = &mut object.geometry {
                center.x_mm += 40.;
            }
        }
        rcam_project::encode_v1(&project).unwrap()
    }
    fn cache_truth(model: &mut Model) -> Vec<editor_core::block::ResolvedBlockObject> {
        model
            .block_display_cache
            .resolve(&model.view.block_definitions[0], &BlockTransform::IDENTITY)
            .unwrap()
    }
    #[test]
    fn move_complete_state_retains_arcs_metrics_viewport_and_cache() {
        let mut model = fixture();
        model.recovery_key = Some("retained key".into());
        model.recovery_reservation = Some("retained reservation".into());
        let snapshot = model.snapshot.clone().unwrap();
        let index = model.world_index.clone();
        let scene = model.view.scene.clone().unwrap();
        let selected = model.view.selected.ordered.clone();
        let metrics = model.metrics_identity.clone();
        let truth = cache_truth(&mut model);
        let serial = model.serial;
        model.viewport = Some((
            MmPoint::new(8., 9.),
            BoundsMm {
                min_x_mm: -5.,
                min_y_mm: -5.,
                max_x_mm: 20.,
                max_y_mm: 20.,
            },
        ));
        model.ppm = 37.;
        let state = model.take_session_state();
        assert!(model.view.info.is_none() && model.snapshot.is_none());
        assert_eq!(model.block_display_cache.stats(), (0, 0));
        model.install_session_state(state);
        assert_eq!(model.recovery_key.as_deref(), Some("retained key"));
        assert_eq!(
            model.recovery_reservation.as_deref(),
            Some("retained reservation")
        );
        assert!(Arc::ptr_eq(&snapshot, model.snapshot.as_ref().unwrap()));
        assert!(Arc::ptr_eq(&index, &model.world_index));
        assert!(Arc::ptr_eq(&scene, model.view.scene.as_ref().unwrap()));
        assert!(selected.shares_storage(&model.view.selected.ordered));
        assert_eq!(model.metrics_identity, metrics);
        assert_eq!(model.ppm.to_bits(), 37_f64.to_bits());
        assert_eq!(model.viewport.unwrap().0, MmPoint::new(8., 9.));
        assert_eq!(cache_truth(&mut model), truth);
        assert_eq!(model.serial, serial);
        assert_eq!(
            model.task_version().unwrap().document_id,
            model.view.info.as_ref().map(|d| d.document_id.clone())
        );
    }
    #[test]
    fn candidate_prepare_cleanup_and_old_close_failures_restore_whole_source() {
        for fault in ["prepare", "cleanup", "old-close"] {
            let mut model = fixture();
            model.recovery_key = Some("source key".into());
            model.recovery_reservation = Some("source reservation".into());
            let before = model.view.info.clone();
            let snapshot = model.snapshot.clone().unwrap();
            let scene = model.view.scene.clone().unwrap();
            let index = model.world_index.clone();
            let metrics = model.metrics_identity.clone();
            let truth = cache_truth(&mut model);
            let serial = model.serial;
            let bytes = variant_bytes(&model);
            let candidate = model.service.project_restore(&bytes).unwrap();
            let candidate_id = candidate.document_id.clone();
            let result = model.install_project_prepared(candidate, false, |model| {
                model.refresh(true)?;
                assert_ne!(cache_truth(model), truth); // colliding ID/revision, distinct real geometry
                if fault == "cleanup" {
                    let info = model.view.info.as_ref().unwrap();
                    model
                        .service
                        .close(&info.document_id, &info.revision, true)?;
                }
                if fault == "old-close" {
                    Ok(())
                } else {
                    Err(error("DISPLAY_TEST", "injected after real prepare"))
                }
            });
            assert!(result.is_err());
            assert_eq!(model.view.info, before);
            assert_eq!(model.recovery_key.as_deref(), Some("source key"));
            assert_eq!(
                model.recovery_reservation.as_deref(),
                Some("source reservation")
            );
            assert!(Arc::ptr_eq(&snapshot, model.snapshot.as_ref().unwrap()));
            assert!(Arc::ptr_eq(&scene, model.view.scene.as_ref().unwrap()));
            assert!(Arc::ptr_eq(&index, &model.world_index));
            assert_eq!(model.metrics_identity, metrics);
            assert_eq!(cache_truth(&mut model), truth);
            assert!(model.service.document_get(&candidate_id).is_err());
            assert!(model.serial > serial); // candidate serial is consumed, never rolled back
            assert!(model.task_version().is_ok());
        }
    }
    #[test]
    fn new_close_failure_keeps_source_and_scene_serial_exhaustion_refuses() {
        let mut model = fixture();
        model.view.info.as_mut().unwrap().revision = "999999".into();
        let before = model.view.info.clone();
        let scene = model.view.scene.clone().unwrap();
        assert!(model.new_workspace(true).is_err());
        assert_eq!(model.view.info, before);
        assert!(Arc::ptr_eq(&scene, model.view.scene.as_ref().unwrap()));
        let id = &model.view.info.as_ref().unwrap().document_id;
        model.view.info = Some(model.service.document_get(id).unwrap());
        model.serial = u64::MAX;
        model.rebuild();
        assert!(model.view.scene.is_none());
        assert!(
            model
                .view
                .blocked
                .as_deref()
                .unwrap()
                .starts_with("RESOURCE_LIMIT")
        );
        assert_eq!(model.serial, u64::MAX);
    }
}
