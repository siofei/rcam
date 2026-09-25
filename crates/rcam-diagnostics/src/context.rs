//! Geometry-free, explicit diagnostic package whitelist.
use editor_core::{
    units::DisplayUnit,
    workspace::{LayerDisplayMode, LayerKind},
};
use serde::{Deserialize, Serialize};
pub fn hash_identity(value: &str) -> String {
    editor_core::hash::sha256_hex(value.as_bytes())[..16].into()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiagnosticContext {
    pub project: Option<ProjectSummary>,
    pub layers: LayerSummary,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub document_id_hash: String,
    pub project_id_hash: String,
    pub manufacturing_revision: u64,
    pub workspace_revision: u64,
    pub project_dirty: bool,
    pub layer_count: usize,
    pub object_count: usize,
    pub block_definition_count: usize,
    pub block_instance_count: usize,
    pub manufacturing_precision_nm: u64,
    pub display_unit: DisplayUnit,
    pub compatibility_layer_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerSummary {
    pub schema_version: u32,
    pub actual_count: usize,
    pub truncated: bool,
    pub layers: Vec<LayerDiagnostic>,
}
impl Default for LayerSummary {
    fn default() -> Self {
        Self {
            schema_version: 1,
            actual_count: 0,
            truncated: false,
            layers: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerDiagnostic {
    pub layer_id_hash: String,
    pub name_hash: String,
    pub kind: LayerKind,
    pub object_count: usize,
    pub visible: bool,
    pub selectable: bool,
    pub locked: bool,
    pub display_mode: LayerDisplayMode,
    pub compatibility_issue_count: usize,
    pub source_content_hash_prefix: Option<String>,
}
