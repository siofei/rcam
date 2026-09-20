//! Atomic, bounded object history. No file I/O or UI state enters a command.
use super::*;
use std::collections::{BTreeMap, HashSet};
use std::mem::size_of;

/// World axes: horizontal y=coordinate_mm, vertical x=coordinate_mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MirrorAxis {
    Horizontal { coordinate_mm: f64 },
    Vertical { coordinate_mm: f64 },
}

pub const MAX_MOVE_OBJECTS: usize = 10_000;
pub const MAX_HISTORY_ENTRIES: usize = 100;
pub const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_EDIT_DOCUMENT_OBJECTS: usize = 500_000;
pub const MAX_EDIT_REGION_EDGES: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    InvalidArgument,
    UnsupportedTransform,
    NotFound { entity: &'static str, id: String },
    ResourceLimit,
    EmptyHistory,
    InvalidGeometry(SemanticError),
}

#[derive(Debug, Clone)]
struct Change {
    object_id: String,
    index: usize,
    before: SemanticGeometry,
    after: SemanticGeometry,
}

#[derive(Debug, Clone)]
struct IndexedObject {
    index: usize,
    object: SemanticObject,
}

#[derive(Debug, Clone)]
struct ApertureResize {
    index: usize,
    definition: ApertureDefinition,
    changes: Vec<Change>,
}

#[derive(Debug, Clone)]
struct BatchChange {
    changes: Vec<Change>,
    inserted_apertures: Vec<(usize, ApertureDefinition)>,
}

#[derive(Debug, Clone)]
pub enum BatchEdit {
    Move {
        object_ids: Vec<String>,
        dx_mm: f64,
        dy_mm: f64,
    },
    Rotate {
        object_ids: Vec<String>,
        angle_deg: f64,
        pivot: MmPoint,
    },
    Mirror {
        object_ids: Vec<String>,
        axis: MirrorAxis,
    },
    SetFlashSize {
        object_ids: Vec<String>,
        width_mm: f64,
        height_mm: Option<f64>,
    },
}

#[derive(Debug, Clone)]
enum Operation {
    Modify(Vec<Change>),
    Insert(Vec<IndexedObject>),
    Delete(Vec<IndexedObject>),
    ApertureResize(ApertureResize),
    Batch(BatchChange),
}

#[derive(Debug, Clone)]
struct Transaction {
    layer_id: String,
    layer: usize,
    operation: Operation,
    // TODO ADR 0011: O(N) order guards must be compacted before large-file editing.
    // Exact order guard for structural edits, without cloning document geometry.
    before_order: Vec<String>,
    after_order: Vec<String>,
    bytes: usize,
}

#[derive(Debug, Clone)]
pub struct EditHistory {
    document_id: Option<String>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    // Consumed only at successful insert commit, never rewound by Undo/Delete.
    next_generated_id: u64,
    next_generated_aperture_id: u64,
    max_entries: usize,
    max_bytes: usize,
    truncated_entries: usize,
    truncated_bytes: usize,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::with_limits(MAX_HISTORY_ENTRIES, MAX_HISTORY_BYTES)
            .expect("default history limits are valid")
    }
}

impl EditHistory {
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Result<Self, EditError> {
        if max_entries == 0 || max_bytes == 0 {
            return Err(EditError::InvalidArgument);
        }
        Ok(Self {
            document_id: None,
            undo: vec![],
            redo: vec![],
            next_generated_id: 0,
            next_generated_aperture_id: 0,
            max_entries,
            max_bytes,
            truncated_entries: 0,
            truncated_bytes: 0,
        })
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(|tx| tx.bytes).sum()
    }

    pub fn truncated_entries(&self) -> usize {
        self.truncated_entries
    }

