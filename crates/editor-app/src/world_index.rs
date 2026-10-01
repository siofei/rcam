//! Cached f64 manufacturing envelopes, queried before any GPU conversion.
use editor_core::BoundsMm;
use editor_service::{LayerInfo, RenderSnapshot};

/// A partial update is permitted only while layer/object identity and reusable
/// definitions remain unchanged. The ordinary full build handles all other edits.
pub fn changed_objects(
    current: &RenderSnapshot,
    previous: &RenderSnapshot,
) -> Option<Vec<(usize, usize)>> {
    if current.document_id != previous.document_id
        || current.apertures != previous.apertures
        || current.block_definitions != previous.block_definitions
        || current.layers.len() != previous.layers.len()
    {
        return None;
    }
    let mut changed = Vec::new();
    for (l, (a, b)) in current.layers.iter().zip(&previous.layers).enumerate() {
        if a.id != b.id || a.objects.len() != b.objects.len() {
            return None;
        }
        for (o, (a, b)) in a.objects.iter().zip(&b.objects).enumerate() {
            if a.object_id != b.object_id {
                return None;
            }
            if a != b {
                changed.push((l, o));
            }
        }
    }
    Some(changed)
}

#[derive(Clone, Default)]
pub struct WorldIndex(editor_core::world_index::WorldIndex);
impl std::ops::Deref for WorldIndex {
    type Target = editor_core::world_index::WorldIndex;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl WorldIndex {
    pub fn build(s: &RenderSnapshot) -> Result<Self, String> {
        editor_core::world_index::WorldIndex::build(&s.layers, &s.apertures, &s.block_definitions)
            .map(Self)
    }
    pub fn update(&self, s: &RenderSnapshot, changed: &[(usize, usize)]) -> Result<Self, String> {
        self.0
            .update(&s.layers, &s.apertures, &s.block_definitions, changed)
            .map(Self)
    }
    pub fn visible_bounds(
        &self,
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
    ) -> Option<BoundsMm> {
        let shapes = editor_core::workspace::aperture_shape_map(&snapshot.apertures);
        let policies: Vec<_> = snapshot
            .layers
            .iter()
            .map(|layer| {
                layers
                    .iter()
                    .find(|l| l.layer_id == layer.id)
                    .map(|l| (l, l.classes.iter().all(|c| c.visible)))
            })
            .collect();
        let mut result: Option<BoundsMm> = None;
        for &(bounds, l, o) in self.entries() {
            let Some((policy, all_visible)) = policies[l] else {
                continue;
            };
            if !policy.visible || !policy.effective_visible {
                continue;
            }
            if !all_visible {
                let class = editor_core::workspace::classify_object(
                    &snapshot.layers[l].objects[o],
                    &shapes,
                );
                if policy
                    .classes
                    .iter()
                    .any(|c| c.class == class && !c.visible)
                {
                    continue;
                }
            }
            result = Some(result.map_or(bounds, |b| b.union(bounds)));
        }
        result
    }

    pub fn query(
        &self,
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        view: BoundsMm,
    ) -> RenderSnapshot {
        let mut candidates: Vec<_> = self
            .query_indices(view)
            .into_iter()
            .filter(|(layer, _)| {
                layers.iter().any(|l| {
                    l.layer_id == snapshot.layers[*layer].id && l.visible && l.effective_visible
                })
            })
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
