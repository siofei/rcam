//! Complete synthetic selections, exact transactions and route-specific budgets.
use editor_core::edit::*;
use editor_core::*;
fn document(n: usize) -> SemanticDocument {
    SemanticDocument {
        id: "synthetic-capacity".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![SemanticLayer {
            id: "layer".into(),
            objects: (0..n)
                .map(|i| SemanticObject {
                    object_id: format!("object-{i}"),
                    geometry: SemanticGeometry::Line {
                        start: MmPoint::new(i as f64 * 0.01, 0.),
                        end: MmPoint::new(i as f64 * 0.01, 1.),
                        width_mm: 0.1,
                    },
                    exposure: if i % 3 == 0 {
                        Exposure::Clear
                    } else {
                        Exposure::Dark
                    },
                    origin: ObjectOrigin::Imported { command_index: i },
                })
                .collect(),
        }],
        apertures: vec![],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}
fn ids(d: &SemanticDocument) -> Vec<String> {
    d.layers[0]
        .objects
        .iter()
        .map(|o| o.object_id.clone())
        .collect()
}
#[test]
fn eighty_thousand_single_layer_move_has_complete_one_undo_and_exact_redo() {
    let mut d = document(80_000);
    let before = d.clone();
    let targets = ids(&d);
    let mut h = EditHistory::default();
    let demand = h.move_objects_demand(&d, "layer", &targets).unwrap();
    assert_eq!(demand.object_count, 80_000);
    assert!(!demand.multi_layer_route);
    assert_eq!(demand.history_limit_bytes, 64 * 1024 * 1024);
    demand.admit().unwrap();
    assert_eq!(
        h.move_objects(&mut d, "layer", &targets, 1., 2.).unwrap(),
        targets
    );
    assert_eq!(h.undo_len(), 1);
    assert_eq!(h.bytes(), demand.history_bytes);
    for (old, new) in before.layers[0].objects.iter().zip(&d.layers[0].objects) {
        assert_eq!(old.object_id, new.object_id);
        assert_eq!(old.exposure, new.exposure);
        assert_eq!(old.origin, new.origin);
        let SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } = new.geometry
        else {
            panic!()
        };
        let SemanticGeometry::Line {
            start: a,
            end: b,
            width_mm: w,
        } = old.geometry
        else {
            panic!()
        };
        assert_eq!(start, MmPoint::new(a.x_mm + 1., a.y_mm + 2.));
        assert_eq!(end, MmPoint::new(b.x_mm + 1., b.y_mm + 2.));
        assert_eq!(width_mm, w);
    }
    let after = d.clone();
    h.undo(&mut d).unwrap();
    assert_eq!(d, before);
    assert_eq!((h.undo_len(), h.redo_len()), (0, 1));
    h.redo(&mut d).unwrap();
    assert_eq!(d, after);
    assert_eq!((h.undo_len(), h.redo_len()), (1, 0));
}
#[test]
fn multi_layer_route_preserves_real_budget_and_small_route_has_one_undo() {
    let mut d = document(80_000);
    let mut upper = d.layers[0].clone();
    upper.id = "upper".into();
    upper.objects = d.layers[0].objects.split_off(40_000);
    d.layers.push(upper);
    let before = d.clone();
    let groups: Vec<_> = d
        .layers
        .iter()
        .map(|l| SelectionGroup {
            layer_id: l.id.clone(),
            object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
        })
        .collect();
    let mut h = EditHistory::default();
    let demand = h.move_selection_demand(&d, &groups).unwrap();
    assert!(demand.multi_layer_route);
    assert_eq!(demand.object_count, 80_000);
    assert!(demand.history_bytes > MAX_HISTORY_BYTES);
    assert_eq!(demand.admit(), Err(EditError::ResourceLimit));
    assert_eq!(
        h.edit_selection(
            &mut d,
            &groups,
            &SelectionEdit::Move {
                dx_mm: 1.,
                dy_mm: 2.
            }
        ),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d, before);
    assert_eq!(h.undo_len(), 0);
    let small: Vec<_> = groups
        .iter()
        .map(|g| SelectionGroup {
            layer_id: g.layer_id.clone(),
            object_ids: g.object_ids[..6000].to_vec(),
        })
        .collect();
    let demand = h.move_selection_demand(&d, &small).unwrap();
    demand.admit().unwrap();
    let out = h
        .edit_selection(
            &mut d,
            &small,
            &SelectionEdit::Move {
                dx_mm: 1.,
                dy_mm: 2.,
            },
        )
        .unwrap();
    assert_eq!(out, small);
    assert_eq!(h.undo_len(), 1);
    assert_eq!(h.bytes(), demand.history_bytes);
    let after = d.clone();
    h.undo(&mut d).unwrap();
    assert_eq!(d, before);
    h.redo(&mut d).unwrap();
    assert_eq!(d, after);
}
#[test]
fn no_op_invalid_delta_stale_target_and_other_edit_limits_are_atomic() {
    let mut d = document(10_001);
    let before = d.clone();
    let targets = ids(&d);
    let mut h = EditHistory::default();
    for (x, y) in [(0., 0.), (f64::NAN, 0.), (1e9, 1e9)] {
        assert!(h.move_objects(&mut d, "layer", &targets, x, y).is_err());
        assert_eq!(d, before);
        assert_eq!(h.undo_len(), 0);
    }
    let mut missing = targets.clone();
    missing[0] = "missing".into();
    assert!(matches!(
        h.move_objects_demand(&d, "layer", &missing),
        Err(EditError::NotFound { .. })
    ));
    let mut duplicate = targets.clone();
    duplicate[0] = duplicate[1].clone();
    assert_eq!(
        h.move_objects_demand(&d, "layer", &duplicate),
        Err(EditError::InvalidArgument)
    );
    assert_eq!(
        h.rotate_objects(&mut d, "layer", &targets, 90., MmPoint::new(0., 0.)),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(
        h.duplicate_objects(&mut d, "layer", &targets, 1., 1.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(
        h.delete_objects(&mut d, "layer", &targets),
        Err(EditError::ResourceLimit)
    );
    let group = SelectionGroup {
        layer_id: "layer".into(),
        object_ids: targets.clone(),
    };
    assert_eq!(
        h.edit_selection(&mut d, &[group], &SelectionEdit::Delete),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(
        h.edit_batch(
            &mut d,
            "layer",
            &[BatchEdit::Move {
                object_ids: targets.clone(),
                dx_mm: 1.,
                dy_mm: 1.
            }]
        ),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d, before);
    assert_eq!(h.undo_len(), 0);
    let mut tiny = EditHistory::with_limits(100, 1).unwrap();
    let demand = tiny.move_objects_demand(&d, "layer", &targets).unwrap();
    assert_eq!(demand.history_limit_bytes, 1);
    assert_eq!(demand.admit(), Err(EditError::ResourceLimit));
    assert_eq!(
        tiny.move_objects(&mut d, "layer", &targets, 1., 1.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d, before);
}

#[test]
fn complex_region_demand_refuses_before_translation_and_history_allocation() {
    let mut d = document(1);
    let points: Vec<_> = (0..400_000)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 400_000.;
            MmPoint::new(a.cos(), a.sin())
        })
        .collect();
    let edges = points
        .iter()
        .enumerate()
        .map(|(i, p)| RegionEdge::Line {
            start: *p,
            end: points[(i + 1) % points.len()],
        })
        .collect();
    d.layers[0].objects[0].geometry = SemanticGeometry::Region {
        contours: vec![RegionContour {
            edges,
            role: RegionRole::Solid,
        }],
    };
    let targets = ids(&d);
    let mut h = EditHistory::default();
    let demand = h.move_objects_demand(&d, "layer", &targets).unwrap();
    assert!(demand.history_bytes > MAX_HISTORY_BYTES);
    assert_eq!(demand.admit(), Err(EditError::ResourceLimit));
    let before = d.layers[0].objects[0].geometry.clone();
    assert_eq!(
        h.move_objects(&mut d, "layer", &targets, 1., 1.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d.layers[0].objects[0].geometry, before);
    assert_eq!((h.undo_len(), h.redo_len(), h.bytes()), (0, 0, 0));
}

#[test]
fn block_expansion_peak_and_repeated_instance_validation_are_bounded_lazily() {
    use editor_core::block::*;
    let mut d = document(1);
    let id = BlockDefinitionId("large".into());
    let object = BlockObject {
        geometry: BlockObjectGeometry::Line {
            start: MmPoint::new(0., 0.),
            end: MmPoint::new(1., 0.),
            width_mm: 0.1,
        },
        exposure: Exposure::Dark,
    };
    d.block_definitions.push(BlockDefinition {
        id: id.clone(),
        name: "synthetic".into(),
        local_origin: MmPoint::new(0., 0.),
        objects: vec![object; 1_000_000],
        revision: 0,
    });
    let targets = ids(&d);
    let mut h = EditHistory::default();
    // Unrelated large definitions are not scanned/expanded when moving a line.
    let mut calls = 0;
    let plain = h
        .move_objects_demand_checked(&d, "layer", &targets, &mut || {
            calls += 1;
            Ok(())
        })
        .unwrap();
    plain.admit().unwrap();
    assert!(calls < 10);
    d.layers[0].objects[0].geometry = SemanticGeometry::BlockInstance {
        definition_id: id,
        transform: BlockTransform::IDENTITY,
    };
    let demand = h.move_objects_demand(&d, "layer", &targets).unwrap();
    assert!(demand.history_bytes < MAX_HISTORY_BYTES);
    assert!(demand.work_bytes > MAX_MOVE_WORK_BYTES);
    assert_eq!(demand.admit(), Err(EditError::ResourceLimit));
    assert_eq!(
        h.move_objects(&mut d, "layer", &targets, 1., 1.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(h.undo_len(), 0);
    d.block_definitions[0].objects.truncate(10_000);
    // Work-only refusal uses an ordinary 10000-child definition. Its temporary
    // expansion fits memory, but 201 full per-instance validations exceed work.
    d.block_definitions[0].objects.truncate(10_000);
    let template = d.layers[0].objects[0].clone();
    d.layers[0].objects = (0..201)
        .map(|n| SemanticObject {
            object_id: format!("instance-{n}"),
            ..template.clone()
        })
        .collect();
    let before = d.layers[0].objects.clone();
    let targets = ids(&d);
    let demand = h.move_objects_demand(&d, "layer", &targets).unwrap();
    assert!(demand.history_bytes < MAX_HISTORY_BYTES);
    assert!(demand.work_bytes <= MAX_MOVE_WORK_BYTES);
    assert!(demand.validation_work > demand.validation_work_limit);
    assert_eq!(demand.admit(), Err(EditError::ResourceLimit));
    assert_eq!(
        h.move_objects(&mut d, "layer", &targets, 1., 1.),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d.layers[0].objects, before);
    assert_eq!(h.undo_len(), 0);
}
#[test]
fn demand_loop_checkpoint_stops_without_any_geometry_or_history_mutation() {
    let d = document(80_000);
    let targets = ids(&d);
    let h = EditHistory::default();
    let mut calls = 0;
    let result = h.move_objects_demand_checked(&d, "layer", &targets, &mut || {
        calls += 1;
        if calls == 3 {
            Err(EditError::ResourceLimit)
        } else {
            Ok(())
        }
    });
    assert_eq!(result, Err(EditError::ResourceLimit));
    assert_eq!(calls, 3);
    assert_eq!((h.undo_len(), h.redo_len(), h.bytes()), (0, 0, 0));
    assert_eq!(d.layers[0].objects.len(), 80_000);
}