    pub fn truncated_bytes(&self) -> usize {
        self.truncated_bytes
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    pub fn next_undo_changes_shape(&self) -> bool {
        self.undo
            .last()
            .is_some_and(|tx| operation_changes_shape(&tx.operation))
    }

    pub fn next_redo_changes_shape(&self) -> bool {
        self.redo
            .last()
            .is_some_and(|tx| operation_changes_shape(&tx.operation))
    }

    fn targets(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
    ) -> Result<(usize, Vec<usize>), EditError> {
        if self
            .document_id
            .as_deref()
            .is_some_and(|id| id != document.id)
            || object_ids.is_empty()
        {
            return Err(EditError::InvalidArgument);
        }
        if object_ids.len() > MAX_MOVE_OBJECTS {
            return Err(EditError::ResourceLimit);
        }
        let targets: HashSet<_> = object_ids.iter().map(String::as_str).collect();
        if targets.len() != object_ids.len() {
            return Err(EditError::InvalidArgument);
        }
        let layer_index = document
            .layers
            .iter()
            .position(|l| l.id == layer_id)
            .ok_or_else(|| EditError::NotFound {
                entity: "layer",
                id: layer_id.into(),
            })?;
        let layer = &document.layers[layer_index];
        let selected: Vec<_> = layer
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| targets.contains(o.object_id.as_str()))
            .map(|(i, _)| i)
            .collect();
        if selected.len() != targets.len() {
            let found: HashSet<_> = selected
                .iter()
                .map(|&i| layer.objects[i].object_id.as_str())
                .collect();
            return Err(EditError::NotFound {
                entity: "object",
                id: object_ids
                    .iter()
                    .find(|id| !found.contains(id.as_str()))
                    .unwrap()
                    .clone(),
            });
        }
        Ok((layer_index, selected))
    }

    fn budget(&self, bytes: usize) -> Result<(), EditError> {
        if bytes > self.max_bytes {
            return Err(EditError::ResourceLimit);
        }
        Ok(())
    }

    fn commit(&mut self, document: &mut SemanticDocument, tx: Transaction) -> Vec<String> {
        let ids = apply(document, &tx, true);
        self.document_id = Some(document.id.clone());
        self.redo.clear();
        self.undo.push(tx);
        while self.undo.len() > self.max_entries || self.bytes() > self.max_bytes {
            let evicted = self.undo.remove(0);
            self.truncated_entries += 1;
            self.truncated_bytes = self.truncated_bytes.saturating_add(evicted.bytes);
        }
        ids
    }

