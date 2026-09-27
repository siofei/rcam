use editor_core::block::{
    BlockDefinition, BlockDefinitionId, BlockObject, BlockObjectGeometry, BlockTransform,
};
use editor_core::edit::{EditError, EditHistory, MAX_EDIT_REGION_EDGES, RectangularArray};
use editor_core::*;

fn object(n: usize) -> SemanticObject {
    SemanticObject {
        object_id: format!("source-{n}"),
        origin: ObjectOrigin::Imported { command_index: n },
        exposure: Exposure::Dark,
        geometry: SemanticGeometry::Flash {
            center: MmPoint::new(n as f64 * 3., 2.),
            aperture_id: "ap".into(),
            transform: LocalTransform::default(),
        },
    }
}
fn doc(count: usize) -> SemanticDocument {
    SemanticDocument {
        id: "array".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![SemanticLayer {
            id: "layer".into(),
            objects: (0..count).map(object).collect(),
        }],
        apertures: vec![ApertureDefinition {
            id: "ap".into(),
            source_dcode: 10,
            shape: ApertureShape::Circle {
                diameter_mm: 1.,
                hole_diameter_mm: None,
            },
        }],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}
fn spec(rows: u64, columns: u64, x: f64, y: f64) -> RectangularArray {
    RectangularArray {
        rows,
        columns,
        pitch_x_mm: x,
        pitch_y_mm: y,
    }
}
fn ids(d: &SemanticDocument) -> Vec<String> {
    d.layers[0]
        .objects
        .iter()
        .map(|o| o.object_id.clone())
        .collect()
}
fn run(
    d: &mut SemanticDocument,
    h: &mut EditHistory,
    ids: &[String],
    s: RectangularArray,
) -> Result<Vec<String>, EditError> {
    h.array_rectangular_objects(d, "layer", ids, s)
}
fn region(role: RegionRole) -> SemanticGeometry {
    let points = [
        MmPoint::new(0., 0.),
        MmPoint::new(2., 0.),
        MmPoint::new(2., 2.),
        MmPoint::new(0., 2.),
    ];
    SemanticGeometry::Region {
        contours: vec![RegionContour {
            role,
            edges: (0..4)
                .map(|i| RegionEdge::Line {
                    start: points[i],
                    end: points[(i + 1) % 4],
                })
                .collect(),
        }],
    }
}

#[test]
fn signed_row_major_cells_originals_apertures_and_exact_single_undo_redo() {
    for s in [
        spec(1, 1, 0., 0.),
        spec(1, 4, 10., 0.),
        spec(4, 1, 0., 15.),
        spec(3, 4, 20., 15.),
        spec(3, 4, -20., 15.),
        spec(3, 4, 20., -15.),
        spec(3, 4, -20., -15.),
    ] {
        let mut d = doc(2);
        let before = d.clone();
        let mut h = EditHistory::default();
        let mut selected = ids(&d);
        selected.reverse();
        let added = run(&mut d, &mut h, &selected, s).unwrap();
        assert_eq!(added.len(), (s.rows * s.columns - 1) as usize * 2);
        assert_eq!(&d.layers[0].objects[..2], &before.layers[0].objects);
        assert_eq!(d.apertures, before.apertures);
        for cell in 0..(s.rows * s.columns) as usize {
            for source in 0..2 {
                let o = &d.layers[0].objects[cell * 2 + source];
                let SemanticGeometry::Flash {
                    center,
                    aperture_id,
                    ..
                } = &o.geometry
                else {
                    panic!()
                };
                assert_eq!(
                    *center,
                    MmPoint::new(
                        source as f64 * 3. + (cell as u64 % s.columns) as f64 * s.pitch_x_mm,
                        2. + (cell as u64 / s.columns) as f64 * s.pitch_y_mm
                    )
                );
                assert_eq!(aperture_id, "ap");
                if cell > 0 {
                    assert!(matches!(o.origin, ObjectOrigin::Generated { .. }));
                }
            }
        }
        if s.rows * s.columns == 1 {
            assert_eq!(d, before);
            assert_eq!(h.undo_len(), 0);
            continue;
        }
        assert_eq!(h.undo_len(), 1);
        let after = d.clone();
        h.undo(&mut d).unwrap();
        assert_eq!(d, before);
        h.redo(&mut d).unwrap();
        assert_eq!(d, after);
    }
}

#[test]
fn exposure_clusters_insert_after_span_before_tail_and_reject_disjoint() {
    let mut d = doc(5);
    d.layers[0].objects[2].exposure = Exposure::Clear;
    let before = d.clone();
    let mut h = EditHistory::default();
    let all = ids(&d);
    assert_eq!(
        run(
            &mut d,
            &mut h,
            &[all[1].clone(), all[3].clone()],
            spec(3, 2, 10., 10.)
        ),
        Err(EditError::InvalidArgument)
    );
    assert_eq!(d, before);
    run(&mut d, &mut h, &all[1..4], spec(3, 2, 10., 10.)).unwrap();
    assert_eq!(d.layers[0].objects[0], before.layers[0].objects[0]);
    assert_eq!(d.layers[0].objects.last(), before.layers[0].objects.last());
    for cluster in d.layers[0].objects[1..19].chunks(3) {
        assert_eq!(
            cluster.iter().map(|o| o.exposure).collect::<Vec<_>>(),
            vec![Exposure::Dark, Exposure::Clear, Exposure::Dark]
        );
    }
    h.undo(&mut d).unwrap();
    assert_eq!(d, before);
}

#[test]
fn text_groups_complete_and_each_source_group_each_cell_has_distinct_identity() {
    let mut d = doc(6);
    for (i, o) in d.layers[0].objects.iter_mut().enumerate() {
        o.origin = ObjectOrigin::GeneratedText {
            operation_id: format!("text-{}", i / 3),
        };
    }
    let before = d.clone();
    let mut h = EditHistory::default();
    let selected = ids(&d);
    assert_eq!(
        run(&mut d, &mut h, &selected[..2], spec(2, 2, 10., 10.)),
        Err(EditError::InvalidArgument)
    );
    assert_eq!(d, before);
    run(&mut d, &mut h, &selected, spec(2, 2, 30., 30.)).unwrap();
    let mut groups = std::collections::HashSet::new();
    for group in d.layers[0].objects.chunks(3) {
        let ObjectOrigin::GeneratedText { operation_id } = &group[0].origin else {
            panic!()
        };
        assert!(groups.insert(operation_id));
        assert!(group.iter().all(|o| o.origin == group[0].origin));
    }
    assert_eq!(groups.len(), 8);
}

#[test]
fn mixed_geometry_translation_retains_types_exposure_and_compatibility_role() {
    let mut d = doc(6);
    d.layers[0].objects[1].geometry = SemanticGeometry::Line {
        start: MmPoint::new(1., 1.),
        end: MmPoint::new(2., 3.),
        width_mm: 0.2,
    };
    d.layers[0].objects[2].geometry = SemanticGeometry::Arc {
        path: ArcGeometry {
            start: MmPoint::new(1., 0.),
            end: MmPoint::new(0., 1.),
            center: MmPoint::new(0., 0.),
            direction: ArcDirection::CounterClockwise,
            full_circle: false,
            source: None,
        },
        width_mm: 0.1,
    };
    d.layers[0].objects[3].geometry = region(RegionRole::Solid);
    d.layers[0].objects[4].geometry = region(RegionRole::CompatibilitySolid);
    d.layers[0].objects[5].geometry = SemanticGeometry::RectangularSweep {
        start: MmPoint::new(0., 0.),
        end: MmPoint::new(3., 0.),
        width_mm: 1.,
        height_mm: 2.,
    };
    let before = d.clone();
    let selected = ids(&d);
    let mut h = EditHistory::default();
    run(&mut d, &mut h, &selected, spec(2, 2, -20., 30.)).unwrap();
    for (cell, group) in d.layers[0].objects.chunks(6).enumerate() {
        for (i, o) in group.iter().enumerate() {
            assert_eq!(
                std::mem::discriminant(&o.geometry),
                std::mem::discriminant(&before.layers[0].objects[i].geometry)
            );
            let source = geometries_bounds([&before.layers[0].objects[i].geometry], &d.apertures)
                .unwrap()
                .unwrap();
            let bounds = geometries_bounds([&o.geometry], &d.apertures)
                .unwrap()
                .unwrap();
            assert!((bounds.min_x_mm - source.min_x_mm + (cell % 2) as f64 * 20.).abs() < 1e-8);
            assert!((bounds.max_y_mm - source.max_y_mm - (cell / 2) as f64 * 30.).abs() < 1e-8);
        }
    }
    let SemanticGeometry::Region { contours } = &d.layers[0].objects[10].geometry else {
        panic!()
    };
    assert_eq!(contours[0].role, RegionRole::CompatibilitySolid);
}

#[test]
fn block_array_shares_definition_and_preserves_rotation_mirror() {
    let mut d = doc(1);
    let definition = BlockDefinition {
        id: BlockDefinitionId("block".into()),
        name: "panel".into(),
        local_origin: MmPoint::new(0., 0.),
        revision: 9,
        objects: vec![BlockObject {
            exposure: Exposure::Dark,
            geometry: BlockObjectGeometry::Line {
                start: MmPoint::new(0., 0.),
                end: MmPoint::new(2., 0.),
                width_mm: 0.2,
            },
        }],
    };
    d.block_definitions.push(definition.clone());
    let transform = BlockTransform {
        translation: MmPoint::new(3., 4.),
        rotation_deg: 37.,
        mirror: true,
    };
    d.layers[0].objects[0].geometry = SemanticGeometry::BlockInstance {
        definition_id: definition.id.clone(),
        transform,
    };
    let selected = ids(&d);
    run(
        &mut d,
        &mut EditHistory::default(),
        &selected,
        spec(10, 10, 10., -10.),
    )
    .unwrap();
    assert_eq!(d.block_definitions, vec![definition]);
    for (i, o) in d.layers[0].objects.iter().enumerate() {
        let SemanticGeometry::BlockInstance {
            definition_id,
            transform: t,
        } = &o.geometry
        else {
            panic!()
        };
        assert_eq!(definition_id.0, "block");
        assert_eq!(t.rotation_deg, 37.);
        assert!(t.mirror);
        assert_eq!(
            t.translation,
            MmPoint::new(3. + (i % 10) as f64 * 10., 4. - (i / 10) as f64 * 10.)
        );
    }
}

#[test]
fn invalid_requests_are_atomic_and_do_not_consume_ids_or_redo() {
    let mut d = doc(1);
    let selected = ids(&d);
    let mut h = EditHistory::default();
    let first = run(&mut d, &mut h, &selected, spec(1, 2, 10., 0.)).unwrap();
    h.undo(&mut d).unwrap();
    let before = d.clone();
    for s in [
        spec(0, 2, 1., 1.),
        spec(2, 0, 1., 1.),
        spec(2, 2, 0., 1.),
        spec(2, 2, 1., 0.),
        spec(1, 2, f64::NAN, 0.),
        spec(1, 2, f64::INFINITY, 0.),
        spec(u64::MAX, 2, 1., 1.),
        spec(101, 100, 1., 1.),
        spec(2, 2, 1e100, 1.),
    ] {
        assert!(run(&mut d, &mut h, &selected, s).is_err());
        assert_eq!(d, before);
        assert_eq!(h.undo_len(), 0);
        assert_eq!(h.redo_len(), 1);
    }
    assert!(run(&mut d, &mut h, &["missing".into()], spec(2, 2, 1., 1.)).is_err());
    assert!(
        run(
            &mut d,
            &mut h,
            &[selected[0].clone(), selected[0].clone()],
            spec(2, 2, 1., 1.)
        )
        .is_err()
    );
    run(&mut d, &mut h, &selected, spec(1, 1, 0., 0.)).unwrap();
    assert_eq!(h.redo_len(), 1);
    let new = run(&mut d, &mut h, &selected, spec(1, 2, 10., 0.)).unwrap();
    assert_eq!(first[0], "array-generated-object-0");
    assert_eq!(new[0], "array-generated-object-1");
}

#[test]
fn object_history_and_edge_budgets_preflight_exact_boundaries() {
    let mut d = doc(1);
    let selected = ids(&d);
    let mut h = EditHistory::default();
    run(&mut d, &mut h, &selected, spec(100, 100, 1., 1.)).unwrap();
    assert_eq!(d.layers[0].objects.len(), 10000);
    let mut d = doc(2);
    let selected = ids(&d);
    let before = d.clone();
    let mut h = EditHistory::default();
    assert_eq!(
        run(&mut d, &mut h, &selected, spec(1, 5002, 1., 0.)),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d, before);
    let budget = h
        .estimate_array_rectangular(&d, "layer", &selected, spec(1, 2, 10., 0.))
        .unwrap()
        .history_bytes;
    let mut short = EditHistory::with_limits(10, budget - 1).unwrap();
    assert_eq!(
        run(&mut d, &mut short, &selected, spec(1, 2, 10., 0.)),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d, before);
    assert_eq!(short.undo_len(), 0);
    let mut exact = EditHistory::with_limits(10, budget).unwrap();
    run(&mut d, &mut exact, &selected, spec(1, 2, 10., 0.)).unwrap();
    assert_eq!(exact.bytes(), budget);
    // Valid regular polygons reach exactly 2M edges; one extra tail edge rejects.
    fn polygon(n: usize) -> SemanticGeometry {
        let points: Vec<_> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                MmPoint::new(a.cos(), a.sin())
            })
            .collect();
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: (0..n)
                    .map(|i| RegionEdge::Line {
                        start: points[i],
                        end: points[(i + 1) % n],
                    })
                    .collect(),
            }],
        }
    }
    let mut d = doc(2);
    d.layers[0].objects[0].geometry = polygon(200);
    d.layers[0].objects[1].geometry = polygon(200);
    d.validate().unwrap();
    let selected = vec![d.layers[0].objects[0].object_id.clone()];
    let mut h = EditHistory::with_limits(100, usize::MAX).unwrap();
    let estimate = h
        .estimate_array_rectangular(&d, "layer", &selected, spec(1, 9999, 1., 0.))
        .unwrap();
    assert_eq!(estimate.added_region_edges + 400, MAX_EDIT_REGION_EDGES);
    d.layers[0].objects[1].geometry = polygon(201);
    d.validate().unwrap();
    let before = d.clone();
    assert_eq!(
        run(&mut d, &mut h, &selected, spec(1, 9999, 1., 0.)),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(d, before);
    assert_eq!(h.undo_len(), 0);
    let created = run(&mut d, &mut h, &selected, spec(1, 2, 10., 0.)).unwrap();
    assert_eq!(created[0], "array-generated-object-0");
}
