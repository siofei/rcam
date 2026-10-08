//! A bounded, non-associative insertion into an ordered exposure stream.
use super::*;

/// Advisory total cell count, including the unchanged source cell (0, 0).
/// This is not a core count ceiling; the GUI may ask before a larger commit.
pub const ARRAY_CELL_WARNING_THRESHOLD: usize = 500_000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RectangularArray {
    pub rows: u64,
    pub columns: u64,
    pub pitch_x_mm: f64,
    pub pitch_y_mm: f64,
}

impl RectangularArray {
    pub fn cell_count(self) -> Result<usize, EditError> {
        if self.rows == 0
            || self.columns == 0
            || !self.pitch_x_mm.is_finite()
            || !self.pitch_y_mm.is_finite()
            || (self.columns > 1 && self.pitch_x_mm == 0.0)
            || (self.rows > 1 && self.pitch_y_mm == 0.0)
        {
            return Err(EditError::InvalidArgument);
        }
        let cells = self
            .rows
            .checked_mul(self.columns)
            .ok_or(EditError::ResourceLimit)?;
        let cells = usize::try_from(cells).map_err(|_| EditError::ResourceLimit)?;
        self.offset(cells - 1)?;
        Ok(cells)
    }

    /// Shared by the numeric preview and commit. Index zero is the unchanged source.
    pub fn offset(self, cell: usize) -> Result<MmPoint, EditError> {
        if self.columns == 0 {
            return Err(EditError::InvalidArgument);
        }
        let point = MmPoint::new(
            (cell as u64 % self.columns) as f64 * self.pitch_x_mm,
            (cell as u64 / self.columns) as f64 * self.pitch_y_mm,
        );
        if !point.is_valid_geometry() {
            return Err(EditError::InvalidArgument);
        }
        Ok(point)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayEstimate {
    pub cell_count: usize,
    pub source_object_count: usize,
    pub created_object_count: usize,
    pub added_region_edges: usize,
    pub history_bytes: usize,
}

fn add(a: usize, b: usize) -> Result<usize, EditError> {
    a.checked_add(b).ok_or(EditError::ResourceLimit)
}
fn mul(a: usize, b: usize) -> Result<usize, EditError> {
    a.checked_mul(b).ok_or(EditError::ResourceLimit)
}

impl EditHistory {
    fn array_plan(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        ids: &[String],
        spec: RectangularArray,
        enforce_resources: bool,
    ) -> Result<(usize, Vec<usize>, ArrayEstimate), EditError> {
        let cells = spec.cell_count()?;
        let (layer_index, selected) = self.targets(document, layer_id, ids)?;
        let layer = &document.layers[layer_index];
        if selected.windows(2).any(|w| w[1] != w[0] + 1) {
            return Err(EditError::InvalidArgument);
        }
        let text_ops: HashSet<_> = selected
            .iter()
            .filter_map(|&i| match &layer.objects[i].origin {
                ObjectOrigin::GeneratedText { operation_id } => Some(operation_id.as_str()),
                _ => None,
            })
            .collect();
        let span = selected[0]..=selected[selected.len() - 1];
        if layer.objects.iter().enumerate().any(|(i, object)| {
            matches!(&object.origin, ObjectOrigin::GeneratedText { operation_id }
                if text_ops.contains(operation_id.as_str()) && !span.contains(&i))
        }) {
            return Err(EditError::InvalidArgument);
        }
        let created = mul(selected.len(), cells - 1)?;
        let mut estimate = ArrayEstimate {
            cell_count: cells,
            source_object_count: selected.len(),
            created_object_count: created,
            added_region_edges: 0,
            history_bytes: 0,
        };
        if created == 0 {
            return Ok((layer_index, selected, estimate));
        }
        self.next_generated_id
            .checked_add(created as u64)
            .ok_or(EditError::ResourceLimit)?;
        let all = document.layers.iter().flat_map(|l| &l.objects);
        let mut total = 0;
        let mut edges = 0;
        for object in all {
            total = add(total, 1)?;
            edges = add(edges, region_edges(&object.geometry))?;
        }
        let mut source_edges = 0;
        let mut per_cell_bytes = 0;
        for &i in &selected {
            let object = &layer.objects[i];
            source_edges = add(source_edges, region_edges(&object.geometry))?;
            // Existing structural-history accounting includes live/cloned objects,
            // both exact order guards and peak merge buffers; IDs use bounded suffixes.
            let bytes = add(
                size_of::<IndexedObject>(),
                geometry_heap_bytes(&object.geometry),
            )?;
            let bytes =
                if let SemanticGeometry::BlockInstance { definition_id, .. } = &object.geometry {
                    add(bytes, definition_id.0.len())?
                } else {
                    bytes
                };
            let bytes = add(bytes, mul(document.id.len(), 2)?)?;
            per_cell_bytes = add(per_cell_bytes, mul(add(bytes, 512)?, 3)?)?;
        }
        estimate.added_region_edges = mul(source_edges, cells - 1)?;
        if enforce_resources
            && (add(total, created)? > MAX_EDIT_DOCUMENT_OBJECTS
                || add(edges, estimate.added_region_edges)? > MAX_EDIT_REGION_EDGES)
        {
            return Err(EditError::ResourceLimit);
        }
        let mut bytes = add(size_of::<Transaction>() + 1024, layer_id.len())?;
        for object in &layer.objects {
            bytes = add(
                bytes,
                add(
                    mul(add(object.object_id.len(), 96)?, 4)?,
                    size_of::<SemanticObject>(),
                )?,
            )?;
        }
        bytes = add(bytes, mul(per_cell_bytes, cells - 1)?)?;
        if enforce_resources {
            self.budget(bytes)?;
        }
        estimate.history_bytes = bytes;
        Ok((layer_index, selected, estimate))
    }

    /// Resource/selection preflight for the UI; it never allocates copied geometry.
    pub fn estimate_array_rectangular(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        ids: &[String],
        spec: RectangularArray,
    ) -> Result<ArrayEstimate, EditError> {
        self.array_plan(document, layer_id, ids, spec, true)
            .map(|(_, _, estimate)| estimate)
    }

    /// Read-only resource demand for explaining a failed preflight. This never
    /// authorizes a commit or bypasses checked arithmetic and selection checks.
    pub fn array_resource_requirements(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        ids: &[String],
        spec: RectangularArray,
    ) -> Result<ArrayEstimate, EditError> {
        self.array_plan(document, layer_id, ids, spec, false)
            .map(|(_, _, estimate)| estimate)
    }

    pub fn array_rectangular_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        ids: &[String],
        spec: RectangularArray,
    ) -> Result<Vec<String>, EditError> {
        let (layer_index, selected, estimate) =
            self.array_plan(document, layer_id, ids, spec, true)?;
        if estimate.created_object_count == 0 {
            return Ok(vec![]);
        }
        let next = self
            .next_generated_id
            .checked_add(estimate.created_object_count as u64)
            .ok_or(EditError::ResourceLimit)?;
        let layer = &document.layers[layer_index];
        let insertion = selected[selected.len() - 1] + 1;
        let aperture_ids = document.apertures.iter().map(|a| a.id.clone()).collect();
        let block_ids = block_definition_ids(document);
        let existing: HashSet<_> = document
            .layers
            .iter()
            .flat_map(|l| &l.objects)
            .map(|o| o.object_id.as_str())
            .collect();
        let mut entries = Vec::with_capacity(estimate.created_object_count);
        for cell in 1..estimate.cell_count {
            let offset = spec.offset(cell)?;
            let cell_id = self.next_generated_id + entries.len() as u64;
            let ordinary_op = format!("{}-generated-op-{cell_id}", document.id);
            let mut text_ops = BTreeMap::new();
            for &index in &selected {
                let serial = self.next_generated_id + entries.len() as u64;
                let mut object = layer.objects[index].clone();
                object.object_id = format!("{}-generated-object-{serial}", document.id);
                if existing.contains(object.object_id.as_str()) {
                    return Err(EditError::InvalidArgument);
                }
                object.origin = match &object.origin {
                    ObjectOrigin::GeneratedText { operation_id } => ObjectOrigin::GeneratedText {
                        operation_id: text_ops
                            .entry(operation_id.clone())
                            .or_insert_with(|| format!("{}-array-text-op-{serial}", document.id))
                            .clone(),
                    },
                    _ => ObjectOrigin::Generated {
                        operation_id: ordinary_op.clone(),
                    },
                };
                translate(&mut object.geometry, offset.x_mm, offset.y_mm)?;
                validate_geometry(&object.geometry, &aperture_ids, &block_ids)
                    .map_err(EditError::InvalidGeometry)?;
                validate_block_resolution(document, &object.geometry)?;
                entries.push(IndexedObject {
                    index: insertion + entries.len(),
                    object,
                });
            }
        }
        let before_order: Vec<_> = layer.objects.iter().map(|o| o.object_id.clone()).collect();
        let mut after_order = before_order.clone();
        after_order.splice(
            insertion..insertion,
            entries.iter().map(|e| e.object.object_id.clone()),
        );
        let tx = Transaction {
            layer_id: layer_id.into(),
            layer: layer_index,
            operation: Operation::Insert(entries),
            before_order,
            after_order,
            bytes: estimate.history_bytes,
        };
        check_transaction(document, &tx, true)?;
        let ids = self.commit(document, tx);
        self.next_generated_id = next;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_id_overflow_is_atomic() {
        let mut document = SemanticDocument {
            id: "id-overflow".into(),
            unit: "mm".into(),
            format: SemanticFormat {
                integer: 4,
                decimal: 6,
                leading_zero_omission: true,
                absolute: true,
            },
            layers: vec![SemanticLayer {
                id: "layer".into(),
                objects: vec![SemanticObject {
                    object_id: "source".into(),
                    origin: ObjectOrigin::Imported { command_index: 0 },
                    exposure: Exposure::Dark,
                    geometry: SemanticGeometry::Line {
                        start: MmPoint::new(0., 0.),
                        end: MmPoint::new(1., 0.),
                        width_mm: 1.,
                    },
                }],
            }],
            apertures: vec![],
            source: SourceMetadata::default(),
            block_definitions: vec![],
        };
        let mut history = EditHistory {
            next_generated_id: u64::MAX,
            ..EditHistory::default()
        };
        let before = document.clone();
        assert_eq!(
            history.array_rectangular_objects(
                &mut document,
                "layer",
                &["source".into()],
                RectangularArray {
                    rows: 1,
                    columns: 2,
                    pitch_x_mm: 1.,
                    pitch_y_mm: 0.
                }
            ),
            Err(EditError::ResourceLimit)
        );
        assert_eq!(document, before);
        assert_eq!(history.next_generated_id, u64::MAX);
        assert_eq!(history.undo_len(), 0);
    }
}
