use super::*;
use editor_core::grip::{GripFeature, GripFeatureId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GripEditParams {
    pub layer_id: String,
    pub object_id: String,
    pub grip_id: GripFeatureId,
    pub target_mm: editor_core::MmPoint,
}

fn allowed(record: &S1DocumentRecord, layer: &str, object: &str) -> Result<(), ServiceError> {
    check_workspace_edit(record, layer, &[object.into()])?;
    let mut ids = vec![object.into()];
    workspace::retain_selectable(record, layer, &mut ids);
    if record.active_layer_id.as_deref() != Some(layer)
        || !workspace::layer_selectable(record, layer)?
        || ids.is_empty()
    {
        return Err(ServiceError::invalid_field(
            "layer_id",
            "Grip target must be visible, selectable and on the active layer",
        ));
    }
    Ok(())
}
impl ApplicationService {
    pub fn objects_grips(
        &self,
        document_id: &str,
        params: ObjectParams,
    ) -> Result<Vec<GripFeature>, ServiceError> {
        let object = self.objects_get(document_id, params.clone())?;
        let record = self
            .documents
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found("document", document_id))?;
        if allowed(record, &params.layer_id, &params.object_id).is_err() {
            return Ok(vec![]);
        }
        let aperture = match &object.object.geometry {
            editor_core::SemanticGeometry::Flash { aperture_id, .. } => record
                .document
                .apertures
                .iter()
                .find(|a| &a.id == aperture_id)
                .map(|a| &a.shape),
            _ => None,
        };
        editor_core::grip::grip_features(&object.object, aperture).map_err(map_edit_error)
    }

    pub fn objects_grip_edit(
        &mut self,
        document_id: &str,
        expected_revision: &str,
        params: GripEditParams,
    ) -> Result<EditResult, ServiceError> {
        let mut operation = rcam_diagnostics::Operation::begin_document(
            "objects.grip_edit",
            document_id,
            self.documents.get(document_id).map(|r| r.revision),
        );
        operation.selection_count(1);
        let kind = self
            .objects_get(
                document_id,
                ObjectParams {
                    layer_id: params.layer_id.clone(),
                    object_id: params.object_id.clone(),
                },
            )
            .ok()
            .map_or(0, |o| match o.object.geometry {
                editor_core::SemanticGeometry::Flash { .. } => 1,
                editor_core::SemanticGeometry::Line { .. } => 2,
                editor_core::SemanticGeometry::RectangularSweep { .. } => 3,
                editor_core::SemanticGeometry::Arc { .. } => 4,
                editor_core::SemanticGeometry::Region { .. } => 5,
                editor_core::SemanticGeometry::BlockInstance { .. } => 6,
            });
        let grip = match params.grip_id {
            GripFeatureId::Start => 1,
            GripFeatureId::End => 2,
            GripFeatureId::Radius => 3,
            GripFeatureId::Left => 4,
            GripFeatureId::Right => 5,
            GripFeatureId::Top => 6,
            GripFeatureId::Bottom => 7,
            GripFeatureId::Corner { .. } => 8,
            GripFeatureId::Vertex { .. } => 9,
        };
        operation.grip_metadata(kind, grip);
        let result = (|| {
            let record = self.edit_record(document_id, expected_revision)?;
            allowed(record, &params.layer_id, &params.object_id)?;
            let ids = record
                .history
                .grip_edit(
                    &mut record.document,
                    &params.layer_id,
                    &params.object_id,
                    params.grip_id,
                    params.target_mm,
                )
                .map_err(map_edit_error)?;
            record.metrics.invalidate_shapes(&ids);
            record.revision += 1;
            Ok(edit_result(document_id, record, ids, 1))
        })();
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

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{
        Exposure, ObjectOrigin, RegionContour, RegionEdge, RegionRole, SemanticFormat,
        SemanticGeometry, SemanticLayer, SemanticObject, SourceMetadata,
        workspace::{Color, LayerKind, LayerWorkspaceState},
    };

    #[test]
    fn objects_grips_returns_empty_for_solid_region_with_line_hole() {
        let point = editor_core::MmPoint::new;
        let contour = |role, vertices: [editor_core::MmPoint; 5]| RegionContour {
            role,
            edges: vertices
                .windows(2)
                .map(|pair| RegionEdge::Line {
                    start: pair[0],
                    end: pair[1],
                })
                .collect(),
        };
        let region = SemanticGeometry::Region {
            contours: vec![
                contour(
                    RegionRole::Solid,
                    [
                        point(0., 0.),
                        point(4., 0.),
                        point(4., 4.),
                        point(0., 4.),
                        point(0., 0.),
                    ],
                ),
                contour(
                    RegionRole::Hole,
                    [
                        point(1., 1.),
                        point(1., 3.),
                        point(3., 3.),
                        point(3., 1.),
                        point(1., 1.),
                    ],
                ),
            ],
        };
        let document = SemanticDocument {
            id: "hole-grip-test".into(),
            unit: "mm".into(),
            format: SemanticFormat {
                integer: 4,
                decimal: 6,
                leading_zero_omission: true,
                absolute: true,
            },
            layers: vec![SemanticLayer {
                id: "layer-hole".into(),
                objects: vec![SemanticObject {
                    object_id: "region-hole".into(),
                    geometry: region,
                    exposure: Exposure::Dark,
                    origin: ObjectOrigin::Imported { command_index: 0 },
                }],
            }],
            apertures: vec![],
            source: SourceMetadata::default(),
            block_definitions: vec![],
        };

        let mut service = ApplicationService::new();
        let mut record = service.new_record(document).unwrap();
        record.active_layer_id = Some("layer-hole".into());
        record.workspace.insert(
            "layer-hole".into(),
            LayerWorkspaceState::new(
                LayerKind::Gerber,
                "Hole test",
                Color::from_hex("#ffffff").unwrap(),
            ),
        );
        service.documents.insert("hole-grip-test".into(), record);

        let response = service.execute_json(
            &serde_json::json!({
                "api_version": 1,
                "request_id": "hole-grips",
                "op": "objects.grips",
                "document_id": "hole-grip-test",
                "expected_revision": null,
                "params": {
                    "layer_id": "layer-hole",
                    "object_id": "region-hole"
                }
            })
            .to_string(),
        );
        assert_eq!(response["status"], "completed", "{response}");
        assert_eq!(response["result"], serde_json::json!([]), "{response}");
    }
}
