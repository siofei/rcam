//! UI-free application boundary retained by the app and headless callers.
//!
//! S0 compatibility remains read-only; the host-authorized S1-A path adds
//! semantic queries, atomic Move/Undo/Redo, validation and safe new-path export.

mod selection_edit;
pub use selection_edit::*;
mod alignment;
mod array;
mod candidates;
mod components;
pub mod task;
pub use candidates::*;
mod pnp_input;
pub use alignment::{AlignParams, AlignmentMode, DistributeParams, DistributionAxis};
pub use array::ArrayRectangularParams;
pub use components::*;
mod grip;
mod metrics;
pub use grip::GripEditParams;
mod content_state;
mod project;
mod text;
mod workspace;
pub use editor_core::edit::MirrorAxis;
pub use editor_core::units::ManufacturingPrecision;
pub use editor_core::workspace::{
    ClassInteractionStyle, Color, ColorMode, DeleteRisk, DisplayClass, ImportProvenance,
    LayerContentSummary, LayerDisplayMode, LayerKind, LayerViewStyle, LayerWorkspaceState,
    auto_layer_color, class_variant_color,
};
pub use workspace::*;

use editor_core::edit::{
    BatchEdit, EditError, EditHistory, MAX_HISTORY_BYTES, MAX_HISTORY_ENTRIES, MAX_MOVE_OBJECTS,
};
use editor_core::{
    ApertureShape, CircleAperture, DocumentSnapshot, DrawObject, Exposure, Geometry, Layer,
    MmPoint, SemanticDocument, SemanticGeometry, SemanticObject,
};
pub use editor_text::{HorizontalAlign, Layout as TextLayout, VerticalAlign};
use gerber_io::{S0Error, S0Scene, S1Error, S1Scene, export_s1_new_path, parse_s0};
pub use metrics::{MetricValue, MetricsItem, MetricsParams, MetricsResult, MetricsSummary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};
pub use text::{
    BUILTIN_STROKE_PATH, FontIdentity, FontInfo, TextParams, TextPreviewResult, TextResult,
    TextTimings, builtin_stroke_font,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub api_version: u32,
    pub stage: String,
    pub read_only: bool,
    pub supported_operations: Vec<String>,
    pub unsupported_operations: Vec<String>,
    pub supported_gerber_subset: Vec<String>,
    pub unsupported_gerber_features: Vec<String>,
    pub precision: PrecisionCapabilities,
    pub resource_limits: ResourceLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    #[serde(default)]
    pub max_nearby_candidates: usize,
    pub max_hit_test_work: usize,
    /// None means exact rectangle selection has no fixed work admission cap.
    #[serde(default)]
    pub max_select_rect_work: Option<usize>,
    pub max_metrics_work: usize,
    pub max_metrics_objects: usize,
    pub max_source_bytes: usize,
    pub max_objects: usize,
    pub max_query_results: usize,
    pub max_move_objects: usize,
    pub max_edit_objects: usize,
    pub max_history_entries: usize,
    pub max_history_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecisionCapabilities {
    pub coordinate_unit: String,
    pub coordinate_type: String,
    pub compare_tolerance_mm: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenResult {
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub layer_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageSample {
    pub point: MmPoint,
    pub layer_id: String,
    pub covered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub samples: Vec<CoverageSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceError {
    pub code: String,
    pub message: String,
    pub details: Value,
}

/// Host supplied file boundary. The service never accepts a caller supplied
/// "trusted" bit; paths are resolved against these explicit roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAccessPolicy {
    pub working_directory: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub write_roots: Vec<PathBuf>,
}

impl FileAccessPolicy {
    pub fn new(
        working_directory: impl Into<PathBuf>,
        read_roots: impl IntoIterator<Item = PathBuf>,
        write_roots: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        Self {
            working_directory: working_directory.into(),
            read_roots: read_roots.into_iter().collect(),
            write_roots: write_roots.into_iter().collect(),
        }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, ServiceError> {
        if path.trim().is_empty() {
            return Err(ServiceError::invalid_field(
                "params.path",
                "path must not be empty",
            ));
        }
        let path = Path::new(path);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_directory.join(path)
        };
        Ok(path)
    }

    fn read_path(&self, path: &str) -> Result<(PathBuf, Vec<u8>), ServiceError> {
        self.read_path_limited(path, gerber_io::S1_MAX_SOURCE_BYTES)
    }

    fn read_path_limited(
        &self,
        path: &str,
        max: usize,
    ) -> Result<(PathBuf, Vec<u8>), ServiceError> {
        let path = self.resolve(path)?;
        let canonical =
            fs::canonicalize(&path).map_err(|error| ServiceError::io("read", &path, error))?;
        if !is_under_any(&canonical, &self.read_roots) {
            return Err(ServiceError::permission(&canonical, "read"));
        }
        let metadata = fs::metadata(&canonical)
            .map_err(|error| ServiceError::io("read", &canonical, error))?;
        if !metadata.is_file() {
            return Err(ServiceError::permission(&canonical, "read regular file"));
        }
        if metadata.len() > max as u64 {
            return Err(ServiceError::resource(
                "source_bytes",
                max,
                metadata.len() as usize,
            ));
        }
        let file = fs::File::open(&canonical)
            .map_err(|error| ServiceError::io("read", &canonical, error))?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ServiceError::io("read", &canonical, error))?;
        if bytes.len() > max {
            return Err(ServiceError::resource("source_bytes", max, bytes.len()));
        }
        Ok((canonical, bytes))
    }

    fn write_path(&self, path: &str) -> Result<PathBuf, ServiceError> {
        let path = self.resolve(path)?;
        let parent = path.parent().ok_or_else(|| {
            ServiceError::invalid_field("params.path", "output path has no parent")
        })?;
        let parent =
            fs::canonicalize(parent).map_err(|error| ServiceError::io("write", parent, error))?;
        if !parent.is_dir() {
            return Err(ServiceError::permission(&parent, "write directory"));
        }
        let file_name = path.file_name().ok_or_else(|| {
            ServiceError::invalid_field("params.path", "output path must name a file")
        })?;
        let target = parent.join(file_name);
        if !is_under_any(&parent, &self.write_roots) {
            return Err(ServiceError::permission(&target, "write"));
        }
        if target.exists() {
            let canonical = fs::canonicalize(&target)
                .map_err(|error| ServiceError::io("write", &target, error))?;
            if !is_under_any(&canonical, &self.write_roots) {
                return Err(ServiceError::permission(&canonical, "write"));
            }
        }
        Ok(target)
    }
}

fn is_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        fs::canonicalize(root)
            .ok()
            .is_some_and(|root| path == root || path.starts_with(root))
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentInfo {
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub project_dirty: bool,
    #[serde(default)]
    pub last_saved_project_hash: Option<String>,
    #[serde(default)]
    pub manufacturing_precision: ManufacturingPrecision,
    #[serde(default)]
    pub export_policy_dirty: bool,
    pub workspace_revision: String,
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    /// Informational provenance of the Gerber this Workspace was opened from
    /// (empty for a new Workspace). Never used for save/export behaviour.
    pub source_path: String,
    pub source_sha256: String,
    pub dirty: bool,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub history_bytes: usize,
    pub history_truncated_entries: usize,
    pub history_truncated_bytes: usize,
    pub layer_ids: Vec<String>,
    /// Panel order, top first.
    #[serde(default)]
    pub display_order: Vec<String>,
    #[serde(default)]
    pub active_layer_id: Option<String>,
    #[serde(default)]
    pub solo_layer_id: Option<String>,
    pub diagnostics: Vec<String>,
}

/// One row of `layers.list`, in Layer Panel order (top of the panel first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerInfo {
    pub layer_id: String,
    pub display_name: String,
    pub visible: bool,
    pub object_count: usize,
    pub locked: bool,
    #[serde(default = "default_true")]
    pub selectable: bool,
    #[serde(default)]
    pub kind: LayerKind,
    #[serde(default = "default_layer_color")]
    pub base_color: Color,
    #[serde(default)]
    pub color_mode: ColorMode,
    #[serde(default)]
    pub display_mode: LayerDisplayMode,
    /// Per-category style, in stable `DisplayClass::ALL` order.
    #[serde(default)]
    pub classes: Vec<ClassStyleInfo>,
    /// Visible after the Solo override; `visible` stays the user's own setting.
    #[serde(default = "default_true")]
    pub effective_visible: bool,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_solo: bool,
    /// Position in the panel; 0 is the top (highest display priority).
    #[serde(default)]
    pub z_index: usize,
    /// Import-time source group; `None` for layers created empty.
    #[serde(default)]
    pub source_id: Option<String>,
    /// Read-only provenance (never a link to the disk file).
    #[serde(default)]
    pub provenance: Option<ImportProvenance>,
    /// Parser diagnostics of the import that created this layer.
    #[serde(default)]
    pub import_diagnostics: Vec<String>,
    #[serde(default)]
    pub compatibility_issue_count: usize,
}

fn default_true() -> bool {
    true
}

fn default_layer_color() -> Color {
    auto_layer_color(0)
}

impl Default for LayerInfo {
    fn default() -> Self {
        Self {
            layer_id: String::new(),
            display_name: String::new(),
            visible: true,
            object_count: 0,
            locked: false,
            selectable: true,
            kind: LayerKind::Gerber,
            base_color: auto_layer_color(0),
            color_mode: ColorMode::LayerColor,
            display_mode: LayerDisplayMode::Filled,
            classes: Vec::new(),
            effective_visible: true,
            is_active: false,
            is_solo: false,
            z_index: 0,
            source_id: None,
            provenance: None,
            import_diagnostics: Vec::new(),
            compatibility_issue_count: 0,
        }
    }
}

/// Per-category style of one layer as reported by `layers.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassStyleInfo {
    pub class: DisplayClass,
    pub visible: bool,
    pub selectable: bool,
    pub locked: bool,
    pub color_override: Option<Color>,
    /// Colour used for this class right now (layer colour or category colour).
    pub effective_color: Color,
}

/// Change of one category style; every field is optional.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassStyleUpdate {
    pub class: Option<DisplayClass>,
    pub visible: Option<bool>,
    pub selectable: Option<bool>,
    pub locked: Option<bool>,
    /// `#rrggbb`, or `"inherit"` to remove the override.
    pub color_override: Option<String>,
}

