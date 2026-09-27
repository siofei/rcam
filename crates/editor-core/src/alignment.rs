//! CAD-style alignment and edge-gap distribution over analytic world bounds.

use crate::edit::EditError;
use crate::{BoundsMm, SemanticDocument, individual_geometries_bounds_with_blocks};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentMode {
    Left,
    Right,
    Top,
    Bottom,
    HCenter,
    VCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionAxis {
    Horizontal,
    Vertical,
}

/// The stable layer position is the tie-breaker for equal distribution edges.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectBoundsMm {
    /// Physical object IDs in this logical unit. GeneratedText glyphs share
    /// one unit and therefore receive one identical translation.
    pub object_ids: Vec<String>,
    pub layer_order: usize,
    pub bounds: BoundsMm,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDelta {
    pub object_ids: Vec<String>,
    pub dx_mm: f64,
    pub dy_mm: f64,
}

/// Compute translations that align every object to the explicit anchor's
/// world manufacturing AABB. Results are returned in layer order.
pub fn compute_alignment_deltas(
    objects: &[ObjectBoundsMm],
    anchor_id: &str,
    mode: AlignmentMode,
) -> Result<Vec<ObjectDelta>, EditError> {
    validate_objects(objects, 2)?;
    let anchor = objects
        .iter()
        .find(|object| object.object_ids.iter().any(|id| id == anchor_id))
        .ok_or(EditError::InvalidArgument)?;
    let target = match mode {
        AlignmentMode::Left => anchor.bounds.min_x_mm,
        AlignmentMode::Right => anchor.bounds.max_x_mm,
        AlignmentMode::Top => anchor.bounds.max_y_mm,
        AlignmentMode::Bottom => anchor.bounds.min_y_mm,
        AlignmentMode::HCenter => center_x(anchor.bounds),
        AlignmentMode::VCenter => center_y(anchor.bounds),
    };

    let mut deltas = objects
        .iter()
        .map(|object| {
            let (dx_mm, dy_mm) = match mode {
                AlignmentMode::Left => (target - object.bounds.min_x_mm, 0.0),
                AlignmentMode::Right => (target - object.bounds.max_x_mm, 0.0),
                AlignmentMode::Top => (0.0, target - object.bounds.max_y_mm),
                AlignmentMode::Bottom => (0.0, target - object.bounds.min_y_mm),
                AlignmentMode::HCenter => (target - center_x(object.bounds), 0.0),
                AlignmentMode::VCenter => (0.0, target - center_y(object.bounds)),
            };
            (
                object.layer_order,
                ObjectDelta {
                    object_ids: object.object_ids.clone(),
                    dx_mm,
                    dy_mm,
                },
            )
        })
        .collect::<Vec<_>>();
    if deltas
        .iter()
        .any(|(_, delta)| !delta.dx_mm.is_finite() || !delta.dy_mm.is_finite())
    {
        return Err(EditError::InvalidArgument);
    }
    deltas.sort_by_key(|(layer_order, _)| *layer_order);
    Ok(deltas.into_iter().map(|(_, delta)| delta).collect())
}

/// Compute equal world AABB edge gaps while preserving the first and last
/// objects after sorting by minimum edge, maximum edge, then layer order.
/// Negative gaps are valid when the fixed endpoints leave less room than the
/// selected objects' combined widths or heights.
pub fn compute_distribution_deltas(
    objects: &[ObjectBoundsMm],
    axis: DistributionAxis,
) -> Result<Vec<ObjectDelta>, EditError> {
    validate_objects(objects, 3)?;
    let min_edge = |bounds: BoundsMm| match axis {
        DistributionAxis::Horizontal => bounds.min_x_mm,
        DistributionAxis::Vertical => bounds.min_y_mm,
    };
    let max_edge = |bounds: BoundsMm| match axis {
        DistributionAxis::Horizontal => bounds.max_x_mm,
        DistributionAxis::Vertical => bounds.max_y_mm,
    };
    let mut order: Vec<usize> = (0..objects.len()).collect();
    order.sort_by(|&left, &right| {
        min_edge(objects[left].bounds)
            .partial_cmp(&min_edge(objects[right].bounds))
            .expect("validated finite bounds")
            .then_with(|| {
                max_edge(objects[left].bounds)
                    .partial_cmp(&max_edge(objects[right].bounds))
                    .expect("validated finite bounds")
            })
            .then_with(|| objects[left].layer_order.cmp(&objects[right].layer_order))
    });

    let span =
        max_edge(objects[*order.last().unwrap()].bounds) - min_edge(objects[order[0]].bounds);
    let total_size = order
        .iter()
        .map(|&index| max_edge(objects[index].bounds) - min_edge(objects[index].bounds))
        .sum::<f64>();
    let gap = (span - total_size) / (order.len() - 1) as f64;
    if !gap.is_finite() {
        return Err(EditError::InvalidArgument);
    }

    let mut deltas = vec![
        ObjectDelta {
            object_ids: Vec::new(),
            dx_mm: 0.0,
            dy_mm: 0.0,
        };
        objects.len()
    ];
    let mut previous_max = max_edge(objects[order[0]].bounds);
    for &index in order.iter().skip(1).take(order.len() - 2) {
        let target_min = previous_max + gap;
        let movement = target_min - min_edge(objects[index].bounds);
        if !target_min.is_finite() || !movement.is_finite() {
            return Err(EditError::InvalidArgument);
        }
        let delta = &mut deltas[index];
        match axis {
            DistributionAxis::Horizontal => delta.dx_mm = movement,
            DistributionAxis::Vertical => delta.dy_mm = movement,
        }
        previous_max =
            target_min + (max_edge(objects[index].bounds) - min_edge(objects[index].bounds));
    }
    let mut ordered = objects
        .iter()
        .zip(deltas)
        .map(|(object, mut delta)| {
            delta.object_ids.clone_from(&object.object_ids);
            (object.layer_order, delta)
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(layer_order, _)| *layer_order);
    Ok(ordered.into_iter().map(|(_, delta)| delta).collect())
}

pub(crate) fn selected_object_bounds(
    document: &SemanticDocument,
    layer_index: usize,
    object_indices: &[usize],
) -> Result<Vec<ObjectBoundsMm>, EditError> {
    let layer = document
        .layers
        .get(layer_index)
        .ok_or(EditError::InvalidArgument)?;
    let selected_indices: HashSet<usize> = object_indices.iter().copied().collect();
    let selected_text_operations: HashSet<&str> = object_indices
        .iter()
        .filter_map(|&index| match &layer.objects.get(index)?.origin {
            crate::ObjectOrigin::GeneratedText { operation_id } => Some(operation_id.as_str()),
            _ => None,
        })
        .collect();
    let mut all_text_members = HashMap::<&str, Vec<usize>>::new();
    for (index, object) in layer.objects.iter().enumerate() {
        if let crate::ObjectOrigin::GeneratedText { operation_id } = &object.origin
            && selected_text_operations.contains(operation_id.as_str())
        {
            all_text_members
                .entry(operation_id.as_str())
                .or_default()
                .push(index);
        }
    }
    if all_text_members.values().any(|members| {
        members
            .iter()
            .any(|index| !selected_indices.contains(index))
    }) {
        return Err(EditError::InvalidArgument);
    }
    let mut group_by_first = HashMap::<usize, Vec<usize>>::new();
    let mut grouped_indices = HashSet::new();
    for members in all_text_members.into_values() {
        if let Some(first) = members.first().copied() {
            grouped_indices.extend(members.iter().copied());
            group_by_first.insert(first, members);
        }
    }
    let geometries = object_indices
        .iter()
        .map(|&index| {
            layer
                .objects
                .get(index)
                .map(|object| &object.geometry)
                .ok_or(EditError::InvalidArgument)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bounds = individual_geometries_bounds_with_blocks(
        geometries,
        &document.apertures,
        &document.block_definitions,
    )
    .map_err(EditError::InvalidGeometry)?;
    if bounds.len() != object_indices.len() {
        return Err(EditError::InvalidArgument);
    }
    let mut physical_bounds = HashMap::with_capacity(object_indices.len());
    for (&index, bounds) in object_indices.iter().zip(bounds) {
        physical_bounds.insert(index, bounds.ok_or(EditError::InvalidArgument)?);
    }
    let mut logical = Vec::with_capacity(object_indices.len());
    for &index in object_indices {
        if grouped_indices.contains(&index) {
            if let Some(members) = group_by_first.get(&index) {
                let mut ids = Vec::with_capacity(members.len());
                let mut combined: Option<BoundsMm> = None;
                for &member in members {
                    let object = layer
                        .objects
                        .get(member)
                        .ok_or(EditError::InvalidArgument)?;
                    ids.push(object.object_id.clone());
                    let bounds = *physical_bounds
                        .get(&member)
                        .ok_or(EditError::InvalidArgument)?;
                    combined = Some(combined.map_or(bounds, |previous| previous.union(bounds)));
                }
                logical.push(ObjectBoundsMm {
                    object_ids: ids,
                    layer_order: index,
                    bounds: combined.ok_or(EditError::InvalidArgument)?,
                });
            }
            continue;
        }
        let object = layer.objects.get(index).ok_or(EditError::InvalidArgument)?;
        logical.push(ObjectBoundsMm {
            object_ids: vec![object.object_id.clone()],
            layer_order: index,
            bounds: *physical_bounds
                .get(&index)
                .ok_or(EditError::InvalidArgument)?,
        });
    }
    Ok(logical)
}

fn center_x(bounds: BoundsMm) -> f64 {
    bounds.min_x_mm + (bounds.max_x_mm - bounds.min_x_mm) / 2.0
}

fn center_y(bounds: BoundsMm) -> f64 {
    bounds.min_y_mm + (bounds.max_y_mm - bounds.min_y_mm) / 2.0
}

fn validate_objects(objects: &[ObjectBoundsMm], minimum: usize) -> Result<(), EditError> {
    if objects.len() < minimum {
        return Err(EditError::InvalidArgument);
    }
    let mut ids = HashSet::with_capacity(objects.len());
    let mut positions = HashSet::with_capacity(objects.len());
    for object in objects {
        let bounds = object.bounds;
        if object.object_ids.is_empty()
            || !positions.insert(object.layer_order)
            || ![
                bounds.min_x_mm,
                bounds.min_y_mm,
                bounds.max_x_mm,
                bounds.max_y_mm,
            ]
            .into_iter()
            .all(f64::is_finite)
            || bounds.min_x_mm > bounds.max_x_mm
            || bounds.min_y_mm > bounds.max_y_mm
        {
            return Err(EditError::InvalidArgument);
        }
        for id in &object.object_ids {
            if id.is_empty() || !ids.insert(id.as_str()) {
                return Err(EditError::InvalidArgument);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use crate::edit::EditHistory;
    use crate::{
        ApertureDefinition, ApertureShape, Exposure, LocalTransform, MmPoint, ObjectOrigin,
        SemanticFormat, SemanticGeometry, SemanticLayer, SemanticObject, SourceMetadata,
    };
    use std::time::Instant;

    #[test]
    #[ignore = "diagnostic timing only; run explicitly when collecting S4-C4 performance evidence"]
    fn perf_1k_alignment_reports_bounds_deltas_transaction_and_total() {
        let objects = (0..1_000)
            .map(|index| SemanticObject {
                object_id: format!("o{index}"),
                geometry: SemanticGeometry::Flash {
                    center: MmPoint::new(index as f64, (index % 7) as f64),
                    aperture_id: "circle".into(),
                    transform: LocalTransform::default(),
                },
                exposure: Exposure::Dark,
                origin: ObjectOrigin::Imported {
                    command_index: index,
                },
            })
            .collect();
        let aperture = ApertureDefinition {
            id: "circle".into(),
            source_dcode: 10,
            shape: ApertureShape::Circle {
                diameter_mm: 1.0,
                hole_diameter_mm: None,
            },
        };
        let mut document = SemanticDocument {
            id: "alignment-perf-1k".into(),
            unit: "mm".into(),
            format: SemanticFormat {
                integer: 4,
                decimal: 6,
                leading_zero_omission: true,
                absolute: true,
            },
            layers: vec![SemanticLayer {
                id: "layer".into(),
                objects,
            }],
            apertures: vec![aperture],
            source: SourceMetadata::default(),
            block_definitions: vec![],
        };
        let selected = (0..1_000).collect::<Vec<_>>();
        let selected_ids = document.layers[0]
            .objects
            .iter()
            .map(|object| object.object_id.clone())
            .collect::<Vec<_>>();
        let mut api_document = document.clone();

        let total_start = Instant::now();
        let bounds_start = Instant::now();
        let logical = selected_object_bounds(&document, 0, &selected).unwrap();
        let bounds_us = bounds_start.elapsed().as_micros();
        let delta_start = Instant::now();
        let deltas =
            compute_alignment_deltas(&logical, &selected_ids[0], AlignmentMode::Left).unwrap();
        let deltas_us = delta_start.elapsed().as_micros();
        let transaction_start = Instant::now();
        let mut history = EditHistory::default();
        history
            .translate_by_deltas(&mut document, "layer", 0, &selected, &deltas)
            .unwrap();
        let transaction_us = transaction_start.elapsed().as_micros();
        let total_us = total_start.elapsed().as_micros();
        assert_eq!(history.undo_len(), 1);
        eprintln!(
            "bounds_us={bounds_us} deltas_us={deltas_us} transaction_us={transaction_us} total_us={total_us}"
        );

        let mut api_history = EditHistory::default();
        let api_start = Instant::now();
        api_history
            .align_objects(
                &mut api_document,
                "layer",
                &selected_ids,
                &selected_ids[0],
                AlignmentMode::Left,
            )
            .unwrap();
        eprintln!("public_align_total_us={}", api_start.elapsed().as_micros());
    }
}
