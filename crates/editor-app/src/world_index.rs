//! Cached f64 manufacturing envelopes, queried before any GPU conversion.
use editor_core::{BoundsMm, geometries_bounds_with_blocks};
use editor_service::{LayerInfo, RenderSnapshot};

#[derive(Default)]
pub struct WorldIndex(Vec<(BoundsMm, usize, usize)>);
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
        Ok(Self(entries))
    }
    pub fn query(
        &self,
        snapshot: &RenderSnapshot,
        layers: &[LayerInfo],
        view: BoundsMm,
    ) -> RenderSnapshot {
        let end = self
            .0
            .partition_point(|(b, _, _)| b.min_x_mm <= view.max_x_mm);
        let mut candidates: Vec<_> = self.0[..end]
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
}
