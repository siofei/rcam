//! One revision-bound multi-layer command, shared by GUI and JSON callers.
use super::*;
pub use editor_core::edit::{MoveDemand, SelectionEdit, SelectionGroup};
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
    /// Read-only resource admission for the GUI's exact submission route.
    /// No geometry transform, revision/history mutation or success promise.
    pub fn selection_move_demand(
        &self,
        document_id: &str,
        expected_revision: &str,
        groups: &[SelectionGroup],
    ) -> Result<MoveDemand, ServiceError> {
        self.selection_move_demand_with_cancel(document_id, expected_revision, groups, None)
    }
    /// Trusted host cancellation; no new JSON operation.
    pub fn selection_move_demand_with_cancel(
        &self,
        document_id: &str,
        expected_revision: &str,
        groups: &[SelectionGroup],
        cancel: Option<&task::CancellationToken>,
    ) -> Result<MoveDemand, ServiceError> {
        if let Some(c) = cancel {
            c.checkpoint()?;
        }
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        check_revision(record.revision, expected_revision)?;
        if record.revision == u64::MAX {
            return Err(ServiceError::resource("revision", usize::MAX, usize::MAX));
        }
        check_groups(groups, MAX_MOVE_TARGETS)?;
        check_move_lookup_work(record, groups.len())?;
        for group in groups {
            check_targets_with_cancel(record, group, cancel)?;
        }
        let mut checkpoint = || {
            if cancel.is_some_and(|c| c.checkpoint().is_err()) {
                Err(EditError::ResourceLimit)
            } else {
                Ok(())
            }
        };
        let demand = if groups.len() == 1 {
            record.history.move_objects_demand_checked(
                &record.document,
                &groups[0].layer_id,
                &groups[0].object_ids,
                &mut checkpoint,
            )
        } else {
            record
                .history
                .move_selection_demand_checked(&record.document, groups, &mut checkpoint)
        };
        // A checkpoint abort never escapes as a geometry/resource error.
        if let Some(c) = cancel {
            c.checkpoint()?;
        }
        let demand = demand.map_err(map_edit_error)?;
        check_move_demand(demand)?;
        Ok(demand)
    }

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
            let limit = if matches!(params.operation, SelectionEdit::Move { .. }) {
                MAX_MOVE_TARGETS
            } else {
                MAX_MOVE_OBJECTS
            };
            check_groups(&params.groups, limit)?;
            if matches!(params.operation, SelectionEdit::Move { .. }) {
                check_move_lookup_work(record, params.groups.len())?;
            }
            // Resolve and check all current permissions before the core builds any delta.
            for group in &params.groups {
                check_targets(record, group)?;
            }
            if let SelectionEdit::Move { dx_mm, dy_mm } = params.operation {
                check_move_delta(dx_mm, dy_mm)?;
                let demand = record
                    .history
                    .move_selection_demand(&record.document, &params.groups)
                    .map_err(map_edit_error)?;
                check_move_demand(demand)?;
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
    check_targets_with_cancel(record, group, None)
}
pub(super) fn check_targets_with_cancel(
    record: &S1DocumentRecord,
    group: &SelectionGroup,
    cancel: Option<&task::CancellationToken>,
) -> Result<(), ServiceError> {
    if let Some(c) = cancel {
        c.checkpoint()?;
    }
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
    for (n, object) in layer.objects.iter().enumerate() {
        if n % 256 == 0
            && let Some(c) = cancel
        {
            c.checkpoint()?;
        }
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

pub(super) fn check_move_demand(demand: MoveDemand) -> Result<(), ServiceError> {
    demand.admit().map_err(|_| ServiceError {
        code: "RESOURCE_LIMIT".into(),
        message: format!("移动资源不足：{}个对象，历史计费{} / {} MiB，工作计费{} / {} MiB，校验工作{} / {}。", demand.object_count,
            demand.history_bytes.div_ceil(1024 * 1024), demand.history_limit_bytes.div_ceil(1024 * 1024),
            demand.work_bytes.div_ceil(1024 * 1024), demand.work_limit_bytes.div_ceil(1024 * 1024), demand.validation_work, demand.validation_work_limit),
        details: serde_json::json!({"resource": "move", "demand": demand, "max_move_objects": MAX_MOVE_TARGETS}),
    })
}

pub(super) fn check_groups(groups: &[SelectionGroup], limit: usize) -> Result<(), ServiceError> {
    let count = groups
        .iter()
        .try_fold(0usize, |n, g| n.checked_add(g.object_ids.len()))
        .ok_or_else(|| map_edit_error(EditError::ResourceLimit))?;
    if groups.is_empty() || count == 0 {
        return Err(map_edit_error(EditError::InvalidArgument));
    }
    if count > limit || groups.len() > MAX_MOVE_OBJECTS {
        return Err(map_edit_error(EditError::ResourceLimit));
    }
    let mut seen = std::collections::HashSet::with_capacity(groups.len());
    if groups
        .iter()
        .any(|g| g.object_ids.is_empty() || !seen.insert(&g.layer_id))
    {
        return Err(map_edit_error(EditError::InvalidArgument));
    }
    Ok(())
}
fn check_move_lookup_work(record: &S1DocumentRecord, groups: usize) -> Result<(), ServiceError> {
    let lookup = record
        .document
        .layers
        .len()
        .checked_add(record.document.apertures.len())
        .and_then(|n| n.checked_mul(groups))
        .and_then(|n| n.checked_mul(4));
    if lookup.is_none_or(|n| n > editor_core::edit::MAX_EDIT_REGION_EDGES) {
        return Err(map_edit_error(EditError::ResourceLimit));
    }
    Ok(())
}
pub(super) fn check_move_delta(dx: f64, dy: f64) -> Result<(), ServiceError> {
    if !MmPoint::new(dx, dy).is_valid_geometry() || (dx == 0. && dy == 0.) {
        Err(map_edit_error(EditError::InvalidArgument))
    } else {
        Ok(())
    }
}
