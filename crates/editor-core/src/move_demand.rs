//! Conservative incremental resources for the exact Move transaction route.
use super::*;

pub const MAX_MOVE_TARGETS: usize = MAX_EDIT_DOCUMENT_OBJECTS;
pub const MAX_MOVE_WORK_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveDemand {
    pub object_count: usize,
    pub history_bytes: usize,
    pub work_bytes: usize,
    pub history_limit_bytes: usize,
    pub work_limit_bytes: usize,
    pub multi_layer_route: bool,
    pub validation_work: usize,
    pub validation_work_limit: usize,
}
impl MoveDemand {
    pub fn admit(&self) -> Result<(), EditError> {
        if self.object_count > MAX_MOVE_TARGETS
            || self.history_bytes > self.history_limit_bytes
            || self.work_bytes > self.work_limit_bytes
            || self.validation_work > self.validation_work_limit
        {
            Err(EditError::ResourceLimit)
        } else {
            Ok(())
        }
    }
}
fn add(a: usize, b: usize) -> Result<usize, EditError> {
    a.checked_add(b).ok_or(EditError::ResourceLimit)
}
fn mul(a: usize, b: usize) -> Result<usize, EditError> {
    a.checked_mul(b).ok_or(EditError::ResourceLimit)
}
fn heap(g: &SemanticGeometry) -> Result<usize, EditError> {
    match g {
        SemanticGeometry::Flash { aperture_id, .. } => add(aperture_id.len(), 64),
        SemanticGeometry::Region { contours } => {
            let mut n = mul(contours.len(), size_of::<RegionContour>() + 64)?;
            for c in contours {
                n = add(n, add(mul(c.edges.len(), size_of::<RegionEdge>())?, 64)?)?;
            }
            Ok(n)
        }
        _ => Ok(0),
    }
}
fn block_resources(
    document: &SemanticDocument,
    id: &crate::block::BlockDefinitionId,
    checkpoint: &mut impl FnMut() -> Result<(), EditError>,
) -> Result<(usize, usize), EditError> {
    let b = document
        .block_definition(id)
        .ok_or(EditError::InvalidArgument)?;
    let mut bytes = 0usize;
    let mut work = 0usize;
    for (index, o) in b.objects.iter().enumerate() {
        if index % 256 == 0 {
            checkpoint()?;
        }
        use crate::block::BlockObjectGeometry;
        let (heap, edges) = match &o.geometry {
            BlockObjectGeometry::Flash { aperture_id, .. } => (add(aperture_id.len(), 64)?, 0),
            BlockObjectGeometry::Region { contours } => {
                let mut n = mul(contours.len(), size_of::<RegionContour>() + 64)?;
                let mut edges = 0usize;
                for c in contours {
                    n = add(n, add(mul(c.edges.len(), size_of::<RegionEdge>())?, 64)?)?;
                    edges = add(edges, c.edges.len())?;
                }
                (n, edges)
            }
            _ => (0, 0),
        };
        bytes = add(
            bytes,
            add(size_of::<crate::block::ResolvedBlockObject>() + 64, heap)?,
        )?;
        work = add(work, add(1, edges)?)?;
    }
    Ok((bytes, work))
}
impl EditHistory {
    pub fn move_objects_demand(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
    ) -> Result<MoveDemand, EditError> {
        self.move_objects_demand_checked(document, layer_id, object_ids, &mut || Ok(()))
    }
    /// Uses the multi-layer transaction route even for one input group, matching
    /// edit_selection rather than guessing which API its caller will submit.
    pub fn move_selection_demand(
        &self,
        document: &SemanticDocument,
        groups: &[SelectionGroup],
    ) -> Result<MoveDemand, EditError> {
        self.move_selection_demand_checked(document, groups, &mut || Ok(()))
    }
    pub fn move_objects_demand_checked(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<MoveDemand, EditError> {
        checkpoint()?;
        let target = self.targets_with_limit(document, layer_id, object_ids, MAX_MOVE_TARGETS)?;
        self.move_demand_checked(document, &[target], false, checkpoint)
    }
    pub fn move_selection_demand_checked(
        &self,
        document: &SemanticDocument,
        groups: &[SelectionGroup],
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<MoveDemand, EditError> {
        checkpoint()?;
        let targets = self.selection_targets(document, groups, MAX_MOVE_TARGETS)?;
        self.move_demand_checked(document, &targets, true, checkpoint)
    }
    pub(super) fn move_demand_for_targets(
        &self,
        document: &SemanticDocument,
        targets: &[(usize, Vec<usize>)],
        multi: bool,
    ) -> Result<MoveDemand, EditError> {
        self.move_demand_checked(document, targets, multi, &mut || Ok(()))
    }
    fn move_demand_checked(
        &self,
        document: &SemanticDocument,
        targets: &[(usize, Vec<usize>)],
        multi: bool,
        checkpoint: &mut impl FnMut() -> Result<(), EditError>,
    ) -> Result<MoveDemand, EditError> {
        let mut history = size_of::<Transaction>() + 256;
        let mut planning = mul(targets.len(), 256)?;
        let mut count = 0usize;
        let lookup = add(document.layers.len(), document.apertures.len())?;
        let mut validation_work = mul(mul(targets.len(), lookup)?, 4)?;
        let mut largest_block_peak = 0usize;
        let mut block_demand = BTreeMap::new();
        for (layer_index, indices) in targets {
            let layer = &document.layers[*layer_index];
            validation_work = add(validation_work, mul(layer.objects.len(), 4)?)?;
            history = add(
                history,
                if multi {
                    add(size_of::<Transaction>() + 1024, layer.id.len())?
                } else {
                    layer.id.len()
                },
            )?;
            for (n, &i) in indices.iter().enumerate() {
                if n % 256 == 0 {
                    checkpoint()?;
                }
                let o = &layer.objects[i];
                count = add(count, 1)?;
                let geometry = heap(&o.geometry)?;
                validation_work = add(validation_work, 1)?;
                match &o.geometry {
                    SemanticGeometry::Region { contours } => {
                        for c in contours {
                            validation_work = add(validation_work, c.edges.len())?;
                        }
                    }
                    SemanticGeometry::BlockInstance { definition_id, .. } => {
                        if !block_demand.contains_key(definition_id.0.as_str()) {
                            block_demand.insert(
                                definition_id.0.as_str(),
                                block_resources(document, definition_id, checkpoint)?,
                            );
                        }
                        let &(bytes, work) = &block_demand[definition_id.0.as_str()];
                        largest_block_peak = largest_block_peak.max(mul(bytes, 2)?);
                        validation_work = add(
                            validation_work,
                            add(work, document.block_definitions.len())?,
                        )?;
                    }
                    _ => {}
                }
                let charge = if multi {
                    let n = add(
                        size_of::<IndexedObject>() + size_of::<Change>() + 256,
                        geometry,
                    )?;
                    let n = add(
                        add(add(n, o.object_id.len())?, origin_bytes(&o.origin))?,
                        document.id.len(),
                    )?;
                    mul(3, n)?
                } else {
                    add(
                        size_of::<Change>() + 256,
                        add(mul(3, o.object_id.len())?, mul(3, geometry)?)?,
                    )?
                };
                history = add(history, charge)?;
                // Conservative per-target set slots, indices, returned ID vector
                // capacity and IDs retained by the service's result/selection.
                planning = add(planning, add(256, mul(4, o.object_id.len())?)?)?;
            }
        }
        // Validation maps clone aperture/definition IDs; commit temporarily owns
        // a third geometry copy. Existing history charges already include it;
        // another full charge bounds transaction validation/commit overlap.
        for (n, a) in document.apertures.iter().enumerate() {
            if n % 256 == 0 {
                checkpoint()?;
            }
            planning = add(planning, add(128, mul(2, a.id.len())?)?)?;
        }
        for (n, b) in document.block_definitions.iter().enumerate() {
            if n % 256 == 0 {
                checkpoint()?;
            }
            planning = add(planning, add(128, mul(2, b.id.0.len())?)?)?;
        }
        Ok(MoveDemand {
            object_count: count,
            history_bytes: history,
            work_bytes: add(add(mul(history, 2)?, planning)?, largest_block_peak)?,
            history_limit_bytes: self.max_bytes,
            work_limit_bytes: MAX_MOVE_WORK_BYTES,
            multi_layer_route: multi,
            validation_work,
            validation_work_limit: MAX_EDIT_REGION_EDGES,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_arithmetic_and_work_budget_cannot_wrap_into_admission() {
        assert_eq!(add(usize::MAX, 1), Err(EditError::ResourceLimit));
        assert_eq!(mul(usize::MAX, 2), Err(EditError::ResourceLimit));
        let d = MoveDemand {
            object_count: 1,
            history_bytes: 1,
            work_bytes: MAX_MOVE_WORK_BYTES + 1,
            history_limit_bytes: MAX_HISTORY_BYTES,
            work_limit_bytes: MAX_MOVE_WORK_BYTES,
            multi_layer_route: false,
            validation_work: 1,
            validation_work_limit: MAX_EDIT_REGION_EDGES,
        };
        assert_eq!(d.admit(), Err(EditError::ResourceLimit));
    }
}
