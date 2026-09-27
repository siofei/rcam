use editor_core::alignment::{
    AlignmentMode, DistributionAxis, ObjectBoundsMm, compute_distribution_deltas,
};
use editor_core::block::{BlockDefinition, BlockDefinitionId, BlockObject, BlockObjectGeometry};
use editor_core::edit::{EditError, EditHistory};
use editor_core::*;
use std::collections::BTreeMap;

fn format() -> SemanticFormat {
    SemanticFormat {
        integer: 4,
        decimal: 6,
        leading_zero_omission: true,
        absolute: true,
    }
}

fn document(
    id: &str,
    objects: Vec<SemanticObject>,
    apertures: Vec<ApertureDefinition>,
    block_definitions: Vec<BlockDefinition>,
) -> SemanticDocument {
    SemanticDocument {
        id: id.into(),
        unit: "mm".into(),
        format: format(),
        layers: vec![SemanticLayer {
            id: "layer".into(),
            objects,
        }],
        apertures,
        source: SourceMetadata::default(),
        block_definitions,
    }
}

fn aperture_circle(id: &str, diameter_mm: f64) -> ApertureDefinition {
    ApertureDefinition {
        id: id.into(),
        source_dcode: 10,
        shape: ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm: None,
        },
    }
}

fn aperture_rectangle(id: &str, width_mm: f64, height_mm: f64) -> ApertureDefinition {
    ApertureDefinition {
        id: id.into(),
        source_dcode: 10,
        shape: ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm: None,
        },
    }
}

fn object(id: &str, geometry: SemanticGeometry, exposure: Exposure) -> SemanticObject {
    SemanticObject {
        object_id: id.into(),
        geometry,
        exposure,
        origin: ObjectOrigin::Imported { command_index: 0 },
    }
}

fn flash(id: &str, aperture_id: &str, center: MmPoint, exposure: Exposure) -> SemanticObject {
    object(
        id,
        SemanticGeometry::Flash {
            center,
            aperture_id: aperture_id.into(),
            transform: LocalTransform::default(),
        },
        exposure,
    )
}

fn rectangular_flash(
    id: &str,
    aperture_id: &str,
    center: MmPoint,
    rotation_deg: f64,
    exposure: Exposure,
) -> SemanticObject {
    object(
        id,
        SemanticGeometry::Flash {
            center,
            aperture_id: aperture_id.into(),
            transform: LocalTransform {
                rotation_deg,
                ..LocalTransform::default()
            },
        },
        exposure,
    )
}

fn line(
    id: &str,
    start: MmPoint,
    end: MmPoint,
    width_mm: f64,
    exposure: Exposure,
) -> SemanticObject {
    object(
        id,
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        },
        exposure,
    )
}

fn bounds(document: &SemanticDocument, object_id: &str) -> BoundsMm {
    let geometry = &document.layers[0]
        .objects
        .iter()
        .find(|object| object.object_id == object_id)
        .unwrap()
        .geometry;
    geometries_bounds_with_blocks([geometry], &document.apertures, &document.block_definitions)
        .unwrap()
        .unwrap()
}

fn flash_center(document: &SemanticDocument, object_id: &str) -> MmPoint {
    let object = document.layers[0]
        .objects
        .iter()
        .find(|object| object.object_id == object_id)
        .unwrap();
    let SemanticGeometry::Flash { center, .. } = object.geometry else {
        panic!("expected Flash {object_id}");
    };
    center
}

