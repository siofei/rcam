//! Multi-Gerber Workspace operations (S4-B1).
//!
//! Layer structure changes (import, create, remove) are manufacturing
//! transactions in `EditHistory`; colours, order, visibility, active/solo layer
//! and category styles are session-only workspace state that never touches the
//! document, the revision counter or the Gerber writer.

use super::*;
use editor_core::edit::{LayerAdd, LayerEffect};
use editor_core::workspace::{
    ImportBaseline, aperture_shape_map, classify_object, default_class_styles, summarize_layer,
    utc_timestamp,
};
use editor_core::{ApertureDefinition, SemanticFormat, SemanticLayer, SourceMetadata};
use std::collections::HashSet;

/// Files accepted by one atomic `document.import_gerber_layers`.
pub const MAX_IMPORT_FILES: usize = 64;
/// Total source bytes of one import batch.
pub const MAX_IMPORT_BATCH_BYTES: usize = 128 * 1024 * 1024;

/// Import-time information of a layer. Provenance is display/audit data only;
/// it is never a link to, or a save target of, the original file.
#[derive(Debug, Clone)]
pub(crate) struct LayerSource {
    /// `src-N` for imported layers, `None` for layers created empty.
    pub(crate) source_id: Option<String>,
    pub(crate) provenance: Option<ImportProvenance>,
    /// IN/LN/legacy declarations of this source only (export policy input).
    pub(crate) metadata: SourceMetadata,
    pub(crate) diagnostics: Vec<String>,
    /// Imported-geometry fingerprints, for "modified object" counts.
    pub(crate) baseline: ImportBaseline,
    /// Aperture namespace this layer owns (`""` for the legacy first source).
    pub(crate) aperture_namespace: Option<String>,
}

impl LayerSource {
    fn empty() -> Self {
        Self {
            source_id: None,
            provenance: None,
            metadata: SourceMetadata::default(),
            diagnostics: Vec::new(),
            baseline: ImportBaseline::default(),
            aperture_namespace: None,
        }
    }
}