    pub fn move_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        dx_mm: f64,
        dy_mm: f64,
    ) -> Result<Vec<String>, EditError> {
        if !MmPoint::new(dx_mm, dy_mm).is_valid_geometry() || (dx_mm == 0.0 && dy_mm == 0.0) {
            return Err(EditError::InvalidArgument);
        }
        self.modify_objects(document, layer_id, object_ids, |geometry| {
            translate(geometry, dx_mm, dy_mm)
        })
    }

    pub fn rotate_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        angle_deg: f64,
        pivot: MmPoint,
    ) -> Result<Vec<String>, EditError> {
        let transform = super::transform::WorldTransform::rotation(angle_deg, pivot)?;
        self.modify_objects(document, layer_id, object_ids, |geometry| {
            transform.apply(geometry)
        })
    }

    pub fn mirror_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        axis: MirrorAxis,
    ) -> Result<Vec<String>, EditError> {
        let transform = super::transform::WorldTransform::reflection(axis)?;
        self.modify_objects(document, layer_id, object_ids, |geometry| {
            transform.apply(geometry)
        })
    }

    fn modify_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        modify: impl Fn(&mut SemanticGeometry) -> Result<(), EditError>,
    ) -> Result<Vec<String>, EditError> {
        let (layer_index, selected) = self.targets(document, layer_id, object_ids)?;
        let layer = &document.layers[layer_index];
        let bytes = size_of::<Transaction>()
            + 256
            + layer_id.len()
            + selected
                .iter()
                .map(|&i| {
                    let object = &layer.objects[i];
                    size_of::<Change>()
                        + 256
                        + 3 * object.object_id.len()
                        + 3 * geometry_heap_bytes(&object.geometry)
                })
                .sum::<usize>();
        self.budget(bytes)?;
        let aperture_ids = document.apertures.iter().map(|a| a.id.clone()).collect();
        let mut changes = Vec::with_capacity(selected.len());
        for index in selected {
            let object = &layer.objects[index];
            let mut after = object.geometry.clone();
            modify(&mut after)?;
            validate_geometry(&after, &aperture_ids).map_err(EditError::InvalidGeometry)?;
            changes.push(Change {
                object_id: object.object_id.clone(),
                index,
                before: object.geometry.clone(),
                after,
            });
        }
        if changes.iter().all(|c| c.before == c.after) {
            return Err(EditError::InvalidArgument);
        }
        Ok(self.commit(
            document,
            Transaction {
                layer_id: layer_id.into(),
                layer: layer_index,
                operation: Operation::Modify(changes),
                before_order: vec![],
                after_order: vec![],
                bytes,
            },
        ))
    }

    pub fn duplicate_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        dx_mm: f64,
        dy_mm: f64,
    ) -> Result<Vec<String>, EditError> {
        if !MmPoint::new(dx_mm, dy_mm).is_valid_geometry() {
            return Err(EditError::InvalidArgument);
        }
        self.structural_edit(document, layer_id, object_ids, Some((dx_mm, dy_mm)))
    }

    pub fn delete_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
    ) -> Result<Vec<String>, EditError> {
        self.structural_edit(document, layer_id, object_ids, None)
    }

    pub fn set_flash_size(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        width_mm: f64,
        height_mm: Option<f64>,
    ) -> Result<Vec<String>, EditError> {
        let (layer_index, selected) = self.targets(document, layer_id, object_ids)?;
        let layer = &document.layers[layer_index];
        let aperture_id = selected
            .iter()
            .map(|&index| match &layer.objects[index].geometry {
                SemanticGeometry::Flash { aperture_id, .. } => Ok(aperture_id.as_str()),
                _ => Err(EditError::UnsupportedTransform),
            })
            .collect::<Result<HashSet<_>, _>>()?;
        if aperture_id.len() != 1 {
            return Err(EditError::InvalidArgument);
        }
        let old_id = *aperture_id.iter().next().unwrap();
        let old = document
            .apertures
            .iter()
            .find(|aperture| aperture.id == old_id)
            .ok_or_else(|| EditError::NotFound {
                entity: "aperture",
                id: old_id.into(),
            })?;
        let shape = resized_shape(&old.shape, width_mm, height_mm)?;
        super::validate_aperture_shape(&shape).map_err(EditError::InvalidGeometry)?;
        if shape == old.shape {
            return Err(EditError::InvalidArgument);
        }
        let generated = self.next_generated_aperture_id;
        let definition = ApertureDefinition {
            id: format!("{}-generated-aperture-{generated}", document.id),
            source_dcode: document
                .apertures
                .iter()
                .map(|aperture| aperture.source_dcode)
                .max()
                .unwrap_or(9)
                .checked_add(1)
                .ok_or(EditError::ResourceLimit)?,
            shape,
        };
        if document
            .apertures
            .iter()
            .any(|aperture| aperture.id == definition.id)
        {
            return Err(EditError::InvalidArgument);
        }
        let mut changes = Vec::with_capacity(selected.len());
        for &index in &selected {
            let object = &layer.objects[index];
            let mut after = object.geometry.clone();
            let SemanticGeometry::Flash { aperture_id, .. } = &mut after else {
                unreachable!()
            };
            *aperture_id = definition.id.clone();
            changes.push(Change {
                object_id: object.object_id.clone(),
                index,
                before: object.geometry.clone(),
                after,
            });
        }
        let bytes = size_of::<Transaction>()
            + size_of::<ApertureResize>()
            + 512
            + layer_id.len()
            + aperture_shape_heap_bytes(&definition.shape)
            + changes
                .iter()
                .map(|change| {
                    size_of::<Change>()
                        + 3 * change.object_id.len()
                        + geometry_heap_bytes(&change.before)
                        + geometry_heap_bytes(&change.after)
                })
                .sum::<usize>();
        self.budget(bytes)?;
        let ids = self.commit(
            document,
            Transaction {
                layer_id: layer_id.into(),
                layer: layer_index,
                operation: Operation::ApertureResize(ApertureResize {
                    index: document.apertures.len(),
                    definition,
                    changes,
                }),
                before_order: vec![],
                after_order: vec![],
                bytes,
            },
        );
        self.next_generated_aperture_id =
            generated.checked_add(1).ok_or(EditError::ResourceLimit)?;
        Ok(ids)
    }

    pub fn edit_batch(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        steps: &[BatchEdit],
    ) -> Result<Vec<String>, EditError> {
        if steps.is_empty() || steps.len() > MAX_MOVE_OBJECTS {
            return Err(EditError::InvalidArgument);
        }
        let layer_index = document
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or_else(|| EditError::NotFound {
                entity: "layer",
                id: layer_id.into(),
            })?;
        let mut working = BTreeMap::<usize, SemanticGeometry>::new();
        let mut step_indices = Vec::with_capacity(steps.len());
        for step in steps {
            let ids = match step {
                BatchEdit::Move { object_ids, .. }
                | BatchEdit::Rotate { object_ids, .. }
                | BatchEdit::Mirror { object_ids, .. }
                | BatchEdit::SetFlashSize { object_ids, .. } => object_ids,
            };
            let (step_layer, indices) = self.targets(document, layer_id, ids)?;
            if step_layer != layer_index {
                return Err(EditError::InvalidArgument);
            }
            for &index in &indices {
                working.entry(index).or_insert_with(|| {
                    document.layers[layer_index].objects[index].geometry.clone()
                });
            }
            step_indices.push(indices);
        }
        let mut apertures = document.apertures.clone();
        let mut inserted_apertures = Vec::new();
        let mut generated = self.next_generated_aperture_id;
        for (step, indices) in steps.iter().zip(&step_indices) {
            match step {
                BatchEdit::Move { dx_mm, dy_mm, .. } => {
                    if !MmPoint::new(*dx_mm, *dy_mm).is_valid_geometry()
                        || (*dx_mm == 0. && *dy_mm == 0.)
                    {
                        return Err(EditError::InvalidArgument);
                    }
                    for index in indices {
                        translate(working.get_mut(index).unwrap(), *dx_mm, *dy_mm)?;
                    }
                }
                BatchEdit::Rotate {
                    angle_deg, pivot, ..
                } => {
                    let transform = super::transform::WorldTransform::rotation(*angle_deg, *pivot)?;
                    for index in indices {
                        transform.apply(working.get_mut(index).unwrap())?;
                    }
                }
                BatchEdit::Mirror { axis, .. } => {
                    let transform = super::transform::WorldTransform::reflection(*axis)?;
                    for index in indices {
                        transform.apply(working.get_mut(index).unwrap())?;
                    }
                }
                BatchEdit::SetFlashSize {
                    width_mm,
                    height_mm,
                    ..
                } => {
                    let aperture_ids = indices
                        .iter()
                        .map(|index| match working.get(index).unwrap() {
                            SemanticGeometry::Flash { aperture_id, .. } => Ok(aperture_id.clone()),
                            _ => Err(EditError::UnsupportedTransform),
                        })
                        .collect::<Result<HashSet<_>, _>>()?;
                    if aperture_ids.len() != 1 {
                        return Err(EditError::InvalidArgument);
                    }
                    let old_id = aperture_ids.iter().next().unwrap();
                    let old = apertures
                        .iter()
                        .find(|aperture| aperture.id == *old_id)
                        .ok_or_else(|| EditError::NotFound {
                            entity: "aperture",
                            id: old_id.clone(),
                        })?;
                    let shape = resized_shape(&old.shape, *width_mm, *height_mm)?;
                    super::validate_aperture_shape(&shape).map_err(EditError::InvalidGeometry)?;
                    if shape == old.shape {
                        return Err(EditError::InvalidArgument);
                    }
                    let definition = ApertureDefinition {
                        id: format!("{}-generated-aperture-{generated}", document.id),
                        source_dcode: apertures
                            .iter()
                            .map(|aperture| aperture.source_dcode)
                            .max()
                            .unwrap_or(9)
                            .checked_add(1)
                            .ok_or(EditError::ResourceLimit)?,
                        shape,
                    };
                    if apertures
                        .iter()
                        .any(|aperture| aperture.id == definition.id)
                    {
                        return Err(EditError::InvalidArgument);
                    }
                    generated = generated.checked_add(1).ok_or(EditError::ResourceLimit)?;
                    let aperture_index = apertures.len();
                    for index in indices {
                        let SemanticGeometry::Flash { aperture_id, .. } =
                            working.get_mut(index).unwrap()
                        else {
                            unreachable!()
                        };
                        *aperture_id = definition.id.clone();
                    }
                    apertures.push(definition.clone());
                    inserted_apertures.push((aperture_index, definition));
                }
            }
        }
        let aperture_ids = apertures
            .iter()
            .map(|aperture| aperture.id.clone())
            .collect();
        let mut changes = Vec::with_capacity(working.len());
        for (index, after) in working {
            validate_geometry(&after, &aperture_ids).map_err(EditError::InvalidGeometry)?;
            let object = &document.layers[layer_index].objects[index];
            if object.geometry != after {
                changes.push(Change {
                    object_id: object.object_id.clone(),
                    index,
                    before: object.geometry.clone(),
                    after,
                });
            }
        }
        if changes.is_empty() {
            return Err(EditError::InvalidArgument);
        }
        let bytes = size_of::<Transaction>()
            + size_of::<BatchChange>()
            + 1024
            + layer_id.len()
            + changes
                .iter()
                .map(|change| {
                    size_of::<Change>()
                        + 3 * change.object_id.len()
                        + geometry_heap_bytes(&change.before)
                        + geometry_heap_bytes(&change.after)
                })
                .sum::<usize>()
            + inserted_apertures
                .iter()
                .map(|(_, aperture)| {
                    size_of::<ApertureDefinition>()
                        + aperture.id.len()
                        + aperture_shape_heap_bytes(&aperture.shape)
                })
                .sum::<usize>();
        self.budget(bytes)?;
        let ids = self.commit(
            document,
            Transaction {
                layer_id: layer_id.into(),
                layer: layer_index,
                operation: Operation::Batch(BatchChange {
                    changes,
                    inserted_apertures,
                }),
                before_order: vec![],
                after_order: vec![],
                bytes,
            },
        );
        self.next_generated_aperture_id = generated;
        Ok(ids)
    }

    fn structural_edit(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        offset: Option<(f64, f64)>,
    ) -> Result<Vec<String>, EditError> {
        let (layer_index, selected) = self.targets(document, layer_id, object_ids)?;
        let layer = &document.layers[layer_index];
        let inserting = offset.is_some();
        let next_id = if inserting {
            self.next_generated_id
                .checked_add(selected.len() as u64)
                .ok_or(EditError::ResourceLimit)?
        } else {
            self.next_generated_id
        };
        if inserting {
            let total: usize = document.layers.iter().map(|l| l.objects.len()).sum();
            let edges: usize = document
                .layers
                .iter()
                .flat_map(|l| &l.objects)
                .map(|o| region_edges(&o.geometry))
                .sum();
            let added_edges: usize = selected
                .iter()
                .map(|&i| region_edges(&layer.objects[i].geometry))
                .sum();
            if total.saturating_add(selected.len()) > MAX_EDIT_DOCUMENT_OBJECTS
                || edges.saturating_add(added_edges) > MAX_EDIT_REGION_EDGES
            {
                return Err(EditError::ResourceLimit);
            }
        }
        // Includes both order guards, temporary ordering/ID checks, flat object
        // merge buffer and cloned selected objects at commit/Undo peak.
        let bytes = size_of::<Transaction>()
            + 1024
            + layer_id.len()
            + layer
                .objects
                .iter()
                .map(|o| 4 * (o.object_id.len() + 96) + size_of::<SemanticObject>())
                .sum::<usize>()
            + selected
                .iter()
                .map(|&i| {
                    let o = &layer.objects[i];
                    3 * (size_of::<IndexedObject>()
                        + geometry_heap_bytes(&o.geometry)
                        + o.object_id.len()
                        + origin_bytes(&o.origin)
                        + document.id.len()
                        + 256)
                })
                .sum::<usize>();
        self.budget(bytes)?;
        let aperture_ids = document.apertures.iter().map(|a| a.id.clone()).collect();
        let mut entries = Vec::with_capacity(selected.len());
        let operation_id = format!("{}-generated-op-{}", document.id, self.next_generated_id);
        for (n, &index) in selected.iter().enumerate() {
            let mut object = layer.objects[index].clone();
            let output_index = if let Some((dx, dy)) = offset {
                object.object_id = format!(
                    "{}-generated-object-{}",
                    document.id,
                    self.next_generated_id + n as u64
                );
                object.origin = ObjectOrigin::Generated {
                    operation_id: operation_id.clone(),
                };
                translate(&mut object.geometry, dx, dy)?;
                validate_geometry(&object.geometry, &aperture_ids)
                    .map_err(EditError::InvalidGeometry)?;
                index + n + 1
            } else {
                index
            };
            entries.push(IndexedObject {
                index: output_index,
                object,
            });
        }
        if inserting {
            let new_ids: HashSet<_> = entries
                .iter()
                .map(|e| e.object.object_id.as_str())
                .collect();
            if document
                .layers
                .iter()
                .flat_map(|l| &l.objects)
                .any(|o| new_ids.contains(o.object_id.as_str()))
            {
                return Err(EditError::InvalidArgument);
            }
        }
        let before_order: Vec<_> = layer.objects.iter().map(|o| o.object_id.clone()).collect();
        let mut after_order = Vec::with_capacity(before_order.len() + entries.len());
        let mut targets = selected.iter().enumerate().peekable();
        for (i, id) in before_order.iter().enumerate() {
            let target = targets.peek().is_some_and(|(_, index)| **index == i);
            if !target || inserting {
                after_order.push(id.clone());
            }
            if target {
                let (n, _) = targets.next().unwrap();
                if inserting {
                    after_order.push(entries[n].object.object_id.clone());
                }
            }
        }
        let operation = if inserting {
            Operation::Insert(entries)
        } else {
            Operation::Delete(entries)
        };
        let tx = Transaction {
            layer_id: layer_id.into(),
            layer: layer_index,
            operation,
            before_order,
            after_order,
            bytes,
        };
        check_transaction(document, &tx, true)?;
        let ids = self.commit(document, tx);
        self.next_generated_id = next_id;
        Ok(ids)
    }

    pub fn undo(&mut self, document: &mut SemanticDocument) -> Result<Vec<String>, EditError> {
        if self.document_id.as_deref() != Some(document.id.as_str()) {
            return Err(EditError::EmptyHistory);
        }
        let tx = self.undo.last().ok_or(EditError::EmptyHistory)?;
        check_transaction(document, tx, false)?;
        let ids = apply(document, tx, false);
        self.redo.push(self.undo.pop().unwrap());
        Ok(ids)
    }
    pub fn redo(&mut self, document: &mut SemanticDocument) -> Result<Vec<String>, EditError> {
        if self.document_id.as_deref() != Some(document.id.as_str()) {
            return Err(EditError::EmptyHistory);
        }
        let tx = self.redo.last().ok_or(EditError::EmptyHistory)?;
        check_transaction(document, tx, true)?;
        let ids = apply(document, tx, true);
        self.undo.push(self.redo.pop().unwrap());
        Ok(ids)
    }
}

