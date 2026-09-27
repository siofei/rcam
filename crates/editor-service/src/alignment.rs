//! Same-layer, revision-bound manufacturing arrangement; GUI selection is not state here.
use super::*;
use editor_core::workspace::{aperture_shape_map, classify_object};
use std::collections::HashSet;

pub use editor_core::edit::{AlignmentMode, DistributionAxis};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlignParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub anchor_object_id: String,
    pub mode: AlignmentMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributeParams {
    pub layer_id: String,
    pub object_ids: Vec<String>,
    pub axis: DistributionAxis,
}

enum Arrangement<'a> {
    Align(&'a str, AlignmentMode),
    Distribute(DistributionAxis),
}

impl ApplicationService {
    pub fn objects_align(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: AlignParams,
    ) -> Result<EditResult, ServiceError> {
        self.arrange(
            document_id,
            expected_revision,
            &params.layer_id,
            &params.object_ids,
            Arrangement::Align(&params.anchor_object_id, params.mode),
        )
    }

    pub fn objects_distribute(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: DistributeParams,
    ) -> Result<EditResult, ServiceError> {
        self.arrange(
            document_id,
            expected_revision,
            &params.layer_id,
            &params.object_ids,
            Arrangement::Distribute(params.axis),
        )
    }

    fn arrange(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        layer_id: &str,
        object_ids: &[String],
        arrangement: Arrangement<'_>,
    ) -> Result<EditResult, ServiceError> {
        let (command, mode, axis, anchor) = match arrangement {
            Arrangement::Align(anchor, mode) => (
                "objects.align",
                Some(match mode {
                    AlignmentMode::Left => 1,
                    AlignmentMode::Right => 2,
                    AlignmentMode::Top => 3,
                    AlignmentMode::Bottom => 4,
                    AlignmentMode::HCenter => 5,
                    AlignmentMode::VCenter => 6,
                }),
                None,
                Some(anchor),
            ),
            Arrangement::Distribute(axis) => (
                "objects.distribute",
                None,
                Some(match axis {
                    DistributionAxis::Horizontal => 1,
                    DistributionAxis::Vertical => 2,
                }),
                None,
            ),
        };
        let mut operation = rcam_diagnostics::Operation::begin_document(
            command,
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        operation.selection_count(object_ids.len());
        operation.arrangement_metadata(mode, axis, anchor, object_ids.len(), 0);
        // Exclusive service access spans validation, bounds planning and commit. Workspace
        // changes do not bump manufacturing revision, so inspect current permissions here.
        let result = (|| {
            let record = self.edit_record(document_id, expected_revision)?;
            check_arrangement_targets(
                record,
                layer_id,
                object_ids,
                if anchor.is_some() { 2 } else { 3 },
            )?;
            let changed = match arrangement {
                Arrangement::Align(anchor, mode) => record.history.align_objects(
                    &mut record.document,
                    layer_id,
                    object_ids,
                    anchor,
                    mode,
                ),
                Arrangement::Distribute(axis) => record.history.distribute_objects(
                    &mut record.document,
                    layer_id,
                    object_ids,
                    axis,
                ),
            }
            .map_err(map_edit_error)?;
            let added = usize::from(!changed.is_empty());
            record.revision += added as u64;
            Ok(edit_result(document_id, record, changed, added))
        })();
        let moved = result
            .as_ref()
            .map_or(0, |r: &EditResult| r.changed_object_ids.len());
        operation.arrangement_metadata(mode, axis, anchor, object_ids.len(), moved);
        operation.end(
            self.documents.get(document_id).map(|r| r.revision),
            result
                .as_ref()
                .err()
                .map(|error: &ServiceError| error.code.as_str()),
        );
        result
    }
}

fn check_arrangement_targets(
    record: &S1DocumentRecord,
    layer_id: &str,
    ids: &[String],
    minimum: usize,
) -> Result<(), ServiceError> {
    if ids.len() > MAX_MOVE_OBJECTS {
        return Err(map_edit_error(EditError::ResourceLimit));
    }
    let wanted: HashSet<_> = ids.iter().map(String::as_str).collect();
    if ids.len() < minimum || wanted.len() != ids.len() {
        return Err(ServiceError::invalid_field(
            "object_ids",
            "对象数量不足或包含重复 ID",
        ));
    }
    // Resolve all supplied identities; never silently select only the active layer subset.
    let mut found = HashSet::with_capacity(ids.len());
    for layer in &record.document.layers {
        for object in &layer.objects {
            if wanted.contains(object.object_id.as_str()) {
                if layer.id != layer_id {
                    return Err(ServiceError {
                        code: "CROSS_LAYER_EDIT_UNSUPPORTED".into(),
                        message: "对齐和分布仅支持同一可编辑图层。".into(),
                        details: serde_json::json!({"field":"object_ids"}),
                    });
                }
                found.insert(object.object_id.as_str());
            }
        }
    }
    if let Some(id) = ids.iter().find(|id| !found.contains(id.as_str())) {
        return Err(ServiceError::not_found("object", id));
    }
    check_workspace_edit(record, layer_id, ids)?;
    if !workspace::layer_selectable(record, layer_id)? {
        return Err(ServiceError::invalid_field(
            "layer_id",
            "对齐和分布要求可见、可选的图层",
        ));
    }
    let state = &record.workspace[layer_id];
    let shapes = aperture_shape_map(&record.document.apertures);
    let layer = record
        .document
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .ok_or_else(|| ServiceError::not_found("layer", layer_id))?;
    for object in &layer.objects {
        if wanted.contains(object.object_id.as_str())
            && !state.effective_selectable(classify_object(object, &shapes))
        {
            return Err(ServiceError::invalid_field(
                "object_ids",
                "包含隐藏或不可选对象",
            ));
        }
    }
    Ok(())
}