#[test]
fn all_six_alignment_modes_use_the_anchor_world_bounds_and_one_exact_undo() {
    let cases = [
        (AlignmentMode::Left, MmPoint::new(1.0, 20.0)),
        (AlignmentMode::Right, MmPoint::new(-1.0, 20.0)),
        (AlignmentMode::Top, MmPoint::new(10.0, -1.0)),
        (AlignmentMode::Bottom, MmPoint::new(10.0, 1.0)),
        (AlignmentMode::HCenter, MmPoint::new(0.0, 20.0)),
        (AlignmentMode::VCenter, MmPoint::new(10.0, 0.0)),
    ];
    for (mode, expected) in cases {
        let mut doc = document(
            "alignment-modes",
            vec![
                flash("anchor", "small", MmPoint::new(0.0, 0.0), Exposure::Dark),
                flash("move", "large", MmPoint::new(10.0, 20.0), Exposure::Clear),
            ],
            vec![aperture_circle("small", 2.0), aperture_circle("large", 4.0)],
            vec![],
        );
        let original = doc.clone();
        let mut history = EditHistory::default();
        let selected = vec!["move".into(), "anchor".into()];
        let changed = history
            .align_objects(&mut doc, "layer", &selected, "anchor", mode)
            .unwrap();
        assert_eq!(changed, ["move"]);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(flash_center(&doc, "move"), expected, "{mode:?}");
        assert_eq!(
            doc.layers[0]
                .objects
                .iter()
                .map(|object| object.object_id.as_str())
                .collect::<Vec<_>>(),
            ["anchor", "move"],
            "alignment must preserve exposure order"
        );
        history.undo(&mut doc).unwrap();
        assert_eq!(doc, original, "exact Undo for {mode:?}");
        history.redo(&mut doc).unwrap();
        assert_eq!(
            flash_center(&doc, "move"),
            expected,
            "exact Redo for {mode:?}"
        );
    }
}

#[test]
fn horizontal_and_vertical_distribution_use_equal_edge_gaps_and_allow_overlap() {
    for axis in [DistributionAxis::Horizontal, DistributionAxis::Vertical] {
        let mut doc = document(
            "negative-gap",
            vec![
                flash("first", "two", MmPoint::new(0.0, 0.0), Exposure::Dark),
                flash("middle", "four", MmPoint::new(2.0, 2.0), Exposure::Clear),
                flash("last", "two", MmPoint::new(3.0, 3.0), Exposure::Dark),
            ],
            vec![
                aperture_rectangle("two", 2.0, 2.0),
                aperture_rectangle("four", 4.0, 4.0),
            ],
            vec![],
        );
        let first_before = bounds(&doc, "first");
        let last_before = bounds(&doc, "last");
        let selected = vec!["last".into(), "middle".into(), "first".into()];
        let mut history = EditHistory::default();
        let changed = history
            .distribute_objects(&mut doc, "layer", &selected, axis)
            .unwrap();
        assert_eq!(changed, ["middle"]);
        assert_eq!(history.undo_len(), 1);
        let first_after = bounds(&doc, "first");
        let middle_after = bounds(&doc, "middle");
        let last_after = bounds(&doc, "last");
        assert_eq!(first_after, first_before, "first endpoint stays fixed");
        assert_eq!(last_after, last_before, "last endpoint stays fixed");
        match axis {
            DistributionAxis::Horizontal => {
                assert_eq!(middle_after.min_x_mm, -0.5);
                assert_eq!(
                    middle_after.min_x_mm - first_after.max_x_mm,
                    last_after.min_x_mm - middle_after.max_x_mm
                );
            }
            DistributionAxis::Vertical => {
                assert_eq!(middle_after.min_y_mm, -0.5);
                assert_eq!(
                    middle_after.min_y_mm - first_after.max_y_mm,
                    last_after.min_y_mm - middle_after.max_y_mm
                );
            }
        }
    }
}