/// A layer that is currently outside the document but reachable by Undo/Redo.
#[derive(Debug, Clone)]
pub(crate) struct StashedLayer {
    state: LayerWorkspaceState,
    source: LayerSource,
    display_index: usize,
    was_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportGerberLayersParams {
    /// Files in file-picker order. All succeed or none is added.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportGerberLayerParams {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedLayerInfo {
    pub layer_id: String,
    pub display_name: String,
    pub source_id: String,
    pub original_file_name: String,
    pub imported_sha256: String,
    pub object_count: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportLayersResult {
    pub document_id: String,
    pub revision: String,
    pub workspace_revision: String,
    /// Same order as the request; the first one is the new active layer.
    pub layers: Vec<ImportedLayerInfo>,
    pub active_layer_id: Option<String>,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEmptyLayerParams {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerStructureResult {
    pub document_id: String,
    pub revision: String,
    pub workspace_revision: String,
    pub layer_id: String,
    pub display_name: String,
    pub active_layer_id: Option<String>,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveLayerParams {
    pub layer_id: String,
    /// Explicit destructive intent. Required for any layer with objects.
    #[serde(default)]
    pub allow_non_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveLayerResult {
    pub document_id: String,
    pub revision: String,
    pub workspace_revision: String,
    pub removed_layer_id: String,
    pub display_name: String,
    pub summary: LayerContentSummary,
    pub risk: DeleteRisk,
    pub active_layer_id: Option<String>,
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerSummaryParams {
    pub layer_id: String,
}

/// What Delete Layer needs to choose between direct delete and (strong) confirm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerSummaryResult {
    pub document_id: String,
    pub revision: String,
    pub layer_id: String,
    pub display_name: String,
    pub summary: LayerContentSummary,
    pub risk: DeleteRisk,
    pub is_active: bool,
    /// Provenance file name (information only).
    pub original_file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderLayersParams {
    pub expected_workspace_revision: String,
    /// Every layer id exactly once, top of the panel first.
    pub layer_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetActiveLayerParams {
    pub expected_workspace_revision: String,
    pub layer_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetSoloLayerParams {
    pub expected_workspace_revision: String,
    /// `None` leaves Solo and restores every layer's own visibility.
    pub layer_id: Option<String>,
}

/// One entry of `layers.update_many`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerPatch {
    pub layer_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub locked: Option<bool>,
    #[serde(default)]
    pub selectable: Option<bool>,
    #[serde(default)]
    pub base_color: Option<String>,
    #[serde(default)]
    pub color_mode: Option<ColorMode>,
    #[serde(default)]
    pub display_mode: Option<LayerDisplayMode>,
    #[serde(default)]
    pub classes: Vec<ClassStyleUpdate>,
    #[serde(default)]
    pub reset_classes: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateLayersParams {
    pub expected_workspace_revision: String,
    pub updates: Vec<LayerPatch>,
}

/// Restore the default palette in one workspace revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetLayerColorsParams {
    pub expected_workspace_revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleBoundsResult {
    pub document_id: String,
    pub revision: String,
    pub workspace_revision: String,
    /// Union of manufacturing bounds of every effectively visible object.
    pub bounds: Option<editor_core::BoundsMm>,
}

impl From<LayerUpdateParams> for LayerPatch {
    fn from(params: LayerUpdateParams) -> Self {
        Self {
            layer_id: params.layer_id,
            display_name: params.display_name,
            visible: params.visible,
            locked: params.locked,
            selectable: params.selectable,
            base_color: params.base_color,
            color_mode: params.color_mode,
            display_mode: params.display_mode,
            classes: params.classes,
            reset_classes: params.reset_classes,
        }
    }
}

pub(crate) fn empty_semantic_document(id: &str) -> SemanticDocument {
    SemanticDocument {
        id: id.into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 3,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: Vec::new(),
        apertures: Vec::new(),
        source: SourceMetadata::default(),
        block_definitions: Vec::new(),
    }
}

fn now_timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    utc_timestamp(seconds)
}

fn is_class_locked_error(layer_id: &str, class: DisplayClass, ids: &[String]) -> ServiceError {
    ServiceError {
        code: "OBJECT_CLASS_LOCKED".into(),
        message: format!("对象类别“{}”已锁定。", class.label()),
        details: serde_json::json!({
            "entity": "object_class",
            "layer_id": layer_id,
            "class": class,
            "object_ids": ids,
        }),
    }
}

/// Layer-level and category-level Lock validation for every manufacturing edit.
/// The GUI may grey buttons out, but this is the enforcement point.
pub(crate) fn check_workspace_edit(
    record: &S1DocumentRecord,
    layer_id: &str,
    object_ids: &[String],
) -> Result<(), ServiceError> {
    let state = record
        .workspace
        .get(layer_id)
        .ok_or_else(|| ServiceError::not_found("layer", layer_id))?;
    if state.locked {
        return Err(ServiceError {
            code: "LAYER_LOCKED".into(),
            message: "图层已锁定。".into(),
            details: serde_json::json!({"entity":"layer", "id":layer_id}),
        });
    }
    if object_ids.is_empty() || !state.style.classes.values().any(|class| class.locked) {
        return Ok(());
    }
    let Some(layer) = record.document.layers.iter().find(|l| l.id == layer_id) else {
        return Ok(());
    };
    let wanted: HashSet<&str> = object_ids.iter().map(String::as_str).collect();
    let shapes = aperture_shape_map(&record.document.apertures);
    let mut locked = Vec::new();
    let mut first_class = None;
    for object in &layer.objects {
        if wanted.contains(object.object_id.as_str()) {
            let class = classify_object(object, &shapes);
            if state.effective_locked(class) {
                first_class.get_or_insert(class);
                locked.push(object.object_id.clone());
            }
        }
    }
    match first_class {
        Some(class) => Err(is_class_locked_error(layer_id, class, &locked)),
        None => Ok(()),
    }
}

/// Lock check for objects that do not exist yet (generated text).
pub(crate) fn check_new_object_class(
    record: &S1DocumentRecord,
    layer_id: &str,
    class: DisplayClass,
) -> Result<(), ServiceError> {
    check_workspace_edit(record, layer_id, &[])?;
    let state = &record.workspace[layer_id];
    if state.effective_locked(class) {
        return Err(is_class_locked_error(layer_id, class, &[]));
    }
    Ok(())
}

pub(crate) fn class_infos(state: &LayerWorkspaceState) -> Vec<ClassStyleInfo> {
    DisplayClass::ALL
        .into_iter()
        .map(|class| {
            let style = state.class_style(class);
            ClassStyleInfo {
                class,
                visible: style.visible,
                selectable: style.selectable,
                locked: style.locked,
                color_override: style.color_override,
                effective_color: state.effective_color(class),
            }
        })
        .collect()
}

fn effective_layer_visible(
    record: &S1DocumentRecord,
    layer_id: &str,
    state: &LayerWorkspaceState,
) -> bool {
    state.visible
        && record
            .solo_layer_id
            .as_deref()
            .is_none_or(|solo| solo == layer_id)
}

fn parse_color(field: &str, text: &str) -> Result<Color, ServiceError> {
    Color::from_hex(text)
        .ok_or_else(|| ServiceError::invalid_field(field, "颜色必须是 #rrggbb 十六进制格式。"))
}

fn apply_patch(
    record: &S1DocumentRecord,
    state: &LayerWorkspaceState,
    patch: &LayerPatch,
) -> Result<LayerWorkspaceState, ServiceError> {
    let _ = record;
    let mut next = state.clone();
    if let Some(name) = &patch.display_name {
        if name.trim().is_empty() || name.len() > 1024 {
            return Err(ServiceError::invalid_field(
                "params.display_name",
                "name must be nonempty and at most 1024 UTF-8 bytes",
            ));
        }
        next.display_name = name.clone();
    }
    if let Some(value) = patch.visible {
        next.visible = value;
    }
    if let Some(value) = patch.locked {
        next.locked = value;
    }
    if let Some(value) = patch.selectable {
        next.selectable = value;
    }
    if let Some(color) = &patch.base_color {
        next.style.base_color = parse_color("params.base_color", color)?;
    }
    if let Some(mode) = patch.color_mode {
        next.style.color_mode = mode;
    }
    if let Some(mode) = patch.display_mode {
        next.style.display_mode = mode;
    }
    if patch.reset_classes {
        next.style.classes = default_class_styles();
    }
    for update in &patch.classes {
        let targets: Vec<DisplayClass> = match update.class {
            Some(class) => vec![class],
            None => DisplayClass::ALL.to_vec(),
        };
        let override_color = match update.color_override.as_deref() {
            None => None,
            Some("inherit") => Some(None),
            Some(text) => Some(Some(parse_color("params.classes.color_override", text)?)),
        };
        for class in targets {
            let style = next.style.classes.entry(class).or_default();
            if let Some(value) = update.visible {
                style.visible = value;
            }
            if let Some(value) = update.selectable {
                style.selectable = value;
            }
            if let Some(value) = update.locked {
                style.locked = value;
            }
            if let Some(color) = override_color {
                style.color_override = color;
            }
        }
    }
    Ok(next)
}

fn unique_display_name(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    (2_usize..)
        .map(|n| format!("{base} ({n})"))
        .find(|candidate| !taken.contains(candidate))
        .expect("an unused suffix exists")
}

/// Deterministic neighbour of a removed active layer: the one below it in the
/// panel, else the one above it, else none.
fn neighbour_after_removal(
    order: &[String],
    active: &str,
    removed: &HashSet<&str>,
) -> Option<String> {
    let index = order.iter().position(|id| id == active)?;
    order[index + 1..]
        .iter()
        .find(|id| !removed.contains(id.as_str()))
        .or_else(|| {
            order[..index]
                .iter()
                .rev()
                .find(|id| !removed.contains(id.as_str()))
        })
        .cloned()
}

fn bump(counter: &mut u64, resource: &str) -> Result<(), ServiceError> {
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| ServiceError::resource(resource, usize::MAX, usize::MAX))?;
    Ok(())
}

/// Bring the workspace side table in step with a layer transaction.
///
/// `prefer_previous_active` is set only when undoing an add: the active layer
/// from before the add is restored instead of picking a neighbour.
pub(crate) fn sync_layer_effect(
    record: &mut S1DocumentRecord,
    effect: LayerEffect,
    prefer_previous_active: bool,
) {
    match effect {
        LayerEffect::Removed(ids) => {
            let order = record.display_order.clone();
            let active = record.active_layer_id.clone();
            let removed: HashSet<&str> = ids.iter().map(String::as_str).collect();
            for id in &ids {
                let display_index = order.iter().position(|x| x == id).unwrap_or(order.len());
                if let (Some(state), Some(source)) =
                    (record.workspace.remove(id), record.sources.remove(id))
                {
                    record.stash.insert(
                        id.clone(),
                        StashedLayer {
                            state,
                            source,
                            display_index,
                            was_active: active.as_deref() == Some(id.as_str()),
                        },
                    );
                }
            }
            record
                .display_order
                .retain(|id| !removed.contains(id.as_str()));
            if record
                .solo_layer_id
                .as_deref()
                .is_some_and(|solo| removed.contains(solo))
            {
                record.solo_layer_id = None;
            }
            if let Some(active) = active
                && removed.contains(active.as_str())
            {
                let previous = prefer_previous_active
                    .then(|| {
                        ids.iter()
                            .find_map(|id| record.active_before_add.get(id).cloned().flatten())
                    })
                    .flatten()
                    .filter(|id| record.workspace.contains_key(id));
                record.active_layer_id =
                    previous.or_else(|| neighbour_after_removal(&order, &active, &removed));
            }
        }
        LayerEffect::Added(ids) => {
            let mut restored: Vec<(usize, String, StashedLayer)> = ids
                .iter()
                .filter_map(|id| {
                    record
                        .stash
                        .remove(id)
                        .map(|stashed| (stashed.display_index, id.clone(), stashed))
                })
                .collect();
            restored.sort_by_key(|(index, id, _)| (*index, id.clone()));
            for (index, id, stashed) in restored {
                let at = index.min(record.display_order.len());
                record.display_order.insert(at, id.clone());
                if stashed.was_active {
                    record.active_layer_id = Some(id.clone());
                }
                record.workspace.insert(id.clone(), stashed.state);
                record.sources.insert(id, stashed.source);
            }
        }
    }
    prune_side_tables(record);
}

pub(crate) fn prune_side_tables(record: &mut S1DocumentRecord) {
    let live = record.history.layer_transaction_ids();
    record.stash.retain(|id, _| live.contains(id));
    record.active_before_add.retain(|id, _| live.contains(id));
}

pub(crate) fn next_workspace_revision(record: &S1DocumentRecord) -> Result<u64, ServiceError> {
    record
        .workspace_revision
        .checked_add(1)
        .ok_or_else(|| ServiceError::resource("workspace_revision", usize::MAX, usize::MAX))
}

struct PreparedSource {
    bytes: usize,
    file_name: String,
    stem: String,
    sha256: String,
    canonical: PathBuf,
    scene: S1Scene,
}

fn with_import_context(mut error: ServiceError, index: usize, path: &str) -> ServiceError {
    if let Some(details) = error.details.as_object_mut() {
        details.insert("import_index".into(), serde_json::json!(index));
        details.insert("import_path".into(), serde_json::json!(path));
        details.insert("import_atomic".into(), serde_json::json!(true));
    }
    error
}

/// Give a parsed source private identities: layer id, `src-N::` object and
/// aperture ids. Nothing of the source's own numbering leaks into another
/// source, so `D10` of file A and file B stay two different apertures.
fn remap_source(
    source_id: &str,
    layer_numbers: &mut u64,
    document: SemanticDocument,
) -> Result<Vec<LayerAdd>, ServiceError> {
    let mut apertures: Vec<ApertureDefinition> = document.apertures;
    for aperture in &mut apertures {
        aperture.id = format!("{source_id}::{}", aperture.id);
    }
    let mut adds = Vec::with_capacity(document.layers.len().max(1));
    for mut layer in document.layers {
        layer.id = format!("layer-{}", *layer_numbers);
        bump(layer_numbers, "layer_ids")?;
        for object in &mut layer.objects {
            object.object_id = format!("{source_id}::{}", object.object_id);
            if let SemanticGeometry::Flash { aperture_id, .. } = &mut object.geometry {
                *aperture_id = format!("{source_id}::{aperture_id}");
            }
        }
        adds.push(LayerAdd {
            layer,
            apertures: Vec::new(),
        });
    }
    if let Some(first) = adds.first_mut() {
        first.apertures = apertures;
    }
    Ok(adds)
}

impl ApplicationService {
    fn allocate_document_id(&mut self) -> Result<String, ServiceError> {
        let id = format!("doc-{}", self.next_document_id);
        self.next_document_id = self
            .next_document_id
            .checked_add(1)
            .ok_or_else(|| ServiceError::resource("document_ids", u64::MAX as usize, usize::MAX))?;
        Ok(id)
    }

    fn new_record(&self, document: SemanticDocument) -> Result<S1DocumentRecord, ServiceError> {
        Ok(S1DocumentRecord {
            manufacturing_precision: ManufacturingPrecision::default(),
            saved_precision: ManufacturingPrecision::default(),
            metrics: metrics::MetricsCache::default(),
            workspace_revision: 0,
            workspace: HashMap::new(),
            sources: HashMap::new(),
            display_order: Vec::new(),
            active_layer_id: None,
            solo_layer_id: None,
            stash: HashMap::new(),
            active_before_add: HashMap::new(),
            next_layer_number: 1,
            next_source_number: 1,
            next_color_index: 0,
            saved_content_hash: content_hash(&document),
            document,
            opened_from: None,
            diagnostics: Vec::new(),
            revision: 0,
            content_hash_cache: HashCache::default(),
            history: EditHistory::with_limits(self.history_max_entries, self.history_max_bytes)
                .map_err(map_edit_error)?,
        })
    }

    /// Create an empty Workspace: no layer, no source, nothing dirty.
    pub fn document_new(&mut self) -> Result<DocumentInfo, ServiceError> {
        let document_id = self.allocate_document_id()?;
        let record = self.new_record(empty_semantic_document(&document_id))?;
        let info = document_info(&document_id, &record);
        self.documents.insert(document_id, record);
        Ok(info)
    }

    fn read_and_parse(
        &self,
        path: &str,
        document_id: &str,
    ) -> Result<PreparedSource, ServiceError> {
        let access = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::permission(Path::new(path), "read"))?;
        let (canonical, bytes) = access.read_path(path)?;
        let sha256 = sha256_hex(&bytes);
        let scene = parse_s1(&bytes, document_id).map_err(map_s1_error)?;
        let file_name = canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let stem = canonical
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(PreparedSource {
            bytes: bytes.len(),
            file_name,
            stem,
            sha256,
            canonical,
            scene,
        })
    }

    /// Open a Gerber as the first source layer of a NEW Workspace. This is an
    /// Import into a fresh Workspace, not a link to the file: the source keeps
    /// the legacy identities of the first file and there is no Undo entry.
    pub fn open(&mut self, path: &str) -> Result<DocumentInfo, ServiceError> {
        let document_id = self.allocate_document_id()?;
        let prepared = self.read_and_parse(path, &document_id)?;
        let PreparedSource {
            bytes: _,
            file_name,
            stem,
            sha256,
            canonical,
            scene,
        } = prepared;
        let mut record = self.new_record(scene.document)?;
        record.saved_content_hash = content_hash(&record.document);
        let source_id = "src-1".to_string();
        record.next_source_number = 2;
        let provenance = ImportProvenance {
            import_id: "import-1".into(),
            original_file_name: file_name,
            imported_sha256: sha256.clone(),
            imported_at: now_timestamp(),
        };
        let mut max_layer_number = 0_u64;
        let layer_ids: Vec<String> = record
            .document
            .layers
            .iter()
            .map(|l| l.id.clone())
            .collect();
        for (offset, layer) in record.document.layers.iter().enumerate() {
            if let Some(number) = layer
                .id
                .strip_prefix("layer-")
                .and_then(|n| n.parse::<u64>().ok())
            {
                max_layer_number = max_layer_number.max(number);
            }
            let state = LayerWorkspaceState::new(
                LayerKind::Gerber,
                if offset == 0 {
                    stem.clone()
                } else {
                    format!("{stem} ({})", offset + 1)
                },
                auto_layer_color(record.next_color_index),
            );
            record.next_color_index += 1;
            record.workspace.insert(layer.id.clone(), state);
            record.sources.insert(
                layer.id.clone(),
                LayerSource {
                    source_id: Some(source_id.clone()),
                    provenance: Some(provenance.clone()),
                    metadata: scene.metadata.clone(),
                    diagnostics: scene.diagnostics.clone(),
                    baseline: ImportBaseline::capture(layer),
                    aperture_namespace: Some(String::new()),
                },
            );
        }
        record.next_layer_number = max_layer_number.max(layer_ids.len() as u64) + 1;
        record.display_order = layer_ids.clone();
        record.active_layer_id = layer_ids.first().cloned();
        record.diagnostics = scene.diagnostics;
        record.opened_from = Some((canonical, sha256));
        let info = document_info(&document_id, &record);
        self.documents.insert(document_id, record);
        Ok(info)
    }

    /// Add one Gerber as a new layer (batch of one).
    pub fn import_gerber_layer(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ImportGerberLayerParams,
    ) -> Result<ImportLayersResult, ServiceError> {
        self.import_gerber_layers(
            document_id,
            expected_revision,
            ImportGerberLayersParams {
                paths: vec![params.path],
            },
        )
    }

    /// Add several Gerbers as ONE atomic transaction: any read, parse or
    /// validation failure adds nothing; one Undo removes the whole batch.
    pub fn import_gerber_layers(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ImportGerberLayersParams,
    ) -> Result<ImportLayersResult, ServiceError> {
        {
            let record = self.edit_record(document_id, expected_revision)?;
            let _ = record;
        }
        if params.paths.is_empty() {
            return Err(ServiceError::invalid_field(
                "params.paths",
                "at least one Gerber path is required",
            ));
        }
        if params.paths.len() > MAX_IMPORT_FILES {
            return Err(ServiceError::resource(
                "import_files",
                MAX_IMPORT_FILES,
                params.paths.len(),
            ));
        }
        // Phase 1: bounded read + parse of every file; no document access.
        let mut prepared = Vec::with_capacity(params.paths.len());
        let mut total_bytes = 0_usize;
        for (index, path) in params.paths.iter().enumerate() {
            let source = self
                .read_and_parse(path, document_id)
                .map_err(|error| with_import_context(error, index, path))?;
            total_bytes = total_bytes.saturating_add(source.bytes);
            if total_bytes > MAX_IMPORT_BATCH_BYTES {
                return Err(with_import_context(
                    ServiceError::resource(
                        "import_batch_bytes",
                        MAX_IMPORT_BATCH_BYTES,
                        total_bytes,
                    ),
                    index,
                    path,
                ));
            }
            prepared.push(source);
        }
        let timestamp = now_timestamp();
        let record = self
            .documents
            .get_mut(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        // Phase 2: identities. Local copies; committed only after the transaction.
        let mut next_layer = record.next_layer_number;
        let mut next_source = record.next_source_number;
        let mut next_color = record.next_color_index;
        let mut taken: HashSet<String> = record
            .workspace
            .values()
            .map(|state| state.display_name.clone())
            .collect();
        struct Pending {
            layer_id: String,
            state: LayerWorkspaceState,
            source: LayerSource,
            info: ImportedLayerInfo,
        }
        let mut pending: Vec<Pending> = Vec::new();
        let mut adds: Vec<LayerAdd> = Vec::new();
        for source in prepared {
            let source_id = format!("src-{next_source}");
            let import_id = format!("import-{next_source}");
            bump(&mut next_source, "source_ids")?;
            let provenance = ImportProvenance {
                import_id,
                original_file_name: source.file_name.clone(),
                imported_sha256: source.sha256.clone(),
                imported_at: timestamp.clone(),
            };
            let metadata = source.scene.metadata.clone();
            let diagnostics = source.scene.diagnostics.clone();
            let layer_adds = remap_source(&source_id, &mut next_layer, source.scene.document)?;
            for (offset, add) in layer_adds.iter().enumerate() {
                let base = if offset == 0 {
                    source.stem.clone()
                } else {
                    format!("{} ({})", source.stem, offset + 1)
                };
                let display_name = unique_display_name(&base, &taken);
                taken.insert(display_name.clone());
                let color = auto_layer_color(next_color);
                next_color += 1;
                pending.push(Pending {
                    layer_id: add.layer.id.clone(),
                    state: LayerWorkspaceState::new(LayerKind::Gerber, display_name.clone(), color),
                    source: LayerSource {
                        source_id: Some(source_id.clone()),
                        provenance: Some(provenance.clone()),
                        metadata: metadata.clone(),
                        diagnostics: diagnostics.clone(),
                        baseline: ImportBaseline::capture(&add.layer),
                        aperture_namespace: Some(source_id.clone()),
                    },
                    info: ImportedLayerInfo {
                        layer_id: add.layer.id.clone(),
                        display_name,
                        source_id: source_id.clone(),
                        original_file_name: source.file_name.clone(),
                        imported_sha256: source.sha256.clone(),
                        object_count: add.layer.objects.len(),
                        diagnostics: diagnostics.clone(),
                    },
                });
            }
            adds.extend(layer_adds);
        }
        let workspace_revision = next_workspace_revision(record)?;
        // Phase 3: one transaction (fails as a whole, leaving nothing behind).
        let ids = record
            .history
            .add_layers(&mut record.document, adds)
            .map_err(map_edit_error)?;
        debug_assert_eq!(ids.len(), pending.len());
        let previous_active = record.active_layer_id.clone();
        let mut batch_ids = Vec::with_capacity(pending.len());
        let mut infos = Vec::with_capacity(pending.len());
        for entry in pending {
            batch_ids.push(entry.layer_id.clone());
            record
                .active_before_add
                .insert(entry.layer_id.clone(), previous_active.clone());
            record.workspace.insert(entry.layer_id.clone(), entry.state);
            record.sources.insert(entry.layer_id, entry.source);
            infos.push(entry.info);
        }
        record.display_order.splice(0..0, batch_ids.iter().cloned());
        record.active_layer_id = batch_ids.first().cloned();
        record.next_layer_number = next_layer;
        record.next_source_number = next_source;
        record.next_color_index = next_color;
        record.revision += 1;
        record.workspace_revision = workspace_revision;
        prune_side_tables(record);
        Ok(ImportLayersResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            workspace_revision: record.workspace_revision.to_string(),
            layers: infos,
            active_layer_id: record.active_layer_id.clone(),
            undo_entries: record.history.undo_len(),
            redo_entries: record.history.redo_len(),
            dirty: record.is_dirty(),
        })
    }

    /// New empty layer on top of the panel; it becomes the active layer.
    pub fn create_empty_layer(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: CreateEmptyLayerParams,
    ) -> Result<LayerStructureResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        if let Some(name) = &params.display_name
            && (name.trim().is_empty() || name.len() > 1024)
        {
            return Err(ServiceError::invalid_field(
                "params.display_name",
                "name must be nonempty and at most 1024 UTF-8 bytes",
            ));
        }
        let workspace_revision = next_workspace_revision(record)?;
        let number = record.next_layer_number;
        let mut next_number = number;
        bump(&mut next_number, "layer_ids")?;
        let layer_id = format!("layer-{number}");
        let taken: HashSet<String> = record
            .workspace
            .values()
            .map(|state| state.display_name.clone())
            .collect();
        let display_name = match params.display_name {
            Some(name) => name,
            None => unique_display_name(&format!("新图层 {number}"), &taken),
        };
        record
            .history
            .add_layers(
                &mut record.document,
                vec![LayerAdd {
                    layer: SemanticLayer {
                        id: layer_id.clone(),
                        objects: Vec::new(),
                    },
                    apertures: Vec::new(),
                }],
            )
            .map_err(map_edit_error)?;
        let color = auto_layer_color(record.next_color_index);
        record.next_color_index += 1;
        record.next_layer_number = next_number;
        record
            .active_before_add
            .insert(layer_id.clone(), record.active_layer_id.clone());
        record.workspace.insert(
            layer_id.clone(),
            LayerWorkspaceState::new(LayerKind::Gerber, display_name.clone(), color),
        );
        record
            .sources
            .insert(layer_id.clone(), LayerSource::empty());
        record.display_order.insert(0, layer_id.clone());
        record.active_layer_id = Some(layer_id.clone());
        record.revision += 1;
        record.workspace_revision = workspace_revision;
        prune_side_tables(record);
        Ok(LayerStructureResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            workspace_revision: record.workspace_revision.to_string(),
            layer_id,
            display_name,
            active_layer_id: record.active_layer_id.clone(),
            undo_entries: record.history.undo_len(),
            redo_entries: record.history.redo_len(),
            dirty: record.is_dirty(),
        })
    }

    fn layer_summary_of(
        record: &S1DocumentRecord,
        layer_id: &str,
    ) -> Result<LayerContentSummary, ServiceError> {
        let layer = record
            .document
            .layers
            .iter()
            .find(|l| l.id == layer_id)
            .ok_or_else(|| ServiceError::not_found("layer", layer_id))?;
        let source = record.sources.get(layer_id);
        Ok(summarize_layer(
            layer,
            source.map(|s| &s.baseline),
            source.is_some_and(|s| s.provenance.is_some()),
        ))
    }

    /// Formal content summary + delete risk for the Delete Layer dialog.
    pub fn layer_summary(
        &self,
        document_id: &str,
        params: LayerSummaryParams,
    ) -> Result<LayerSummaryResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let summary = Self::layer_summary_of(record, &params.layer_id)?;
        Ok(LayerSummaryResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            display_name: record.workspace[&params.layer_id].display_name.clone(),
            is_active: record.active_layer_id.as_deref() == Some(params.layer_id.as_str()),
            original_file_name: record
                .sources
                .get(&params.layer_id)
                .and_then(|s| s.provenance.as_ref())
                .map(|p| p.original_file_name.clone()),
            risk: summary.delete_risk(),
            summary,
            layer_id: params.layer_id,
        })
    }

    /// Remove one layer as ONE Undo transaction. A layer with objects needs the
    /// explicit `allow_non_empty` flag (the GUI sets it after its confirmation).
    pub fn remove_layer(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: RemoveLayerParams,
    ) -> Result<RemoveLayerResult, ServiceError> {
        let record = self.edit_record(document_id, expected_revision)?;
        let summary = Self::layer_summary_of(record, &params.layer_id)?;
        let risk = summary.delete_risk();
        if !summary.is_empty() && !params.allow_non_empty {
            return Err(ServiceError {
                code: "CONFIRMATION_REQUIRED".into(),
                message: "图层含有制造对象；删除需要明确确认（allow_non_empty）。".into(),
                details: serde_json::json!({
                    "field": "params.allow_non_empty",
                    "layer_id": params.layer_id,
                    "summary": summary,
                    "risk": risk,
                }),
            });
        }
        let workspace_revision = next_workspace_revision(record)?;
        let display_name = record.workspace[&params.layer_id].display_name.clone();
        let namespace = record
            .sources
            .get(&params.layer_id)
            .and_then(|s| s.aperture_namespace.clone());
        record
            .history
            .remove_layer(&mut record.document, &params.layer_id, namespace.as_deref())
            .map_err(map_edit_error)?;
        sync_layer_effect(
            record,
            LayerEffect::Removed(vec![params.layer_id.clone()]),
            false,
        );
        record.metrics = metrics::MetricsCache::default();
        record.revision += 1;
        record.workspace_revision = workspace_revision;
        Ok(RemoveLayerResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            workspace_revision: record.workspace_revision.to_string(),
            removed_layer_id: params.layer_id,
            display_name,
            summary,
            risk,
            active_layer_id: record.active_layer_id.clone(),
            undo_entries: record.history.undo_len(),
            redo_entries: record.history.redo_len(),
            dirty: record.is_dirty(),
        })
    }

    fn workspace_record(
        &mut self,
        document_id: &str,
        expected_workspace_revision: &str,
    ) -> Result<&mut S1DocumentRecord, ServiceError> {
        let record = self
            .documents
            .get_mut(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.workspace_revision, expected_workspace_revision).map_err(
            |mut error| {
                error.details["field"] = serde_json::json!("params.expected_workspace_revision");
                error
            },
        )?;
        Ok(record)
    }

    /// Apply workspace-only patches to several layers atomically.
    pub fn layers_update_many(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: UpdateLayersParams,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self.workspace_record(document_id, &params.expected_workspace_revision)?;
        check_revision(record.revision, expected_revision)?;
        let mut staged: Vec<(String, LayerWorkspaceState)> = Vec::new();
        for patch in &params.updates {
            if patch.layer_id.trim().is_empty() {
                return Err(ServiceError::invalid_field(
                    "params.layer_id",
                    "layer_id is required",
                ));
            }
            let current = staged
                .iter()
                .rev()
                .find(|(id, _)| id == &patch.layer_id)
                .map(|(_, state)| state.clone())
                .or_else(|| record.workspace.get(&patch.layer_id).cloned())
                .ok_or_else(|| ServiceError::not_found("layer", &patch.layer_id))?;
            let next = apply_patch(record, &current, patch)?;
            staged.push((patch.layer_id.clone(), next));
        }
        let changed = staged
            .iter()
            .any(|(id, state)| record.workspace.get(id) != Some(state));
        if changed {
            let revision = next_workspace_revision(record)?;
            for (id, state) in staged {
                record.workspace.insert(id, state);
            }
            record.workspace_revision = revision;
        }
        Ok(document_info(document_id, record))
    }

    /// Give every layer its deterministic default colour again (panel order).
    pub fn layers_reset_colors(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ResetLayerColorsParams,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self.workspace_record(document_id, &params.expected_workspace_revision)?;
        check_revision(record.revision, expected_revision)?;
        let mut changed = false;
        let bottom_up: Vec<String> = record.display_order.iter().rev().cloned().collect();
        for (index, id) in bottom_up.iter().enumerate() {
            if let Some(state) = record.workspace.get_mut(id) {
                let color = auto_layer_color(index);
                if state.style.base_color != color {
                    state.style.base_color = color;
                    changed = true;
                }
            }
        }
        if changed {
            record.workspace_revision = next_workspace_revision(record)?;
        }
        Ok(document_info(document_id, record))
    }

    /// Replace the panel order (top first). Workspace-only: no manufacturing
    /// revision, no history entry, no effect on any object's exposure order.
    pub fn layers_reorder(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ReorderLayersParams,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self.workspace_record(document_id, &params.expected_workspace_revision)?;
        check_revision(record.revision, expected_revision)?;
        let requested: HashSet<&str> = params.layer_ids.iter().map(String::as_str).collect();
        if params.layer_ids.len() != record.display_order.len()
            || requested.len() != params.layer_ids.len()
            || record
                .display_order
                .iter()
                .any(|id| !requested.contains(id.as_str()))
        {
            return Err(ServiceError::invalid_field(
                "params.layer_ids",
                "layer_ids must list every layer exactly once",
            ));
        }
        if record.display_order != params.layer_ids {
            let revision = next_workspace_revision(record)?;
            record.display_order = params.layer_ids;
            record.workspace_revision = revision;
        }
        Ok(document_info(document_id, record))
    }

    pub fn layers_set_active(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: SetActiveLayerParams,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self.workspace_record(document_id, &params.expected_workspace_revision)?;
        check_revision(record.revision, expected_revision)?;
        if let Some(id) = &params.layer_id
            && !record.workspace.contains_key(id)
        {
            return Err(ServiceError::not_found("layer", id));
        }
        if record.active_layer_id != params.layer_id {
            let revision = next_workspace_revision(record)?;
            record.active_layer_id = params.layer_id;
            record.workspace_revision = revision;
        }
        Ok(document_info(document_id, record))
    }

    /// Solo overrides visibility temporarily; each layer keeps its own `visible`.
    pub fn layers_set_solo(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: SetSoloLayerParams,
    ) -> Result<DocumentInfo, ServiceError> {
        let record = self.workspace_record(document_id, &params.expected_workspace_revision)?;
        check_revision(record.revision, expected_revision)?;
        if let Some(id) = &params.layer_id
            && !record.workspace.contains_key(id)
        {
            return Err(ServiceError::not_found("layer", id));
        }
        if record.solo_layer_id != params.layer_id {
            let revision = next_workspace_revision(record)?;
            record.solo_layer_id = params.layer_id;
            record.workspace_revision = revision;
        }
        Ok(document_info(document_id, record))
    }

    /// Bounds of everything currently visible (layer, Solo and category filters).
    pub fn visible_bounds(&self, document_id: &str) -> Result<VisibleBoundsResult, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        let shapes = aperture_shape_map(&record.document.apertures);
        let mut geometries = Vec::new();
        for layer in &record.document.layers {
            let Some(state) = record.workspace.get(&layer.id) else {
                continue;
            };
            if !effective_layer_visible(record, &layer.id, state) {
                continue;
            }
            for object in &layer.objects {
                if state.class_style(classify_object(object, &shapes)).visible {
                    geometries.push(&object.geometry);
                }
            }
        }
        let bounds = editor_core::geometries_bounds_with_blocks(
            geometries,
            &record.document.apertures,
            &record.document.block_definitions,
        )
        .map_err(map_semantic_error)?;
        Ok(VisibleBoundsResult {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            workspace_revision: record.workspace_revision.to_string(),
            bounds,
        })
    }
}

