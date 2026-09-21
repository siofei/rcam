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
    /// Whole-layer structure change (import batch, new empty layer, remove).
    Layers(LayerMove),
}

/// A self-contained layer handed to `EditHistory::add_layers`: the layer and
/// the apertures it exclusively owns (already namespace-remapped).
#[derive(Debug, Clone, PartialEq)]
pub struct LayerAdd {
    pub layer: SemanticLayer,
    pub apertures: Vec<ApertureDefinition>,
}

/// Effect of a layer transaction on the layer set. The service uses it to keep
/// its workspace side table (colours, order, active layer) in step with Undo/Redo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerEffect {
    Added(Vec<String>),
    Removed(Vec<String>),
}

/// Layers and apertures that move between the document and this transaction.
/// Data is moved, never cloned: while the layers are in the document the
/// transaction only remembers their positions and identities.
#[derive(Debug, Clone)]
struct LayerMove {
    forward_inserts: bool,
    layer_slots: Vec<(usize, String)>,
    aperture_slots: Vec<(usize, String)>,
    held_layers: Vec<SemanticLayer>,
    held_apertures: Vec<ApertureDefinition>,
}

impl LayerMove {
    fn held(&self) -> bool {
        !self.held_layers.is_empty() || !self.held_apertures.is_empty()
    }

    fn payload_bytes(&self) -> usize {
        let layers: usize = self
            .held_layers
            .iter()
            .map(|layer| {
                layer.id.len()
                    + 64
                    + layer
                        .objects
                        .iter()
                        .map(|o| {
                            size_of::<SemanticObject>()
                                + o.object_id.len()
                                + origin_bytes(&o.origin)
                                + geometry_heap_bytes(&o.geometry)
                                + 32
                        })
                        .sum::<usize>()
            })
            .sum();
        let apertures: usize = self
            .held_apertures
            .iter()
            .map(|a| {
                size_of::<ApertureDefinition>() + a.id.len() + aperture_shape_heap_bytes(&a.shape)
            })
            .sum();
        size_of::<Transaction>() + 512 + layers + apertures
    }

    fn resident_bytes(&self) -> usize {
        size_of::<Transaction>()
            + 512
            + self
                .layer_slots
                .iter()
                .chain(&self.aperture_slots)
                .map(|(_, id)| id.len() + 32)
                .sum::<usize>()
    }

    fn ids(&self) -> Vec<String> {
        self.layer_slots.iter().map(|(_, id)| id.clone()).collect()
    }

    /// Put the held layers/apertures back at their recorded positions.
    fn insert_into(&mut self, document: &mut SemanticDocument) -> Result<(), EditError> {
        if self.held_layers.len() != self.layer_slots.len()
            || self.held_apertures.len() != self.aperture_slots.len()
            || self
                .layer_slots
                .iter()
                .enumerate()
                .any(|(i, (slot, _))| *slot > document.layers.len() + i)
            || self
                .aperture_slots
                .iter()
                .enumerate()
                .any(|(i, (slot, _))| *slot > document.apertures.len() + i)
        {
            return Err(EditError::InvalidArgument);
        }
        for ((slot, _), layer) in self.layer_slots.iter().zip(self.held_layers.drain(..)) {
            document.layers.insert(*slot, layer);
        }
        for ((slot, _), aperture) in self
            .aperture_slots
            .iter()
            .zip(self.held_apertures.drain(..))
        {
            document.apertures.insert(*slot, aperture);
        }
        Ok(())
    }

