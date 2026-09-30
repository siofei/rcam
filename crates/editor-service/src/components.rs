//! Independent project metadata, explicit mapping and revision-fenced queries.
use super::*;
use editor_core::{board::*, pnp::*};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportPnpParams {
    pub path: String,
    pub mapping: PnpMapping,
    pub preview_sha256: String,
    #[serde(default)]
    pub allow_replace: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PnpFilePreview {
    pub sha256: String,
    pub preview: PnpPreview,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefdesMatch {
    Exact,
    Prefix,
    Substring,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentQuery {
    pub revision: String,
    pub query: String,
    pub mode: RefdesMatch,
    pub side: Option<BoardSide>,
    pub footprint: Option<String>,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub component: ComponentPlacement,
    pub world_position: Option<MmPoint>,
    pub world_rotation_deg: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentPage {
    pub document_id: String,
    pub revision: String,
    pub total: usize,
    pub items: Vec<ComponentInfo>,
}
pub fn component_info(c: &ComponentPlacement, board: &BoardState) -> ComponentInfo {
    let t = board.registration.as_ref().map(|r| r.transform);
    ComponentInfo {
        component: c.clone(),
        world_position: t.map(|t| t.apply(MmPoint::new(c.position.x_mm, c.position.y_mm))),
        world_rotation_deg: t.map(|t| t.apply_direction_deg(c.rotation_deg)),
    }
}
fn matches_query(c: &ComponentPlacement, q: &ComponentQuery) -> bool {
    !q.side.is_some_and(|s| c.side != s)
        && q.footprint
            .as_ref()
            .is_none_or(|f| c.footprint.as_ref() == Some(f))
        && (q.query.is_empty()
            || match q.mode {
                RefdesMatch::Exact => c.refdes == q.query,
                RefdesMatch::Prefix => c.refdes.starts_with(&q.query),
                RefdesMatch::Substring => c.refdes.contains(&q.query),
            })
}
impl ApplicationService {
    /// Worker-owned virtual table index. Rebuilt per explicit query, never per frame.
    pub fn component_indices(
        &self,
        document_id: &str,
        q: &ComponentQuery,
    ) -> Result<Arc<Vec<usize>>, ServiceError> {
        self.components_search(document_id, q)?;
        Ok(Arc::new(self.board_state(document_id)?.map_or(
            vec![],
            |b| {
                b.components
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| matches_query(c, q))
                    .map(|(i, _)| i)
                    .collect()
            },
        )))
    }

    pub fn board_state(&self, document_id: &str) -> Result<Option<Arc<BoardState>>, ServiceError> {
        Ok(self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?
            .board
            .clone())
    }
    pub fn board_get_registration(
        &self,
        document_id: &str,
    ) -> Result<Option<BoardRegistration>, ServiceError> {
        Ok(self
            .board_state(document_id)?
            .and_then(|b| b.registration.clone()))
    }
    pub fn components_preview_pnp(
        &self,
        path: &str,
        mapping: &PnpMapping,
    ) -> Result<PnpFilePreview, ServiceError> {
        let op = rcam_diagnostics::Operation::begin("components.preview_pnp", None);
        let result = (|| {
            let policy = self
                .file_access
                .as_ref()
                .ok_or_else(|| ServiceError::permission(Path::new(path), "read"))?;
            let (_, bytes) = policy.read_path_limited(path, MAX_PNP_BYTES)?;
            let preview = parse_pnp(&bytes, mapping);
            rcam_diagnostics::measurements(
                rcam_diagnostics::Level::Info,
                "components.preview.counts",
                &[
                    ("rows", preview.row_count as u64),
                    ("diagnostics", preview.diagnostic_count as u64),
                ],
            );
            Ok(PnpFilePreview {
                sha256: sha256_hex(&bytes),
                preview,
            })
        })();
        op.end(
            None,
            result
                .as_ref()
                .err()
                .map(|e: &ServiceError| e.code.as_str()),
        );
        result
    }
    pub fn components_import_pnp(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ImportPnpParams,
    ) -> Result<EditResult, ServiceError> {
        let op = rcam_diagnostics::Operation::begin_document(
            "components.import_pnp",
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        let result = (|| {
            let record = self
                .documents
                .get(document_id)
                .ok_or_else(|| ServiceError::not_found("document", document_id))?;
            check_revision(record.revision, expected_revision)?;
            if record.board.is_some() && !params.allow_replace {
                return Err(ServiceError {
                    code: "CONFIRMATION_REQUIRED".into(),
                    message: "Replacing the component table requires explicit confirmation".into(),
                    details: serde_json::json!({}),
                });
            }
            // Explicit I/O and parsing happen before the memory transaction.
            let policy = self
                .file_access
                .as_ref()
                .ok_or_else(|| ServiceError::permission(Path::new(&params.path), "read"))?;
            let (path, bytes) = policy.read_path_limited(&params.path, MAX_PNP_BYTES)?;
            let hash = sha256_hex(&bytes);
            if hash != params.preview_sha256 {
                return Err(ServiceError {
                    code: "EXTERNAL_MODIFICATION".into(),
                    message: "PnP source changed since preview".into(),
                    details: serde_json::json!({}),
                });
            }
            let mut preview = parse_pnp(&bytes, &params.mapping);
            rcam_diagnostics::measurements(
                rcam_diagnostics::Level::Info,
                "components.import.counts",
                &[
                    ("rows", preview.row_count as u64),
                    ("diagnostics", preview.diagnostic_count as u64),
                ],
            );
            let categories = [
                "invalid_number",
                "unknown_side",
                "duplicate_identity",
                "invalid_mapping",
                "field_count",
                "invalid_utf8",
                "field_budget",
                "row_budget",
            ];
            let counts: Vec<_> = categories
                .iter()
                .map(|code| {
                    (
                        *code,
                        preview
                            .diagnostics
                            .iter()
                            .filter(|d| d.code == *code)
                            .count() as u64,
                    )
                })
                .collect();
            rcam_diagnostics::measurements(
                rcam_diagnostics::Level::Info,
                "components.import.diagnostics",
                &counts,
            );
            if !preview.valid() {
                return Err(ServiceError {
                    code: "VALIDATION_FAILED".into(),
                    message: "PnP batch rejected; fix mapping or diagnosed rows".into(),
                    details: serde_json::json!({"diagnostics":preview.diagnostics,"diagnostic_count":preview.diagnostic_count}),
                });
            }
            let record = self.edit_record(document_id, expected_revision)?;
            let next = record
                .next_component_id
                .checked_add(preview.components.len() as u64)
                .ok_or_else(|| ServiceError::resource("component_ids", usize::MAX, usize::MAX))?;
            for (i, c) in preview.components.iter_mut().enumerate() {
                c.id = ComponentId(format!("component-{}", record.next_component_id + i as u64));
            }
            let board = Arc::new(BoardState {
                components: Arc::new(preview.components),
                mapping: params.mapping,
                provenance: PnpProvenance {
                    basename: path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .ok_or_else(|| ServiceError::invalid("PnP basename must be UTF-8"))?
                        .into(),
                    sha256: hash,
                    imported_unix_seconds: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                },
                registration: None,
            });
            board.validate().map_err(ServiceError::invalid)?;
            record
                .history
                .commit_board(&record.document.id, &mut record.board, Some(board))
                .map_err(map_edit_error)?;
            record.next_component_id = next;
            record.revision += 1;
            Ok(edit_result(document_id, record, vec![], 1))
        })();
        op.end(
            self.documents.get(document_id).map(|r| r.revision),
            result
                .as_ref()
                .err()
                .map(|e: &ServiceError| e.code.as_str()),
        );
        result
    }
    pub fn board_set_registration(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        input: RegistrationInput,
    ) -> Result<EditResult, ServiceError> {
        let op = rcam_diagnostics::Operation::begin_document(
            "board.set_registration",
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        let result = (|| {
            let registration = registration(input).map_err(ServiceError::invalid)?;
            let record = self.edit_record(document_id, expected_revision)?;
            let mut board = record
                .board
                .as_ref()
                .ok_or_else(|| ServiceError::invalid("Import PnP before registration"))?
                .as_ref()
                .clone();
            board.registration = Some(registration);
            board.validate().map_err(ServiceError::invalid)?;
            let changed = record
                .history
                .commit_board(
                    &record.document.id,
                    &mut record.board,
                    Some(Arc::new(board)),
                )
                .map_err(map_edit_error)?;
            record.revision += u64::from(changed);
            Ok(edit_result(
                document_id,
                record,
                vec![],
                usize::from(changed),
            ))
        })();
        op.end(
            self.documents.get(document_id).map(|r| r.revision),
            result
                .as_ref()
                .err()
                .map(|e: &ServiceError| e.code.as_str()),
        );
        result
    }
    pub fn components_search(
        &self,
        document_id: &str,
        q: &ComponentQuery,
    ) -> Result<ComponentPage, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, &q.revision)?;
        if q.query.len() > 128
            || q.query.chars().any(char::is_control)
            || q.limit == 0
            || q.limit > 500
            || q.offset > MAX_COMPONENTS
            || q.footprint
                .as_ref()
                .is_some_and(|f| f.len() > MAX_FIELD_BYTES)
        {
            return Err(ServiceError::invalid("component query budget"));
        }
        let mut items = Vec::with_capacity(q.limit);
        let mut total = 0;
        if let Some(board) = &record.board {
            for c in board.components.iter() {
                if !matches_query(c, q) {
                    continue;
                }
                if total >= q.offset && items.len() < q.limit {
                    items.push(component_info(c, board));
                }
                total += 1;
            }
        }
        Ok(ComponentPage {
            document_id: document_id.into(),
            revision: record.revision.to_string(),
            total,
            items,
        })
    }
    pub fn components_list(
        &self,
        document_id: &str,
        q: &ComponentQuery,
    ) -> Result<ComponentPage, ServiceError> {
        if !q.query.is_empty() {
            return Err(ServiceError::invalid("list requires empty query"));
        }
        self.components_search(document_id, q)
    }
    pub fn components_get(
        &self,
        document_id: &str,
        component_id: &str,
    ) -> Result<ComponentInfo, ServiceError> {
        if component_id.len() > MAX_FIELD_BYTES || component_id.chars().any(char::is_control) {
            return Err(ServiceError::invalid("component id budget"));
        }
        let board = self
            .board_state(document_id)?
            .ok_or_else(|| ServiceError::not_found("component", component_id))?;
        let c = board
            .components
            .iter()
            .find(|c| c.id.0 == component_id)
            .ok_or_else(|| ServiceError::not_found("component", component_id))?;
        Ok(component_info(c, &board))
    }
}
