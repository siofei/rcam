//! Explicit numeric requests, independent of GUI selection and Snap settings.
use super::*;
use editor_core::edit::RectangularArray;

fn array_edit_error(
    record: &S1DocumentRecord,
    params: &ArrayRectangularParams,
    error: EditError,
) -> ServiceError {
    if error != EditError::ResourceLimit {
        return map_edit_error(error);
    }
    let mut mapped = map_edit_error(error);
    mapped.message = "阵列资源预算或数值容量不足；没有阵列格数硬上限".into();
    mapped.details = serde_json::json!({
        "max_source_objects": editor_core::edit::MAX_MOVE_OBJECTS,
        "max_document_objects": editor_core::edit::MAX_EDIT_DOCUMENT_OBJECTS,
        "max_region_edges": editor_core::edit::MAX_EDIT_REGION_EDGES,
        "max_history_bytes": record.history.max_bytes(),
    });
    if let Ok(estimate) = record.history.array_resource_requirements(
        &record.document,
        &params.layer_id,
        &params.object_ids,
        RectangularArray {
            rows: params.rows,
            columns: params.columns,
            pitch_x_mm: params.pitch_x_mm,
            pitch_y_mm: params.pitch_y_mm,
        },
    ) {
        mapped.details["required_history_bytes"] = estimate.history_bytes.into();
        mapped.details["created_object_count"] = estimate.created_object_count.into();
        mapped.details["added_region_edges"] = estimate.added_region_edges.into();
    }
    mapped
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrayRectangularParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub rows: u64,
    pub columns: u64,
    pub pitch_x_mm: f64,
    pub pitch_y_mm: f64,
}

impl ApplicationService {
    /// Read-only preflight used by the GUI worker; no IDs or history are consumed.
    pub fn estimate_array_rectangular(
        &self,
        document_id: &str,
        expected_revision: &str,
        params: &ArrayRectangularParams,
    ) -> Result<editor_core::edit::ArrayEstimate, ServiceError> {
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        if record.revision.to_string() != expected_revision {
            return Err(ServiceError::invalid("阵列预览已过期"));
        }
        alignment::check_arrangement_targets(record, &params.layer_id, &params.object_ids, 1)?;
        record
            .history
            .estimate_array_rectangular(
                &record.document,
                &params.layer_id,
                &params.object_ids,
                RectangularArray {
                    rows: params.rows,
                    columns: params.columns,
                    pitch_x_mm: params.pitch_x_mm,
                    pitch_y_mm: params.pitch_y_mm,
                },
            )
            .map_err(|error| array_edit_error(record, params, error))
    }
    pub fn objects_array_rectangular(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: ArrayRectangularParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.array_rectangular",
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        let spec = RectangularArray {
            rows: params.rows,
            columns: params.columns,
            pitch_x_mm: params.pitch_x_mm,
            pitch_y_mm: params.pitch_y_mm,
        };
        let contains_block = self.documents.get(document_id).is_some_and(|record| {
            let wanted: std::collections::HashSet<_> =
                params.object_ids.iter().map(String::as_str).collect();
            record
                .document
                .layers
                .iter()
                .filter(|l| l.id == params.layer_id)
                .flat_map(|l| &l.objects)
                .any(|o| {
                    wanted.contains(o.object_id.as_str())
                        && matches!(o.geometry, SemanticGeometry::BlockInstance { .. })
                })
        });
        let result = (|| {
            let record = self.edit_record(document_id, expected_revision)?;
            alignment::check_arrangement_targets(record, &params.layer_id, &params.object_ids, 1)?;
            let changed = record
                .history
                .array_rectangular_objects(
                    &mut record.document,
                    &params.layer_id,
                    &params.object_ids,
                    spec,
                )
                .map_err(|error| array_edit_error(record, &params, error))?;
            let added = usize::from(!changed.is_empty());
            record.revision += added as u64;
            Ok(edit_result(document_id, record, changed, added))
        })();
        operation.array_metadata(
            spec,
            params.object_ids.len(),
            result
                .as_ref()
                .map_or(0, |r: &EditResult| r.changed_object_ids.len()),
            contains_block,
        );
        operation.end(
            self.documents.get(document_id).map(|r| r.revision),
            result
                .as_ref()
                .err()
                .map(|e: &ServiceError| e.code.as_str()),
        );
        result
    }
}