#[test]
fn four_unequal_objects_get_equal_horizontal_gaps_with_fixed_endpoints() {
    let mut doc = document(
        "four-object-distribution",
        vec![
            flash("first", "w2", MmPoint::new(0.0, 0.0), Exposure::Dark),
            flash("second", "w4", MmPoint::new(4.0, 0.0), Exposure::Dark),
            flash("third", "w1", MmPoint::new(8.0, 0.0), Exposure::Dark),
            flash("last", "w3", MmPoint::new(13.0, 0.0), Exposure::Dark),
        ],
        vec![
            aperture_rectangle("w1", 1.0, 2.0),
            aperture_rectangle("w2", 2.0, 2.0),
            aperture_rectangle("w3", 3.0, 2.0),
            aperture_rectangle("w4", 4.0, 2.0),
        ],
        vec![],
    );
    let endpoints = (bounds(&doc, "first"), bounds(&doc, "last"));
    let mut history = EditHistory::default();
    history
        .distribute_objects(
            &mut doc,
            "layer",
            &[
                "third".into(),
                "last".into(),
                "first".into(),
                "second".into(),
            ],
            DistributionAxis::Horizontal,
        )
        .unwrap();
    assert_eq!(bounds(&doc, "first"), endpoints.0);
    assert_eq!(bounds(&doc, "last"), endpoints.1);
    let first_gap = bounds(&doc, "second").min_x_mm - bounds(&doc, "first").max_x_mm;
    let second_gap = bounds(&doc, "third").min_x_mm - bounds(&doc, "second").max_x_mm;
    let third_gap = bounds(&doc, "last").min_x_mm - bounds(&doc, "third").max_x_mm;
    assert!((first_gap - second_gap).abs() < 1e-9);
    assert!((second_gap - third_gap).abs() < 1e-9);
}