/// `layers.list` rows and renderer styles are built from the same state.
pub(crate) fn layer_rows(record: &S1DocumentRecord) -> Vec<LayerInfo> {
    record
        .display_order
        .iter()
        .enumerate()
        .filter_map(|(z_index, id)| {
            let layer = record.document.layers.iter().find(|l| &l.id == id)?;
            let state = record.workspace.get(id)?;
            let source = record.sources.get(id);
            Some(LayerInfo {
                layer_id: id.clone(),
                display_name: state.display_name.clone(),
                visible: state.visible,
                object_count: layer.objects.len(),
                locked: state.locked,
                selectable: state.selectable,
                kind: state.kind,
                base_color: state.style.base_color,
                color_mode: state.style.color_mode,
                display_mode: state.style.display_mode,
                classes: class_infos(state),
                effective_visible: effective_layer_visible(record, id, state),
                is_active: record.active_layer_id.as_deref() == Some(id.as_str()),
                is_solo: record.solo_layer_id.as_deref() == Some(id.as_str()),
                z_index,
                source_id: source.and_then(|s| s.source_id.clone()),
                provenance: source.and_then(|s| s.provenance.clone()),
                import_diagnostics: source.map(|s| s.diagnostics.clone()).unwrap_or_default(),
            })
        })
        .collect()
}

