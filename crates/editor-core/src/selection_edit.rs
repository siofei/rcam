//! Multi-layer planning is read-only. The single commit contains exact per-layer deltas.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionGroup {
    pub layer_id: String,
    pub object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectionEdit {
    Move {
        dx_mm: f64,
        dy_mm: f64,
    },
    Rotate {
        angle_deg: f64,
        #[serde(deserialize_with = "strict_point")]
        pivot_mm: MmPoint,
    },
    Mirror {
        axis: MirrorAxis,
    },
    Duplicate {
        dx_mm: f64,
        dy_mm: f64,
    },
    Delete,
}

impl SelectionEdit {
    /// Read-only preview uses the same rigid transform/translation as commit.
    /// No history, IDs, aperture changes or display geometry participates.
    pub fn preview_geometry(
        &self,
        geometry: &SemanticGeometry,
    ) -> Result<SemanticGeometry, EditError> {
        let mut result = geometry.clone();
        match self {
            Self::Move { dx_mm, dy_mm } | Self::Duplicate { dx_mm, dy_mm } => {
                if !MmPoint::new(*dx_mm, *dy_mm).is_valid_geometry() {
                    return Err(EditError::InvalidArgument);
                }
                crate::edit::translate(&mut result, *dx_mm, *dy_mm)?;
            }
            Self::Rotate {
                angle_deg,
                pivot_mm,
            } => {
                crate::transform::WorldTransform::rotation(*angle_deg, *pivot_mm)?
                    .apply(&mut result)?;
            }
            Self::Mirror { axis } => {
                crate::transform::WorldTransform::reflection(*axis)?.apply(&mut result)?;
            }
            Self::Delete => return Err(EditError::InvalidArgument),
        }
        Ok(result)
    }
}

