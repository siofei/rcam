//! One revision-bound multi-layer command, shared by GUI and JSON callers.
use super::*;
pub use editor_core::edit::{SelectionEdit, SelectionGroup};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditSelectionParams {
    pub groups: Vec<SelectionGroup>,
    pub operation: SelectionEdit,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditSelectionResult {
    pub edit: EditResult,
    pub groups: Vec<SelectionGroup>,
}
impl ApplicationService {
    pub fn objects_edit_selection(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: EditSelectionParams,
    ) -> Result<EditSelectionResult, ServiceError> {
        let mut observation = rcam_diagnostics::Operation::begin_document(
            "objects.edit_selection",
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        observation.selection_count(params.groups.iter().map(|g| g.object_ids.len()).sum());
        let result = (|| {
            let record = self.edit_record(document_id, expected_revision)?;
            let count = params
                .groups
                .iter()
                .try_fold(0usize, |n, g| n.checked_add(g.object_ids.len()))
                .ok_or_else(|| map_edit_error(EditError::ResourceLimit))?;
            if count > MAX_MOVE_OBJECTS {
                return Err(map_edit_error(EditError::ResourceLimit));
            }
            // Resolve and check all current permissions before the core builds any delta.
            for group in &params.groups {
                check_targets(record, group)?;
            }
            let groups = record
                .history
                .edit_selection(&mut record.document, &params.groups, &params.operation)
                .map_err(map_edit_error)?;
            let ids = groups
                .iter()
                .flat_map(|g| g.object_ids.iter().cloned())
                .collect();
            record.revision += 1;
            let edit = edit_result(document_id, record, ids, 1);
            Ok(EditSelectionResult { edit, groups })
        })();
        observation.end(
            self.documents.get(document_id).map(|r| r.revision),
            result
                .as_ref()
                .err()
                .map(|e: &ServiceError| e.code.as_str()),
        );
        result
    }
}

fn check_targets(record: &S1DocumentRecord, group: &SelectionGroup) -> Result<(), ServiceError> {
    use editor_core::workspace::{aperture_shape_map, classify_object};
    let wanted: std::collections::HashSet<_> =
        group.object_ids.iter().map(String::as_str).collect();
    if wanted.is_empty() || wanted.len() != group.object_ids.len() {
        return Err(ServiceError::invalid("目标列表为空或包含重复ID"));
    }
    check_workspace_edit(record, &group.layer_id, &group.object_ids)?;
    if !workspace::layer_selectable(record, &group.layer_id)? {
        return Err(ServiceError::invalid("操作要求可见、可选的图层"));
    }
    let layer = record
        .document
        .layers
        .iter()
        .find(|l| l.id == group.layer_id)
        .ok_or_else(|| ServiceError::not_found("layer", &group.layer_id))?;
    let state = &record.workspace[&group.layer_id];
    let shapes = aperture_shape_map(&record.document.apertures);
    let mut found = std::collections::HashSet::new();
    for object in &layer.objects {
        if wanted.contains(object.object_id.as_str()) {
            if !state.effective_selectable(classify_object(object, &shapes)) {
                return Err(ServiceError::invalid("选择包含隐藏或不可选对象"));
            }
            found.insert(object.object_id.as_str());
        }
    }
    if let Some(id) = group
        .object_ids
        .iter()
        .find(|id| !found.contains(id.as_str()))
    {
        return Err(ServiceError::not_found("object", id));
    }
    Ok(())
}