    /// Remove the layers/apertures from the document and hold them here.
    fn extract_from(&mut self, document: &mut SemanticDocument) -> Result<(), EditError> {
        if self
            .layer_slots
            .iter()
            .any(|(slot, id)| document.layers.get(*slot).map(|l| &l.id) != Some(id))
            || self
                .aperture_slots
                .iter()
                .any(|(slot, id)| document.apertures.get(*slot).map(|a| &a.id) != Some(id))
        {
            return Err(EditError::InvalidArgument);
        }
        let mut layers = Vec::with_capacity(self.layer_slots.len());
        for (slot, _) in self.layer_slots.iter().rev() {
            layers.push(document.layers.remove(*slot));
        }
        layers.reverse();
        let mut apertures = Vec::with_capacity(self.aperture_slots.len());
        for (slot, _) in self.aperture_slots.iter().rev() {
            apertures.push(document.apertures.remove(*slot));
        }
        apertures.reverse();
        self.held_layers = layers;
        self.held_apertures = apertures;
        Ok(())
    }
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

    fn generated_plan(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        geometries: &[SemanticGeometry],
    ) -> Result<(usize, u64, usize), EditError> {
        if geometries.is_empty()
            || self
                .document_id
                .as_deref()
                .is_some_and(|id| id != document.id)
        {
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
        let total: usize = document.layers.iter().map(|l| l.objects.len()).sum();
        let edges: usize = document
            .layers
            .iter()
            .flat_map(|l| &l.objects)
            .map(|o| region_edges(&o.geometry))
            .sum();
        if geometries.len() > MAX_MOVE_OBJECTS
            || total.saturating_add(geometries.len()) > MAX_EDIT_DOCUMENT_OBJECTS
            || edges.saturating_add(geometries.iter().map(region_edges).sum())
                > MAX_EDIT_REGION_EDGES
        {
            return Err(EditError::ResourceLimit);
        }
        let next = self
            .next_generated_id
            .checked_add(geometries.len() as u64)
            .ok_or(EditError::ResourceLimit)?;
        let layer = &document.layers[layer_index];
        let bytes = size_of::<Transaction>()
            + 1024
            + layer_id.len()
            + layer
                .objects
                .iter()
                .map(|o| 4 * (o.object_id.len() + 96) + size_of::<SemanticObject>())
                .sum::<usize>()
            + geometries
                .iter()
                .map(|g| {
                    3 * (size_of::<IndexedObject>()
                        + geometry_heap_bytes(g)
                        + 2 * document.id.len()
                        + 512)
                })
                .sum::<usize>();
        self.budget(bytes)?;
        let aperture_ids = document.apertures.iter().map(|a| a.id.clone()).collect();
        let existing: HashSet<_> = document
            .layers
            .iter()
            .flat_map(|l| &l.objects)
            .map(|o| o.object_id.as_str())
            .collect();
        for (i, geometry) in geometries.iter().enumerate() {
            validate_geometry(geometry, &aperture_ids).map_err(EditError::InvalidGeometry)?;
            let id = format!(
                "{}-generated-object-{}",
                document.id,
                self.next_generated_id + i as u64
            );
            if existing.contains(id.as_str()) {
                return Err(EditError::InvalidArgument);
            }
        }
        Ok((layer_index, next, bytes))
    }

    /// Read-only preflight shared by text preview and the insertion transaction.
    pub fn validate_generated(
        &self,
        document: &SemanticDocument,
        layer_id: &str,
        geometries: &[SemanticGeometry],
    ) -> Result<(), EditError> {
        self.generated_plan(document, layer_id, geometries)
            .map(|_| ())
    }

    /// Append generated manufacturing geometry as one bounded history transaction.
    pub fn insert_generated(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        geometries: Vec<SemanticGeometry>,
    ) -> Result<Vec<String>, EditError> {
        self.insert_generated_with(document, layer_id, geometries, false)
    }

    /// Same transaction as `insert_generated`, but objects carry the text origin.
    pub fn insert_generated_text(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        geometries: Vec<SemanticGeometry>,
    ) -> Result<Vec<String>, EditError> {
        self.insert_generated_with(document, layer_id, geometries, true)
    }

    fn insert_generated_with(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        geometries: Vec<SemanticGeometry>,
        text: bool,
    ) -> Result<Vec<String>, EditError> {
        let (layer_index, next, bytes) = self.generated_plan(document, layer_id, &geometries)?;
        let layer = &document.layers[layer_index];
        let operation_id = format!("{}-generated-op-{}", document.id, self.next_generated_id);
        let mut entries = Vec::with_capacity(geometries.len());
        for (i, geometry) in geometries.into_iter().enumerate() {
            let object_id = format!(
                "{}-generated-object-{}",
                document.id,
                self.next_generated_id + i as u64
            );
            entries.push(IndexedObject {
                index: layer.objects.len() + i,
                object: SemanticObject {
                    object_id,
                    geometry,
                    exposure: Exposure::Dark,
                    origin: if text {
                        ObjectOrigin::GeneratedText {
                            operation_id: operation_id.clone(),
                        }
                    } else {
                        ObjectOrigin::Generated {
                            operation_id: operation_id.clone(),
                        }
                    },
                },
            });
        }
        let before_order: Vec<_> = layer.objects.iter().map(|o| o.object_id.clone()).collect();
        let mut after_order = before_order.clone();
        after_order.extend(entries.iter().map(|e| e.object.object_id.clone()));
        let ids = self.commit(
            document,
            Transaction {
                layer_id: layer_id.into(),
                layer: layer_index,
                operation: Operation::Insert(entries),
                before_order,
                after_order,
                bytes,
            },
        );
        self.next_generated_id = next;
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
                // A copy of generated text remains text for display classification.
                object.origin = if matches!(object.origin, ObjectOrigin::GeneratedText { .. }) {
                    ObjectOrigin::GeneratedText {
                        operation_id: operation_id.clone(),
                    }
                } else {
                    ObjectOrigin::Generated {
                        operation_id: operation_id.clone(),
                    }
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

    /// Effect on the layer set of undoing the next transaction, if it is a layer one.
    pub fn peek_undo_layer_effect(&self) -> Option<LayerEffect> {
        Self::layer_effect(self.undo.last()?, false)
    }

    /// Effect on the layer set of redoing the next transaction, if it is a layer one.
    pub fn peek_redo_layer_effect(&self) -> Option<LayerEffect> {
        Self::layer_effect(self.redo.last()?, true)
    }

    /// Layer ids referenced by layer transactions still on the Undo/Redo stacks.
    /// The service keeps Workspace side data only for these.
    pub fn layer_transaction_ids(&self) -> HashSet<String> {
        self.undo
            .iter()
            .chain(&self.redo)
            .filter_map(|tx| match &tx.operation {
                Operation::Layers(mv) => Some(mv.ids()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn layer_effect(tx: &Transaction, forward: bool) -> Option<LayerEffect> {
        let Operation::Layers(mv) = &tx.operation else {
            return None;
        };
        let ids = mv.ids();
        Some(if mv.forward_inserts == forward {
            LayerEffect::Added(ids)
        } else {
            LayerEffect::Removed(ids)
        })
    }

    fn push_layer_transaction(&mut self, document: &SemanticDocument, mut tx: Transaction) {
        if let Operation::Layers(mv) = &tx.operation {
            tx.bytes = if mv.held() {
                mv.payload_bytes()
            } else {
                mv.resident_bytes()
            };
        }
        self.document_id = Some(document.id.clone());
        self.redo.clear();
        self.undo.push(tx);
        while self.undo.len() > self.max_entries || self.bytes() > self.max_bytes {
            let evicted = self.undo.remove(0);
            self.truncated_entries += 1;
            self.truncated_bytes = self.truncated_bytes.saturating_add(evicted.bytes);
        }
    }

    /// Append self-contained layers as ONE transaction. Ids must not collide with
    /// anything already in the document; nothing changes on failure.
    pub fn add_layers(
        &mut self,
        document: &mut SemanticDocument,
        adds: Vec<LayerAdd>,
    ) -> Result<Vec<String>, EditError> {
        if adds.is_empty()
            || self
                .document_id
                .as_deref()
                .is_some_and(|id| id != document.id)
        {
            return Err(EditError::InvalidArgument);
        }
        let new_objects: usize = adds.iter().map(|a| a.layer.objects.len()).sum();
        let new_edges: usize = adds
            .iter()
            .flat_map(|a| &a.layer.objects)
            .map(|o| region_edges(&o.geometry))
            .sum();
        let total: usize = document.layers.iter().map(|l| l.objects.len()).sum();
        let edges: usize = document
            .layers
            .iter()
            .flat_map(|l| &l.objects)
            .map(|o| region_edges(&o.geometry))
            .sum();
        if total.saturating_add(new_objects) > MAX_EDIT_DOCUMENT_OBJECTS
            || edges.saturating_add(new_edges) > MAX_EDIT_REGION_EDGES
        {
            return Err(EditError::ResourceLimit);
        }
        // The new content must be valid on its own (ids, apertures, geometry).
        let mut layers = Vec::with_capacity(adds.len());
        let mut apertures = Vec::new();
        for add in adds {
            layers.push(add.layer);
            apertures.extend(add.apertures);
        }
        let candidate = SemanticDocument {
            id: document.id.clone(),
            unit: document.unit.clone(),
            format: document.format.clone(),
            layers,
            apertures,
            source: SourceMetadata::default(),
        };
        candidate.validate().map_err(EditError::InvalidGeometry)?;
        let existing_layers: HashSet<&str> =
            document.layers.iter().map(|l| l.id.as_str()).collect();
        let existing_objects: HashSet<&str> = document
            .layers
            .iter()
            .flat_map(|l| &l.objects)
            .map(|o| o.object_id.as_str())
            .collect();
        let existing_apertures: HashSet<&str> =
            document.apertures.iter().map(|a| a.id.as_str()).collect();
        let existing_codes: HashSet<(&str, i32)> = document
            .apertures
            .iter()
            .map(|a| (aperture_namespace(&a.id), a.source_dcode))
            .collect();
        if candidate.layers.iter().any(|l| {
            existing_layers.contains(l.id.as_str())
                || l.objects
                    .iter()
                    .any(|o| existing_objects.contains(o.object_id.as_str()))
        }) || candidate.apertures.iter().any(|a| {
            existing_apertures.contains(a.id.as_str())
                || existing_codes.contains(&(aperture_namespace(&a.id), a.source_dcode))
        }) {
            return Err(EditError::InvalidArgument);
        }
        let first_layer = document.layers.len();
        let first_aperture = document.apertures.len();
        let mut mv = LayerMove {
            forward_inserts: true,
            layer_slots: candidate
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| (first_layer + i, l.id.clone()))
                .collect(),
            aperture_slots: candidate
                .apertures
                .iter()
                .enumerate()
                .map(|(i, a)| (first_aperture + i, a.id.clone()))
                .collect(),
            held_layers: candidate.layers,
            held_apertures: candidate.apertures,
        };
        if mv.payload_bytes() > self.max_bytes {
            return Err(EditError::ResourceLimit);
        }
        mv.insert_into(document)?;
        let ids = mv.ids();
        self.push_layer_transaction(
            document,
            Transaction {
                layer_id: String::new(),
                layer: 0,
                operation: Operation::Layers(mv),
                before_order: vec![],
                after_order: vec![],
                bytes: 0,
            },
        );
        Ok(ids)
    }

    /// Remove one layer as ONE transaction, together with the apertures only that
    /// layer uses (plus unused apertures of `owned_namespace`). Undo restores the
    /// same layer, position and apertures.
    pub fn remove_layer(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        owned_namespace: Option<&str>,
    ) -> Result<String, EditError> {
        if self
            .document_id
            .as_deref()
            .is_some_and(|id| id != document.id)
        {
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
        fn used_by(layer: &SemanticLayer) -> HashSet<&str> {
            layer
                .objects
                .iter()
                .filter_map(|o| match &o.geometry {
                    SemanticGeometry::Flash { aperture_id, .. } => Some(aperture_id.as_str()),
                    _ => None,
                })
                .collect()
        }
        let own = used_by(&document.layers[layer_index]);
        let others: HashSet<&str> = document
            .layers
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != layer_index)
            .flat_map(|(_, l)| used_by(l))
            .collect();
        let aperture_slots: Vec<(usize, String)> = document
            .apertures
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                !others.contains(a.id.as_str())
                    && (own.contains(a.id.as_str())
                        || owned_namespace.is_some_and(|ns| aperture_namespace(&a.id) == ns))
            })
            .map(|(i, a)| (i, a.id.clone()))
            .collect();
        let held_bytes = document.layers[layer_index]
            .objects
            .iter()
            .map(|o| {
                size_of::<SemanticObject>()
                    + o.object_id.len()
                    + origin_bytes(&o.origin)
                    + geometry_heap_bytes(&o.geometry)
                    + 32
            })
            .sum::<usize>()
            + aperture_slots.len() * (size_of::<ApertureDefinition>() + 128)
            + size_of::<Transaction>()
            + 512;
        if held_bytes > self.max_bytes {
            return Err(EditError::ResourceLimit);
        }
        let mut mv = LayerMove {
            forward_inserts: false,
            layer_slots: vec![(layer_index, layer_id.to_string())],
            aperture_slots,
            held_layers: vec![],
            held_apertures: vec![],
        };
        mv.extract_from(document)?;
        self.push_layer_transaction(
            document,
            Transaction {
                layer_id: layer_id.into(),
                layer: layer_index,
                operation: Operation::Layers(mv),
                before_order: vec![],
                after_order: vec![],
                bytes: 0,
            },
        );
        Ok(layer_id.to_string())
    }

    /// Undo (`forward == false`) or redo the top layer transaction.
    fn step_layers(
        &mut self,
        document: &mut SemanticDocument,
        forward: bool,
    ) -> Result<Vec<String>, EditError> {
        let mut tx = if forward {
            self.redo.pop()
        } else {
            self.undo.pop()
        }
        .ok_or(EditError::EmptyHistory)?;
        let Operation::Layers(mv) = &mut tx.operation else {
            unreachable!("caller checked the operation kind")
        };
        let inserts = mv.forward_inserts == forward;
        let result = if inserts {
            mv.insert_into(document)
        } else {
            mv.extract_from(document)
        };
        let ids = mv.ids();
        if result.is_ok() {
            tx.bytes = if mv.held() {
                mv.payload_bytes()
            } else {
                mv.resident_bytes()
            };
        }
        // Success moves the transaction to the opposite stack; failure puts it back.
        let to_undo = forward == result.is_ok();
        if to_undo {
            self.undo.push(tx);
        } else {
            self.redo.push(tx);
        }
        result.map(|()| ids)
    }

    pub fn undo(&mut self, document: &mut SemanticDocument) -> Result<Vec<String>, EditError> {
        if self.document_id.as_deref() != Some(document.id.as_str()) {
            return Err(EditError::EmptyHistory);
        }
        let tx = self.undo.last().ok_or(EditError::EmptyHistory)?;
        if matches!(tx.operation, Operation::Layers(_)) {
            return self.step_layers(document, false);
        }
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
        if matches!(tx.operation, Operation::Layers(_)) {
            return self.step_layers(document, true);
        }
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
    if matches!(tx.operation, Operation::Layers(_)) {
        // Verified by `LayerMove::{insert_into, extract_from}` before mutation.
        return Ok(());
    }
    let layer = document
        .layers
        .get(tx.layer)
        .filter(|l| l.id == tx.layer_id)
        .ok_or(EditError::InvalidArgument)?;
    match &tx.operation {
        Operation::Layers(_) => {}
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
        // Layer moves mutate the transaction itself and never reach this path.
        Operation::Layers(_) => Vec::new(),
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
        Operation::Modify(_)
        | Operation::Insert(_)
        | Operation::Delete(_)
        | Operation::Layers(_) => false,
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
        ObjectOrigin::Generated { operation_id } | ObjectOrigin::GeneratedText { operation_id } => {
            operation_id.len()
        }
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