impl EditHistory {
    pub fn edit_selection(
        &mut self,
        document: &mut SemanticDocument,
        groups: &[SelectionGroup],
        operation: &SelectionEdit,
    ) -> Result<Vec<SelectionGroup>, EditError> {
        let count = groups
            .iter()
            .try_fold(0usize, |n, g| n.checked_add(g.object_ids.len()))
            .ok_or(EditError::ResourceLimit)?;
        if groups.is_empty() || count == 0 {
            return Err(EditError::InvalidArgument);
        }
        if count > MAX_MOVE_OBJECTS || groups.len() > MAX_MOVE_OBJECTS {
            return Err(EditError::ResourceLimit);
        }
        let mut seen = HashSet::new();
        let mut targets = Vec::with_capacity(groups.len());
        for group in groups {
            if !seen.insert(&group.layer_id) {
                return Err(EditError::InvalidArgument);
            }
            let (layer, indices) = self.targets(document, &group.layer_id, &group.object_ids)?;
            // Generated text is an indivisible logical object, even for headless callers.
            let text_groups: HashSet<_> = indices
                .iter()
                .filter_map(|&i| match &document.layers[layer].objects[i].origin {
                    ObjectOrigin::GeneratedText { operation_id } => Some(operation_id.as_str()),
                    _ => None,
                })
                .collect();
            let selected: HashSet<_> = indices.iter().copied().collect();
            if document.layers[layer].objects.iter().enumerate().any(|(i,o)| matches!(&o.origin,ObjectOrigin::GeneratedText{operation_id} if text_groups.contains(operation_id.as_str()) && !selected.contains(&i))) {
                return Err(EditError::InvalidArgument);
            }
            targets.push((layer, indices));
        }
        let transform = match operation {
            SelectionEdit::Move { dx_mm, dy_mm } | SelectionEdit::Duplicate { dx_mm, dy_mm } => {
                if !MmPoint::new(*dx_mm, *dy_mm).is_valid_geometry()
                    || (matches!(operation, SelectionEdit::Move { .. })
                        && *dx_mm == 0.
                        && *dy_mm == 0.)
                {
                    return Err(EditError::InvalidArgument);
                }
                None
            }
            SelectionEdit::Rotate {
                angle_deg,
                pivot_mm,
            } => Some(crate::transform::WorldTransform::rotation(
                *angle_deg, *pivot_mm,
            )?),
            SelectionEdit::Mirror { axis } => {
                Some(crate::transform::WorldTransform::reflection(*axis)?)
            }
            SelectionEdit::Delete => None,
        };
        let inserting = matches!(operation, SelectionEdit::Duplicate { .. });
        let structural = inserting || matches!(operation, SelectionEdit::Delete);
        if inserting {
            let total: usize = document.layers.iter().map(|l| l.objects.len()).sum();
            let edges: usize = document
                .layers
                .iter()
                .flat_map(|l| &l.objects)
                .map(|o| region_edges(&o.geometry))
                .sum();
            let added: usize = targets
                .iter()
                .map(|(l, ii)| {
                    ii.iter()
                        .map(|&i| region_edges(&document.layers[*l].objects[i].geometry))
                        .sum::<usize>()
                })
                .sum();
            if total.saturating_add(count) > MAX_EDIT_DOCUMENT_OBJECTS
                || edges.saturating_add(added) > MAX_EDIT_REGION_EDGES
            {
                return Err(EditError::ResourceLimit);
            }
        }
        // Charge all delta/order guards and structural merge peak before allocating geometry.
        let bytes = targets
            .iter()
            .try_fold(size_of::<Transaction>() + 256, |bytes, (l, indices)| {
                let layer = &document.layers[*l];
                let order = if structural {
                    layer
                        .objects
                        .iter()
                        .map(|o| 4 * (o.object_id.len() + 96) + size_of::<SemanticObject>())
                        .sum()
                } else {
                    0
                };
                let delta: usize = indices
                    .iter()
                    .map(|&i| {
                        let o = &layer.objects[i];
                        3 * (size_of::<IndexedObject>()
                            + size_of::<Change>()
                            + geometry_heap_bytes(&o.geometry)
                            + o.object_id.len()
                            + origin_bytes(&o.origin)
                            + document.id.len()
                            + 256)
                    })
                    .sum();
                bytes.checked_add(size_of::<Transaction>() + 1024 + layer.id.len() + order + delta)
            })
            .ok_or(EditError::ResourceLimit)?;
        self.budget(bytes)?;
        let apertures = document.apertures.iter().map(|a| a.id.clone()).collect();
        let definitions = block_definition_ids(document);
        let mut generated = self.next_generated_id;
        let mut transactions = Vec::with_capacity(targets.len());
        let mut result = Vec::with_capacity(targets.len());
        let existing: HashSet<_> = if inserting {
            document
                .layers
                .iter()
                .flat_map(|l| &l.objects)
                .map(|o| o.object_id.as_str())
                .collect()
        } else {
            HashSet::new()
        };
        for (layer_index, indices) in targets {
            let layer = &document.layers[layer_index];
            let mut changes = Vec::new();
            let mut entries = Vec::new();
            let mut text_copies = BTreeMap::<String, String>::new();
            let mut ids = Vec::with_capacity(indices.len());
            for (n, &index) in indices.iter().enumerate() {
                let object = &layer.objects[index];
                let mut after = object.geometry.clone();
                match operation {
                    SelectionEdit::Move { dx_mm, dy_mm }
                    | SelectionEdit::Duplicate { dx_mm, dy_mm } => {
                        translate(&mut after, *dx_mm, *dy_mm)?
                    }
                    SelectionEdit::Rotate { .. } | SelectionEdit::Mirror { .. } => {
                        transform.as_ref().unwrap().apply(&mut after)?
                    }
                    SelectionEdit::Delete => {}
                }
                validate_geometry(&after, &apertures, &definitions)
                    .map_err(EditError::InvalidGeometry)?;
                validate_block_resolution(document, &after)?;
                if structural {
                    let mut output = object.clone();
                    if inserting {
                        output.object_id = format!("{}-generated-object-{generated}", document.id);
                        if existing.contains(output.object_id.as_str()) {
                            return Err(EditError::InvalidArgument);
                        }
                        let new_group = format!("{}-generated-op-{generated}", document.id);
                        output.origin = match &object.origin {
                            ObjectOrigin::GeneratedText { operation_id } => {
                                ObjectOrigin::GeneratedText {
                                    operation_id: text_copies
                                        .entry(operation_id.clone())
                                        .or_insert(new_group)
                                        .clone(),
                                }
                            }
                            _ => ObjectOrigin::Generated {
                                operation_id: new_group,
                            },
                        };
                        generated = generated.checked_add(1).ok_or(EditError::ResourceLimit)?;
                        output.geometry = after;
                    }
                    ids.push(output.object_id.clone());
                    entries.push(IndexedObject {
                        index: if inserting { index + n + 1 } else { index },
                        object: output,
                    });
                } else {
                    ids.push(object.object_id.clone());
                    changes.push(Change {
                        object_id: object.object_id.clone(),
                        index,
                        before: object.geometry.clone(),
                        after,
                    });
                }
            }
            let before_order: Vec<_> = if structural {
                layer.objects.iter().map(|o| o.object_id.clone()).collect()
            } else {
                vec![]
            };
            let mut after_order = Vec::new();
            if structural {
                let mut selected = indices.iter().enumerate().peekable();
                for (index, id) in before_order.iter().enumerate() {
                    let target = selected.peek().is_some_and(|(_, i)| **i == index);
                    if !target || inserting {
                        after_order.push(id.clone());
                    }
                    if target {
                        let (n, _) = selected.next().unwrap();
                        if inserting {
                            after_order.push(entries[n].object.object_id.clone());
                        }
                    }
                }
            }
            let op = if inserting {
                Operation::Insert(entries)
            } else if structural {
                Operation::Delete(entries)
            } else {
                Operation::Modify(changes)
            };
            transactions.push(Transaction {
                layer_id: layer.id.clone(),
                layer: layer_index,
                operation: op,
                before_order,
                after_order,
                bytes: 0,
            });
            result.push(SelectionGroup {
                layer_id: layer.id.clone(),
                object_ids: ids,
            });
        }
        let transaction = Transaction {
            layer_id: String::new(),
            layer: 0,
            operation: Operation::Selection(transactions),
            before_order: vec![],
            after_order: vec![],
            bytes,
        };
        check_transaction(document, &transaction, true)?;
        self.commit(document, transaction);
        self.next_generated_id = generated;
        Ok(result)
    }
}

fn strict_point<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<MmPoint, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Point {
        x_mm: f64,
        y_mm: f64,
    }
    let p = Point::deserialize(deserializer)?;
    Ok(MmPoint::new(p.x_mm, p.y_mm))
}