pub(crate) fn render_snapshot_of(document_id: &str, record: &S1DocumentRecord) -> RenderSnapshot {
    let mut layers = Vec::with_capacity(record.display_order.len());
    let mut styles = Vec::with_capacity(record.display_order.len());
    // Panel top = highest display priority = drawn last.
    for id in record.display_order.iter().rev() {
        let (Some(layer), Some(state)) = (
            record.document.layers.iter().find(|l| &l.id == id),
            record.workspace.get(id),
        ) else {
            continue;
        };
        layers.push(layer.clone());
        styles.push(RenderLayerStyle {
            layer_id: id.clone(),
            visible: effective_layer_visible(record, id, state),
            selectable: state.selectable,
            locked: state.locked,
            base_color: state.style.base_color,
            color_mode: state.style.color_mode,
            display_mode: state.style.display_mode,
            classes: class_infos(state),
        });
    }
    RenderSnapshot {
        document_id: document_id.into(),
        revision: record.revision.to_string(),
        workspace_revision: record.workspace_revision.to_string(),
        layers,
        apertures: record.document.apertures.clone(),
        styles,
        block_definitions: record.document.block_definitions.clone(),
    }
}

/// Everything one exported Gerber needs and nothing else: this layer, the
/// apertures it actually references, and its own source metadata.
pub(crate) fn layer_export_snapshot(
    record: &S1DocumentRecord,
    layer_id: &str,
) -> Result<SemanticDocument, ServiceError> {
    let layer = record
        .document
        .layers
        .iter()
        .find(|l| l.id == layer_id)
        .ok_or_else(|| ServiceError::not_found("layer", layer_id))?;
    let mut used: HashSet<&str> = HashSet::new();
    let mut used_definitions: HashSet<&editor_core::block::BlockDefinitionId> = HashSet::new();
    for object in &layer.objects {
        match &object.geometry {
            SemanticGeometry::Flash { aperture_id, .. } => {
                used.insert(aperture_id.as_str());
            }
            SemanticGeometry::BlockInstance { definition_id, .. } => {
                used_definitions.insert(definition_id);
            }
            _ => {}
        }
    }
    let block_definitions: Vec<_> = record
        .document
        .block_definitions
        .iter()
        .filter(|definition| used_definitions.contains(&definition.id))
        .cloned()
        .collect();
    // A block-local Flash also needs its aperture in the single-layer export
    // document, since the writer flattens instances through this same table.
    for definition in &block_definitions {
        for object in &definition.objects {
            if let editor_core::block::BlockObjectGeometry::Flash { aperture_id, .. } =
                &object.geometry
            {
                used.insert(aperture_id.as_str());
            }
        }
    }
    Ok(SemanticDocument {
        id: record.document.id.clone(),
        unit: record.document.unit.clone(),
        format: record.document.format.clone(),
        layers: vec![layer.clone()],
        apertures: record
            .document
            .apertures
            .iter()
            .filter(|a| used.contains(a.id.as_str()))
            .cloned()
            .collect(),
        source: record
            .sources
            .get(layer_id)
            .map(|s| s.metadata.clone())
            .unwrap_or_default(),
        block_definitions,
    })
}