fn check_transaction(
    document: &SemanticDocument,
    tx: &Transaction,
    forward: bool,
) -> Result<(), EditError> {
    let layer = document
        .layers
        .get(tx.layer)
        .filter(|l| l.id == tx.layer_id)
        .ok_or(EditError::InvalidArgument)?;
    match &tx.operation {
        Operation::Modify(changes) => {
            for c in changes {
                let o = layer
                    .objects
                    .get(c.index)
                    .filter(|o| o.object_id == c.object_id)
                    .ok_or(EditError::InvalidArgument)?;
                if o.geometry != *if forward { &c.before } else { &c.after } {
                    return Err(EditError::InvalidArgument);
                }
            }
        }
        Operation::Insert(entries) | Operation::Delete(entries) => {
            let order = if forward {
                &tx.before_order
            } else {
                &tx.after_order
            };
            if !layer.objects.iter().map(|o| &o.object_id).eq(order.iter()) {
                return Err(EditError::InvalidArgument);
            }
            let inserting = matches!(tx.operation, Operation::Insert(_)) == forward;
            if !inserting {
                for entry in entries {
                    if layer.objects.get(entry.index) != Some(&entry.object) {
                        return Err(EditError::InvalidArgument);
                    }
                }
            } else {
                let ids: HashSet<_> = entries
                    .iter()
                    .map(|e| e.object.object_id.as_str())
                    .collect();
                if document
                    .layers
                    .iter()
                    .flat_map(|l| &l.objects)
                    .any(|o| ids.contains(o.object_id.as_str()))
                {
                    return Err(EditError::InvalidArgument);
                }
            }
        }
        Operation::ApertureResize(resize) => {
            for change in &resize.changes {
                let object = layer
                    .objects
                    .get(change.index)
                    .filter(|object| object.object_id == change.object_id)
                    .ok_or(EditError::InvalidArgument)?;
                if object.geometry
                    != *if forward {
                        &change.before
                    } else {
                        &change.after
                    }
                {
                    return Err(EditError::InvalidArgument);
                }
            }
            if forward {
                if document.apertures.iter().any(|aperture| {
                    aperture.id == resize.definition.id
                        || aperture.source_dcode == resize.definition.source_dcode
                }) {
                    return Err(EditError::InvalidArgument);
                }
            } else if document.apertures.get(resize.index) != Some(&resize.definition) {
                return Err(EditError::InvalidArgument);
            }
        }
        Operation::Batch(batch) => {
            for change in &batch.changes {
                let object = layer
                    .objects
                    .get(change.index)
                    .filter(|object| object.object_id == change.object_id)
                    .ok_or(EditError::InvalidArgument)?;
                if object.geometry
                    != *if forward {
                        &change.before
                    } else {
                        &change.after
                    }
                {
                    return Err(EditError::InvalidArgument);
                }
            }
            for (offset, (index, definition)) in batch.inserted_apertures.iter().enumerate() {
                if forward {
                    if *index != document.apertures.len() + offset
                        || document.apertures.iter().any(|aperture| {
                            aperture.id == definition.id
                                || aperture.source_dcode == definition.source_dcode
                        })
                    {
                        return Err(EditError::InvalidArgument);
                    }
                } else if document.apertures.get(*index) != Some(definition) {
                    return Err(EditError::InvalidArgument);
                }
            }
        }
    }
    Ok(())
}

