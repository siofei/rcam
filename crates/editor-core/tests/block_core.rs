//! S4-B2 Block Core: `EditHistory` create/duplicate/rotate/mirror/move/
//! explode/delete/undo/redo, plus bounds/metrics/hit-test resolution.
use editor_core::block::BlockObjectGeometry;
use editor_core::edit::{EditHistory, MirrorAxis};
use editor_core::*;

fn close_bounds(a: Option<BoundsMm>, b: Option<BoundsMm>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            (a.min_x_mm - b.min_x_mm).abs() < 1e-9
                && (a.min_y_mm - b.min_y_mm).abs() < 1e-9
                && (a.max_x_mm - b.max_x_mm).abs() < 1e-9
                && (a.max_y_mm - b.max_y_mm).abs() < 1e-9
        }
        _ => false,
    }
}

fn format() -> SemanticFormat {
    SemanticFormat {
        integer: 4,
        decimal: 6,
        leading_zero_omission: true,
        absolute: true,
    }
}

fn empty_doc() -> SemanticDocument {
    SemanticDocument {
        id: "block-core".into(),
        unit: "mm".into(),
        format: format(),
        layers: vec![SemanticLayer {
            id: "l1".into(),
            objects: vec![],
        }],
        apertures: vec![],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}

fn line(id: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> SemanticObject {
    SemanticObject {
        object_id: id.into(),
        geometry: SemanticGeometry::Line {
            start: MmPoint::new(x0, y0),
            end: MmPoint::new(x1, y1),
            width_mm: 0.2,
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 0 },
    }
}

/// Four lines forming a small square-ish fixture, all on layer `l1`.
fn seed_four_lines(doc: &mut SemanticDocument) {
    doc.layers[0].objects = vec![
        line("o1", 0., 0., 1., 0.),
        line("o2", 1., 0., 1., 1.),
        line("o3", 1., 1., 0., 1.),
        line("o4", 0., 1., 0., 0.),
    ];
}

#[test]
fn create_definition_preserves_world_appearance_and_ids_are_stable() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let before_bounds = doc.manufacturing_bounds(None).unwrap();
    let mut history = EditHistory::default();

    let object_ids = vec![
        "o1".to_string(),
        "o2".to_string(),
        "o3".to_string(),
        "o4".to_string(),
    ];
    let (definition_id, instance_id) = history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "square".into(),
        )
        .unwrap();

    assert_eq!(doc.layers[0].objects.len(), 1);
    assert_eq!(doc.layers[0].objects[0].object_id, instance_id);
    assert_eq!(doc.block_definitions.len(), 1);
    assert_eq!(doc.block_definitions[0].id, definition_id);
    assert_eq!(doc.block_definitions[0].objects.len(), 4);
    doc.validate().unwrap();

    // World appearance (bounds) is exactly preserved by the capture.
    let after_bounds = doc.manufacturing_bounds(None).unwrap();
    assert!(close_bounds(before_bounds, after_bounds));

    // The definition id is stable across a second lookup and a clone.
    let same_id = doc.block_definition(&definition_id).unwrap().id.clone();
    assert_eq!(same_id, definition_id);
}

#[test]
fn no_nested_block_from_capture() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let mut history = EditHistory::default();
    let object_ids = vec!["o1".into(), "o2".into(), "o3".into(), "o4".into()];
    history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "sq".into(),
        )
        .unwrap();
    let instance_id = doc.layers[0].objects[0].object_id.clone();
    // Trying to capture the instance itself into a new definition must fail:
    // `BlockObjectGeometry` cannot represent a nested `BlockInstance`.
    let err = history
        .create_block_definition(
            &mut doc,
            "l1",
            std::slice::from_ref(&instance_id),
            MmPoint::new(0., 0.),
            "nested".into(),
        )
        .unwrap_err();
    assert_eq!(err, editor_core::edit::EditError::UnsupportedTransform);
}

