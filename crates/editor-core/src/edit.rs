//! Atomic, bounded geometry history. No file I/O or UI state enters a command.
use super::*;
use std::collections::HashSet;
use std::mem::size_of;

pub const MAX_MOVE_OBJECTS: usize = 10_000;
pub const MAX_HISTORY_ENTRIES: usize = 100;
pub const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    InvalidArgument,
    NotFound(String),
    LayerLocked(String),
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
struct Transaction {
    layer_id: String,
    layer: usize,
    changes: Vec<Change>,
    bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct EditHistory {
    document_id: Option<String>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

impl EditHistory {
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn move_objects(
        &mut self,
        document: &mut SemanticDocument,
        layer_id: &str,
        object_ids: &[String],
        dx_mm: f64,
        dy_mm: f64,
    ) -> Result<Vec<String>, EditError> {
        if self
            .document_id
            .as_deref()
            .is_some_and(|id| id != document.id)
        {
            return Err(EditError::InvalidArgument);
        }
        if object_ids.is_empty() || !MmPoint::new(dx_mm, dy_mm).is_valid_geometry() {
            return Err(EditError::InvalidArgument);
        }
        if object_ids.len() > MAX_MOVE_OBJECTS || self.undo.len() >= MAX_HISTORY_ENTRIES {
            return Err(EditError::ResourceLimit);
        }
        let targets: HashSet<_> = object_ids.iter().map(String::as_str).collect();
        if targets.len() != object_ids.len() {
            return Err(EditError::InvalidArgument);
        }
        let layer_index = document
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or_else(|| EditError::NotFound(layer_id.into()))?;
        let layer = &document.layers[layer_index];
        if layer.locked {
            return Err(EditError::LayerLocked(layer_id.into()));
        }
        let selected: Vec<_> = layer
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| targets.contains(object.object_id.as_str()))
            .collect();
        if selected.len() != targets.len() {
            let found: HashSet<_> = selected
                .iter()
                .map(|(_, object)| object.object_id.as_str())
                .collect();
            return Err(EditError::NotFound(
                object_ids
                    .iter()
                    .find(|id| !found.contains(id.as_str()))
                    .unwrap()
                    .clone(),
            ));
        }
        // Conservative accounting includes both geometry copies, nested vectors and
        // allocator overhead. Redo stays alive until commit, so count its peak too.
        let bytes = size_of::<Transaction>()
            + 256
            + selected
                .iter()
                .map(|(_, object)| {
                    size_of::<Change>()
                        + 256
                        + 3 * object.object_id.len()
                        + 3 * geometry_heap_bytes(&object.geometry)
                })
                .sum::<usize>();
        let used: usize = self.undo.iter().chain(&self.redo).map(|tx| tx.bytes).sum();
        if bytes > MAX_HISTORY_BYTES.saturating_sub(used) {
            return Err(EditError::ResourceLimit);
        }
        let aperture_ids = document.apertures.iter().map(|a| a.id.clone()).collect();
        let mut changes = Vec::with_capacity(selected.len());
        let mut changed_ids = Vec::with_capacity(selected.len());
        for (index, object) in selected {
            let mut after = object.geometry.clone();
            translate(&mut after, dx_mm, dy_mm)?;
            validate_geometry(&after, &aperture_ids).map_err(EditError::InvalidGeometry)?;
            changes.push(Change {
                object_id: object.object_id.clone(),
                index,
                before: object.geometry.clone(),
                after,
            });
            changed_ids.push(object.object_id.clone());
        }
        // No fallible semantic operation follows this point.
        let transaction = Transaction {
            layer_id: layer_id.into(),
            layer: layer_index,
            changes,
            bytes,
        };
        apply(document, &transaction, true);
        self.document_id = Some(document.id.clone());
        self.redo.clear();
        self.undo.push(transaction);
        Ok(changed_ids)
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
        .filter(|layer| layer.id == tx.layer_id)
        .ok_or(EditError::InvalidArgument)?;
    if layer.locked {
        return Err(EditError::LayerLocked(layer.id.clone()));
    }
    for change in &tx.changes {
        let object = layer
            .objects
            .get(change.index)
            .filter(|object| object.object_id == change.object_id)
            .ok_or(EditError::InvalidArgument)?;
        let expected = if forward {
            &change.before
        } else {
            &change.after
        };
        if &object.geometry != expected {
            return Err(EditError::InvalidArgument);
        }
    }
    Ok(())
}

fn apply(document: &mut SemanticDocument, tx: &Transaction, forward: bool) -> Vec<String> {
    tx.changes
        .iter()
        .map(|change| {
            let object = &mut document.layers[tx.layer].objects[change.index];
            object.geometry = if forward {
                &change.after
            } else {
                &change.before
            }
            .clone();
            object.object_id.clone()
        })
        .collect()
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