fn apply(document: &mut SemanticDocument, tx: &Transaction, forward: bool) -> Vec<String> {
    match &tx.operation {
        Operation::Modify(changes) => {
            let objects = &mut document.layers[tx.layer].objects;
            changes
                .iter()
                .map(|c| {
                    objects[c.index].geometry = if forward { &c.after } else { &c.before }.clone();
                    c.object_id.clone()
                })
                .collect()
        }
        Operation::Insert(entries) | Operation::Delete(entries) => {
            let objects = &mut document.layers[tx.layer].objects;
            let inserting = matches!(tx.operation, Operation::Insert(_)) == forward;
            if inserting {
                let mut merged = Vec::with_capacity(objects.len() + entries.len());
                let mut source = std::mem::take(objects).into_iter();
                for entry in entries {
                    while merged.len() < entry.index {
                        merged.push(source.next().unwrap());
                    }
                    merged.push(entry.object.clone());
                }
                merged.extend(source);
                *objects = merged;
            } else {
                let mut entries = entries.iter().peekable();
                let mut index = 0;
                objects.retain(|_| {
                    let remove = entries.peek().is_some_and(|e| e.index == index);
                    if remove {
                        entries.next();
                    }
                    index += 1;
                    !remove
                });
            }
            entries.iter().map(|e| e.object.object_id.clone()).collect()
        }
        Operation::ApertureResize(resize) => {
            if forward {
                document
                    .apertures
                    .insert(resize.index, resize.definition.clone());
            }
            let objects = &mut document.layers[tx.layer].objects;
            for change in &resize.changes {
                objects[change.index].geometry = if forward {
                    &change.after
                } else {
                    &change.before
                }
                .clone();
            }
            if !forward {
                document.apertures.remove(resize.index);
            }
            resize
                .changes
                .iter()
                .map(|change| change.object_id.clone())
                .collect()
        }
        Operation::Batch(batch) => {
            if forward {
                for (index, definition) in &batch.inserted_apertures {
                    document.apertures.insert(*index, definition.clone());
                }
            }
            let objects = &mut document.layers[tx.layer].objects;
            for change in &batch.changes {
                objects[change.index].geometry = if forward {
                    &change.after
                } else {
                    &change.before
                }
                .clone();
            }
            if !forward {
                for (index, _) in batch.inserted_apertures.iter().rev() {
                    document.apertures.remove(*index);
                }
            }
            batch
                .changes
                .iter()
                .map(|change| change.object_id.clone())
                .collect()
        }
    }
}