#[test]
fn duplicate_rotate_mirror_move_all_reuse_the_generic_object_edit_ops() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let mut history = EditHistory::default();
    let object_ids = vec!["o1".into(), "o2".into(), "o3".into(), "o4".into()];
    let (definition_id, instance_id) = history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "sq".into(),
        )
        .unwrap();

    // Duplicate: cheap (copies definition_id + transform, not geometry).
    let dup_ids = history
        .duplicate_objects(&mut doc, "l1", std::slice::from_ref(&instance_id), 5., 0.)
        .unwrap();
    assert_eq!(dup_ids.len(), 1);
    let dup_id = dup_ids[0].clone();
    let SemanticGeometry::BlockInstance {
        definition_id: dup_def,
        transform: dup_transform,
    } = &doc.layers[0]
        .objects
        .iter()
        .find(|o| o.object_id == dup_id)
        .unwrap()
        .geometry
    else {
        panic!("expected a block instance");
    };
    assert_eq!(*dup_def, definition_id);
    assert!((dup_transform.translation.x_mm - 5.5).abs() < 1e-9);
    let dup_pivot = dup_transform.translation;

    // Rotate the duplicate 90 degrees about its own translation: only its
    // transform changes, never the shared definition.
    history
        .rotate_objects(
            &mut doc,
            "l1",
            std::slice::from_ref(&dup_id),
            90.,
            dup_pivot,
        )
        .unwrap();
    let rotated = doc.layers[0]
        .objects
        .iter()
        .find(|o| o.object_id == dup_id)
        .unwrap();
    let SemanticGeometry::BlockInstance { transform, .. } = &rotated.geometry else {
        panic!()
    };
    assert!((transform.rotation_deg - 90.).abs() < 1e-9);
    assert_eq!(
        doc.block_definitions.len(),
        1,
        "no extra definition created"
    );

    // Mirror the original instance: still one definition, shared.
    history
        .mirror_objects(
            &mut doc,
            "l1",
            std::slice::from_ref(&instance_id),
            MirrorAxis::Vertical { coordinate_mm: 0. },
        )
        .unwrap();
    let mirrored = doc.layers[0]
        .objects
        .iter()
        .find(|o| o.object_id == instance_id)
        .unwrap();
    let SemanticGeometry::BlockInstance { transform, .. } = &mirrored.geometry else {
        panic!()
    };
    assert!(transform.mirror);
    assert_eq!(doc.block_definitions.len(), 1);

    // Move: only translation changes.
    history
        .move_objects(&mut doc, "l1", std::slice::from_ref(&instance_id), 2., 3.)
        .unwrap();
    let moved = doc.layers[0]
        .objects
        .iter()
        .find(|o| o.object_id == instance_id)
        .unwrap();
    let SemanticGeometry::BlockInstance { transform, .. } = &moved.geometry else {
        panic!()
    };
    // The instance was mirrored about x=0 before this move (translation.x
    // flipped from 0.5 to -0.5), so the move lands at (-0.5+2, 0.5+3).
    assert!((transform.translation.x_mm - 1.5).abs() < 1e-9);
    assert!((transform.translation.y_mm - 3.5).abs() < 1e-9);
    doc.validate().unwrap();
}

#[test]
fn definition_edit_affects_all_instances_but_instance_edit_affects_one() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let mut history = EditHistory::default();
    let object_ids = vec!["o1".into(), "o2".into(), "o3".into(), "o4".into()];
    let (definition_id, instance_id) = history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "sq".into(),
        )
        .unwrap();
    let other_id = history
        .duplicate_objects(&mut doc, "l1", std::slice::from_ref(&instance_id), 5., 0.)
        .unwrap()[0]
        .clone();

    let before_a = doc.manufacturing_bounds(None).unwrap().unwrap();

    // Instance edit: moving one instance never touches the shared definition.
    history
        .move_objects(&mut doc, "l1", std::slice::from_ref(&instance_id), 1., 0.)
        .unwrap();
    let after_move = doc.manufacturing_bounds(None).unwrap().unwrap();
    assert_ne!(before_a, after_move);
    assert_eq!(doc.block_definitions[0].revision, 0);

    // Definition edit: mutate the shared geometry directly (no dedicated
    // Block Editor ships this phase) and bump its revision; both instances
    // must reflect the new shape without any per-instance geometry copy.
    let definition = doc
        .block_definitions
        .iter_mut()
        .find(|d| d.id == definition_id)
        .unwrap();
    if let BlockObjectGeometry::Line { width_mm, .. } = &mut definition.objects[0].geometry {
        *width_mm = 0.5;
    }
    definition.revision += 1;
    doc.validate().unwrap();

    let metrics_a = metrics::calculate(
        &doc,
        &doc.layers[0]
            .objects
            .iter()
            .find(|o| o.object_id == instance_id)
            .unwrap()
            .geometry,
        &mut 0,
    )
    .unwrap();
    let metrics_b = metrics::calculate(
        &doc,
        &doc.layers[0]
            .objects
            .iter()
            .find(|o| o.object_id == other_id)
            .unwrap()
            .geometry,
        &mut 0,
    )
    .unwrap();
    assert_eq!(
        metrics_a, metrics_b,
        "both instances see the same new definition"
    );
}