#[test]
fn distribution_ties_use_layer_order_and_ignore_selection_order() {
    let objects = vec![
        ObjectBoundsMm {
            object_ids: vec!["a".into()],
            layer_order: 0,
            bounds: BoundsMm {
                min_x_mm: 0.0,
                min_y_mm: 0.0,
                max_x_mm: 2.0,
                max_y_mm: 1.0,
            },
        },
        ObjectBoundsMm {
            object_ids: vec!["b".into()],
            layer_order: 1,
            bounds: BoundsMm {
                min_x_mm: 0.0,
                min_y_mm: 0.0,
                max_x_mm: 4.0,
                max_y_mm: 1.0,
            },
        },
        ObjectBoundsMm {
            object_ids: vec!["c".into()],
            layer_order: 2,
            bounds: BoundsMm {
                min_x_mm: 0.0,
                min_y_mm: 0.0,
                max_x_mm: 2.0,
                max_y_mm: 1.0,
            },
        },
        ObjectBoundsMm {
            object_ids: vec!["d".into()],
            layer_order: 3,
            bounds: BoundsMm {
                min_x_mm: 10.0,
                min_y_mm: 0.0,
                max_x_mm: 12.0,
                max_y_mm: 1.0,
            },
        },
    ];
    let forward = compute_distribution_deltas(&objects, DistributionAxis::Horizontal).unwrap();
    let mut permuted = objects.clone();
    permuted.reverse();
    let reverse = compute_distribution_deltas(&permuted, DistributionAxis::Horizontal).unwrap();
    let as_map = |deltas: Vec<editor_core::alignment::ObjectDelta>| {
        deltas
            .into_iter()
            .map(|delta| (delta.object_ids[0].clone(), (delta.dx_mm, delta.dy_mm)))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(as_map(forward.clone()), as_map(reverse));
    assert_eq!(forward[0].object_ids, ["a"]);
    assert_eq!(forward[1].object_ids, ["b"]);
    assert_eq!(forward[2].object_ids, ["c"]);
    assert_eq!(forward[3].object_ids, ["d"]);
    assert_eq!(forward[0].dx_mm, 0.0);
    assert_eq!(forward[3].dx_mm, 0.0);
}

#[test]
fn exact_noops_preserve_history_and_redo_branch() {
    let mut aligned = document(
        "aligned-noop",
        vec![
            flash("a", "circle", MmPoint::new(0.0, 0.0), Exposure::Dark),
            flash("b", "circle", MmPoint::new(0.0, 5.0), Exposure::Clear),
        ],
        vec![aperture_circle("circle", 2.0)],
        vec![],
    );
    let mut history = EditHistory::default();
    history
        .move_objects(&mut aligned, "layer", &["a".into()], 1.0, 0.0)
        .unwrap();
    history.undo(&mut aligned).unwrap();
    assert_eq!(history.redo_len(), 1);
    let original = aligned.clone();
    let changed = history
        .align_objects(
            &mut aligned,
            "layer",
            &["b".into(), "a".into()],
            "a",
            AlignmentMode::Left,
        )
        .unwrap();
    assert!(changed.is_empty());
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 1);
    assert_eq!(aligned, original);

    let mut distributed = document(
        "distributed-noop",
        vec![
            flash("first", "circle", MmPoint::new(0.0, 0.0), Exposure::Dark),
            flash("middle", "circle", MmPoint::new(5.0, 0.0), Exposure::Clear),
            flash("last", "circle", MmPoint::new(10.0, 0.0), Exposure::Dark),
        ],
        vec![aperture_circle("circle", 2.0)],
        vec![],
    );
    let before = distributed.clone();
    let mut no_history = EditHistory::default();
    assert!(
        no_history
            .distribute_objects(
                &mut distributed,
                "layer",
                &["last".into(), "middle".into(), "first".into()],
                DistributionAxis::Horizontal,
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(distributed, before);
    assert_eq!(no_history.undo_len(), 0);
}

#[test]
fn sub_grid_alignment_delta_is_applied_exactly_and_undo_restores_storage() {
    let anchor_x = 1.0;
    let mover_x = anchor_x + 1.0e-10;
    let mut doc = document(
        "sub-grid-alignment",
        vec![
            flash(
                "anchor",
                "circle",
                MmPoint::new(anchor_x, 1.0),
                Exposure::Dark,
            ),
            flash(
                "move",
                "circle",
                MmPoint::new(mover_x, 1.0),
                Exposure::Clear,
            ),
        ],
        vec![aperture_circle("circle", 1.0)],
        vec![],
    );
    let original = doc.clone();
    let mut history = EditHistory::default();
    let changed = history
        .align_objects(
            &mut doc,
            "layer",
            &["anchor".into(), "move".into()],
            "anchor",
            AlignmentMode::Left,
        )
        .unwrap();
    assert_eq!(changed, ["move"]);
    let aligned_x = flash_center(&doc, "move").x_mm;
    assert_eq!(aligned_x, anchor_x);
    assert!(mover_x - aligned_x > 0.0 && mover_x - aligned_x < 2.0e-10);
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, original);
    history.redo(&mut doc).unwrap();
    assert_eq!(flash_center(&doc, "move").x_mm, anchor_x);
}

#[test]
fn mixed_manufacturing_shapes_align_in_world_space_without_reordering_or_editing_blocks() {
    let block_id = BlockDefinitionId("block-shape".into());
    let definition = BlockDefinition {
        id: block_id.clone(),
        name: "sparse".into(),
        local_origin: MmPoint::new(0.0, 0.0),
        objects: vec![
            BlockObject {
                geometry: BlockObjectGeometry::Flash {
                    center: MmPoint::new(-2.0, 0.0),
                    aperture_id: "circle".into(),
                    transform: LocalTransform::default(),
                },
                exposure: Exposure::Dark,
            },
            BlockObject {
                geometry: BlockObjectGeometry::Flash {
                    center: MmPoint::new(2.0, 0.0),
                    aperture_id: "circle".into(),
                    transform: LocalTransform::default(),
                },
                exposure: Exposure::Clear,
            },
        ],
        revision: 7,
    };
    let arc = ArcGeometry {
        start: MmPoint::new(14.0, 0.0),
        end: MmPoint::new(16.0, 0.0),
        center: MmPoint::new(15.0, 0.0),
        direction: ArcDirection::CounterClockwise,
        full_circle: false,
        source: None,
    };
    let region = SemanticGeometry::Region {
        contours: vec![RegionContour {
            edges: vec![
                RegionEdge::Line {
                    start: MmPoint::new(20.0, 0.0),
                    end: MmPoint::new(22.0, 0.0),
                },
                RegionEdge::Line {
                    start: MmPoint::new(22.0, 0.0),
                    end: MmPoint::new(22.0, 2.0),
                },
                RegionEdge::Line {
                    start: MmPoint::new(22.0, 2.0),
                    end: MmPoint::new(20.0, 2.0),
                },
                RegionEdge::Line {
                    start: MmPoint::new(20.0, 2.0),
                    end: MmPoint::new(20.0, 0.0),
                },
            ],
            role: RegionRole::Solid,
        }],
    };
    let objects = vec![
        rectangular_flash(
            "anchor",
            "rect",
            MmPoint::new(0.0, 0.0),
            45.0,
            Exposure::Dark,
        ),
        line(
            "line",
            MmPoint::new(8.0, 0.0),
            MmPoint::new(10.0, 2.0),
            0.4,
            Exposure::Clear,
        ),
        object(
            "arc",
            SemanticGeometry::Arc {
                path: arc,
                width_mm: 0.2,
            },
            Exposure::Dark,
        ),
        object("region", region, Exposure::Clear),
        object(
            "block",
            SemanticGeometry::BlockInstance {
                definition_id: block_id,
                transform: block::BlockTransform {
                    translation: MmPoint::new(30.0, 1.0),
                    rotation_deg: 45.0,
                    mirror: true,
                },
            },
            Exposure::Dark,
        ),
    ];
    let mut doc = document(
        "mixed-alignment",
        objects,
        vec![
            aperture_rectangle("rect", 4.0, 2.0),
            aperture_circle("circle", 1.0),
        ],
        vec![definition],
    );
    let before = doc.clone();
    let before_order = doc.layers[0]
        .objects
        .iter()
        .map(|object| object.object_id.clone())
        .collect::<Vec<_>>();
    let anchor_min_x = bounds(&doc, "anchor").min_x_mm;
    let block_before = doc.layers[0]
        .objects
        .iter()
        .find(|object| object.object_id == "block")
        .unwrap()
        .geometry
        .clone();
    let mut history = EditHistory::default();
    let selected = vec![
        "block".into(),
        "region".into(),
        "arc".into(),
        "line".into(),
        "anchor".into(),
    ];
    let changed = history
        .align_objects(&mut doc, "layer", &selected, "anchor", AlignmentMode::Left)
        .unwrap();
    assert_eq!(changed, ["line", "arc", "region", "block"]);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(
        doc.layers[0]
            .objects
            .iter()
            .map(|object| object.object_id.clone())
            .collect::<Vec<_>>(),
        before_order
    );
    for id in ["anchor", "line", "arc", "region", "block"] {
        assert!(
            (bounds(&doc, id).min_x_mm - anchor_min_x).abs() < 1e-9,
            "{id}"
        );
    }
    assert_eq!(doc.block_definitions, before.block_definitions);
    let block_after = &doc.layers[0]
        .objects
        .iter()
        .find(|object| object.object_id == "block")
        .unwrap()
        .geometry;
    let (
        SemanticGeometry::BlockInstance {
            transform: before_transform,
            ..
        },
        SemanticGeometry::BlockInstance {
            transform: after_transform,
            ..
        },
    ) = (&block_before, block_after)
    else {
        panic!("expected a block instance");
    };
    assert_ne!(before_transform.translation, after_transform.translation);
    assert_eq!(before_transform.rotation_deg, after_transform.rotation_deg);
    assert_eq!(before_transform.mirror, after_transform.mirror);
    assert_eq!(
        doc.layers[0]
            .objects
            .iter()
            .map(|object| object.exposure)
            .collect::<Vec<_>>(),
        before.layers[0]
            .objects
            .iter()
            .map(|object| object.exposure)
            .collect::<Vec<_>>()
    );
    let after = doc.clone();
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, before);
    history.redo(&mut doc).unwrap();
    assert_eq!(doc, after);
}

