//! Read-only Block-definition local adapter for the frozen A material engine.
use super::*;
use editor_core::{
    ApertureShape, MacroPrimitive, SemanticDocument, SemanticFormat, SemanticLayer, SemanticObject,
    SourceMetadata, block::BlockObjectGeometry,
};
use std::time::{Duration, Instant};
impl ApplicationService {
    pub fn geometry_block_definition_centers_cancellable(
        &self,
        id: &str,
        revision: &str,
        definition_id: &str,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SelectionCentersResult, ServiceError> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut check = || {
            if cancelled() {
                Err(ServiceError {
                    code: "CANCELLED".into(),
                    message: "定义中心查询已取消".into(),
                    details: serde_json::json!({}),
                })
            } else if Instant::now() >= deadline {
                Err(ServiceError {
                    code: "RESOURCE_LIMIT".into(),
                    message: "定义中心查询超过总预算".into(),
                    details: serde_json::json!({}),
                })
            } else {
                Ok(())
            }
        };
        check()?;
        let record = self
            .documents
            .get(id)
            .ok_or_else(|| ServiceError::not_found("document", id))?;
        if revision != record.revision.to_string() {
            return Err(ServiceError {
                code: "REVISION_CONFLICT".into(),
                message: "定义中心查询已过期".into(),
                details: serde_json::json!({}),
            });
        }
        let d = record
            .document
            .block_definitions
            .iter()
            .find(|d| d.id.0 == definition_id)
            .ok_or_else(|| ServiceError::not_found("block_definition", definition_id))?;
        if d.objects.is_empty() || d.objects.len() > 10_000 {
            return Err(ServiceError::invalid("定义对象为空或超过选择上限"));
        }
        // Admit stored source sizes before any geometry/aperture cloning.
        let mut cost = 0usize;
        let mut ids = std::collections::HashSet::new();
        for o in &d.objects {
            check()?;
            cost = cost.saturating_add(match &o.geometry {
                BlockObjectGeometry::Region { contours } => contours
                    .iter()
                    .map(|c| c.edges.len().saturating_add(1))
                    .sum(),
                BlockObjectGeometry::Flash { aperture_id, .. } => {
                    ids.insert(aperture_id.as_str());
                    1
                }
                _ => 1,
            });
        }
        let mut apertures = Vec::new();
        for a in &record.document.apertures {
            check()?;
            if !ids.contains(a.id.as_str()) {
                continue;
            }
            if let ApertureShape::Macro { primitives } = &a.shape {
                for p in primitives {
                    check()?;
                    cost = cost.saturating_add(match p {
                        MacroPrimitive::Outline { points, .. } => points.len(),
                        _ => 4,
                    });
                }
            }
            if cost.saturating_mul(1024) > 128 * 1024 * 1024 {
                return Err(ServiceError {
                    code: "RESOURCE_LIMIT".into(),
                    message: "定义中心临时数据超限".into(),
                    details: serde_json::json!({}),
                });
            }
            apertures.push(a.clone());
        }
        if cost.saturating_mul(1024) > 128 * 1024 * 1024 {
            return Err(ServiceError {
                code: "RESOURCE_LIMIT".into(),
                message: "定义中心源数据超限".into(),
                details: serde_json::json!({}),
            });
        }
        let mut objects = Vec::with_capacity(d.objects.len());
        let mut object_ids = Vec::with_capacity(d.objects.len());
        for (n, o) in d.objects.iter().enumerate() {
            check()?;
            let object_id = n.to_string();
            object_ids.push(object_id.clone());
            objects.push(SemanticObject {
                object_id,
                geometry: o.geometry.clone().into(),
                exposure: o.exposure,
                origin: editor_core::ObjectOrigin::Generated {
                    operation_id: "definition-point-query".into(),
                },
            });
        }
        let document = SemanticDocument {
            id: "definition-local-query".into(),
            unit: "mm".into(),
            format: SemanticFormat {
                integer: 4,
                decimal: 6,
                leading_zero_omission: true,
                absolute: true,
            },
            layers: vec![SemanticLayer {
                id: "local".into(),
                objects,
            }],
            apertures,
            source: SourceMetadata::default(),
            block_definitions: vec![],
        };
        check()?;
        let bounds = editor_core::geometries_bounds_with_blocks(
            document.layers[0].objects.iter().map(|o| &o.geometry),
            &document.apertures,
            &[],
        )
        .map_err(|e| ServiceError::invalid(e.to_string()))?;
        check()?;
        let calculated = editor_core::hit_test::selection_geometry::calculate_with_deadline(
            &document,
            &[SelectionGroup {
                layer_id: "local".into(),
                object_ids,
            }],
            record.manufacturing_precision.resolution_mm,
            deadline,
            || check().is_err(),
        );
        check()?;
        let material = match calculated {
            Ok(q) => SelectionMaterialResult::Computed { value: q.material },
            Err(SelectionGeometryError::Cancelled) => {
                return Err(ServiceError {
                    code: "CANCELLED".into(),
                    message: "定义中心已取消".into(),
                    details: serde_json::json!({}),
                });
            }
            Err(error) => SelectionMaterialResult::Unavailable { error },
        };
        check()?;
        Ok(SelectionCentersResult {
            document_id: id.into(),
            computed_revision: revision.into(),
            resolution_mm: record.manufacturing_precision.resolution_mm,
            bounds_mm: bounds,
            bounding_center_mm: bounds.map(|b| b.center()),
            material,
            selected_count: d.objects.len(),
            cache_hit: false,
        })
    }
}