fn operation_changes_shape(operation: &Operation) -> bool {
    match operation {
        Operation::ApertureResize(_) => true,
        Operation::Batch(batch) => !batch.inserted_apertures.is_empty(),
        Operation::Modify(_) | Operation::Insert(_) | Operation::Delete(_) => false,
    }
}

fn resized_shape(
    shape: &ApertureShape,
    width_mm: f64,
    height_mm: Option<f64>,
) -> Result<ApertureShape, EditError> {
    if !width_mm.is_finite() || height_mm.is_some_and(|height| !height.is_finite()) {
        return Err(EditError::InvalidArgument);
    }
    Ok(match shape {
        ApertureShape::Circle {
            hole_diameter_mm, ..
        } => {
            if height_mm.is_some_and(|height| height != width_mm) {
                return Err(EditError::InvalidArgument);
            }
            ApertureShape::Circle {
                diameter_mm: width_mm,
                hole_diameter_mm: *hole_diameter_mm,
            }
        }
        ApertureShape::Rectangle {
            hole_diameter_mm, ..
        } => ApertureShape::Rectangle {
            width_mm,
            height_mm: height_mm.ok_or(EditError::InvalidArgument)?,
            hole_diameter_mm: *hole_diameter_mm,
        },
        ApertureShape::Obround {
            hole_diameter_mm, ..
        } => ApertureShape::Obround {
            width_mm,
            height_mm: height_mm.ok_or(EditError::InvalidArgument)?,
            hole_diameter_mm: *hole_diameter_mm,
        },
        ApertureShape::Polygon {
            vertices,
            rotation_deg,
            hole_diameter_mm,
            ..
        } => {
            if height_mm.is_some_and(|height| height != width_mm) {
                return Err(EditError::InvalidArgument);
            }
            ApertureShape::Polygon {
                diameter_mm: width_mm,
                vertices: *vertices,
                rotation_deg: *rotation_deg,
                hole_diameter_mm: *hole_diameter_mm,
            }
        }
        ApertureShape::Macro { .. } => return Err(EditError::UnsupportedTransform),
    })
}

