//! UI-free application boundary retained by the app and headless callers.
//!
//! S0 compatibility remains read-only; the host-authorized S1-A path adds
//! semantic queries, atomic Move/Undo/Redo, validation and safe new-path export.

use editor_core::edit::{
    EditError, EditHistory, MAX_HISTORY_BYTES, MAX_HISTORY_ENTRIES, MAX_MOVE_OBJECTS,
};
use editor_core::{
    ApertureShape, CircleAperture, DocumentSnapshot, DrawObject, Exposure, Geometry, Layer,
    MmPoint, SemanticDocument, SemanticGeometry, SemanticObject,
};
use gerber_io::{S0Error, S0Scene, S1Error, S1Scene, export_s1_new_path, parse_s0, parse_s1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};

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
    pub max_source_bytes: usize,
    pub max_objects: usize,
    pub max_query_results: usize,
    pub max_move_objects: usize,
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
        let max = gerber_io::S1_MAX_SOURCE_BYTES;
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
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub source_path: String,
    pub source_sha256: String,
    pub dirty: bool,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub layer_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerInfo {
    pub layer_id: String,
    pub name: String,
    pub object_count: usize,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub layer_id: String,
    pub object: SemanticObject,
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
    document: SemanticDocument,
    source_path: PathBuf,
    source_sha256: String,
    metadata: Value,
    diagnostics: Vec<String>,
    revision: u64,
    history: EditHistory,
    saved_content_hash: String,
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
pub struct EditResult {
    pub document_id: String,
    pub revision: String,
    pub changed_object_ids: Vec<String>,
    pub undo_entries_added: usize,
    pub undo_entries: usize,
    pub redo_entries: usize,
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
        }
    }

    pub fn with_file_access(file_access: FileAccessPolicy) -> Self {
        Self {
            file_access: Some(file_access),
            ..Self::new()
        }
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
                    "objects.duplicate".into(),
                    "objects.delete".into(),
                    "history.undo".into(),
                    "gerber.export_layer".into(),
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
                    max_source_bytes: gerber_io::MAX_SOURCE_BYTES,
                    max_objects: gerber_io::MAX_OBJECTS,
                    max_query_results: 0,
                    max_move_objects: 0,
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
            stage: "S1-B1 move/undo/redo/export/reopen".into(),
            read_only: false,
            supported_operations: vec![
                "system.capabilities".into(),
                "document.open_s0".into(),
                "document.snapshot".into(),
                "document.analyze_s0".into(),
                "document.open".into(),
                "document.get".into(),
                "document.close".into(),
                "objects.move".into(),
                "history.undo".into(),
                "history.redo".into(),
                "layers.list".into(),
                "objects.query".into(),
                "objects.get".into(),
                "document.validate".into(),
                "gerber.export_layer".into(),
            ],
            unsupported_operations: vec![
                "objects.duplicate".into(),
                "objects.delete".into(),
                "text.create".into(),
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
                "production export".into(),
            ],
            resource_limits: ResourceLimits {
                max_source_bytes: gerber_io::S1_MAX_SOURCE_BYTES,
                max_objects: gerber_io::S1_MAX_OBJECTS,
                max_query_results: 1000,
                max_move_objects: MAX_MOVE_OBJECTS,
                max_history_entries: MAX_HISTORY_ENTRIES,
                max_history_bytes: MAX_HISTORY_BYTES,
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
            .ok_or_else(|| ServiceError::not_found(&result.document_id))?;
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

    /// Open a real local Gerber through the host-authorized path boundary.
    pub fn open(&mut self, path: &str) -> Result<DocumentInfo, ServiceError> {
        let access = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::permission(Path::new(path), "read"))?;
        let (canonical, bytes) = access.read_path(path)?;
        let source_sha256 = sha256_hex(&bytes);
        let document_id = format!("doc-{}", self.next_document_id);
        self.next_document_id = self
            .next_document_id
            .checked_add(1)
            .ok_or_else(|| ServiceError::resource("document_ids", u64::MAX as usize, usize::MAX))?;
        let scene: S1Scene = parse_s1(&bytes, &document_id).map_err(map_s1_error)?;
        let metadata = serde_json::to_value(&scene.metadata).map_err(serialize_error)?;
        let record = S1DocumentRecord {
            saved_content_hash: content_hash(&scene.document),
            document: scene.document,
            source_path: canonical,
            source_sha256,
            metadata,
            diagnostics: scene.diagnostics,
            revision: 0,
            history: EditHistory::default(),
        };
        let info = document_info(&document_id, &record);
        self.documents.insert(document_id, record);
        Ok(info)
    }

    pub fn document_get(&self, document_id: &str) -> Result<DocumentInfo, ServiceError> {
        self.documents
            .get(document_id)
            .map(|record| document_info(document_id, record))
            .ok_or_else(|| ServiceError::not_found(document_id))
    }

    pub fn layers_list(&self, document_id: &str) -> Result<Vec<LayerInfo>, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found(document_id))?;
        Ok(record
            .document
            .layers
            .iter()
            .map(|layer| LayerInfo {
                layer_id: layer.id.clone(),
                name: layer.name.clone(),
                object_count: layer.objects.len(),
                locked: layer.locked,
            })
            .collect())
    }

    pub fn objects_get(
        &self,
        document_id: &str,
        params: ObjectParams,
    ) -> Result<ObjectInfo, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found(document_id))?;
        let layer = record
            .document
            .layers
            .iter()
            .find(|layer| layer.id == params.layer_id)
            .ok_or_else(|| ServiceError::not_found(&params.layer_id))?;
        let object = layer
            .objects
            .iter()
            .find(|object| object.object_id == params.object_id)
            .ok_or_else(|| ServiceError::not_found(&params.object_id))?;
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
            .ok_or_else(|| ServiceError::not_found(document_id))?;
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
            .ok_or_else(|| ServiceError::not_found(&params.layer_id))?;
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
        let record = self.edit_record(document_id, expected_revision)?;
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

    pub fn history_undo(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        let ids = record
            .history
            .undo(&mut record.document)
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 0))
    }

    pub fn history_redo(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<EditResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        let ids = record
            .history
            .redo(&mut record.document)
            .map_err(map_edit_error)?;
        record.revision += 1;
        Ok(edit_result(document_id, record, ids, 0))
    }

    fn edit_record(
        &mut self,
        document_id: &str,
        expected_revision: &str,
    ) -> Result<&mut S1DocumentRecord, ServiceError> {
        let record = self
            .documents
            .get_mut(document_id)
            .ok_or_else(|| ServiceError::not_found(document_id))?;
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
            .ok_or_else(|| ServiceError::not_found(document_id))?;
        check_revision(record.revision, expected_revision)?;
        if !discard_changes && content_hash(&record.document) != record.saved_content_hash {
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
            .ok_or_else(|| ServiceError::not_found(document_id))?;
        record.document.validate().map_err(map_semantic_error)?;
        Ok(ValidationResult {
            api_version: API_VERSION,
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            valid: true,
            diagnostics: record.diagnostics.clone(),
        })
    }

    pub fn export_layer(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ExportParams,
    ) -> Result<ExportResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found(document_id))?;
        check_revision(record.revision, expected_revision)?;
        let access = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::permission(Path::new(&params.path), "write"))?;
        let target = access.write_path(&params.path)?;
        if target == record.source_path {
            return Err(ServiceError::invalid_field(
                "params.path",
                "export target must be a new path",
            ));
        }
        validate_export_policy(&params, &target, &record.metadata)?;
        let mut document = record.document.clone();
        if !document
            .layers
            .iter()
            .any(|layer| layer.id == params.layer_id)
        {
            return Err(ServiceError::not_found(&params.layer_id));
        }
        document.layers.retain(|layer| layer.id == params.layer_id);
        document.validate().map_err(map_semantic_error)?;
        let temp = temporary_output_path(&target)?;
        if temp.exists() {
            return Err(ServiceError {
                code: "FILE_CONFLICT".into(),
                message: "temporary writer path already exists".into(),
                details: serde_json::json!({"path": temp.to_string_lossy()}),
            });
        }
        export_s1_new_path(&document, &temp).map_err(map_s1_error)?;
        let _temp_guard = TemporaryOutput::new(temp.clone());
        let bytes = read_bounded_file(&temp, gerber_io::S1_MAX_WRITER_BYTES)?;
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
        let saved_content_hash = content_hash(&record.document);
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
        self.documents
            .get_mut(document_id)
            .unwrap()
            .saved_content_hash = saved_content_hash;
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
            "objects.move" | "history.undo" | "history.redo" | "document.close"
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
                    "objects.move" => serde_json::to_value(self.objects_move(
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
            .ok_or_else(|| ServiceError::not_found(document_id))
    }

    pub fn analyze_s0(&self, document_id: &str) -> Result<AnalysisResult, ServiceError> {
        let scene = self
            .scenes
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found(document_id))?;
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

    fn not_found(document_id: &str) -> Self {
        Self {
            code: "NOT_FOUND".into(),
            message: format!("document {document_id:?} is not open"),
            details: serde_json::json!({"document_id": document_id}),
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
        api_version: API_VERSION,
        document_id: document_id.into(),
        revision: record.revision.to_string(),
        source_path: record.source_path.to_string_lossy().into_owned(),
        source_sha256: record.source_sha256.clone(),
        dirty: content_hash(&record.document) != record.saved_content_hash,
        undo_entries: record.history.undo_len(),
        redo_entries: record.history.redo_len(),
        layer_ids: record
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect(),
        diagnostics: record.diagnostics.clone(),
    }
}

fn edit_result(id: &str, record: &S1DocumentRecord, ids: Vec<String>, added: usize) -> EditResult {
    EditResult {
        document_id: id.into(),
        revision: record.revision.to_string(),
        changed_object_ids: ids,
        undo_entries_added: added,
        undo_entries: record.history.undo_len(),
        redo_entries: record.history.redo_len(),
        dirty: content_hash(&record.document) != record.saved_content_hash,
    }
}

fn map_edit_error(error: EditError) -> ServiceError {
    match error {
        EditError::NotFound(id) => ServiceError::not_found(&id),
        EditError::InvalidArgument => {
            ServiceError::invalid_field("params", "目标集合或移动距离无效，不能提交。")
        }
        EditError::LayerLocked(id) => ServiceError {
            code: "LAYER_LOCKED".into(),
            message: "图层已锁定。".into(),
            details: serde_json::json!({"layer_id": id}),
        },
        EditError::ResourceLimit => ServiceError {
            code: "RESOURCE_LIMIT".into(),
            message: "移动对象数量或撤销历史超过预算。".into(),
            details: serde_json::json!({"max_move_objects": MAX_MOVE_OBJECTS, "max_history_entries": MAX_HISTORY_ENTRIES, "max_history_bytes": MAX_HISTORY_BYTES}),
        },
        EditError::EmptyHistory => ServiceError::invalid_field("op", "没有可撤销或重做的事务。"),
        EditError::InvalidGeometry(error) => map_semantic_error(error),
    }
}

// Stream the stable owned model into the existing SHA-256 implementation;
// no document-sized serialization buffer or revision-derived dirty flag.
fn content_hash(document: &SemanticDocument) -> String {
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
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, document)
        .expect("validated finite model and infallible hash writer");
    writer.0.finish()
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
                .ok_or_else(|| ServiceError::not_found(aperture_id))?;
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
) -> Result<(), ServiceError> {
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
            let categories = metadata_categories(metadata);
            if !categories.is_empty() {
                return Err(ServiceError {
                    code: "CONFIRMATION_REQUIRED".into(),
                    message: "export drops source metadata; explicit categories are required"
                        .into(),
                    details: serde_json::json!({
                        "reason": "metadata_loss",
                        "categories": categories,
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
            let required = metadata_categories(metadata);
            let unknown = listed
                .iter()
                .filter(|category| {
                    !is_lossy_metadata_category(category)
                        || metadata.get(category.as_str()).is_none()
                })
                .cloned()
                .collect::<Vec<_>>();
            let missing = required
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

fn metadata_categories(metadata: &Value) -> Vec<String> {
    let Some(object) = metadata.as_object() else {
        return Vec::new();
    };
    let mut categories: Vec<String> = [
        "image_name",
        "layer_name",
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
        "image_name" | "layer_name" | "encoding" | "file_attributes" | "dropped_categories"
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finish()
}

struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    length: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: Vec::with_capacity(64),
            length: 0,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        self.length = self.length.saturating_add(bytes.len() as u64);
        let mut input = bytes;
        if !self.buffer.is_empty() {
            let needed = 64 - self.buffer.len();
            let take = needed.min(input.len());
            self.buffer.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.buffer.len() == 64 {
                let block = self.buffer.clone();
                self.buffer.clear();
                self.compress(&block);
            }
        }
        let full_len = input.len() / 64 * 64;
        for block in input[..full_len].chunks_exact(64) {
            self.compress(block);
        }
        self.buffer.extend_from_slice(&input[full_len..]);
    }

    fn finish(mut self) -> String {
        let bit_len = self.length * 8;
        self.buffer.push(0x80);
        while self.buffer.len() % 64 != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());
        while !self.buffer.is_empty() {
            let block = self.buffer[..64].to_vec();
            self.buffer.drain(..64);
            self.compress(&block);
        }
        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }

    fn compress(&mut self, block: &[u8]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for (index, word) in w[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes([
                block[index * 4],
                block[index * 4 + 1],
                block[index * 4 + 2],
                block[index * 4 + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

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
        service.documents.get_mut(&id).unwrap().document.layers[0].locked = true;
        let before = service.documents[&id].document.clone();
        let info = service.document_get(&id).unwrap();
        let result=service.execute_json(&serde_json::json!({"api_version":1,"request_id":"locked","document_id":id,"expected_revision":"0","op":"objects.move","params":{"layer_id":layer,"object_ids":[object],"dx_mm":5,"dy_mm":-3}}).to_string());
        assert_eq!(result["error"]["code"], "LAYER_LOCKED");
        assert_eq!(service.documents[&id].document, before);
        assert_eq!(service.document_get(&id).unwrap(), info);
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
