//! Cached f64 manufacturing envelopes, queried before any GPU conversion.
use editor_core::{BoundsMm, geometries_bounds_with_blocks};
use editor_service::{LayerInfo, RenderSnapshot};

#[derive(Clone, Default)]
pub struct WorldIndex {
    entries: Vec<(BoundsMm, usize, usize)>,
    max_span_x_mm: f64,
}
impl WorldIndex {
    pub fn build(snapshot: &RenderSnapshot) -> Result<Self, String> {
        let mut entries = Vec::new();
        for (layer, data) in snapshot.layers.iter().enumerate() {
            for (object, data) in data.objects.iter().enumerate() {
                if let Some(bounds) = geometries_bounds_with_blocks(
                    [&data.geometry],
                    &snapshot.apertures,
                    &snapshot.block_definitions,
                )
                .map_err(|e| format!("VALIDATION_FAILED: world envelope: {e}"))?
                {
                    entries.push((bounds, layer, object));
                }
            }
        }
        entries.sort_by(|a, b| a.0.min_x_mm.total_cmp(&b.0.min_x_mm));
        let max_span_x_mm = entries
            .iter()
            .map(|(bounds, _, _)| bounds.max_x_mm - bounds.min_x_mm)
            .fold(0., f64::max);
        Ok(Self {
            entries,
            max_span_x_mm,
        })
    }
    pub fn query(
        &self,
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        view: BoundsMm,
    ) -> RenderSnapshot {
        let start = self
            .entries
            .partition_point(|(bounds, _, _)| bounds.min_x_mm < view.min_x_mm - self.max_span_x_mm);
        let end = self
            .entries
            .partition_point(|(b, _, _)| b.min_x_mm <= view.max_x_mm);
        let mut candidates: Vec<_> = self.entries[start..end]
            .iter()
            .filter(|(b, layer, _)| {
                b.max_x_mm >= view.min_x_mm
                    && b.min_y_mm <= view.max_y_mm
                    && b.max_y_mm >= view.min_y_mm
                    && layers.iter().any(|l| {
                        l.layer_id == snapshot.layers[*layer].id && l.visible && l.effective_visible
                    })
            })
            .map(|(_, l, o)| (*l, *o))
            .collect();
        // Restore source exposure order after the spatial query, including Clear.
        candidates.sort_unstable();
        let mut result = RenderSnapshot {
            document_id: snapshot.document_id.clone(),
            revision: snapshot.revision.clone(),
            workspace_revision: snapshot.workspace_revision.clone(),
            styles: Vec::new(),
            apertures: snapshot.apertures.clone(),
            block_definitions: snapshot.block_definitions.clone(),
            layers: snapshot
                .layers
                .iter()
                .map(|l| editor_core::SemanticLayer {
                    id: l.id.clone(),
                    objects: vec![],
                })
                .collect(),
        };
        for (l, o) in candidates {
            result.layers[l]
                .objects
                .push(snapshot.layers[l].objects[o].clone());
        }
        result
    }

    /// Object-level neighborhood for Object Snap. This returns only stable
    /// snapshot indices; feature generation remains lazy in the caller.
    pub fn query_indices(&self, view: BoundsMm) -> Vec<(usize, usize)> {
        let start = self
            .entries
            .partition_point(|(bounds, _, _)| bounds.min_x_mm < view.min_x_mm - self.max_span_x_mm);
        let end = self
            .entries
            .partition_point(|(bounds, _, _)| bounds.min_x_mm <= view.max_x_mm);
        self.entries[start..end]
            .iter()
            .filter(|(bounds, _, _)| {
                bounds.max_x_mm >= view.min_x_mm
                    && bounds.min_y_mm <= view.max_y_mm
                    && bounds.max_y_mm >= view.min_y_mm
            })
            .map(|(_, layer, object)| (*layer, *object))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{
        ApertureDefinition, ApertureShape, Exposure, LocalTransform, MmPoint, ObjectOrigin,
        SemanticGeometry, SemanticLayer, SemanticObject,
    };

    #[test]
    fn hundred_thousand_objects_query_only_the_local_neighborhood() {
        let aperture = ApertureDefinition {
            id: "a".into(),
            source_dcode: 10,
            shape: ApertureShape::Circle {
                diameter_mm: 0.2,
                hole_diameter_mm: None,
            },
        };
        let objects = (0..100_000)
            .map(|index| SemanticObject {
                object_id: format!("o-{index}"),
                geometry: SemanticGeometry::Flash {
                    center: MmPoint::new(index as f64, (index % 10) as f64),
                    aperture_id: aperture.id.clone(),
                    transform: LocalTransform::default(),
                },
                exposure: Exposure::Dark,
                origin: ObjectOrigin::Generated {
                    operation_id: "snap-perf".into(),
                },
            })
            .collect();
        let snapshot = RenderSnapshot {
            document_id: "snap-perf".into(),
            revision: "0".into(),
            workspace_revision: "0".into(),
            layers: vec![SemanticLayer {
                id: "layer".into(),
                objects,
            }],
            apertures: vec![aperture],
            styles: vec![],
            block_definitions: vec![],
        };
        let index = WorldIndex::build(&snapshot).unwrap();
        let nearby = index.query_indices(BoundsMm {
            min_x_mm: 49_999.5,
            min_y_mm: -1.,
            max_x_mm: 50_000.5,
            max_y_mm: 10.,
        });
        assert!(nearby.len() <= 2, "nearby={}", nearby.len());
        assert!(nearby.contains(&(0, 50_000)));
    }
}