#[test]
fn rotated_block_bounds_resolve_sparse_child_geometry_instead_of_rotating_its_aabb() {
    let definition = BlockDefinition {
        id: BlockDefinitionId("sparse-block".into()),
        name: "two circles".into(),
        local_origin: MmPoint::new(0.0, 0.0),
        objects: [-10.0, 10.0]
            .into_iter()
            .map(|x| BlockObject {
                geometry: BlockObjectGeometry::Flash {
                    center: MmPoint::new(x, 0.0),
                    aperture_id: "circle".into(),
                    transform: LocalTransform::default(),
                },
                exposure: Exposure::Dark,
            })
            .collect(),
        revision: 1,
    };
    let geometry = SemanticGeometry::BlockInstance {
        definition_id: definition.id.clone(),
        transform: block::BlockTransform {
            translation: MmPoint::new(0.0, 0.0),
            rotation_deg: 45.0,
            mirror: false,
        },
    };
    let expected_extent = 10.0 / 2.0_f64.sqrt() + 0.5;
    let actual = geometries_bounds_with_blocks(
        [&geometry],
        &[aperture_circle("circle", 1.0)],
        std::slice::from_ref(&definition),
    )
    .unwrap()
    .unwrap();
    assert!((actual.min_x_mm + expected_extent).abs() < 1e-9);
    assert!((actual.max_x_mm - expected_extent).abs() < 1e-9);
    assert!((actual.min_y_mm + expected_extent).abs() < 1e-9);
    assert!((actual.max_y_mm - expected_extent).abs() < 1e-9);
    assert!(actual.max_x_mm < 8.0, "rotated local AABB would be wider");
}