fn aperture_shape_heap_bytes(shape: &ApertureShape) -> usize {
    match shape {
        ApertureShape::Macro { primitives } => primitives.len() * size_of::<MacroPrimitive>() + 128,
        _ => 0,
    }
}

fn origin_bytes(origin: &ObjectOrigin) -> usize {
    match origin {
        ObjectOrigin::Imported { .. } => 0,
        ObjectOrigin::Generated { operation_id } => operation_id.len(),
    }
}
fn region_edges(geometry: &SemanticGeometry) -> usize {
    match geometry {
        SemanticGeometry::Region { contours } => contours.iter().map(|c| c.edges.len()).sum(),
        _ => 0,
    }
}

fn geometry_heap_bytes(geometry: &SemanticGeometry) -> usize {
    match geometry {
        SemanticGeometry::Flash { aperture_id, .. } => aperture_id.len() + 64,
        SemanticGeometry::Region { contours } => {
            contours.len() * (size_of::<RegionContour>() + 64)
                + contours
                    .iter()
                    .map(|c| c.edges.len() * size_of::<RegionEdge>() + 64)
                    .sum::<usize>()
        }
        _ => 0,
    }
}

fn translate_point(point: &mut MmPoint, dx: f64, dy: f64) -> Result<(), EditError> {
    let next = MmPoint::new(point.x_mm + dx, point.y_mm + dy);
    // Bound roundoff before accepting a translation, independently of screen scale.
    let uncertainty = 4.0
        * f64::EPSILON
        * point
            .x_mm
            .abs()
            .max(point.y_mm.abs())
            .max(dx.abs())
            .max(dy.abs())
            .max(1.0);
    if !next.is_valid_geometry() || uncertainty > EPSILON_MM {
        return Err(EditError::InvalidArgument);
    }
    *point = next;
    Ok(())
}