/// Workspace-only change of one layer. Never a manufacturing edit.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerUpdateParams {
    pub layer_id: String,
    pub expected_workspace_revision: String,
    pub display_name: Option<String>,
    pub visible: Option<bool>,
    pub locked: Option<bool>,
    #[serde(default)]
    pub selectable: Option<bool>,
    /// `#rrggbb`.
    #[serde(default)]
    pub base_color: Option<String>,
    #[serde(default)]
    pub color_mode: Option<ColorMode>,
    #[serde(default)]
    pub display_mode: Option<LayerDisplayMode>,
    /// Category changes; `class: null` applies to every category.
    #[serde(default)]
    pub classes: Vec<ClassStyleUpdate>,
    /// Restore the default category styles before applying `classes`.
    #[serde(default)]
    pub reset_classes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestPoint {
    pub x_mm: f64,
    pub y_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestParams {
    pub layer_id: String,
    pub point: HitTestPoint,
    pub tolerance_mm: f64,
    /// Apply the workspace selection policy (layer/category visible + selectable)
    /// before the exact test. Default: pure manufacturing query.
    #[serde(default)]
    pub selectable_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestResult {
    pub document_id: String,
    pub revision: String,
    pub layer_id: String,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectRectParams {
    pub layer_id: String,
    pub rect_mm: editor_core::BoundsMm,
    pub mode: editor_core::hit_test::SelectRectMode,
    /// Apply the workspace selection policy (see `HitTestParams`).
    #[serde(default)]
    pub selectable_only: bool,
}

/// Revision-bound read-only manufacturing envelope, including Clear objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundsResult {
    pub document_id: String,
    pub revision: String,
    pub bounds: Option<editor_core::BoundsMm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerBoundsParams {
    pub layer_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub layer_id: String,
    pub object: SemanticObject,
}

/// Owned, revision-bound display input. Never a mutable reference into a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSnapshot {
    pub document_id: String,
    pub revision: String,
    /// Changes with every colour / visibility / order / active-layer change.
    #[serde(default)]
    pub workspace_revision: String,
    /// Manufacturing layers in composite order: drawn first (bottom) to last (top).
    pub layers: Vec<editor_core::SemanticLayer>,
    pub apertures: Vec<editor_core::ApertureDefinition>,
    /// View style of every entry of `layers`, same order. Never manufacturing data.
    #[serde(default)]
    pub styles: Vec<RenderLayerStyle>,
    /// Project-level Block definitions a `BlockInstance` in `layers` may
    /// reference (S4-B2 display path). Same unfiltered-clone treatment as
    /// `apertures` above.
    #[serde(default)]
    pub block_definitions: Vec<editor_core::block::BlockDefinition>,
}

/// View-only style handed to the renderer next to the manufacturing scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLayerStyle {
    pub layer_id: String,
    /// `layer.visible` after the Solo override.
    pub visible: bool,
    pub selectable: bool,
    pub locked: bool,
    pub base_color: Color,
    pub color_mode: ColorMode,
    pub display_mode: LayerDisplayMode,
    pub classes: Vec<ClassStyleInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryRegion {
    pub min_x_mm: f64,
    pub min_y_mm: f64,
    pub max_x_mm: f64,
    pub max_y_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryRelation {
    Intersects,
    Contains,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub objects: Vec<ObjectInfo>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub valid: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportResult {
    pub api_version: u32,
    pub document_id: String,
    pub layer_id: String,
    pub exported_revision: String,
    pub current_revision: String,
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
struct S1DocumentRecord {
    project_id: String,
    project_path: Option<PathBuf>,
    last_saved_project_hash: Option<String>,
    saved_project_state_hash: String,
    board: Option<std::sync::Arc<editor_core::pnp::BoardState>>,
    next_component_id: u64,
    candidates: candidates::CandidateCache,
    project_settings: rcam_project::WorkspaceProjectState,
    manufacturing_precision: ManufacturingPrecision,
    saved_precision: ManufacturingPrecision,
    metrics: metrics::MetricsCache,
    workspace_revision: u64,
    /// Session-only view/workspace state per layer (never enters the document).
    workspace: HashMap<String, LayerWorkspaceState>,
    /// Import provenance and import-time metadata per layer.
    sources: HashMap<String, workspace::LayerSource>,
    /// Panel order, top first. The single source of truth for display priority.
    display_order: Vec<String>,
    active_layer_id: Option<String>,
    solo_layer_id: Option<String>,
    /// Layers currently outside the document that Undo/Redo may bring back.
    stash: HashMap<String, workspace::StashedLayer>,
    /// Active layer before a layer was added, for a faithful Undo.
    active_before_add: HashMap<String, Option<String>>,
    /// Counters are never reused, so ids stay stable across Undo/Redo.
    next_layer_number: u64,
    next_source_number: u64,
    next_color_index: usize,
    document: SemanticDocument,
    /// Informational only: the first Gerber this Workspace was opened from.
    opened_from: Option<(PathBuf, String)>,
    diagnostics: Vec<String>,
    revision: u64,
    history: EditHistory,
    /// Saved/current canonical chunk signatures; never derived from revision.
    content_state: content_state::ContentState,
}

impl S1DocumentRecord {
    fn is_dirty(&self) -> bool {
        self.content_state
            .is_dirty(&self.document, self.revision, &self.history)
    }
    fn reset_dirty_baseline(&mut self) {
        self.content_state = content_state::ContentState::new(
            &self.document,
            self.revision,
            self.history.content_generation(),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestEnvelope {
    api_version: u32,
    request_id: String,
    op: String,
    document_id: Option<String>,
    expected_revision: Option<String>,
    params: Value,
}

// Separate decode retains only an unambiguous string request ID on invalid
// envelopes. Duplicate request_id fields and malformed JSON still fail.
#[derive(Deserialize)]
struct RequestIdentity {
    request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenS0Params {
    source: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenParams {
    path: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSaveAsParams {
    path: String,
    #[serde(default)]
    allow_replace: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryParams {
    pub layer_id: String,
    pub geometry_type: Option<String>,
    pub region_mm: Option<QueryRegion>,
    pub relation: Option<QueryRelation>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectParams {
    pub layer_id: String,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub dx_mm: f64,
    pub dy_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PivotMm {
    pub x_mm: f64,
    pub y_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotateParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub angle_deg: f64,
    pub pivot_mm: PivotMm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub axis: MirrorAxis,
}

/// Duplicate uses the same explicit offset DTO as Move; zero offset is valid.
pub type DuplicateParams = MoveParams;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBlockDefinitionParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub local_origin_mm: PivotMm,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockTransformParams {
    pub translation_mm: PivotMm,
    pub rotation_deg: f64,
    pub mirror: bool,
}

fn block_transform_from_params(
    params: &BlockTransformParams,
) -> editor_core::block::BlockTransform {
    editor_core::block::BlockTransform {
        translation: MmPoint::new(params.translation_mm.x_mm, params.translation_mm.y_mm),
        rotation_deg: params.rotation_deg,
        mirror: params.mirror,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBlockInstanceParams {
    pub layer_id: String,
    pub definition_id: String,
    pub transform: BlockTransformParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBlockInstanceTransformParams {
    pub layer_id: String,
    pub object_id: String,
    pub transform: BlockTransformParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameBlockDefinitionParams {
    pub definition_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDefinitionIdParams {
    pub definition_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplodeBlockInstanceParams {
    pub layer_id: String,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDefinitionSummary {
    pub id: String,
    pub name: String,
    pub object_count: usize,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDefinitionDetail {
    pub id: String,
    pub name: String,
    pub local_origin_mm: PivotMm,
    pub revision: u64,
    pub objects: Vec<editor_core::block::BlockObject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateBlockDefinitionResult {
    pub definition_id: String,
    pub instance_object_id: String,
    #[serde(flatten)]
    pub edit: EditResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockInstanceResult {
    pub object_id: String,
    #[serde(flatten)]
    pub edit: EditResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPropertiesParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub width_mm: f64,
    pub height_mm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum BatchStepParams {
    #[serde(rename = "objects.move")]
    Move {
        object_ids: Vec<String>,
        dx_mm: f64,
        dy_mm: f64,
    },
    #[serde(rename = "objects.rotate")]
    Rotate {
        object_ids: Vec<String>,
        angle_deg: f64,
        pivot_mm: PivotMm,
    },
    #[serde(rename = "objects.mirror")]
    Mirror {
        object_ids: Vec<String>,
        axis: MirrorAxis,
    },
    #[serde(rename = "objects.set_properties")]
    SetProperties {
        object_ids: Vec<String>,
        width_mm: f64,
        height_mm: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchParams {
    pub layer_id: String,
    pub steps: Vec<BatchStepParams>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditResult {
    pub document_id: String,
    pub revision: String,
    pub changed_object_ids: Vec<String>,
    /// Layers added or removed by an Undo/Redo of a layer transaction.
    #[serde(default)]
    pub changed_layer_ids: Vec<String>,
    pub undo_entries_added: usize,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub history_bytes: usize,
    pub history_truncated_entries: usize,
    pub history_truncated_bytes: usize,
    pub dirty: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseParams {
    #[serde(default)]
    discard_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    pub layer_id: String,
    pub path: String,
    pub overwrite: OverwritePolicy,
    pub metadata_policy: MetadataPolicy,
    /// Explicitly approved finer manufacturing grid for compatibility geometry.
    #[serde(default)]
    pub compatibility_precision_override_mm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverwritePolicy {
    pub mode: String,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPolicy {
    pub mode: String,
    pub categories: Option<Vec<String>>,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

pub struct ApplicationService {
    scenes: HashMap<String, S0Scene>,
    documents: HashMap<String, S1DocumentRecord>,
    file_access: Option<FileAccessPolicy>,
    next_document_id: u64,
    history_max_entries: usize,
    history_max_bytes: usize,
}

impl Default for ApplicationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationService {
    pub fn new() -> Self {
        Self {
            scenes: HashMap::new(),
            documents: HashMap::new(),
            file_access: None,
            next_document_id: 1,
            history_max_entries: MAX_HISTORY_ENTRIES,
            history_max_bytes: MAX_HISTORY_BYTES,
        }
    }

    pub fn with_file_access(file_access: FileAccessPolicy) -> Self {
        Self {
            file_access: Some(file_access),
            ..Self::new()
        }
    }

    pub fn with_file_access_and_history_limits(
        file_access: FileAccessPolicy,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Self, ServiceError> {
        EditHistory::with_limits(max_entries, max_bytes).map_err(map_edit_error)?;
        Ok(Self {
            file_access: Some(file_access),
            history_max_entries: max_entries,
            history_max_bytes: max_bytes,
            ..Self::new()
        })
    }

    pub fn capabilities(&self) -> Capabilities {
        if self.file_access.is_none() {
            return Capabilities {
                api_version: API_VERSION,
                stage: "S0 technology validation".into(),
                read_only: true,
                supported_operations: vec![
                    "system.capabilities".into(),
                    "document.open_s0".into(),
                    "document.snapshot".into(),
                    "document.analyze_s0".into(),
                ],
                unsupported_operations: vec![
                    "objects.move".into(),
                    "objects.edit_selection".into(),
                    "objects.duplicate".into(),
                    "objects.delete".into(),
                    "history.undo".into(),
                    "gerber.export_layer".into(),
                    "document.set_manufacturing_precision".into(),
                ],
                supported_gerber_subset: vec![
                    "FS absolute coordinates".into(),
                    "MO MM/IN".into(),
                    "AD C circle aperture".into(),
                    "LPD/LPC ordered flash exposure".into(),
                    "D03 circle flash".into(),
                ],
                unsupported_gerber_features: vec![
                    "D01/D02 line and arc interpolation".into(),
                    "regions".into(),
                    "AM/AB/SR".into(),
                    "deprecated image transforms".into(),
                    "production export".into(),
                ],
                resource_limits: ResourceLimits {
                    max_nearby_candidates: 0,
                    max_hit_test_work: 0,
                    max_select_rect_work: None,
                    max_metrics_work: 0,
                    max_metrics_objects: 0,
                    max_source_bytes: gerber_io::MAX_SOURCE_BYTES,
                    max_objects: gerber_io::MAX_OBJECTS,
                    max_query_results: 0,
                    max_move_objects: 0,
                    max_edit_objects: 0,
                    max_history_entries: 0,
                    max_history_bytes: 0,
                },
                precision: PrecisionCapabilities {
                    coordinate_unit: "mm".into(),
                    coordinate_type: "f64".into(),
                    compare_tolerance_mm: "1e-6".into(),
                },
            };
        }
        Capabilities {
            api_version: API_VERSION,
            stage: "S4-D2 RefDes-Assisted Stencil Candidate Selection v1 (Mac-first bounded)".into(),
            read_only: false,
            supported_operations: vec![
                "system.capabilities".into(),
                "document.open_s0".into(),
                "document.snapshot".into(),
                "document.analyze_s0".into(),
                "document.open".into(),
                "document.get".into(),
                "render.snapshot".into(),
                "document.close".into(),
                "objects.move".into(),
                    "objects.edit_selection".into(),
                "objects.rotate".into(),
                "objects.mirror".into(),
                "objects.duplicate".into(),
                "objects.delete".into(),
                "objects.set_properties".into(),
                "objects.grips".into(),
                "objects.grip_edit".into(),
                "objects.array_rectangular".into(),
                "objects.align".into(),
                "objects.distribute".into(),
                "edit.batch".into(),
                "text.create".into(),
                "text.preview".into(),
                "history.undo".into(),
                "history.redo".into(),
                "layers.list".into(),
                "layer.update".into(),
                "objects.hit_test".into(),
                "objects.select_rect".into(),
                "objects.metrics".into(),
                "layer.bounds".into(),
                "document.bounds".into(),
                "objects.query".into(),
                "objects.get".into(),
                "document.validate".into(),
                "gerber.export_layer".into(),
                "document.set_manufacturing_precision".into(),
                "document.new".into(),
                "project.new".into(),
                "project.open".into(),
                "project.save".into(),
                "project.save_as".into(),
                "project.info".into(),
                "document.import_gerber_layers".into(),
                "document.import_gerber_layer".into(),
                "document.create_empty_layer".into(),
                "document.remove_layer".into(),
                "document.visible_bounds".into(),
                "layer.summary".into(),
                "layers.reorder".into(),
                "layers.set_active".into(),
                "layers.set_solo".into(),
                "layers.update_many".into(),
                "layers.reset_colors".into(),
                "components.preview_pnp".into(),
                "components.import_pnp".into(),
                "components.list".into(),
                "components.search".into(),
                "components.get".into(),
                "components.nearby_manufacturing".into(),
                "board.get_registration".into(),
                "board.set_registration".into(),
                "blocks.list_definitions".into(),
                "blocks.get_definition".into(),
                "blocks.create_definition_from_objects".into(),
                "blocks.create_instance".into(),
                "blocks.update_instance_transform".into(),
                "blocks.rename_definition".into(),
                "blocks.explode_instance".into(),
                "blocks.delete_definition".into(),
            ],
            // Reserved boundaries (S4-B2 architecture placeholders): named here so a
            // caller can tell "not yet" from "unknown". None of these is dispatchable.
            unsupported_operations: vec![
                "drill.import".into(),
                "snap.resolve".into(),
                "layers.merge".into(),
            ],
            supported_gerber_subset: vec![
                "FS absolute coordinates".into(),
                "FS incremental coordinates (I/G91)".into(),
                "MO MM/IN".into(),
                "G70/G71 unit state".into(),
                "AD C/R/O/P standard apertures".into(),
                "AM primitive 1/4/21 with bounded expressions".into(),
                "D01/D02/D03 ordered operations".into(),
                "G01 circular and rectangular sweeps".into(),
                "G75 strokes with nonzero arc deviation (annular canonical coverage)".into(),
                "G74 strokes with least-deviation center and zero-sweep dot".into(),
                "G36/G37 circular Region interpretations with annular-envelope topology checks".into(),
                "LPD/LPC ordered flash exposure".into(),
                "identity SR and image-state declarations".into(),
                "front-matter ICAS/IO/IN/LN".into(),
            ],
            unsupported_gerber_features: vec![
                "Region arcs without a verified circular interpretation or separated uncertainty envelopes".into(),
                "AB aperture blocks".into(),
                "complex SR".into(),
                "non-identity MI/OF/SF/IR transforms".into(),
                "unsupported AM primitives and expressions".into(),
                "Excellon and RS-274D external apertures".into(),
                "RectangularSweep rotation except exact multiples of 90 degrees; diagonal mirror axes".into(),
            ],
            resource_limits: ResourceLimits {
                    max_nearby_candidates: MAX_NEARBY_CANDIDATES,
                    max_hit_test_work: editor_core::hit_test::MAX_HIT_TEST_WORK,
                    max_select_rect_work: None,
                    max_metrics_work: editor_core::metrics::MAX_METRICS_WORK,
                    max_metrics_objects: metrics::MAX_METRICS_OBJECTS,
                max_source_bytes: gerber_io::S1_MAX_SOURCE_BYTES,
                max_objects: gerber_io::S1_MAX_OBJECTS,
                max_query_results: 1000,
                max_move_objects: MAX_MOVE_OBJECTS,
                max_edit_objects: MAX_MOVE_OBJECTS,
                max_history_entries: self.history_max_entries,
                max_history_bytes: self.history_max_bytes,
            },
            precision: PrecisionCapabilities {
                coordinate_unit: "mm".into(),
                coordinate_type: "f64".into(),
                compare_tolerance_mm: "1e-6".into(),
            },
        }
    }

    pub fn open_s0(
        &mut self,
        document_id: impl Into<String>,
        bytes: &[u8],
    ) -> Result<OpenResult, ServiceError> {
        let document_id = document_id.into();
        if document_id.trim().is_empty() {
            return Err(ServiceError::invalid("document_id must not be empty"));
        }
        if self.scenes.contains_key(&document_id) {
            return Err(ServiceError {
                code: "ALREADY_EXISTS".into(),
                message: format!("document {document_id:?} is already open"),
                details: serde_json::json!({"document_id": document_id}),
            });
        }
        let scene = parse_s0(bytes, &document_id).map_err(map_parse_error)?;
        let layer_ids = scene
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        let diagnostics = scene.diagnostics.clone();
        self.scenes.insert(document_id.clone(), scene);
        Ok(OpenResult {
            api_version: API_VERSION,
            document_id,
            revision: "0".into(),
            layer_ids,
            diagnostics,
        })
    }

    /// Opens the fixed, local S0 risk scene used by the demo window. The
    /// ordinary `open_s0` path never appends these constructed fixtures.
    pub fn open_demo_s0(
        &mut self,
        document_id: impl Into<String>,
        bytes: &[u8],
    ) -> Result<OpenResult, ServiceError> {
        let mut result = self.open_s0(document_id, bytes)?;
        let scene = self
            .scenes
            .get_mut(&result.document_id)
            .ok_or_else(|| ServiceError::not_found("document", &result.document_id))?;
        add_s0_risk_layers(scene);
        result.layer_ids = scene
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        result.diagnostics = scene.diagnostics.clone();
        Ok(result)
    }

    pub fn document_get(&self, document_id: &str) -> Result<DocumentInfo, ServiceError> {
        self.documents
            .get(document_id)
            .map(|record| document_info(document_id, record))
            .ok_or_else(|| ServiceError::not_found("document", document_id))
    }

    pub fn render_snapshot(&self, document_id: &str) -> Result<RenderSnapshot, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        Ok(workspace::render_snapshot_of(document_id, record))
    }

    /// Trusted host only; not exposed through request DTOs. The GUI calls this
    /// after the user chooses a file/directory. Existing document state is kept.
    pub fn grant_file_access(
        &mut self,
        path: &Path,
        write_directory: bool,
    ) -> Result<(), ServiceError> {
        let path = fs::canonicalize(path).map_err(|e| ServiceError::io("authorize", path, e))?;
        if write_directory && !path.is_dir() {
            return Err(ServiceError::permission(&path, "write directory"));
        }
        let access = self.file_access.get_or_insert_with(|| {
            FileAccessPolicy::new(path.parent().unwrap_or(&path), Vec::new(), Vec::new())
        });
        let roots = if write_directory {
            &mut access.write_roots
        } else {
            &mut access.read_roots
        };
        if !roots.contains(&path) {
            roots.push(path);
        }
        Ok(())
    }

    /// Layers in Layer Panel order (top of the panel first).
    pub fn layers_list(&self, document_id: &str) -> Result<Vec<LayerInfo>, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        Ok(workspace::layer_rows(record))
    }

    /// Workspace-only change of one layer (name, colours, visibility, lock,
    /// selectability, display mode, category styles). Never a manufacturing edit.
    pub fn layer_update(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: LayerUpdateParams,
    ) -> Result<DocumentInfo, ServiceError> {
        let expected_workspace_revision = params.expected_workspace_revision.clone();
        self.layers_update_many(
            document_id,
            expected_revision,
            UpdateLayersParams {
                expected_workspace_revision,
                updates: vec![params.into()],
            },
        )
    }

    pub fn objects_hit_test(
        &self,
        document_id: &str,
        params: HitTestParams,
    ) -> Result<HitTestResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let point = MmPoint::new(params.point.x_mm, params.point.y_mm);
        if params.selectable_only && !workspace::layer_selectable(record, &params.layer_id)? {
            return Ok(HitTestResult {
                document_id: document_id.into(),
                revision: record.revision.to_string(),
                layer_id: params.layer_id,
                object_ids: Vec::new(),
            });
        }
        let mut object_ids = record.document.hit_test(&params.layer_id, point, params.tolerance_mm)
            .map_err(|error| {
                use editor_core::hit_test::HitTestError;
                match error {
                    HitTestError::Cancelled => ServiceError { code: "CANCELLED".into(), message: "任务已取消".into(), details: serde_json::json!({}) },
                    HitTestError::InvalidArgument(field) => ServiceError::invalid_field(field, "invalid hit-test parameter"),
                    HitTestError::MissingLayer(id) => ServiceError::not_found("layer", &id),
                    HitTestError::Geometry(error) => map_semantic_error(error),
                    HitTestError::ResourceLimit { limit, attempted } => ServiceError::resource("hit_test_work", limit, attempted),
                    HitTestError::Unsupported(reason) => ServiceError { code: "UNSUPPORTED_FEATURE".into(), message: reason.into(), details: serde_json::json!({"operation":"objects.hit_test","reason":reason}) },
                }
            })?;
        if params.selectable_only {
            workspace::retain_selectable(record, &params.layer_id, &mut object_ids);
        }
        Ok(HitTestResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            layer_id: params.layer_id,
            object_ids,
        })
    }

    pub fn objects_select_rect(
        &self,
        document_id: &str,
        params: SelectRectParams,
    ) -> Result<HitTestResult, ServiceError> {
        self.objects_select_rect_with_cancel(document_id, params, None)
    }
    pub fn objects_select_rect_with_cancel(
        &self,
        document_id: &str,
        params: SelectRectParams,
        cancel: Option<&task::CancellationToken>,
    ) -> Result<HitTestResult, ServiceError> {
        if let Some(cancel) = cancel {
            cancel.checkpoint()?;
        }
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        if params.selectable_only && !workspace::layer_selectable(record, &params.layer_id)? {
            return Ok(HitTestResult {
                document_id: document_id.into(),
                revision: record.revision.to_string(),
                layer_id: params.layer_id,
                object_ids: Vec::new(),
            });
        }
        let mut object_ids = record.document.select_rect_cancellable(&params.layer_id, params.rect_mm, params.mode,
            || cancel.is_some_and(|c| c.checkpoint().is_err()))
            .map_err(|error| {
                use editor_core::hit_test::HitTestError;
                match error {
                    HitTestError::Cancelled => ServiceError { code: "CANCELLED".into(), message: "任务已取消".into(), details: serde_json::json!({}) },
                    HitTestError::InvalidArgument(field) => ServiceError::invalid_field(field, "invalid selection parameter"),
                    HitTestError::MissingLayer(id) => ServiceError::not_found("layer", &id),
                    HitTestError::Geometry(error) => map_semantic_error(error),
                    HitTestError::ResourceLimit { limit, attempted } => ServiceError::resource("select_rect_work", limit, attempted),
                    HitTestError::Unsupported(reason) => ServiceError { code: "UNSUPPORTED_FEATURE".into(), message: reason.into(), details: serde_json::json!({"operation":"objects.select_rect","reason":reason}) },
                }
            })?;
        if params.selectable_only {
            workspace::retain_selectable(record, &params.layer_id, &mut object_ids);
        }
        Ok(HitTestResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            layer_id: params.layer_id,
            object_ids,
        })
    }

    pub fn document_bounds(&self, document_id: &str) -> Result<BoundsResult, ServiceError> {
        self.bounds(document_id, None)
    }

    pub fn layer_bounds(
        &self,
        document_id: &str,
        params: LayerBoundsParams,
    ) -> Result<BoundsResult, ServiceError> {
        if params.layer_id.trim().is_empty() {
            return Err(ServiceError::invalid_field(
                "layer_id",
                "layer_id must not be empty",
            ));
        }
        self.bounds(document_id, Some(&params.layer_id))
    }

    fn bounds(
        &self,
        document_id: &str,
        layer_id: Option<&str>,
    ) -> Result<BoundsResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        if let Some(id) = layer_id
            && !record.document.layers.iter().any(|layer| layer.id == id)
        {
            return Err(ServiceError::not_found("layer", id));
        }
        Ok(BoundsResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            bounds: record
                .document
                .manufacturing_bounds(layer_id)
                .map_err(map_semantic_error)?,
        })
    }

    pub fn objects_get(
        &self,
        document_id: &str,
        params: ObjectParams,
    ) -> Result<ObjectInfo, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let layer = record
            .document
            .layers
            .iter()
            .find(|layer| layer.id == params.layer_id)
            .ok_or_else(|| ServiceError::not_found("layer", &params.layer_id))?;
        let object = layer
            .objects
            .iter()
            .find(|object| object.object_id == params.object_id)
            .ok_or_else(|| ServiceError::not_found("object", &params.object_id))?;
        Ok(ObjectInfo {
            layer_id: layer.id.clone(),
            object: object.clone(),
        })
    }

    pub fn objects_query(
        &self,
        document_id: &str,
        params: QueryParams,
    ) -> Result<QueryResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        validate_query_params(&params)?;
        let limit = params.limit.unwrap_or(1000).min(1000);
        let fingerprint = query_fingerprint(document_id, &params);
        let offset = decode_cursor(
            params.cursor.as_deref(),
            document_id,
            record.revision,
            fingerprint,
        )?;
        let layer = record
            .document
            .layers
            .iter()
            .find(|layer| layer.id == params.layer_id)
            .ok_or_else(|| ServiceError::not_found("layer", &params.layer_id))?;
        let mut matches = Vec::new();
        let mut total = 0usize;
        for object in &layer.objects {
            if let Some(kind) = &params.geometry_type
                && !geometry_kind_matches(&object.geometry, kind)
            {
                continue;
            }
            if let Some(region) = &params.region_mm
                && !matches_region(
                    &record.document,
                    &object.geometry,
                    region,
                    params.relation.as_ref().unwrap(),
                )?
            {
                continue;
            }
            if total >= offset && matches.len() < limit {
                matches.push(ObjectInfo {
                    layer_id: layer.id.clone(),
                    object: object.clone(),
                });
            }
            total = total
                .checked_add(1)
                .ok_or_else(|| ServiceError::resource("query_results", 1000, usize::MAX))?;
        }
        let next_offset = offset
            .checked_add(matches.len())
            .ok_or_else(|| ServiceError::resource("query_results", 1000, usize::MAX))?;
        let next_cursor = (next_offset < total)
            .then(|| encode_cursor(document_id, record.revision, fingerprint, next_offset));
        Ok(QueryResult {
            api_version: API_VERSION,
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            objects: matches,
            next_cursor,
            total,
        })
    }

    pub fn objects_move(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: MoveParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.move",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        operation.selection_count(params.object_ids.len());
        let result = self.objects_move_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn objects_move_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: MoveParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        check_workspace_edit(record, &params.layer_id, &params.object_ids)?;
        let ids = record
            .history
            .move_objects(
                &mut record.document,
                &params.layer_id,
                &params.object_ids,
                params.dx_mm,
                params.dy_mm,
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    pub fn objects_rotate(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: RotateParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.rotate",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        operation.selection_count(params.object_ids.len());
        let result = self.objects_rotate_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn objects_rotate_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: RotateParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        check_workspace_edit(record, &params.layer_id, &params.object_ids)?;
        let ids = record
            .history
            .rotate_objects(
                &mut record.document,
                &params.layer_id,
                &params.object_ids,
                params.angle_deg,
                MmPoint::new(params.pivot_mm.x_mm, params.pivot_mm.y_mm),
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    pub fn objects_mirror(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: MirrorParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.mirror",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        operation.selection_count(params.object_ids.len());
        let result = self.objects_mirror_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn objects_mirror_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: MirrorParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        check_workspace_edit(record, &params.layer_id, &params.object_ids)?;
        let ids = record
            .history
            .mirror_objects(
                &mut record.document,
                &params.layer_id,
                &params.object_ids,
                params.axis,
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    pub fn objects_duplicate(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: DuplicateParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.duplicate",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        operation.selection_count(params.object_ids.len());
        let result = self.objects_duplicate_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn objects_duplicate_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: DuplicateParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        check_workspace_edit(record, &params.layer_id, &params.object_ids)?;
        let source_set: std::collections::HashSet<_> = params.object_ids.iter().collect();
        let sources: Vec<_> = record
            .document
            .layers
            .iter()
            .filter(|l| l.id == params.layer_id)
            .flat_map(|l| &l.objects)
            .filter(|o| source_set.contains(&o.object_id))
            .map(|o| o.object_id.clone())
            .collect();
        let ids = record
            .history
            .duplicate_objects(
                &mut record.document,
                &params.layer_id,
                &params.object_ids,
                params.dx_mm,
                params.dy_mm,
            )
            .map_err(map_edit_error)?;
        record.metrics.duplicate(&sources, &ids);
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    pub fn objects_delete(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: DeleteParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.delete",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        operation.selection_count(params.object_ids.len());
        let result = self.objects_delete_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn objects_delete_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: DeleteParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        check_workspace_edit(record, &params.layer_id, &params.object_ids)?;
        let ids = record
            .history
            .delete_objects(&mut record.document, &params.layer_id, &params.object_ids)
            .map_err(map_edit_error)?;
        record.metrics.reconcile(&record.document, &ids);
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    pub fn objects_set_properties(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: SetPropertiesParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.set_properties",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        operation.selection_count(params.object_ids.len());
        let result = self.objects_set_properties_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn objects_set_properties_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: SetPropertiesParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        check_workspace_edit(record, &params.layer_id, &params.object_ids)?;
        let ids = record
            .history
            .set_flash_size(
                &mut record.document,
                &params.layer_id,
                &params.object_ids,
                params.width_mm,
                params.height_mm,
            )
            .map_err(map_edit_error)?;
        record.metrics.invalidate_shapes(&ids);
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    /// `blocks.list_definitions`: read-only, no `expected_revision`.
    pub fn blocks_list_definitions(
        &self,
        document_id: &str,
    ) -> Result<Vec<BlockDefinitionSummary>, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        Ok(record
            .document
            .block_definitions
            .iter()
            .map(|definition| BlockDefinitionSummary {
                id: definition.id.0.clone(),
                name: definition.name.clone(),
                object_count: definition.objects.len(),
                revision: definition.revision,
            })
            .collect())
    }

    /// `blocks.get_definition`: read-only, no `expected_revision`.
    pub fn blocks_get_definition(
        &self,
        document_id: &str,
        params: BlockDefinitionIdParams,
    ) -> Result<BlockDefinitionDetail, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let definition = record
            .document
            .block_definitions
            .iter()
            .find(|d| d.id.0 == params.definition_id)
            .ok_or_else(|| ServiceError::not_found("block_definition", &params.definition_id))?;
        Ok(BlockDefinitionDetail {
            id: definition.id.0.clone(),
            name: definition.name.clone(),
            local_origin_mm: PivotMm {
                x_mm: definition.local_origin.x_mm,
                y_mm: definition.local_origin.y_mm,
            },
            revision: definition.revision,
            objects: definition.objects.clone(),
        })
    }

    /// `blocks.create_definition_from_objects`.
    fn record_block_summary(
        &self,
        command: &'static str,
        document_id: &str,
        definition_id: Option<&str>,
        layer_id: Option<&str>,
    ) {
        let Some(record) = self.documents.get(document_id) else {
            return;
        };
        let definition = definition_id.and_then(|id| {
            record
                .document
                .block_definitions
                .iter()
                .find(|d| d.id.0 == id)
        });
        let instances = record.document.layers.iter().flat_map(|l| &l.objects).filter(|o| matches!(&o.geometry, SemanticGeometry::BlockInstance {definition_id:id, ..} if Some(id.0.as_str()) == definition_id)).count();
        let hash = definition_id.map(|id| editor_core::hash::sha256_hex(id.as_bytes()));
        rcam_diagnostics::identified_measurements(
            rcam_diagnostics::Level::Info,
            command,
            hash.as_deref(),
            layer_id,
            &[
                (
                    "object_count",
                    definition.map_or(0, |d| d.objects.len()) as u64,
                ),
                ("instance_count", instances as u64),
            ],
        );
    }

    pub fn blocks_create_definition_from_objects(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: CreateBlockDefinitionParams,
    ) -> Result<CreateBlockDefinitionResult, ServiceError> {
        let summary_layer = Some(params.layer_id.clone());
        let operation = rcam_diagnostics::Operation::begin_document(
            "blocks.create_definition_from_objects",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.blocks_create_definition_from_objects_observed(
            document_id,
            expected_revision,
            params,
        );
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        let summary_definition = result.as_ref().ok().map(|r| r.definition_id.clone());
        self.record_block_summary(
            "blocks.create_definition_from_objects",
            document_id,
            summary_definition.as_deref(),
            summary_layer.as_deref(),
        );
        result
    }

    fn blocks_create_definition_from_objects_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: CreateBlockDefinitionParams,
    ) -> Result<CreateBlockDefinitionResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        workspace::check_block_edit(record, &params.layer_id, &params.object_ids, true)?;
        let (definition_id, instance_id) = record
            .history
            .create_block_definition(
                &mut record.document,
                &params.layer_id,
                &params.object_ids,
                MmPoint::new(params.local_origin_mm.x_mm, params.local_origin_mm.y_mm),
                params.name,
            )
            .map_err(map_edit_error)?;
        record
            .metrics
            .reconcile(&record.document, &params.object_ids);
        record.revision += 1;
        let edit = edit_result(document_id, record, vec![instance_id.clone()], 1);
        Ok(CreateBlockDefinitionResult {
            definition_id: definition_id.0,
            instance_object_id: instance_id,
            edit,
        })
    }

    /// `blocks.create_instance`: place a new instance of an existing
    /// definition; distinct from `objects.duplicate` (no source instance).
    pub fn blocks_create_instance(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: CreateBlockInstanceParams,
    ) -> Result<BlockInstanceResult, ServiceError> {
        let summary_layer = Some(params.layer_id.clone());
        let summary_definition = Some(params.definition_id.clone());
        let operation = rcam_diagnostics::Operation::begin_document(
            "blocks.create_instance",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.blocks_create_instance_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        self.record_block_summary(
            "blocks.create_instance",
            document_id,
            summary_definition.as_deref(),
            summary_layer.as_deref(),
        );
        result
    }

    fn blocks_create_instance_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: CreateBlockInstanceParams,
    ) -> Result<BlockInstanceResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        workspace::check_block_edit(record, &params.layer_id, &[], true)?;
        let object_id = record
            .history
            .create_block_instance(
                &mut record.document,
                &params.layer_id,
                &editor_core::block::BlockDefinitionId(params.definition_id),
                block_transform_from_params(&params.transform),
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        let edit = edit_result(document_id, record, vec![object_id.clone()], 1);
        Ok(BlockInstanceResult { object_id, edit })
    }

    /// `blocks.update_instance_transform`: set the instance's placement
    /// outright (Move/Rotate/Mirror already work as deltas via the existing
    /// `objects.*` ops, since a rigid instance transform composes exactly
    /// like any other manufacturing geometry — see ADR 0032).
    pub fn blocks_update_instance_transform(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: UpdateBlockInstanceTransformParams,
    ) -> Result<EditResult, ServiceError> {
        let summary_layer = Some(params.layer_id.clone());
        let summary_definition = self
            .documents
            .get(document_id)
            .and_then(|record| {
                record
                    .document
                    .layers
                    .iter()
                    .find(|l| l.id == params.layer_id)
            })
            .and_then(|layer| {
                layer
                    .objects
                    .iter()
                    .find(|o| o.object_id == params.object_id)
            })
            .and_then(|object| match &object.geometry {
                SemanticGeometry::BlockInstance { definition_id, .. } => {
                    Some(definition_id.0.clone())
                }
                _ => None,
            });
        let operation = rcam_diagnostics::Operation::begin_document(
            "blocks.update_instance_transform",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result =
            self.blocks_update_instance_transform_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        self.record_block_summary(
            "blocks.update_instance_transform",
            document_id,
            summary_definition.as_deref(),
            summary_layer.as_deref(),
        );
        result
    }

    fn blocks_update_instance_transform_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: UpdateBlockInstanceTransformParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        workspace::check_block_edit(
            record,
            &params.layer_id,
            std::slice::from_ref(&params.object_id),
            false,
        )?;
        let ids = record
            .history
            .set_block_instance_transform(
                &mut record.document,
                &params.layer_id,
                &params.object_id,
                block_transform_from_params(&params.transform),
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    /// `blocks.rename_definition`: metadata-only, no layer scope.
    pub fn blocks_rename_definition(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: RenameBlockDefinitionParams,
    ) -> Result<EditResult, ServiceError> {
        let summary_layer: Option<String> = None;
        let summary_definition = Some(params.definition_id.clone());
        let operation = rcam_diagnostics::Operation::begin_document(
            "blocks.rename_definition",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.blocks_rename_definition_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        self.record_block_summary(
            "blocks.rename_definition",
            document_id,
            summary_definition.as_deref(),
            summary_layer.as_deref(),
        );
        result
    }

    fn blocks_rename_definition_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: RenameBlockDefinitionParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        record
            .history
            .rename_block_definition(
                &mut record.document,
                &editor_core::block::BlockDefinitionId(params.definition_id),
                params.name,
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, Vec::new(), 1))
    }

    /// `blocks.explode_instance`: resolve one instance into world-space
    /// primitives and remove it; the definition itself is untouched.
    pub fn blocks_explode_instance(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ExplodeBlockInstanceParams,
    ) -> Result<EditResult, ServiceError> {
        let summary_layer = Some(params.layer_id.clone());
        let summary_definition = self
            .documents
            .get(document_id)
            .and_then(|record| {
                record
                    .document
                    .layers
                    .iter()
                    .find(|l| l.id == params.layer_id)
            })
            .and_then(|layer| {
                layer
                    .objects
                    .iter()
                    .find(|o| o.object_id == params.object_id)
            })
            .and_then(|object| match &object.geometry {
                SemanticGeometry::BlockInstance { definition_id, .. } => {
                    Some(definition_id.0.clone())
                }
                _ => None,
            });
        let operation = rcam_diagnostics::Operation::begin_document(
            "blocks.explode_instance",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.blocks_explode_instance_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        self.record_block_summary(
            "blocks.explode_instance",
            document_id,
            summary_definition.as_deref(),
            summary_layer.as_deref(),
        );
        result
    }

    fn blocks_explode_instance_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ExplodeBlockInstanceParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        workspace::check_block_edit(
            record,
            &params.layer_id,
            std::slice::from_ref(&params.object_id),
            false,
        )?;
        let ids = record
            .history
            .explode_block_instance(&mut record.document, &params.layer_id, &params.object_id)
            .map_err(map_edit_error)?;
        record.metrics.reconcile(&record.document, &ids);
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    /// `blocks.delete_definition`: rejects a definition still referenced by
    /// an instance (`BLOCK_DEFINITION_REFERENCED`; explode/delete the
    /// instances first).
    pub fn blocks_delete_definition(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: BlockDefinitionIdParams,
    ) -> Result<EditResult, ServiceError> {
        let summary_layer: Option<String> = None;
        let summary_definition = Some(params.definition_id.clone());
        let operation = rcam_diagnostics::Operation::begin_document(
            "blocks.delete_definition",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.blocks_delete_definition_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        self.record_block_summary(
            "blocks.delete_definition",
            document_id,
            summary_definition.as_deref(),
            summary_layer.as_deref(),
        );
        result
    }

    fn blocks_delete_definition_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: BlockDefinitionIdParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        record
            .history
            .delete_block_definition(
                &mut record.document,
                &editor_core::block::BlockDefinitionId(params.definition_id),
            )
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, Vec::new(), 1))
    }

    pub fn edit_batch(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: BatchParams,
    ) -> Result<EditResult, ServiceError> {
        let operation = rcam_diagnostics::Operation::begin_document(
            "edit.batch",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.edit_batch_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn edit_batch_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: BatchParams,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        let batch_ids: Vec<String> = params
            .steps
            .iter()
            .flat_map(|step| match step {
                BatchStepParams::Move { object_ids, .. }
                | BatchStepParams::Rotate { object_ids, .. }
                | BatchStepParams::Mirror { object_ids, .. }
                | BatchStepParams::SetProperties { object_ids, .. } => object_ids.iter().cloned(),
            })
            .collect();
        check_workspace_edit(record, &params.layer_id, &batch_ids)?;
        let shape_changed = params
            .steps
            .iter()
            .any(|step| matches!(step, BatchStepParams::SetProperties { .. }));
        let steps: Vec<_> = params
            .steps
            .into_iter()
            .map(|step| match step {
                BatchStepParams::Move {
                    object_ids,
                    dx_mm,
                    dy_mm,
                } => BatchEdit::Move {
                    object_ids,
                    dx_mm,
                    dy_mm,
                },
                BatchStepParams::Rotate {
                    object_ids,
                    angle_deg,
                    pivot_mm,
                } => BatchEdit::Rotate {
                    object_ids,
                    angle_deg,
                    pivot: MmPoint::new(pivot_mm.x_mm, pivot_mm.y_mm),
                },
                BatchStepParams::Mirror { object_ids, axis } => {
                    BatchEdit::Mirror { object_ids, axis }
                }
                BatchStepParams::SetProperties {
                    object_ids,
                    width_mm,
                    height_mm,
                } => BatchEdit::SetFlashSize {
                    object_ids,
                    width_mm,
                    height_mm,
                },
            })
            .collect();
        let ids = record
            .history
            .edit_batch(&mut record.document, &params.layer_id, &steps)
            .map_err(map_edit_error)?;
        if shape_changed {
            record.metrics.invalidate_shapes(&ids);
        }
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 1))
    }

    pub fn history_undo(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<EditResult, ServiceError> {
        let operation = rcam_diagnostics::Operation::begin_document(
            "history.undo",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.history_undo_observed(document_id, expected_revision);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn history_undo_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        if record.history.next_is_board(false) {
            record
                .history
                .step_board(&record.document.id, &mut record.board, false)
                .map_err(map_edit_error)?;
            record.revision += 1;
            return Ok(edit_result(document_id, record, vec![], 0));
        }
        let shape_changed = record.history.next_undo_changes_shape();
        let layer_effect = record.history.peek_undo_layer_effect();
        let workspace_revision = workspace::next_workspace_revision(record)?;
        let ids = record
            .history
            .undo(&mut record.document)
            .map_err(map_edit_error)?;
        let layer_ids = match layer_effect {
            Some(effect) => {
                let changed = match &effect {
                    editor_core::edit::LayerEffect::Added(ids)
                    | editor_core::edit::LayerEffect::Removed(ids) => ids.clone(),
                };
                let undoing_add = matches!(effect, editor_core::edit::LayerEffect::Removed(_));
                workspace::sync_layer_effect(record, effect, undoing_add);
                record.metrics = metrics::MetricsCache::default();
                record.workspace_revision = workspace_revision;
                changed
            }
            None => {
                if shape_changed {
                    record.metrics.invalidate_shapes(&ids);
                } else {
                    record.metrics.reconcile(&record.document, &ids);
                }
                Vec::new()
            }
        };
        record.revision += 1;
        let object_ids = if layer_ids.is_empty() {
            ids
        } else {
            Vec::new()
        };
        let mut result = edit_result(document_id, record, object_ids, 0);
        result.changed_layer_ids = layer_ids;
        Ok(result)
    }

    pub fn history_redo(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<EditResult, ServiceError> {
        let operation = rcam_diagnostics::Operation::begin_document(
            "history.redo",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.history_redo_observed(document_id, expected_revision);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn history_redo_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        if record.history.next_is_board(true) {
            record
                .history
                .step_board(&record.document.id, &mut record.board, true)
                .map_err(map_edit_error)?;
            record.revision += 1;
            return Ok(edit_result(document_id, record, vec![], 0));
        }
        let shape_changed = record.history.next_redo_changes_shape();
        let layer_effect = record.history.peek_redo_layer_effect();
        let workspace_revision = workspace::next_workspace_revision(record)?;
        let ids = record
            .history
            .redo(&mut record.document)
            .map_err(map_edit_error)?;
        let layer_ids = match layer_effect {
            Some(effect) => {
                let changed = match &effect {
                    editor_core::edit::LayerEffect::Added(ids)
                    | editor_core::edit::LayerEffect::Removed(ids) => ids.clone(),
                };
                workspace::sync_layer_effect(record, effect, false);
                record.metrics = metrics::MetricsCache::default();
                record.workspace_revision = workspace_revision;
                changed
            }
            None => {
                if shape_changed {
                    record.metrics.invalidate_shapes(&ids);
                } else {
                    record.metrics.reconcile(&record.document, &ids);
                }
                Vec::new()
            }
        };
        record.revision += 1;
        let object_ids = if layer_ids.is_empty() {
            ids
        } else {
            Vec::new()
        };
        let mut result = edit_result(document_id, record, object_ids, 0);
        result.changed_layer_ids = layer_ids;
        Ok(result)
    }

    fn edit_record(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<&mut S1DocumentRecord, ServiceError> {
        let record = self
            .documents
            .get_mut(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        if record.revision == u64::MAX {
            return Err(ServiceError::resource("revision", usize::MAX, usize::MAX));
        }
        Ok(record)
    }

    pub fn close(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        discard_changes: bool,
    ) -> Result<(), ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        if !discard_changes && (record.is_dirty() || record.is_project_dirty()) {
            return Err(ServiceError {
                code: "CONFIRMATION_REQUIRED".into(),
                message: "文档有未保存修改，需要明确放弃。".into(),
                details: serde_json::json!({"field": "params.discard_changes"}),
            });
        }
        self.documents.remove(document_id);
        Ok(())
    }

    pub fn validate(&self, document_id: &str) -> Result<ValidationResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        record.document.validate().map_err(map_semantic_error)?;
        Ok(ValidationResult {
            api_version: API_VERSION,
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            valid: true,
            diagnostics: record.diagnostics.clone(),
        })
    }

    /// Export policy is separate from content history. Revision fences stale work.
    pub fn set_manufacturing_precision(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        precision: ManufacturingPrecision,
    ) -> Result<DocumentInfo, ServiceError> {
        precision
            .validate()
            .map_err(|e| ServiceError::invalid_field("resolution_mm", &e))?;
        let record = self
            .documents
            .get_mut(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        if record.manufacturing_precision != precision {
            let revision = record
                .revision
                .checked_add(1)
                .ok_or_else(|| ServiceError::invalid("revision exhausted"))?;
            record.manufacturing_precision = precision;
            record.revision = revision;
        }
        Ok(document_info(document_id, record))
    }

    /// Export ONE layer as a new Gerber file. Export is an interchange copy,
    /// not a Save: it never clears the manufacturing dirty state, never links
    /// the layer to the output path and never changes any layer identity.
    pub fn export_layer(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ExportParams,
    ) -> Result<ExportResult, ServiceError> {
        let operation = rcam_diagnostics::Operation::begin_document(
            "export.layer",
            document_id,
            self.documents
                .get(document_id)
                .map(|record| record.revision),
        );
        let result = self.export_layer_observed(document_id, expected_revision, params);
        operation.end(
            self.documents
                .get(document_id)
                .map(|record| record.revision),
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    fn export_layer_observed(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ExportParams,
    ) -> Result<ExportResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        let access = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::permission(Path::new(&params.path), "write"))?;
        let target = access.write_path(&params.path)?;
        let snapshot = workspace::layer_export_snapshot(record, &params.layer_id)?;
        let metadata = serde_json::to_value(&snapshot.source).map_err(serialize_error)?;
        validate_export_policy(&params, &target, &metadata, &snapshot)?;
        // Resolve BlockInstance geometry into ordinary primitives before
        // precision normalization runs, so the current ManufacturingPrecision
        // governs former block geometry exactly like top-level geometry
        // (S4-B2 Final Closeout B0) instead of whatever precision was active
        // when the block definition was captured.
        let export_started = std::time::Instant::now();
        let flatten_started = std::time::Instant::now();
        let flattened =
            gerber_io::flatten_block_instances_for_export(&snapshot).map_err(map_s1_error)?;
        let flatten_us = flatten_started.elapsed().as_micros() as u64;
        let normalize_started = std::time::Instant::now();
        let project_precision = record.manufacturing_precision;
        let project_result = gerber_io::normalize_manufacturing(&flattened, project_precision);
        let document = match project_result {
            Ok(document) => {
                if params.compatibility_precision_override_mm.is_some() {
                    return Err(ServiceError::invalid_field(
                        "params.compatibility_precision_override_mm",
                        "project precision already preserves this geometry",
                    ));
                }
                document
            }
            Err(error) if !flattened.source.compatibility_issues.is_empty() => {
                let required = [0.001, 0.0001, 0.00001, 0.000001]
                    .into_iter()
                    .filter(|q| *q < project_precision.resolution_mm)
                    .find_map(|q| {
                        gerber_io::normalize_manufacturing(
                            &flattened,
                            ManufacturingPrecision { resolution_mm: q },
                        )
                        .ok()
                        .map(|document| (q, document))
                    });
                let Some((required_resolution_mm, document)) = required else {
                    return Err(ServiceError::invalid_field(
                        "manufacturing_precision",
                        &error,
                    ));
                };
                if params.compatibility_precision_override_mm != Some(required_resolution_mm) {
                    return Err(ServiceError {
                        code: "CONFIRMATION_REQUIRED".into(),
                        message: "compatibility geometry requires an explicitly approved finer export precision".into(),
                        details: serde_json::json!({
                            "reason": "compatibility_precision_override",
                            "project_resolution_mm": project_precision.resolution_mm,
                            "required_resolution_mm": required_resolution_mm,
                            "project_normalization_error": error,
                        }),
                    });
                }
                document
            }
            Err(error) => {
                return Err(ServiceError::invalid_field(
                    "manufacturing_precision",
                    &error,
                ));
            }
        };
        let normalize_us = normalize_started.elapsed().as_micros() as u64;
        let temp = temporary_output_path(&target)?;
        if temp.exists() {
            return Err(ServiceError {
                code: "FILE_CONFLICT".into(),
                message: "temporary writer path already exists".into(),
                details: serde_json::json!({"path": temp.to_string_lossy()}),
            });
        }
        gerber_io::export_timings::take();
        export_s1_new_path(&document, &temp).map_err(map_s1_error)?;
        let mut stages = gerber_io::export_timings::take();
        let _temp_guard = TemporaryOutput::new(temp.clone());
        let readback_started = std::time::Instant::now();
        let bytes = read_bounded_file(&temp, gerber_io::S1_MAX_WRITER_BYTES)?;
        *stages.entry("readback_us").or_default() += readback_started.elapsed().as_micros() as u64;
        let publish_started = std::time::Instant::now();
        if let Err(error) = fs::hard_link(&temp, &target) {
            if error.kind() == io::ErrorKind::AlreadyExists {
                return Err(ServiceError {
                    code: "FILE_CONFLICT".into(),
                    message: "output target appeared during no-clobber publish".into(),
                    details: serde_json::json!({"path": target.to_string_lossy()}),
                });
            }
            return Err(ServiceError::io(
                "publish output without replacement",
                &target,
                error,
            ));
        }
        stages.extend([
            ("flatten_us", flatten_us),
            ("normalize_us", normalize_us),
            ("publish_us", publish_started.elapsed().as_micros() as u64),
            ("total_us", export_started.elapsed().as_micros() as u64),
            ("output_bytes", bytes.len() as u64),
            (
                "object_count",
                document.layers.iter().map(|l| l.objects.len() as u64).sum(),
            ),
            (
                "project_precision_nm",
                (project_precision.resolution_mm * 1_000_000.).round() as u64,
            ),
            (
                "compatibility_override_used",
                u64::from(params.compatibility_precision_override_mm.is_some()),
            ),
            (
                "compatibility_warning_count",
                snapshot.source.compatibility_issues.len() as u64,
            ),
            (
                "block_flatten_count",
                snapshot
                    .layers
                    .iter()
                    .flat_map(|l| &l.objects)
                    .filter(|o| {
                        matches!(
                            o.geometry,
                            editor_core::SemanticGeometry::BlockInstance { .. }
                        )
                    })
                    .count() as u64,
            ),
            ("success", 1),
        ]);
        rcam_diagnostics::identified_measurements(
            rcam_diagnostics::Level::Info,
            "gerber.export.summary",
            None,
            Some(&params.layer_id),
            &stages.into_iter().collect::<Vec<_>>(),
        );
        let result = ExportResult {
            api_version: API_VERSION,
            document_id: document_id.into(),
            layer_id: params.layer_id,
            exported_revision: expected_revision.into(),
            current_revision: record.revision.to_string(),
            path: target.to_string_lossy().into_owned(),
            sha256: sha256_hex(&bytes),
            bytes: bytes.len(),
        };
        // Only the export policy baseline moves: the chosen precision has now
        // been applied to an output. Project (manufacturing) dirty is untouched.
        let record = self.documents.get_mut(document_id).unwrap();
        record.saved_precision = record.manufacturing_precision;
        Ok(result)
    }

    /// Every JSON call, including rejection, returns the same response shape.
    pub fn execute_json(&mut self, raw: &str) -> Value {
        let request: RequestEnvelope = match serde_json::from_str(raw) {
            Ok(request) => request,
            Err(error) => {
                let identity = serde_json::from_str::<RequestIdentity>(raw).ok();
                return response(
                    identity
                        .as_ref()
                        .and_then(|value| value.request_id.as_deref()),
                    None,
                    None,
                    Err(ServiceError {
                        code: "INVALID_ARGUMENT".into(),
                        message: format!("invalid request: {error}"),
                        details: serde_json::json!({"field": "request", "line": error.line(), "column": error.column(), "diagnostic": error.to_string()}),
                    }),
                );
            }
        };
        let result = self.dispatch(&request);
        let revision = request.document_id.as_ref().and_then(|id| {
            self.documents
                .get(id)
                .map(|record| record.revision.to_string())
                .or_else(|| self.scenes.contains_key(id).then(|| "0".into()))
        });
        response(
            Some(&request.request_id),
            request.document_id.as_deref(),
            revision.as_deref(),
            result,
        )
    }

    fn dispatch(&mut self, request: &RequestEnvelope) -> Result<Value, ServiceError> {
        if request.api_version != API_VERSION {
            return Err(ServiceError {
                code: "UNSUPPORTED_API_VERSION".into(),
                message: format!("api_version {} is not supported", request.api_version),
                details: serde_json::json!({"requested": request.api_version, "supported": API_VERSION}),
            });
        }
        if request.request_id.trim().is_empty() {
            return Err(ServiceError::invalid("request_id must not be empty"));
        }
        let result = match request.op.as_str() {
            "system.capabilities" => {
                parse_empty_params(&request.params)?;
                if request.document_id.is_some() || request.expected_revision.is_some() {
                    return Err(ServiceError {
                        code: "INVALID_ARGUMENT".into(),
                        message:
                            "system.capabilities does not accept document_id or expected_revision"
                                .into(),
                        details: serde_json::json!({"field": request.document_id.as_ref().map(|_| "document_id").unwrap_or("expected_revision")}),
                    });
                }
                serde_json::to_value(self.capabilities()).map_err(serialize_error)?
            }
            "components.preview_pnp" if self.file_access.is_some() => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Params {
                    path: String,
                    mapping: editor_core::pnp::PnpMapping,
                }
                let p: Params = parse_params(&request.params)?;
                if request.expected_revision.is_some() || request.document_id.is_some() {
                    return Err(ServiceError::invalid(
                        "preview does not accept document_id or expected_revision",
                    ));
                }
                serde_json::to_value(self.components_preview_pnp(&p.path, &p.mapping)?)
                    .map_err(serialize_error)?
            }
            "components.import_pnp" | "board.set_registration" if self.file_access.is_some() => {
                let id = required_document_id(request)?;
                let revision = request
                    .expected_revision
                    .as_deref()
                    .ok_or_else(|| ServiceError::invalid("expected_revision required"))?;
                let result = if request.op == "components.import_pnp" {
                    self.components_import_pnp(id, revision, parse_params(&request.params)?)?
                } else {
                    self.board_set_registration(id, revision, parse_params(&request.params)?)?
                };
                serde_json::to_value(result).map_err(serialize_error)?
            }
            "components.nearby_manufacturing" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid("read query uses params.revision"));
                }
                let q: NearbyManufacturingQuery = parse_params(&request.params)?;
                serde_json::to_value(
                    self.components_nearby_manufacturing(required_document_id(request)?, &q)?,
                )
                .map_err(serialize_error)?
            }
            "components.list"
            | "components.search"
            | "components.get"
            | "board.get_registration"
                if self.file_access.is_some() =>
            {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid("read query uses params.revision"));
                }
                let id = required_document_id(request)?;
                match request.op.as_str() {
                    "components.get" => {
                        #[derive(Deserialize)]
                        #[serde(deny_unknown_fields)]
                        struct Params {
                            component_id: String,
                        }
                        let p: Params = parse_params(&request.params)?;
                        serde_json::to_value(self.components_get(id, &p.component_id)?)
                            .map_err(serialize_error)?
                    }
                    "board.get_registration" => {
                        parse_empty_params(&request.params)?;
                        serde_json::to_value(self.board_get_registration(id)?)
                            .map_err(serialize_error)?
                    }
                    _ => {
                        let q: ComponentQuery = parse_params(&request.params)?;
                        serde_json::to_value(if request.op == "components.list" {
                            self.components_list(id, &q)?
                        } else {
                            self.components_search(id, &q)?
                        })
                        .map_err(serialize_error)?
                    }
                }
            }
            "document.open_s0" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "document.open_s0 does not accept expected_revision",
                    ));
                }
                let document_id = request
                    .document_id
                    .clone()
                    .ok_or_else(|| ServiceError::invalid("document_id is required"))?;
                let params: OpenS0Params = parse_params(&request.params)?;
                serde_json::to_value(self.open_s0(document_id, params.source.as_bytes())?)
                    .map_err(serialize_error)?
            }
            "document.open" => {
                if request.document_id.is_some() || request.expected_revision.is_some() {
                    return Err(ServiceError::invalid(
                        "document.open does not accept document_id or expected_revision",
                    ));
                }
                let params: OpenParams = parse_params(&request.params)?;
                serde_json::to_value(self.open(&params.path)?).map_err(serialize_error)?
            }
            "document.new" if self.file_access.is_some() => {
                parse_empty_params(&request.params)?;
                if request.document_id.is_some() || request.expected_revision.is_some() {
                    return Err(ServiceError::invalid(
                        "document.new does not accept document_id or expected_revision",
                    ));
                }
                serde_json::to_value(self.document_new()?).map_err(serialize_error)?
            }
            "project.new" if self.file_access.is_some() => {
                parse_empty_params(&request.params)?;
                if request.document_id.is_some() || request.expected_revision.is_some() {
                    return Err(ServiceError::invalid(
                        "project.new does not accept document_id or expected_revision",
                    ));
                }
                serde_json::to_value(self.document_new()?).map_err(serialize_error)?
            }
            "project.open" if self.file_access.is_some() => {
                if request.document_id.is_some() || request.expected_revision.is_some() {
                    return Err(ServiceError::invalid(
                        "project.open does not accept document_id or expected_revision",
                    ));
                }
                let params: OpenParams = parse_params(&request.params)?;
                serde_json::to_value(self.project_open(&params.path)?).map_err(serialize_error)?
            }
            "project.info" if self.file_access.is_some() => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                serde_json::to_value(self.document_get(required_document_id(request)?)?)
                    .map_err(serialize_error)?
            }
            "project.save" | "project.save_as" if self.file_access.is_some() => {
                let id = required_document_id(request)?;
                let revision = request.expected_revision.as_deref().ok_or_else(|| {
                    ServiceError::invalid_field(
                        "expected_revision",
                        "expected_revision is required",
                    )
                })?;
                let info = if request.op == "project.save" {
                    parse_empty_params(&request.params)?;
                    self.project_save(id, revision, None, false)?
                } else {
                    let params: ProjectSaveAsParams = parse_params(&request.params)?;
                    self.project_save(id, revision, Some(&params.path), params.allow_replace)?
                };
                serde_json::to_value(info).map_err(serialize_error)?
            }
            "layer.summary" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let id = required_document_id(request)?;
                serde_json::to_value(self.layer_summary(id, parse_params(&request.params)?)?)
                    .map_err(serialize_error)?
            }
            "document.visible_bounds" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                serde_json::to_value(self.visible_bounds(required_document_id(request)?)?)
                    .map_err(serialize_error)?
            }
            "document.get" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                serde_json::to_value(self.document_get(document_id)?).map_err(serialize_error)?
            }
            "render.snapshot" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation",
                    ));
                }
                serde_json::to_value(self.render_snapshot(required_document_id(request)?)?)
                    .map_err(serialize_error)?
            }
            "layers.list" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                let layers = self.layers_list(document_id)?;
                serde_json::to_value(layers).map_err(serialize_error)?
            }
            "objects.metrics" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let id = required_document_id(request)?;
                serde_json::to_value(self.objects_metrics(id, parse_params(&request.params)?)?)
                    .map_err(serialize_error)?
            }
            "blocks.list_definitions" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                serde_json::to_value(self.blocks_list_definitions(document_id)?)
                    .map_err(serialize_error)?
            }
            "blocks.get_definition" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                serde_json::to_value(
                    self.blocks_get_definition(document_id, parse_params(&request.params)?)?,
                )
                .map_err(serialize_error)?
            }
            "objects.select_rect" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let id = required_document_id(request)?;
                serde_json::to_value(self.objects_select_rect(id, parse_params(&request.params)?)?)
                    .map_err(serialize_error)?
            }
            "objects.hit_test" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let id = required_document_id(request)?;
                serde_json::to_value(self.objects_hit_test(id, parse_params(&request.params)?)?)
                    .map_err(serialize_error)?
            }
            "document.bounds" | "layer.bounds" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                let result = if request.op == "document.bounds" {
                    parse_empty_params(&request.params)?;
                    self.document_bounds(document_id)?
                } else {
                    self.layer_bounds(document_id, parse_params(&request.params)?)?
                };
                serde_json::to_value(result).map_err(serialize_error)?
            }
            "layer.update" if self.file_access.is_some() => {
                let id = required_document_id(request)?;
                let revision = request.expected_revision.as_deref().ok_or_else(|| {
                    ServiceError::invalid_field(
                        "expected_revision",
                        "expected_revision is required",
                    )
                })?;
                serde_json::to_value(self.layer_update(
                    id,
                    revision,
                    parse_params(&request.params)?,
                )?)
                .map_err(serialize_error)?
            }
            "objects.query" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                let params: QueryParams = parse_params(&request.params)?;
                serde_json::to_value(self.objects_query(document_id, params)?)
                    .map_err(serialize_error)?
            }
            "objects.grips" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                serde_json::to_value(self.objects_grips(
                    required_document_id(request)?,
                    parse_params(&request.params)?,
                )?)
                .map_err(serialize_error)?
            }
            "objects.get" => {
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                let params: ObjectParams = parse_params(&request.params)?;
                serde_json::to_value(self.objects_get(document_id, params)?)
                    .map_err(serialize_error)?
            }
            "document.import_gerber_layers"
            | "document.import_gerber_layer"
            | "document.create_empty_layer"
            | "document.remove_layer"
            | "layers.reorder"
            | "layers.set_active"
            | "layers.set_solo"
            | "layers.update_many"
            | "layers.reset_colors"
            | "objects.edit_selection"
            | "objects.move"
            | "objects.array_rectangular"
            | "objects.align"
            | "objects.distribute"
            | "objects.rotate"
            | "objects.mirror"
            | "objects.duplicate"
            | "objects.delete"
            | "objects.set_properties"
            | "objects.grip_edit"
            | "blocks.create_definition_from_objects"
            | "blocks.create_instance"
            | "blocks.update_instance_transform"
            | "blocks.rename_definition"
            | "blocks.explode_instance"
            | "blocks.delete_definition"
            | "edit.batch"
            | "text.create"
            | "text.preview"
            | "history.undo"
            | "history.redo"
            | "document.close"
                if self.file_access.is_some() =>
            {
                let id = required_document_id(request)?;
                let revision = request.expected_revision.as_deref().ok_or_else(|| {
                    ServiceError::invalid_field(
                        "expected_revision",
                        "expected_revision is required",
                    )
                })?;
                match request.op.as_str() {
                    "document.import_gerber_layers" => serde_json::to_value(
                        self.import_gerber_layers(id, revision, parse_params(&request.params)?)?,
                    )
                    .map_err(serialize_error)?,
                    "document.import_gerber_layer" => serde_json::to_value(
                        self.import_gerber_layer(id, revision, parse_params(&request.params)?)?,
                    )
                    .map_err(serialize_error)?,
                    "document.create_empty_layer" => serde_json::to_value(
                        self.create_empty_layer(id, revision, parse_params(&request.params)?)?,
                    )
                    .map_err(serialize_error)?,
                    "document.remove_layer" => serde_json::to_value(self.remove_layer(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "layers.reorder" => serde_json::to_value(self.layers_reorder(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "layers.set_active" => serde_json::to_value(self.layers_set_active(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "layers.set_solo" => serde_json::to_value(self.layers_set_solo(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "layers.update_many" => serde_json::to_value(self.layers_update_many(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "layers.reset_colors" => serde_json::to_value(self.layers_reset_colors(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.edit_selection" => serde_json::to_value(self.objects_edit_selection(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.move" => serde_json::to_value(self.objects_move(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.array_rectangular" => {
                        serde_json::to_value(self.objects_array_rectangular(
                            id,
                            revision,
                            parse_params(&request.params)?,
                        )?)
                        .map_err(serialize_error)?
                    }
                    "objects.align" => serde_json::to_value(self.objects_align(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.distribute" => serde_json::to_value(self.objects_distribute(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.rotate" => serde_json::to_value(self.objects_rotate(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.mirror" => serde_json::to_value(self.objects_mirror(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.duplicate" => serde_json::to_value(self.objects_duplicate(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.delete" => serde_json::to_value(self.objects_delete(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.grip_edit" => serde_json::to_value(self.objects_grip_edit(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "objects.set_properties" => serde_json::to_value(self.objects_set_properties(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "blocks.create_definition_from_objects" => {
                        serde_json::to_value(self.blocks_create_definition_from_objects(
                            id,
                            revision,
                            parse_params(&request.params)?,
                        )?)
                        .map_err(serialize_error)?
                    }
                    "blocks.create_instance" => serde_json::to_value(self.blocks_create_instance(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "blocks.update_instance_transform" => {
                        serde_json::to_value(self.blocks_update_instance_transform(
                            id,
                            revision,
                            parse_params(&request.params)?,
                        )?)
                        .map_err(serialize_error)?
                    }
                    "blocks.rename_definition" => {
                        serde_json::to_value(self.blocks_rename_definition(
                            id,
                            revision,
                            parse_params(&request.params)?,
                        )?)
                        .map_err(serialize_error)?
                    }
                    "blocks.explode_instance" => serde_json::to_value(
                        self.blocks_explode_instance(id, revision, parse_params(&request.params)?)?,
                    )
                    .map_err(serialize_error)?,
                    "blocks.delete_definition" => {
                        serde_json::to_value(self.blocks_delete_definition(
                            id,
                            revision,
                            parse_params(&request.params)?,
                        )?)
                        .map_err(serialize_error)?
                    }
                    "text.preview" => serde_json::to_value(self.text_preview(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "text.create" => serde_json::to_value(self.text_create(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "edit.batch" => serde_json::to_value(self.edit_batch(
                        id,
                        revision,
                        parse_params(&request.params)?,
                    )?)
                    .map_err(serialize_error)?,
                    "document.close" => {
                        let params: CloseParams = parse_params(&request.params)?;
                        self.close(id, revision, params.discard_changes)?;
                        serde_json::json!({"closed": true})
                    }
                    _ => {
                        parse_empty_params(&request.params)?;
                        let result = if request.op == "history.undo" {
                            self.history_undo(id, revision)?
                        } else {
                            self.history_redo(id, revision)?
                        };
                        serde_json::to_value(result).map_err(serialize_error)?
                    }
                }
            }
            "document.validate" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = required_document_id(request)?;
                serde_json::to_value(self.validate(document_id)?).map_err(serialize_error)?
            }
            "document.set_manufacturing_precision" => {
                let id = required_document_id(request)?;
                let revision = request
                    .expected_revision
                    .as_deref()
                    .ok_or_else(|| ServiceError::invalid("expected_revision is required"))?;
                let precision = serde_json::from_value(request.params.clone())
                    .map_err(|e| ServiceError::invalid(e.to_string()))?;
                serde_json::to_value(self.set_manufacturing_precision(id, revision, precision)?)
                    .map_err(serialize_error)?
            }
            "gerber.export_layer" => {
                let document_id = required_document_id(request)?;
                let expected_revision = request.expected_revision.as_deref().ok_or_else(|| {
                    ServiceError::invalid_field(
                        "expected_revision",
                        "expected_revision is required",
                    )
                })?;
                let params: ExportParams = parse_params(&request.params)?;
                serde_json::to_value(self.export_layer(document_id, expected_revision, params)?)
                    .map_err(serialize_error)?
            }
            "document.snapshot" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = request
                    .document_id
                    .clone()
                    .ok_or_else(|| ServiceError::invalid("document_id is required"))?;
                serde_json::to_value(self.snapshot(&document_id)?).map_err(serialize_error)?
            }
            "document.analyze_s0" => {
                parse_empty_params(&request.params)?;
                if request.expected_revision.is_some() {
                    return Err(ServiceError::invalid_field(
                        "expected_revision",
                        "read-only operation does not accept expected_revision",
                    ));
                }
                let document_id = request
                    .document_id
                    .clone()
                    .ok_or_else(|| ServiceError::invalid("document_id is required"))?;
                serde_json::to_value(self.analyze_s0(&document_id)?).map_err(serialize_error)?
            }
            _ => {
                return Err(ServiceError {
                    code: "UNSUPPORTED_OPERATION".into(),
                    message: format!("unsupported operation: {}", request.op),
                    details: serde_json::json!({"op": request.op}),
                });
            }
        };
        Ok(result)
    }

    pub fn snapshot(&self, document_id: &str) -> Result<DocumentSnapshot, ServiceError> {
        self.scenes
            .get(document_id)
            .map(|scene| scene.document.snapshot(0))
            .ok_or_else(|| ServiceError::not_found("document", document_id))
    }

    pub fn analyze_s0(&self, document_id: &str) -> Result<AnalysisResult, ServiceError> {
        let scene = self
            .scenes
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let layer_id = scene
            .document
            .layers
            .first()
            .map(|layer| layer.id.clone())
            .ok_or_else(|| ServiceError::invalid("document has no layer"))?;
        let points = [0.0, 2.0, 4.0, 6.0].map(|x| MmPoint::new(x, 0.0));
        let samples = points
            .into_iter()
            .map(|point| CoverageSample {
                point,
                layer_id: layer_id.clone(),
                covered: scene
                    .document
                    .layer_coverage_at(&layer_id, point)
                    .unwrap_or(false),
            })
            .collect();
        Ok(AnalysisResult {
            api_version: API_VERSION,
            document_id: document_id.into(),
            revision: "0".into(),
            samples,
        })
    }
}

impl ServiceError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_ARGUMENT".into(),
            message: message.into(),
            details: serde_json::json!({"field": "request"}),
        }
    }

    fn invalid_field(field: &str, message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_ARGUMENT".into(),
            message: message.into(),
            details: serde_json::json!({"field": field}),
        }
    }

    fn permission(path: &Path, operation: &str) -> Self {
        Self {
            code: "PERMISSION_DENIED".into(),
            message: format!("host file policy denied {operation} for {}", path.display()),
            details: serde_json::json!({"path": path.to_string_lossy(), "operation": operation}),
        }
    }

    fn io(operation: &str, path: &Path, error: io::Error) -> Self {
        Self {
            code: "IO_ERROR".into(),
            message: format!("{operation} {}: {error}", path.display()),
            details: serde_json::json!({"path": path.to_string_lossy(), "operation": operation, "kind": format!("{:?}", error.kind())}),
        }
    }

    fn resource(resource: &str, limit: usize, actual: usize) -> Self {
        Self {
            code: "RESOURCE_LIMIT".into(),
            message: format!("{resource} exceeds configured budget"),
            details: serde_json::json!({"resource": resource, "limit": limit, "actual": actual}),
        }
    }

    fn not_found(entity: &str, id: &str) -> Self {
        Self {
            code: "NOT_FOUND".into(),
            message: format!("{entity} {id:?} was not found"),
            details: serde_json::json!({"entity": entity, "id": id}),
        }
    }
}

fn required_document_id(request: &RequestEnvelope) -> Result<&str, ServiceError> {
    request
        .document_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| ServiceError::invalid_field("document_id", "document_id is required"))
}

fn document_info(document_id: &str, record: &S1DocumentRecord) -> DocumentInfo {
    DocumentInfo {
        project_id: record.project_id.clone(),
        project_path: record
            .project_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        project_dirty: record.is_project_dirty(),
        last_saved_project_hash: record.last_saved_project_hash.clone(),
        manufacturing_precision: record.manufacturing_precision,
        export_policy_dirty: record.manufacturing_precision != record.saved_precision,
        workspace_revision: record.workspace_revision.to_string(),
        api_version: API_VERSION,
        document_id: document_id.into(),
        revision: record.revision.to_string(),
        source_path: record
            .opened_from
            .as_ref()
            .map(|(path, _)| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        source_sha256: record
            .opened_from
            .as_ref()
            .map(|(_, sha)| sha.clone())
            .unwrap_or_default(),
        dirty: record.is_dirty(),
        undo_entries: record.history.undo_len(),
        redo_entries: record.history.redo_len(),
        history_bytes: record.history.bytes(),
        history_truncated_entries: record.history.truncated_entries(),
        history_truncated_bytes: record.history.truncated_bytes(),
        layer_ids: record
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect(),
        display_order: record.display_order.clone(),
        active_layer_id: record.active_layer_id.clone(),
        solo_layer_id: record.solo_layer_id.clone(),
        diagnostics: record.diagnostics.clone(),
    }
}

fn edit_result(id: &str, record: &S1DocumentRecord, ids: Vec<String>, added: usize) -> EditResult {
    EditResult {
        document_id: id.into(),
        revision: record.revision.to_string(),
        changed_object_ids: ids,
        changed_layer_ids: Vec::new(),
        undo_entries_added: added,
        undo_entries: record.history.undo_len(),
        redo_entries: record.history.redo_len(),
        history_bytes: record.history.bytes(),
        history_truncated_entries: record.history.truncated_entries(),
        history_truncated_bytes: record.history.truncated_bytes(),
        dirty: record.is_dirty(),
    }
}

fn map_edit_error(error: EditError) -> ServiceError {
    match error {
        EditError::NotFound { entity, id } => ServiceError::not_found(entity, &id),
        EditError::InvalidArgument => {
            ServiceError::invalid_field("params", "目标集合、变换参数或数值精度无效，不能提交。")
        }
        EditError::UnsupportedTransform => ServiceError {
            code: "UNSUPPORTED_FEATURE".into(),
            message: "矩形扫掠仅支持整数 90° 旋转及水平／垂直镜像。".into(),
            details: serde_json::json!({"geometry": "RectangularSweep", "rotation_step_deg": 90}),
        },
        EditError::ResourceLimit => ServiceError {
            code: "RESOURCE_LIMIT".into(),
            message: "编辑对象数量或撤销历史超过预算。".into(),
            details: serde_json::json!({"max_edit_objects": MAX_MOVE_OBJECTS, "max_move_objects": MAX_MOVE_OBJECTS, "max_document_objects": gerber_io::S1_MAX_OBJECTS, "max_region_edges": gerber_io::S1_MAX_REGION_EDGES, "max_history_entries": MAX_HISTORY_ENTRIES, "max_history_bytes": MAX_HISTORY_BYTES}),
        },
        EditError::EmptyHistory => ServiceError::invalid_field("op", "没有可撤销或重做的事务。"),
        EditError::InvalidGeometry(error) => map_semantic_error(error),
        EditError::BlockDefinitionReferenced => ServiceError {
            code: "BLOCK_DEFINITION_REFERENCED".into(),
            message: "该 Block Definition 仍被实例引用，需先删除/Explode 引用它的实例。".into(),
            details: serde_json::json!({}),
        },
    }
}

// Stream the stable owned model into the existing SHA-256 implementation;
// no document-sized serialization buffer or revision-derived dirty flag.
fn content_hash<T: Serialize + ?Sized>(document: &T) -> String {
    struct HashWriter(Sha256);
    impl io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // JSON emits many tiny writes. Buffer those without allocating a
    // document-sized byte vector, while hashing exactly the same JSON bytes.
    let mut writer = io::BufWriter::with_capacity(64 * 1024, HashWriter(Sha256::new()));
    serde_json::to_writer(&mut writer, document)
        .expect("validated finite model and infallible hash writer");
    writer
        .into_inner()
        .unwrap_or_else(|_| unreachable!("infallible hash writer"))
        .0
        .finish()
}

fn check_revision(actual: u64, expected: &str) -> Result<(), ServiceError> {
    let Ok(expected) = expected.parse::<u64>() else {
        return Err(ServiceError::invalid_field(
            "expected_revision",
            "expected_revision must be a decimal integer string",
        ));
    };
    if expected != actual {
        return Err(ServiceError {
            code: "REVISION_CONFLICT".into(),
            message: "文档已修改，请重新查询后提交。".into(),
            details: serde_json::json!({"expected_revision": expected.to_string(), "actual_revision": actual.to_string()}),
        });
    }
    Ok(())
}

fn validate_query_params(params: &QueryParams) -> Result<(), ServiceError> {
    if params.layer_id.trim().is_empty() {
        return Err(ServiceError::invalid_field(
            "params.layer_id",
            "layer_id must not be empty",
        ));
    }
    if let Some(kind) = &params.geometry_type
        && !matches!(
            kind.as_str(),
            "flash" | "line" | "rectangular_sweep" | "rectangular_draw" | "arc" | "region"
        )
    {
        return Err(ServiceError::invalid_field(
            "params.geometry_type",
            "unsupported geometry_type",
        ));
    }
    if let Some(region) = &params.region_mm {
        if !region.min_x_mm.is_finite()
            || !region.min_y_mm.is_finite()
            || !region.max_x_mm.is_finite()
            || !region.max_y_mm.is_finite()
            || region.min_x_mm > region.max_x_mm
            || region.min_y_mm > region.max_y_mm
        {
            return Err(ServiceError::invalid_field(
                "params.region_mm",
                "region coordinates must be finite and ordered",
            ));
        }
        if params.relation.is_none() {
            return Err(ServiceError::invalid_field(
                "params.relation",
                "relation is required when region_mm is present",
            ));
        }
    } else if params.relation.is_some() {
        return Err(ServiceError::invalid_field(
            "params.relation",
            "relation requires region_mm",
        ));
    }
    if params.limit.is_some_and(|limit| limit == 0 || limit > 1000) {
        return Err(ServiceError::invalid_field(
            "params.limit",
            "limit must be between 1 and 1000",
        ));
    }
    Ok(())
}

fn query_fingerprint(document_id: &str, params: &QueryParams) -> u64 {
    let mut hasher = DefaultHasher::new();
    document_id.hash(&mut hasher);
    params.layer_id.hash(&mut hasher);
    params.geometry_type.hash(&mut hasher);
    params.relation.hash(&mut hasher);
    params.limit.hash(&mut hasher);
    if let Some(region) = &params.region_mm {
        region.min_x_mm.to_bits().hash(&mut hasher);
        region.min_y_mm.to_bits().hash(&mut hasher);
        region.max_x_mm.to_bits().hash(&mut hasher);
        region.max_y_mm.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn encode_cursor(document_id: &str, revision: u64, fingerprint: u64, offset: usize) -> String {
    format!("d={document_id}:r{revision}:h{fingerprint}:o{offset}")
}

fn decode_cursor(
    cursor: Option<&str>,
    document_id: &str,
    revision: u64,
    fingerprint: u64,
) -> Result<usize, ServiceError> {
    let Some(cursor) = cursor else { return Ok(0) };
    let mut pieces = cursor.split(':');
    let (Some(doc), Some(rev), Some(hash), Some(offset), None) = (
        pieces.next(),
        pieces.next(),
        pieces.next(),
        pieces.next(),
        pieces.next(),
    ) else {
        return Err(ServiceError::invalid_field(
            "params.cursor",
            "invalid query cursor",
        ));
    };
    let Some(doc) = doc.strip_prefix("d=") else {
        return Err(ServiceError::invalid_field(
            "params.cursor",
            "invalid query cursor document",
        ));
    };
    if doc != document_id {
        return Err(ServiceError {
            code: "REVISION_CONFLICT".into(),
            message: "query cursor belongs to another document".into(),
            details: serde_json::json!({"cursor_document_id": doc, "document_id": document_id}),
        });
    }
    let Ok(rev) = rev.strip_prefix('r').unwrap_or_default().parse::<u64>() else {
        return Err(ServiceError::invalid_field(
            "params.cursor",
            "invalid query cursor revision",
        ));
    };
    let Ok(hash) = hash.strip_prefix('h').unwrap_or_default().parse::<u64>() else {
        return Err(ServiceError::invalid_field(
            "params.cursor",
            "invalid query cursor fingerprint",
        ));
    };
    let Ok(offset) = offset
        .strip_prefix('o')
        .unwrap_or_default()
        .parse::<usize>()
    else {
        return Err(ServiceError::invalid_field(
            "params.cursor",
            "invalid query cursor offset",
        ));
    };
    if rev != revision || hash != fingerprint {
        return Err(ServiceError {
            code: "REVISION_CONFLICT".into(),
            message: "query cursor is stale; query the current document again".into(),
            details: serde_json::json!({"cursor_revision": rev.to_string(), "actual_revision": revision.to_string()}),
        });
    }
    Ok(offset)
}

fn geometry_kind(geometry: &SemanticGeometry) -> &'static str {
    match geometry {
        SemanticGeometry::Flash { .. } => "flash",
        SemanticGeometry::Line { .. } => "line",
        SemanticGeometry::RectangularSweep { .. } => "rectangular_sweep",
        SemanticGeometry::Arc { .. } => "arc",
        SemanticGeometry::Region { .. } => "region",
        SemanticGeometry::BlockInstance { .. } => "block_instance",
    }
}

fn geometry_kind_matches(geometry: &SemanticGeometry, requested: &str) -> bool {
    geometry_kind(geometry) == requested
        || (requested == "rectangular_draw"
            && matches!(geometry, SemanticGeometry::RectangularSweep { .. }))
}

fn matches_region(
    document: &SemanticDocument,
    geometry: &SemanticGeometry,
    region: &QueryRegion,
    relation: &QueryRelation,
) -> Result<bool, ServiceError> {
    match geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            transform,
        } => {
            let aperture = document
                .apertures
                .iter()
                .find(|aperture| aperture.id == *aperture_id)
                .ok_or_else(|| ServiceError::not_found("aperture", aperture_id))?;
            let ApertureShape::Circle {
                diameter_mm,
                hole_diameter_mm,
            } = &aperture.shape
            else {
                return Err(unsupported_region_query("non-circular flash aperture"));
            };
            if hole_diameter_mm.is_some() {
                return Err(unsupported_region_query("annulus relation"));
            }
            if transform.mirror != editor_core::Mirror::None
                || !transform.rotation_deg.is_finite()
                || !transform.scale.is_finite()
            {
                return Err(unsupported_region_query("transformed flash aperture"));
            }
            let radius = diameter_mm * transform.scale / 2.0;
            match relation {
                QueryRelation::Intersects => Ok(circle_intersects_region(*center, radius, region)),
                QueryRelation::Contains => Ok(region_corners(region)
                    .into_iter()
                    .all(|point| point.distance_mm(*center) <= radius + editor_core::EPSILON_MM)),
            }
        }
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => match relation {
            QueryRelation::Intersects => Ok(segment_distance_to_region(*start, *end, region)
                <= width_mm / 2.0 + editor_core::EPSILON_MM),
            QueryRelation::Contains => Ok(false),
        },
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => {
            if start.x_mm != end.x_mm && start.y_mm != end.y_mm {
                return Err(unsupported_region_query("diagonal rectangular sweep"));
            }
            let half_x = width_mm / 2.0;
            let half_y = height_mm / 2.0;
            let min_x = start.x_mm.min(end.x_mm) - half_x;
            let max_x = start.x_mm.max(end.x_mm) + half_x;
            let min_y = start.y_mm.min(end.y_mm) - half_y;
            let max_y = start.y_mm.max(end.y_mm) + half_y;
            match relation {
                QueryRelation::Intersects => Ok(min_x <= region.max_x_mm + editor_core::EPSILON_MM
                    && max_x >= region.min_x_mm - editor_core::EPSILON_MM
                    && min_y <= region.max_y_mm + editor_core::EPSILON_MM
                    && max_y >= region.min_y_mm - editor_core::EPSILON_MM),
                QueryRelation::Contains => Ok(min_x <= region.min_x_mm + editor_core::EPSILON_MM
                    && max_x >= region.max_x_mm - editor_core::EPSILON_MM
                    && min_y <= region.min_y_mm + editor_core::EPSILON_MM
                    && max_y >= region.max_y_mm - editor_core::EPSILON_MM),
            }
        }
        SemanticGeometry::Arc { .. } | SemanticGeometry::Region { .. } => {
            Err(unsupported_region_query("arc/region exact relation"))
        }
        SemanticGeometry::BlockInstance { .. } => {
            Err(unsupported_region_query("block instance exact relation"))
        }
    }
}

fn unsupported_region_query(reason: &str) -> ServiceError {
    ServiceError {
        code: "UNSUPPORTED_FEATURE".into(),
        message: "exact rectangle query is unavailable for this geometry".into(),
        details: serde_json::json!({"reason": reason}),
    }
}

fn region_corners(region: &QueryRegion) -> [MmPoint; 4] {
    [
        MmPoint::new(region.min_x_mm, region.min_y_mm),
        MmPoint::new(region.min_x_mm, region.max_y_mm),
        MmPoint::new(region.max_x_mm, region.min_y_mm),
        MmPoint::new(region.max_x_mm, region.max_y_mm),
    ]
}

fn circle_intersects_region(center: MmPoint, radius: f64, region: &QueryRegion) -> bool {
    let nearest = MmPoint::new(
        center.x_mm.clamp(region.min_x_mm, region.max_x_mm),
        center.y_mm.clamp(region.min_y_mm, region.max_y_mm),
    );
    nearest.distance_mm(center) <= radius + editor_core::EPSILON_MM
}

fn segment_distance_to_region(start: MmPoint, end: MmPoint, region: &QueryRegion) -> f64 {
    if point_in_region(start, region)
        || point_in_region(end, region)
        || segment_hits_region(start, end, region)
    {
        return 0.0;
    }
    let corners = region_corners(region);
    let edges = [
        (corners[0], corners[2]),
        (corners[2], corners[3]),
        (corners[3], corners[1]),
        (corners[1], corners[0]),
    ];
    edges
        .into_iter()
        .map(|(a, b)| segment_distance(start, end, a, b))
        .fold(f64::INFINITY, f64::min)
}

fn point_in_region(point: MmPoint, region: &QueryRegion) -> bool {
    point.x_mm >= region.min_x_mm - editor_core::EPSILON_MM
        && point.x_mm <= region.max_x_mm + editor_core::EPSILON_MM
        && point.y_mm >= region.min_y_mm - editor_core::EPSILON_MM
        && point.y_mm <= region.max_y_mm + editor_core::EPSILON_MM
}

fn segment_hits_region(start: MmPoint, end: MmPoint, region: &QueryRegion) -> bool {
    let dx = end.x_mm - start.x_mm;
    let dy = end.y_mm - start.y_mm;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-dx, start.x_mm - region.min_x_mm),
        (dx, region.max_x_mm - start.x_mm),
        (-dy, start.y_mm - region.min_y_mm),
        (dy, region.max_y_mm - start.y_mm),
    ] {
        if p.abs() <= f64::EPSILON {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

fn segment_distance(a: MmPoint, b: MmPoint, c: MmPoint, d: MmPoint) -> f64 {
    if segment_hits_region(
        a,
        b,
        &QueryRegion {
            min_x_mm: c.x_mm.min(d.x_mm),
            min_y_mm: c.y_mm.min(d.y_mm),
            max_x_mm: c.x_mm.max(d.x_mm),
            max_y_mm: c.y_mm.max(d.y_mm),
        },
    ) {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
}

fn point_segment_distance(point: MmPoint, start: MmPoint, end: MmPoint) -> f64 {
    let dx = end.x_mm - start.x_mm;
    let dy = end.y_mm - start.y_mm;
    let len2 = dx * dx + dy * dy;
    if len2 <= f64::EPSILON {
        return point.distance_mm(start);
    }
    let t =
        (((point.x_mm - start.x_mm) * dx + (point.y_mm - start.y_mm) * dy) / len2).clamp(0.0, 1.0);
    point.distance_mm(MmPoint::new(start.x_mm + t * dx, start.y_mm + t * dy))
}

fn validate_export_policy(
    params: &ExportParams,
    target: &Path,
    metadata: &Value,
    snapshot: &SemanticDocument,
) -> Result<(), ServiceError> {
    let compatibility_regions = snapshot
        .layers
        .iter()
        .flat_map(|layer| &layer.objects)
        .filter(|object| matches!(
            &object.geometry,
            SemanticGeometry::Region { contours }
                if contours.iter().any(|contour| contour.role == editor_core::RegionRole::CompatibilitySolid)
        ))
        .count();
    let mut required_categories = metadata_categories(metadata);
    if compatibility_regions > 0 {
        required_categories.push("nonstandard_compatibility_region".into());
    }
    match params.overwrite.mode.as_str() {
        "deny" => {
            if target.exists() {
                let actual = sha256_file(target)?;
                return Err(ServiceError {
                    code: "CONFIRMATION_REQUIRED".into(),
                    message: "output target exists; explicit replacement authorization is required"
                        .into(),
                    details: serde_json::json!({"reason": "target_exists", "path": target.to_string_lossy(), "target_sha256": actual}),
                });
            }
        }
        "replace_if_unchanged" => {
            return Err(ServiceError {
                code: "UNSUPPORTED_FEATURE".into(),
                message: "S1-A export accepts new paths only".into(),
                details: serde_json::json!({"field": "params.overwrite.mode", "mode": "replace_if_unchanged"}),
            });
        }
        _ => {
            return Err(ServiceError::invalid_field(
                "params.overwrite.mode",
                "unsupported overwrite mode",
            ));
        }
    }
    match params.metadata_policy.mode.as_str() {
        "require_confirmation" => {
            if !required_categories.is_empty() {
                return Err(ServiceError {
                    code: "CONFIRMATION_REQUIRED".into(),
                    message: "export requires explicit source metadata and compatibility warning confirmation"
                        .into(),
                    details: serde_json::json!({
                        "reason": "metadata_loss",
                        "categories": required_categories,
                        "compatibility_warning": {
                            "layer_id": params.layer_id,
                            "issue_categories": snapshot.source.compatibility_issues,
                            "issue_category_codes": compatibility_issue_category_codes(&snapshot.source.compatibility_issues),
                            "contains_nonstandard_compatibility_region": compatibility_regions > 0,
                            "nonstandard_compatibility_region_count": compatibility_regions,
                            "contains_lossy_zero_aperture_conversion": snapshot.source.compatibility_issues.iter().any(|issue| issue.contains("零直径圆光圈")),
                        },
                    }),
                });
            }
        }
        "drop_listed" => {
            let listed = params
                .metadata_policy
                .categories
                .as_deref()
                .unwrap_or_default();
            if listed.is_empty() {
                return Err(ServiceError::invalid_field(
                    "params.metadata_policy.categories",
                    "drop_listed requires one or more categories",
                ));
            }
            let unknown = listed
                .iter()
                .filter(|category| {
                    !is_lossy_metadata_category(category)
                        || (metadata.get(category.as_str()).is_none()
                            && !(category.as_str() == "nonstandard_compatibility_region"
                                && compatibility_regions > 0))
                })
                .cloned()
                .collect::<Vec<_>>();
            let missing = required_categories
                .iter()
                .filter(|category| !listed.iter().any(|item| item == *category))
                .cloned()
                .collect::<Vec<_>>();
            if !unknown.is_empty() || !missing.is_empty() {
                return Err(ServiceError {
                    code: "CONFIRMATION_REQUIRED".into(),
                    message: "metadata removal requires authorization for every affected category"
                        .into(),
                    details: serde_json::json!({
                        "categories": params.metadata_policy.categories,
                        "unrecognized_categories": unknown,
                        "missing_categories": missing,
                    }),
                });
            }
        }
        _ => {
            return Err(ServiceError::invalid_field(
                "params.metadata_policy.mode",
                "unsupported metadata policy",
            ));
        }
    }
    Ok(())
}

fn compatibility_issue_category_codes(issues: &[String]) -> Vec<&'static str> {
    let mut codes = Vec::new();
    for issue in issues {
        let code = if issue.contains("零直径") {
            "manufacturing_size_changed"
        } else if issue.contains("非规范轮廓") {
            "nonstandard_region"
        } else if issue.contains("已忽略")
            || issue.contains("日期")
            || issue.contains("属性")
            || issue.contains("UTF-8")
            || issue.contains("DOS EOF")
        {
            "metadata_ignored"
        } else {
            "geometry_repaired"
        };
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    codes
}

fn metadata_categories(metadata: &Value) -> Vec<String> {
    let Some(object) = metadata.as_object() else {
        return Vec::new();
    };
    let mut categories: Vec<String> = [
        "image_name",
        "layer_name",
        "section_names",
        "compatibility_issues",
        "encoding",
        "file_attributes",
        "dropped_categories",
    ]
    .iter()
    .filter_map(|name| {
        let value = object.get(*name)?;
        let present = match value {
            Value::Null => false,
            Value::String(value) => !value.is_empty(),
            Value::Array(value) => !value.is_empty(),
            _ => true,
        };
        present.then(|| (*name).to_string())
    })
    .collect();
    categories.sort();
    categories
}

fn is_lossy_metadata_category(category: &str) -> bool {
    matches!(
        category,
        "image_name"
            | "layer_name"
            | "section_names"
            | "compatibility_issues"
            | "nonstandard_compatibility_region"
            | "encoding"
            | "file_attributes"
            | "dropped_categories"
    )
}

fn temporary_output_path(target: &Path) -> Result<PathBuf, ServiceError> {
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ServiceError::invalid_field("params.path", "output path must be UTF-8"))?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(target.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        counter
    )))
}

struct TemporaryOutput(PathBuf);

impl TemporaryOutput {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read_bounded_file(path: &Path, max: usize) -> Result<Vec<u8>, ServiceError> {
    let metadata = fs::metadata(path).map_err(|error| ServiceError::io("read", path, error))?;
    if !metadata.is_file() {
        return Err(ServiceError::permission(path, "read regular file"));
    }
    if metadata.len() > max as u64 {
        return Err(ServiceError::resource(
            "writer_bytes",
            max,
            metadata.len() as usize,
        ));
    }
    let file = fs::File::open(path).map_err(|error| ServiceError::io("read", path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ServiceError::io("read", path, error))?;
    if bytes.len() > max {
        return Err(ServiceError::resource("writer_bytes", max, bytes.len()));
    }
    Ok(bytes)
}

fn sha256_file(path: &Path) -> Result<String, ServiceError> {
    let bytes = read_bounded_file(path, gerber_io::S1_MAX_WRITER_BYTES)?;
    Ok(sha256_hex(&bytes))
}

use editor_core::hash::{Sha256, sha256_hex};

fn map_parse_error(error: S0Error) -> ServiceError {
    let code = match &error {
        S0Error::Unsupported { .. } => "UNSUPPORTED_FEATURE",
        S0Error::DuplicateModal { .. }
        | S0Error::ContentAfterEnd { .. }
        | S0Error::MissingModal(_)
        | S0Error::Parser(_)
        | S0Error::ParserCommand(_) => "VALIDATION_FAILED",
        S0Error::ResourceLimit { .. } => "RESOURCE_LIMIT",
        S0Error::InvalidGeometry(_) | S0Error::InvalidUtf8 | S0Error::Empty => "INVALID_ARGUMENT",
    };
    ServiceError {
        code: code.into(),
        message: error.to_string(),
        details: match error {
            S0Error::ResourceLimit {
                resource,
                limit,
                actual,
            } => serde_json::json!({"resource": resource, "limit": limit, "actual": actual}),
            S0Error::Unsupported { line, source } => {
                serde_json::json!({"line": line, "diagnostic": source})
            }
            S0Error::DuplicateModal { kind, line } => {
                serde_json::json!({"line": line, "kind": kind})
            }
            S0Error::ContentAfterEnd { line } => serde_json::json!({"line": line}),
            S0Error::MissingModal(kind) => serde_json::json!({"missing_modal": kind}),
            _ => serde_json::json!({"field": "params.source"}),
        },
    }
}

fn map_s1_error(error: S1Error) -> ServiceError {
    let code = match &error {
        S1Error::Cancelled => "CANCELLED",
        S1Error::Unsupported { .. } => "UNSUPPORTED_FEATURE",
        S1Error::ResourceLimit { .. } => "RESOURCE_LIMIT",
        S1Error::InvalidUtf8 | S1Error::Empty => "INVALID_ARGUMENT",
        S1Error::Syntax { .. } | S1Error::Semantic { .. } => "VALIDATION_FAILED",
        S1Error::TargetExists(_) => "CONFIRMATION_REQUIRED",
        S1Error::Io { .. } => "IO_ERROR",
    };
    let details = match &error {
        S1Error::ResourceLimit {
            resource,
            limit,
            actual,
        } => {
            serde_json::json!({"resource": resource, "limit": limit, "actual": actual})
        }
        S1Error::Unsupported { line, feature } => {
            serde_json::json!({"line": line, "feature": feature})
        }
        S1Error::Syntax { line, message } | S1Error::Semantic { line, message } => {
            serde_json::json!({"line": line, "diagnostic": message})
        }
        S1Error::TargetExists(path) | S1Error::Io { path, .. } => {
            serde_json::json!({"path": path.to_string_lossy()})
        }
        _ => serde_json::json!({"field": "params.path"}),
    };
    ServiceError {
        code: code.into(),
        message: error.to_string(),
        details,
    }
}

fn map_semantic_error<E: std::fmt::Display>(error: E) -> ServiceError {
    ServiceError {
        code: "VALIDATION_FAILED".into(),
        message: error.to_string(),
        details: serde_json::json!({"stage": "semantic_validation"}),
    }
}

fn response(
    request_id: Option<&str>,
    document_id: Option<&str>,
    revision: Option<&str>,
    outcome: Result<Value, ServiceError>,
) -> Value {
    let (status, result, error) = match outcome {
        Ok(value) => ("completed", Some(value), None),
        Err(error) if error.code == "CONFIRMATION_REQUIRED" => {
            ("confirmation_required", None, Some(error))
        }
        Err(error) => ("error", None, Some(error)),
    };
    serde_json::json!({
        "api_version": API_VERSION, "request_id": request_id, "status": status,
        "document_id": document_id, "revision": revision, "result": result,
        "warnings": [], "error": error, "job_id": null
    })
}

fn add_s0_risk_layers(scene: &mut S0Scene) {
    let mut line_ring = Layer::new("line-ring", "S0 line + local-hole ring");
    line_ring.objects.push(DrawObject {
        object_id: "line-ring-line".into(),
        geometry: Geometry::Line {
            start: MmPoint::new(7.0, 0.0),
            end: MmPoint::new(17.0, 0.0),
            width_mm: 0.5,
        },
        exposure: Exposure::Dark,
    });
    line_ring.objects.push(DrawObject {
        object_id: "line-ring-ring".into(),
        geometry: Geometry::CircleFlash {
            center: MmPoint::new(12.0, 0.0),
            aperture: CircleAperture::new(10.0, Some(4.0)).expect("fixed S0 geometry"),
        },
        exposure: Exposure::Dark,
    });
    let mut cross_dark = Layer::new("cross-dark", "S0 cross-layer dark");
    cross_dark.objects.push(DrawObject {
        object_id: "cross-dark-flash".into(),
        geometry: Geometry::CircleFlash {
            center: MmPoint::new(-12.0, 0.0),
            aperture: CircleAperture::new(5.0, None).expect("fixed S0 geometry"),
        },
        exposure: Exposure::Dark,
    });
    let mut cross_clear = Layer::new("cross-clear", "S0 cross-layer clear");
    cross_clear.objects.push(DrawObject {
        object_id: "cross-clear-flash".into(),
        geometry: Geometry::CircleFlash {
            center: MmPoint::new(-12.0, 0.0),
            aperture: CircleAperture::new(5.0, None).expect("fixed S0 geometry"),
        },
        exposure: Exposure::Clear,
    });
    // The clear layer is independent, so its operation cannot erase the dark
    // circle on the other layer.
    scene
        .document
        .layers
        .extend([line_ring, cross_dark, cross_clear]);
    scene
        .diagnostics
        .push("S0 also displays independent line/ring and cross-layer Clear fixtures".into());
}

fn parse_empty_params(value: &Value) -> Result<(), ServiceError> {
    parse_params::<EmptyParams>(value).map(|_| ())
}

fn parse_params<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, ServiceError> {
    serde_json::from_value(value.clone()).map_err(|error| ServiceError {
        code: "INVALID_ARGUMENT".into(),
        message: format!("invalid params: {error}"),
        details: serde_json::json!({"field": "params", "diagnostic": error.to_string()}),
    })
}

fn serialize_error(error: serde_json::Error) -> ServiceError {
    ServiceError::invalid(format!("response serialization failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"G04 service S0*
%FSLAX24Y24*%
%MOMM*%
%ADD10C,10*%
%ADD11C,6*%
%ADD12C,2*%
%LPD*%
D10*
X0Y0D03*
%LPC*%
D11*
X0Y0D03*
%LPD*%
D12*
X0Y0D03*
M02*
"#;

    #[test]
    fn service_returns_real_parser_snapshot_and_analysis() {
        let mut service = ApplicationService::new();
        let opened = service.open_s0("demo", SAMPLE).unwrap();
        assert_eq!(opened.revision, "0");
        let snapshot = service.snapshot("demo").unwrap();
        assert_eq!(snapshot.layers[0].objects.len(), 3);
        let analysis = service.analyze_s0("demo").unwrap();
        assert_eq!(
            analysis
                .samples
                .iter()
                .map(|sample| sample.covered)
                .collect::<Vec<_>>(),
            vec![true, false, true, false]
        );
    }

    #[test]
    fn capability_dto_is_json_round_trippable() {
        let service = ApplicationService::new();
        let capabilities = service.capabilities();
        let json = serde_json::to_string(&capabilities).unwrap();
        assert_eq!(
            serde_json::from_str::<Capabilities>(&json).unwrap(),
            capabilities
        );
    }

    #[test]
    fn json_boundary_rejects_unknown_versions_operations_and_fields() {
        let mut service = ApplicationService::new();
        let request = serde_json::json!({
            "api_version": 1,
            "request_id": "cap-1",
            "op": "system.capabilities",
            "params": {}
        });
        let response = service.execute_json(&request.to_string());
        assert_eq!(response["request_id"], "cap-1");
        assert_eq!(response["status"], "completed");

        let unknown = serde_json::json!({
            "api_version": 1,
            "request_id": "bad",
            "op": "system.capabilities",
            "params": {},
            "extra": true
        });
        assert_eq!(
            service.execute_json(&unknown.to_string())["error"]["code"],
            "INVALID_ARGUMENT"
        );

        let op = serde_json::json!({
            "api_version": 1,
            "request_id": "bad-op",
            "op": "objects.move",
            "params": {}
        });
        assert_eq!(
            service.execute_json(&op.to_string())["error"]["code"],
            "UNSUPPORTED_OPERATION"
        );
    }

    #[test]
    fn duplicate_document_id_is_rejected_without_revision_reset() {
        let mut service = ApplicationService::new();
        service.open_s0("demo", SAMPLE).unwrap();
        let error = service.open_s0("demo", SAMPLE).unwrap_err();
        assert_eq!(error.code, "ALREADY_EXISTS");
        assert_eq!(service.snapshot("demo").unwrap().revision, "0");
    }

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}

#[cfg(test)]
mod s1b_guards {
    use super::*;

    #[test]
    fn buffered_content_hash_matches_exact_serialized_model_bytes() {
        let (service, id, _, _) = service();
        let document = &service.documents[&id].document;
        assert_eq!(
            content_hash(document),
            sha256_hex(&serde_json::to_vec(document).unwrap())
        );
    }

    fn service() -> (ApplicationService, String, String, String) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic")
            .canonicalize()
            .unwrap();
        let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
            &root,
            [root.clone()],
            [std::env::temp_dir()],
        ));
        let opened = service.open("s1a1/g75_exact.gbr").unwrap();
        let id = opened.document_id;
        let layer = opened.layer_ids[0].clone();
        let object = service.documents[&id].document.layers[0].objects[0]
            .object_id
            .clone();
        (service, id, layer, object)
    }

    #[test]
    fn locked_layer_is_rejected_by_real_json_entry_without_history_changes() {
        let (mut service, id, layer, object) = service();
        service
            .layer_update(
                &id,
                "0",
                LayerUpdateParams {
                    layer_id: layer.clone(),
                    expected_workspace_revision: "0".into(),
                    display_name: None,
                    visible: None,
                    locked: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        let before = service.documents[&id].document.clone();
        let info = service.document_get(&id).unwrap();
        for op in ["objects.move", "objects.duplicate", "objects.delete"] {
            let params = if op == "objects.delete" {
                serde_json::json!({"layer_id":layer,"object_ids":[object]})
            } else {
                serde_json::json!({"layer_id":layer,"object_ids":[object],"dx_mm":5,"dy_mm":-3})
            };
            let result = service.execute_json(&serde_json::json!({"api_version":1,"request_id":"locked","document_id":id,"expected_revision":"0","op":op,"params":params}).to_string());
            assert_eq!(result["error"]["code"], "LAYER_LOCKED");
            assert_eq!(service.documents[&id].document, before);
            assert_eq!(service.document_get(&id).unwrap(), info);
        }
    }

    #[test]
    fn revision_overflow_and_large_revision_are_safe() {
        let (mut service, id, layer, object) = service();
        service.documents.get_mut(&id).unwrap().revision = 9_007_199_254_740_993;
        let params = MoveParams {
            layer_id: layer,
            object_ids: vec![object],
            dx_mm: 5.0,
            dy_mm: -3.0,
        };
        assert_eq!(
            service
                .objects_move(&id, "9007199254740993", params.clone())
                .unwrap()
                .revision,
            "9007199254740994"
        );
        service.documents.get_mut(&id).unwrap().revision = u64::MAX;
        let info = service.document_get(&id).unwrap();
        let geometry = service.documents[&id].document.clone();
        assert_eq!(
            service
                .objects_move(&id, &u64::MAX.to_string(), params)
                .unwrap_err()
                .code,
            "RESOURCE_LIMIT"
        );
        assert_eq!(
            service
                .history_undo(&id, &u64::MAX.to_string())
                .unwrap_err()
                .code,
            "RESOURCE_LIMIT"
        );
        assert_eq!(service.document_get(&id).unwrap(), info);
        assert_eq!(service.documents[&id].document, geometry);
    }
}

/// Builds a geometry-free diagnostic DTO from a consistent read-only service snapshot.
pub fn diagnostic_context(
    info: &DocumentInfo,
    layers: &[LayerInfo],
    snapshot: Option<&RenderSnapshot>,
    unit: editor_core::units::DisplayUnit,
) -> rcam_diagnostics::DiagnosticContext {
    use rcam_diagnostics::{
        DiagnosticContext, LayerDiagnostic, LayerSummary, ProjectSummary, hash_identity,
    };
    let compatibility_count = |layer: &LayerInfo| layer.compatibility_issue_count;
    DiagnosticContext {
        project: Some(ProjectSummary {
            document_id_hash: hash_identity(&info.document_id),
            project_id_hash: hash_identity(&info.project_id),
            manufacturing_revision: info.revision.parse().unwrap_or_default(),
            workspace_revision: info.workspace_revision.parse().unwrap_or_default(),
            project_dirty: info.project_dirty,
            layer_count: layers.len(),
            object_count: layers.iter().map(|l| l.object_count).sum(),
            block_definition_count: snapshot.map_or(0, |s| s.block_definitions.len()),
            block_instance_count: snapshot.map_or(0, |s| {
                s.layers
                    .iter()
                    .flat_map(|l| &l.objects)
                    .filter(|o| {
                        matches!(
                            o.geometry,
                            editor_core::SemanticGeometry::BlockInstance { .. }
                        )
                    })
                    .count()
            }),
            manufacturing_precision_nm: (info.manufacturing_precision.resolution_mm * 1_000_000.)
                .round() as u64,
            display_unit: unit,
            compatibility_layer_count: layers.iter().filter(|l| compatibility_count(l) > 0).count(),
        }),
        layers: LayerSummary {
            schema_version: 1,
            actual_count: layers.len(),
            truncated: layers.len() > 256,
            layers: layers
                .iter()
                .take(256)
                .map(|l| LayerDiagnostic {
                    layer_id_hash: hash_identity(&l.layer_id),
                    name_hash: hash_identity(&l.display_name),
                    kind: l.kind,
                    object_count: l.object_count,
                    visible: l.visible,
                    selectable: l.selectable,
                    locked: l.locked,
                    display_mode: l.display_mode,
                    compatibility_issue_count: compatibility_count(l),
                    source_content_hash_prefix: l
                        .provenance
                        .as_ref()
                        .filter(|p| {
                            p.imported_sha256.len() == 64
                                && p.imported_sha256.bytes().all(|c| c.is_ascii_hexdigit())
                        })
                        .map(|p| p.imported_sha256[..16].to_ascii_lowercase()),
                })
                .collect(),
        },
    }
}
