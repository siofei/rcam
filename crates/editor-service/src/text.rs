use super::*;
use editor_text::{Layout, TextError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontIdentity {
    pub path: String,
    pub sha256: String,
    pub face_index: u32,
    /// Caller-supplied provenance, not a software assertion of legal permission.
    pub license_status: String,
    pub redistribution_allowed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextParams {
    pub layer_id: String,
    pub layout: Layout,
    pub font: FontIdentity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextResult {
    pub generated_object_ids: Vec<String>,
    pub revision: String,
    pub undo_entries_added: usize,
    pub font: FontIdentity,
    pub manufacturing_error_bound_mm: f64,
}
/// Immutable manufacturing preview. The caller must reject a stale document,
/// revision, layer or draft generation before displaying it. This is not a commit token.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextPreviewResult {
    pub document_id: String,
    pub revision: String,
    pub params: TextParams,
    pub geometries: Vec<SemanticGeometry>,
    pub manufacturing_error_bound_mm: f64,
}
impl ApplicationService {
    fn prepare_text(
        &self,
        document_id: &str,
        expected_revision: &str,
        params: &TextParams,
    ) -> Result<Vec<SemanticGeometry>, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        if record.revision == u64::MAX {
            return Err(ServiceError::resource("revision", usize::MAX, usize::MAX));
        }
        check_workspace_edit(record, &params.layer_id)?;
        if params.font.sha256.len() != 64
            || !params.font.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || params.font.license_status.trim().is_empty()
            || params.font.license_status.len() > 1024
        {
            return Err(ServiceError::invalid_field(
                "font",
                "explicit SHA-256 and license status are required",
            ));
        }
        let policy = self
            .file_access
            .as_ref()
            .ok_or_else(|| ServiceError::invalid_field("font", "file access policy required"))?;
        let (_, bytes) = policy.read_path_limited(&params.font.path, 64 * 1024 * 1024)?;
        if sha256_hex(&bytes) != params.font.sha256.to_ascii_lowercase() {
            return Err(ServiceError::invalid_field(
                "font.sha256",
                "font hash mismatch",
            ));
        }
        let geometries = editor_text::generate(&bytes, params.font.face_index, &params.layout)
            .map_err(|e| ServiceError {
                code: match e {
                    TextError::ResourceLimit => "RESOURCE_LIMIT",
                    TextError::UnsupportedOutline => "UNSUPPORTED_FEATURE",
                    _ => "INVALID_ARGUMENT",
                }
                .into(),
                message: format!("text generation rejected: {e:?}"),
                details: serde_json::json!({"field":"text"}),
            })?;
        record
            .history
            .validate_generated(&record.document, &params.layer_id, &geometries)
            .map_err(map_edit_error)?;
        Ok(geometries)
    }

    /// Run on a worker: font I/O and geometry generation can be expensive.
    /// Does not allocate object IDs or mutate revision, dirty state or history.
    pub fn text_preview(
        &self,
        document_id: &str,
        expected_revision: &str,
        params: TextParams,
    ) -> Result<TextPreviewResult, ServiceError> {
        let geometries = self.prepare_text(document_id, expected_revision, &params)?;
        Ok(TextPreviewResult {
            document_id: document_id.into(),
            revision: expected_revision.into(),
            params,
            geometries,
            manufacturing_error_bound_mm: 0.001,
        })
    }

    pub fn text_create(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: TextParams,
    ) -> Result<TextResult, ServiceError> {
        // Re-read/hash and validate: a prior preview never authorizes changed font bytes.
        let geometries = self.prepare_text(document_id, expected_revision, &params)?;
        let record = self.edit_record(document_id, expected_revision)?;
        let ids = record
            .history
            .insert_generated(&mut record.document, &params.layer_id, geometries)
            .map_err(map_edit_error)?;
        record.metrics.reconcile(&record.document, &ids);
        record.revision += 1;
        Ok(TextResult {
            generated_object_ids: ids,
            revision: record.revision.to_string(),
            undo_entries_added: 1,
            font: params.font,
            manufacturing_error_bound_mm: 0.001,
        })
    }
}