#[test]
fn delete_referenced_definition_is_rejected_explode_then_allows_it() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let mut history = EditHistory::default();
    let object_ids = vec!["o1".into(), "o2".into(), "o3".into(), "o4".into()];
    let (definition_id, instance_id) = history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "sq".into(),
        )
        .unwrap();

    let err = history
        .delete_block_definition(&mut doc, &definition_id)
        .unwrap_err();
    assert_eq!(err, editor_core::edit::EditError::BlockDefinitionReferenced);

    let before_bounds = doc.manufacturing_bounds(None).unwrap();
    let exploded_ids = history
        .explode_block_instance(&mut doc, "l1", &instance_id)
        .unwrap();
    assert_eq!(exploded_ids.len(), 5, "1 removed + 4 inserted primitives");
    assert_eq!(doc.layers[0].objects.len(), 4);
    assert!(
        doc.layers[0]
            .objects
            .iter()
            .all(|o| !matches!(o.geometry, SemanticGeometry::BlockInstance { .. }))
    );
    let after_bounds = doc.manufacturing_bounds(None).unwrap();
    assert!(
        close_bounds(before_bounds, after_bounds),
        "explode preserves world geometry"
    );
    // Definition still exists; only the instance was removed.
    assert!(doc.block_definition(&definition_id).is_some());

    history
        .delete_block_definition(&mut doc, &definition_id)
        .unwrap();
    assert!(doc.block_definitions.is_empty());
}

#[test]
fn undo_redo_restores_definitions_and_instances_exactly() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let original = doc.clone();
    let mut history = EditHistory::default();
    let object_ids = vec!["o1".into(), "o2".into(), "o3".into(), "o4".into()];
    history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "sq".into(),
        )
        .unwrap();
    assert_eq!(doc.block_definitions.len(), 1);

    history.undo(&mut doc).unwrap();
    assert!(doc.block_definitions.is_empty());
    assert_eq!(doc.layers[0].objects.len(), 4);
    assert_eq!(doc, original);

    history.redo(&mut doc).unwrap();
    assert_eq!(doc.block_definitions.len(), 1);
    assert_eq!(doc.layers[0].objects.len(), 1);
}

#[test]
fn rotating_a_block_instance_mirror_arc_child_flips_direction() {
    let mut doc = empty_doc();
    doc.layers[0].objects = vec![SemanticObject {
        object_id: "arc1".into(),
        geometry: SemanticGeometry::Arc {
            path: ArcGeometry {
                start: MmPoint::new(1., 0.),
                end: MmPoint::new(0., 1.),
                center: MmPoint::new(0., 0.),
                direction: ArcDirection::CounterClockwise,
                full_circle: false,
                source: None,
            },
            width_mm: 0.2,
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 0 },
    }];
    let mut history = EditHistory::default();
    let (_, instance_id) = history
        .create_block_definition(
            &mut doc,
            "l1",
            &["arc1".to_string()],
            MmPoint::new(0., 0.),
            "arcdef".into(),
        )
        .unwrap();
    history
        .mirror_objects(
            &mut doc,
            "l1",
            std::slice::from_ref(&instance_id),
            MirrorAxis::Vertical { coordinate_mm: 0. },
        )
        .unwrap();
    let resolved = {
        let object = doc.layers[0]
            .objects
            .iter()
            .find(|o| o.object_id == instance_id)
            .unwrap();
        let SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } = &object.geometry
        else {
            panic!()
        };
        block::resolve_instance(doc.block_definition(definition_id).unwrap(), transform).unwrap()
    };
    let SemanticGeometry::Arc { path, .. } = &resolved[0].geometry else {
        panic!()
    };
    assert_eq!(path.direction, ArcDirection::Clockwise);
}

#[test]
fn hit_test_returns_whole_instance_identity() {
    let mut doc = empty_doc();
    seed_four_lines(&mut doc);
    let mut history = EditHistory::default();
    let object_ids = vec!["o1".into(), "o2".into(), "o3".into(), "o4".into()];
    let (_, instance_id) = history
        .create_block_definition(
            &mut doc,
            "l1",
            &object_ids,
            MmPoint::new(0.5, 0.5),
            "sq".into(),
        )
        .unwrap();
    // A point exactly on the bottom edge of the captured square (0,0)-(1,0).
    let hits = doc.hit_test("l1", MmPoint::new(0.5, 0.0), 0.15).unwrap();
    assert_eq!(hits, vec![instance_id]);
}