/// Whether objects of this layer can be picked at all (layer visible incl. Solo,
/// and layer selectable). Unknown layer is an error like every other query.
pub(crate) fn layer_selectable(
    record: &S1DocumentRecord,
    layer_id: &str,
) -> Result<bool, ServiceError> {
    let state = record
        .workspace
        .get(layer_id)
        .ok_or_else(|| ServiceError::not_found("layer", layer_id))?;
    Ok(effective_layer_visible(record, layer_id, state) && state.selectable)
}

/// Keep only objects whose category is visible and selectable on this layer.
pub(crate) fn retain_selectable(
    record: &S1DocumentRecord,
    layer_id: &str,
    object_ids: &mut Vec<String>,
) {
    let Some(state) = record.workspace.get(layer_id) else {
        return;
    };
    if state
        .style
        .classes
        .values()
        .all(|class| class.visible && class.selectable)
    {
        return;
    }
    let Some(layer) = record.document.layers.iter().find(|l| l.id == layer_id) else {
        return;
    };
    let shapes = aperture_shape_map(&record.document.apertures);
    let allowed: HashSet<&str> = layer
        .objects
        .iter()
        .filter(|object| state.effective_selectable(classify_object(object, &shapes)))
        .map(|object| object.object_id.as_str())
        .collect();
    object_ids.retain(|id| allowed.contains(id.as_str()));
}