fn text_line(id: &str, start_x: f64, end_x: f64, operation_id: &str) -> SemanticObject {
    SemanticObject {
        object_id: id.into(),
        geometry: SemanticGeometry::Line {
            start: MmPoint::new(start_x, 0.0),
            end: MmPoint::new(end_x, 0.0),
            width_mm: 0.2,
        },
        exposure: Exposure::Dark,
        origin: ObjectOrigin::GeneratedText {
            operation_id: operation_id.into(),
        },
    }
}

#[test]
fn generated_text_is_one_atomic_alignment_and_distribution_unit() {
    let mut aligned = document(
        "text-align",
        vec![
            text_line("g1", 10.0, 11.0, "text-op"),
            text_line("g2", 12.0, 13.0, "text-op"),
            flash("anchor", "circle", MmPoint::new(0.0, 0.0), Exposure::Clear),
        ],
        vec![aperture_circle("circle", 2.0)],
        vec![],
    );
    let before = aligned.clone();
    let mut history = EditHistory::default();
    let changed = history
        .align_objects(
            &mut aligned,
            "layer",
            &["g2".into(), "anchor".into(), "g1".into()],
            "anchor",
            AlignmentMode::Left,
        )
        .unwrap();
    assert_eq!(changed, ["g1", "g2"]);
    let moved_first = &aligned.layers[0].objects[0].geometry;
    let moved_second = &aligned.layers[0].objects[1].geometry;
    let (
        SemanticGeometry::Line {
            start: before_a, ..
        },
        SemanticGeometry::Line { start: after_a, .. },
    ) = (&before.layers[0].objects[0].geometry, moved_first)
    else {
        panic!()
    };
    let (
        SemanticGeometry::Line {
            start: before_b, ..
        },
        SemanticGeometry::Line { start: after_b, .. },
    ) = (&before.layers[0].objects[1].geometry, moved_second)
    else {
        panic!()
    };
    assert_eq!(after_a.x_mm - before_a.x_mm, after_b.x_mm - before_b.x_mm);
    let aligned_after = aligned.clone();
    history.undo(&mut aligned).unwrap();
    assert_eq!(aligned, before);
    history.redo(&mut aligned).unwrap();
    assert_eq!(aligned, aligned_after);

    let mut partial = before.clone();
    let before_partial = partial.clone();
    let mut partial_history = EditHistory::default();
    assert_eq!(
        partial_history.align_objects(
            &mut partial,
            "layer",
            &["g1".into(), "anchor".into()],
            "anchor",
            AlignmentMode::Left,
        ),
        Err(EditError::InvalidArgument)
    );
    assert_eq!(partial, before_partial);
    assert_eq!(partial_history.undo_len(), 0);

    let mut distributed = document(
        "text-distribution",
        vec![
            flash("first", "circle", MmPoint::new(0.0, 0.0), Exposure::Dark),
            text_line("tg1", 4.0, 5.0, "text-middle"),
            text_line("tg2", 6.0, 7.0, "text-middle"),
            flash("last", "circle", MmPoint::new(12.0, 0.0), Exposure::Clear),
        ],
        vec![aperture_circle("circle", 2.0)],
        vec![],
    );
    let before_distribution = distributed.clone();
    let mut distribution_history = EditHistory::default();
    let changed = distribution_history
        .distribute_objects(
            &mut distributed,
            "layer",
            &["tg2".into(), "last".into(), "first".into(), "tg1".into()],
            DistributionAxis::Horizontal,
        )
        .unwrap();
    assert_eq!(changed, ["tg1", "tg2"]);
    let starts = distributed.layers[0]
        .objects
        .iter()
        .filter(|object| object.object_id.starts_with('t'))
        .map(|object| match object.geometry {
            SemanticGeometry::Line { start, .. } => start.x_mm,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(starts, [4.5, 6.5]);
    let first = bounds(&distributed, "first");
    let text = bounds(&distributed, "tg1").union(bounds(&distributed, "tg2"));
    let last = bounds(&distributed, "last");
    assert_eq!(
        text.min_x_mm - first.max_x_mm,
        last.min_x_mm - text.max_x_mm
    );
    let after_distribution = distributed.clone();
    distribution_history.undo(&mut distributed).unwrap();
    assert_eq!(distributed, before_distribution);
    distribution_history.redo(&mut distributed).unwrap();
    assert_eq!(distributed, after_distribution);
}

#[test]
fn selection_limit_rejects_more_than_ten_thousand_ids_without_mutation() {
    let mut doc = document("selection-limit", vec![], vec![], vec![]);
    let ids = (0..=editor_core::edit::MAX_MOVE_OBJECTS)
        .map(|index| format!("missing-{index}"))
        .collect::<Vec<_>>();
    let before = doc.clone();
    let mut history = EditHistory::default();
    assert_eq!(
        history.align_objects(&mut doc, "layer", &ids, &ids[0], AlignmentMode::Left),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(doc, before);
    assert_eq!(history.undo_len(), 0);
}

#[test]
fn signed_zero_edges_use_numeric_ties_and_keep_layer_first_endpoint() {
    for axis in [DistributionAxis::Horizontal, DistributionAxis::Vertical] {
        for signed_max in [false, true] {
            let mut objects: Vec<_> = (0..3)
                .map(|i| ObjectBoundsMm {
                    object_ids: vec![format!("o{i}")],
                    layer_order: i,
                    bounds: BoundsMm {
                        min_x_mm: if i == 2 { 10.0 } else { 0.0 },
                        min_y_mm: if i == 2 { 10.0 } else { 0.0 },
                        max_x_mm: if i == 2 { 11.0 } else { 1.0 },
                        max_y_mm: if i == 2 { 11.0 } else { 1.0 },
                    },
                })
                .collect();
            for (i, object) in objects[..2].iter_mut().enumerate() {
                let zero = if i == 0 { 0.0 } else { -0.0 };
                if signed_max {
                    object.bounds.min_x_mm = -1.0;
                    object.bounds.min_y_mm = -1.0;
                    object.bounds.max_x_mm = zero;
                    object.bounds.max_y_mm = zero;
                } else {
                    object.bounds.min_x_mm = zero;
                    object.bounds.min_y_mm = zero;
                }
            }
            let deltas = compute_distribution_deltas(&objects, axis).unwrap();
            assert_eq!((deltas[0].dx_mm, deltas[0].dy_mm), (0.0, 0.0));
            assert_eq!((deltas[2].dx_mm, deltas[2].dy_mm), (0.0, 0.0));
            assert!(deltas[1].dx_mm > 0.0 || deltas[1].dy_mm > 0.0);
            objects.reverse();
            assert_eq!(compute_distribution_deltas(&objects, axis).unwrap(), deltas);
        }
    }
}