fn translate_arc(path: &mut ArcGeometry, dx: f64, dy: f64) -> Result<(), EditError> {
    let old = *path;
    translate_point(&mut path.start, dx, dy)?;
    translate_point(&mut path.end, dx, dy)?;
    translate_point(&mut path.center, dx, dy)?;
    // Translation must not collapse a near-full arc into a full/zero circle.
    if (old.start == old.end) != (path.start == path.end)
        || ((old.start.distance_mm(old.center) - path.start.distance_mm(path.center)).abs()
            > EPSILON_MM)
        || ((old.end.distance_mm(old.center) - path.end.distance_mm(path.center)).abs()
            > EPSILON_MM)
    {
        return Err(EditError::InvalidArgument);
    }
    Ok(())
}

fn translate(geometry: &mut SemanticGeometry, dx: f64, dy: f64) -> Result<(), EditError> {
    match geometry {
        SemanticGeometry::Flash { center, .. } => translate_point(center, dx, dy)?,
        SemanticGeometry::Line { start, end, .. }
        | SemanticGeometry::RectangularSweep { start, end, .. } => {
            translate_point(start, dx, dy)?;
            translate_point(end, dx, dy)?;
        }
        SemanticGeometry::Arc { path, .. } => translate_arc(path, dx, dy)?,
        SemanticGeometry::Region { contours } => {
            for edge in contours.iter_mut().flat_map(|c| &mut c.edges) {
                match edge {
                    RegionEdge::Line { start, end } => {
                        translate_point(start, dx, dy)?;
                        translate_point(end, dx, dy)?;
                    }
                    RegionEdge::Arc(path) => translate_arc(path, dx, dy)?,
                }
            }
        }
    }
    Ok(())
}
