//! S4-C2 manufacturing Grip regressions (C2-GRIP-02/03, AT-013/014/015/037/039/040).
use editor_core::edit::EditHistory;
use editor_core::grip::{GripFeatureId as Grip, grip_features, preview_grip_edit};
use editor_core::{
    ApertureDefinition, ApertureShape, ArcDirection, ArcGeometry, ArcSource, Exposure,
    LocalTransform, Mirror, MmPoint, ObjectOrigin, RegionContour, RegionEdge, RegionRole,
    SemanticDocument, SemanticFormat, SemanticGeometry, SemanticLayer, SemanticObject,
    SourceMetadata,
};

fn p(x: f64, y: f64) -> MmPoint {
    MmPoint::new(x, y)
}
fn near(a: MmPoint, b: MmPoint) {
    assert!(
        (a.x_mm - b.x_mm).abs() < 1e-9 && (a.y_mm - b.y_mm).abs() < 1e-9,
        "{a:?} != {b:?}"
    );
}
fn object(id: &str, geometry: SemanticGeometry) -> SemanticObject {
    SemanticObject {
        object_id: id.into(),
        geometry,
        exposure: Exposure::Dark,
        origin: ObjectOrigin::Imported { command_index: 0 },
    }
}
fn flash(id: &str, center: MmPoint, transform: LocalTransform) -> SemanticObject {
    object(
        id,
        SemanticGeometry::Flash {
            center,
            aperture_id: "a10".into(),
            transform,
        },
    )
}
fn document(objects: Vec<SemanticObject>, shape: ApertureShape) -> SemanticDocument {
    SemanticDocument {
        id: "grip-test".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 3,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![SemanticLayer {
            id: "l1".into(),
            objects,
        }],
        apertures: vec![ApertureDefinition {
            id: "a10".into(),
            source_dcode: 10,
            shape,
        }],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}
fn square() -> SemanticGeometry {
    let vertices = [p(0., 0.), p(4., 0.), p(4., 4.), p(0., 4.), p(0., 0.)];
    SemanticGeometry::Region {
        contours: vec![RegionContour {
            role: RegionRole::Solid,
            edges: vertices
                .windows(2)
                .map(|pair| RegionEdge::Line {
                    start: pair[0],
                    end: pair[1],
                })
                .collect(),
        }],
    }
}
fn arc(start: MmPoint, end: MmPoint, full_circle: bool) -> SemanticObject {
    object(
        "arc",
        SemanticGeometry::Arc {
            path: ArcGeometry {
                start,
                end,
                center: p(0., 0.),
                direction: ArcDirection::CounterClockwise,
                full_circle,
                source: Some(ArcSource {
                    resolution_mm: 0.000001,
                    single_quadrant: false,
                }),
            },
            width_mm: 0.1,
        },
    )
}

#[test]
fn standard_flash_shapes_keep_holes_and_reject_hole_collisions() {
    let variants = [
        (
            ApertureShape::Circle {
                diameter_mm: 4.,
                hole_diameter_mm: Some(1.),
            },
            Grip::Radius,
            p(3., 0.),
        ),
        (
            ApertureShape::Rectangle {
                width_mm: 4.,
                height_mm: 2.,
                hole_diameter_mm: Some(1.),
            },
            Grip::Right,
            p(3., 0.),
        ),
        (
            ApertureShape::Obround {
                width_mm: 4.,
                height_mm: 2.,
                hole_diameter_mm: Some(1.),
            },
            Grip::Right,
            p(3., 0.),
        ),
        (
            ApertureShape::Polygon {
                diameter_mm: 4.,
                vertices: 6,
                rotation_deg: 15.,
                hole_diameter_mm: Some(1.),
            },
            Grip::Radius,
            p(3., 0.),
        ),
    ];
    for (shape, grip, target) in variants {
        let f = flash("f", p(0., 0.), LocalTransform::default());
        let before = f.clone();
        let preview = preview_grip_edit(&f, Some(&shape), grip, target).unwrap();
        assert_eq!(f, before, "preview must not mutate source");
        let resized = preview.aperture_shape.unwrap();
        match resized {
            ApertureShape::Circle {
                diameter_mm,
                hole_diameter_mm,
            } => {
                assert_eq!(diameter_mm, 6.);
                assert_eq!(hole_diameter_mm, Some(1.));
            }
            ApertureShape::Rectangle {
                width_mm,
                height_mm,
                hole_diameter_mm,
            }
            | ApertureShape::Obround {
                width_mm,
                height_mm,
                hole_diameter_mm,
            } => {
                assert_eq!((width_mm, height_mm, hole_diameter_mm), (5., 2., Some(1.)));
            }
            ApertureShape::Polygon {
                diameter_mm,
                vertices,
                rotation_deg,
                hole_diameter_mm,
            } => {
                assert_eq!(
                    (diameter_mm, vertices, rotation_deg, hole_diameter_mm),
                    (6., 6, 15., Some(1.))
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        let collision = if grip == Grip::Radius {
            p(0.1, 0.)
        } else {
            p(-1.5, 0.)
        };
        assert!(preview_grip_edit(&f, Some(&shape), grip, collision).is_err());
    }
}

#[test]
fn mirrored_rotated_rectangle_keeps_opposite_anchor() {
    let transform = LocalTransform {
        mirror: Mirror::X,
        rotation_deg: 90.,
        scale: 1.,
    };
    let shape = ApertureShape::Rectangle {
        width_mm: 4.,
        height_mm: 2.,
        hole_diameter_mm: None,
    };
    let f = flash("f", p(10., 20.), transform);
    let features = grip_features(&f, Some(&shape)).unwrap();
    near(
        features
            .iter()
            .find(|f| f.id == Grip::Right)
            .unwrap()
            .position_mm,
        p(10., 18.),
    );
    near(
        features
            .iter()
            .find(|f| f.id == Grip::Left)
            .unwrap()
            .position_mm,
        p(10., 22.),
    );
    let preview = preview_grip_edit(&f, Some(&shape), Grip::Right, p(10., 17.)).unwrap();
    assert_eq!(
        preview.aperture_shape,
        Some(ApertureShape::Rectangle {
            width_mm: 5.,
            height_mm: 2.,
            hole_diameter_mm: None
        })
    );
    let SemanticGeometry::Flash { center, .. } = preview.geometry else {
        panic!()
    };
    near(center, p(10., 19.5));
    let preview =
        preview_grip_edit(&f, Some(&shape), Grip::Corner { index: 2 }, p(8., 17.)).unwrap();
    assert_eq!(
        preview.aperture_shape,
        Some(ApertureShape::Rectangle {
            width_mm: 5.,
            height_mm: 3.,
            hole_diameter_mm: None
        })
    );
    let SemanticGeometry::Flash { center, .. } = preview.geometry else {
        panic!()
    };
    near(center, p(9.5, 19.5));
    // Corner 0 remains at its original world position (11, 22).
    near(p(center.x_mm + 1.5, center.y_mm + 2.5), p(11., 22.));
}

#[test]
fn line_and_rectangular_sweep_edit_only_requested_endpoint() {
    for geometry in [
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(0., 0.),
            width_mm: 0.2,
        },
        SemanticGeometry::RectangularSweep {
            start: p(0., 0.),
            end: p(1., 0.),
            width_mm: 0.2,
            height_mm: 0.4,
        },
    ] {
        let source = object("path", geometry.clone());
        assert_eq!(grip_features(&source, None).unwrap().len(), 2);
        let target = if matches!(geometry, SemanticGeometry::RectangularSweep { .. }) {
            p(3., 0.)
        } else {
            p(3., 4.)
        };
        let edited = preview_grip_edit(&source, None, Grip::End, target)
            .unwrap()
            .geometry;
        match edited {
            SemanticGeometry::Line {
                start,
                end,
                width_mm,
            } => assert_eq!((start, end, width_mm), (p(0., 0.), p(3., 4.), 0.2)),
            SemanticGeometry::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            } => assert_eq!(
                (start, end, width_mm, height_mm),
                (p(0., 0.), p(3., 0.), 0.2, 0.4)
            ),
            _ => panic!(),
        }
        assert_eq!(source.geometry, geometry);
    }
    let sweep = object(
        "sweep",
        SemanticGeometry::RectangularSweep {
            start: p(0., 0.),
            end: p(1., 0.),
            width_mm: 0.2,
            height_mm: 0.4,
        },
    );
    assert!(preview_grip_edit(&sweep, None, Grip::End, p(3., 4.)).is_err());
}

#[test]
fn arc_grips_project_endpoints_and_resize_radius_without_losing_direction() {
    let source = arc(p(1., 0.), p(0., 1.), false);
    let edited = preview_grip_edit(&source, None, Grip::End, p(-2., 0.)).unwrap();
    let SemanticGeometry::Arc { path, width_mm } = edited.geometry else {
        panic!()
    };
    near(path.end, p(-1., 0.));
    assert_eq!(path.start, p(1., 0.));
    assert_eq!(path.center, p(0., 0.));
    assert_eq!(path.direction, ArcDirection::CounterClockwise);
    assert_eq!(path.source, None);
    assert_eq!(width_mm, 0.1);
    let edited = preview_grip_edit(&source, None, Grip::Radius, p(0., 2.)).unwrap();
    let SemanticGeometry::Arc { path, .. } = edited.geometry else {
        panic!()
    };
    near(path.start, p(2., 0.));
    near(path.end, p(0., 2.));
    assert_eq!(path.source, None);
    assert!(preview_grip_edit(&source, None, Grip::Radius, p(0., 0.)).is_err());
    let mut clockwise = arc(p(1., 0.), p(0., -1.), false);
    let SemanticGeometry::Arc { path, .. } = &mut clockwise.geometry else {
        panic!()
    };
    path.direction = ArcDirection::Clockwise;
    let edited = preview_grip_edit(&clockwise, None, Grip::Start, p(0., 2.)).unwrap();
    let SemanticGeometry::Arc { path, .. } = edited.geometry else {
        panic!()
    };
    near(path.start, p(0., 1.));
    assert_eq!(path.end, p(0., -1.));
    assert_eq!(path.direction, ArcDirection::Clockwise);
}

#[test]
fn full_circle_and_zero_sweep_keep_distinct_identities() {
    let full = arc(p(1., 0.), p(1., 0.), true);
    assert_eq!(
        grip_features(&full, None)
            .unwrap()
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        [Grip::Radius]
    );
    let edited = preview_grip_edit(&full, None, Grip::Radius, p(0., 2.)).unwrap();
    let SemanticGeometry::Arc { path, .. } = edited.geometry else {
        panic!()
    };
    assert!(path.full_circle);
    near(path.start, p(2., 0.));
    assert_eq!(path.start, path.end);
    assert!(preview_grip_edit(&full, None, Grip::Start, p(0., 1.)).is_err());
    let zero = arc(p(1., 0.), p(1., 0.), false);
    let edited = preview_grip_edit(&zero, None, Grip::Radius, p(0., 2.)).unwrap();
    let SemanticGeometry::Arc { path, .. } = edited.geometry else {
        panic!()
    };
    assert!(path.zero_sweep());
    assert!(!path.full_circle);
    near(path.start, p(2., 0.));
    assert_eq!(path.start, path.end);
}

#[test]
fn region_vertex_updates_both_edges_but_self_intersection_rejects_commit() {
    let mut doc = document(
        vec![object("region", square())],
        ApertureShape::Circle {
            diameter_mm: 1.,
            hole_diameter_mm: None,
        },
    );
    let before = doc.clone();
    let source = &doc.layers[0].objects[0];
    let preview = preview_grip_edit(
        source,
        None,
        Grip::Vertex {
            contour: 0,
            vertex: 0,
        },
        p(-1., -1.),
    )
    .unwrap();
    let SemanticGeometry::Region { contours } = preview.geometry else {
        panic!()
    };
    assert_eq!(
        contours[0].edges[0],
        RegionEdge::Line {
            start: p(-1., -1.),
            end: p(4., 0.)
        }
    );
    assert_eq!(
        contours[0].edges[3],
        RegionEdge::Line {
            start: p(0., 4.),
            end: p(-1., -1.)
        }
    );
    assert_eq!(doc, before);
    let mut history = EditHistory::default();
    history
        .grip_edit(
            &mut doc,
            "l1",
            "region",
            Grip::Vertex {
                contour: 0,
                vertex: 0,
            },
            p(-1., -1.),
        )
        .unwrap();
    assert_eq!(history.undo_len(), 1);
    let committed = doc.clone();
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, before);
    history.redo(&mut doc).unwrap();
    assert_eq!(doc, committed);
    let rejected = doc.clone();
    assert!(
        history
            .grip_edit(
                &mut doc,
                "l1",
                "region",
                Grip::Vertex {
                    contour: 0,
                    vertex: 1
                },
                p(-1., 2.)
            )
            .is_err()
    );
    assert_eq!(doc, rejected);
    assert_eq!(history.undo_len(), 1);
}

#[test]
fn unsupported_generated_text_compatibility_and_arc_region_have_no_grips() {
    let mut text = object(
        "text",
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(1., 0.),
            width_mm: 0.2,
        },
    );
    text.origin = ObjectOrigin::GeneratedText {
        operation_id: "t1".into(),
    };
    assert!(grip_features(&text, None).unwrap().is_empty());
    assert!(preview_grip_edit(&text, None, Grip::End, p(2., 0.)).is_err());
    for role in [RegionRole::CompatibilitySolid, RegionRole::Solid] {
        let edges = if role == RegionRole::CompatibilitySolid {
            vec![RegionEdge::Line {
                start: p(0., 0.),
                end: p(1., 0.),
            }]
        } else {
            vec![RegionEdge::Arc(ArcGeometry {
                start: p(1., 0.),
                end: p(1., 0.),
                center: p(0., 0.),
                direction: ArcDirection::CounterClockwise,
                full_circle: true,
                source: None,
            })]
        };
        let region = object(
            "unsupported",
            SemanticGeometry::Region {
                contours: vec![RegionContour { role, edges }],
            },
        );
        assert!(grip_features(&region, None).unwrap().is_empty());
        assert!(
            preview_grip_edit(
                &region,
                None,
                Grip::Vertex {
                    contour: 0,
                    vertex: 0
                },
                p(2., 0.)
            )
            .is_err()
        );
    }
}

#[test]
fn solid_outer_with_line_hole_has_no_editable_region_grips() {
    let mut contours = match square() {
        SemanticGeometry::Region { contours } => contours,
        _ => unreachable!(),
    };
    let vertices = [p(1., 1.), p(1., 3.), p(3., 3.), p(3., 1.), p(1., 1.)];
    contours.push(RegionContour {
        role: RegionRole::Hole,
        edges: vertices
            .windows(2)
            .map(|pair| RegionEdge::Line {
                start: pair[0],
                end: pair[1],
            })
            .collect(),
    });
    let region = object("solid-with-hole", SemanticGeometry::Region { contours });

    assert!(grip_features(&region, None).unwrap().is_empty());
    assert!(matches!(
        preview_grip_edit(
            &region,
            None,
            Grip::Vertex {
                contour: 0,
                vertex: 0,
            },
            p(-1., 0.)
        ),
        Err(editor_core::edit::EditError::UnsupportedTransform)
    ));
}

#[test]
fn shared_flash_aperture_uses_cow_and_exact_undo_redo() {
    let shape = ApertureShape::Circle {
        diameter_mm: 4.,
        hole_diameter_mm: Some(1.),
    };
    let mut doc = document(
        vec![
            flash("a", p(0., 0.), LocalTransform::default()),
            flash("b", p(10., 0.), LocalTransform::default()),
        ],
        shape.clone(),
    );
    let before = doc.clone();
    let mut history = EditHistory::default();
    history
        .grip_edit(&mut doc, "l1", "a", Grip::Radius, p(3., 0.))
        .unwrap();
    assert_eq!(history.undo_len(), 1);
    assert_eq!(doc.apertures.len(), 2);
    assert_eq!(doc.apertures[0].shape, shape);
    assert_eq!(doc.layers[0].objects[1], before.layers[0].objects[1]);
    let SemanticGeometry::Flash {
        aperture_id: generated,
        ..
    } = &doc.layers[0].objects[0].geometry
    else {
        panic!()
    };
    assert_ne!(generated, "a10");
    assert_eq!(doc.apertures[1].id, *generated);
    assert_eq!(
        doc.apertures[1].shape,
        ApertureShape::Circle {
            diameter_mm: 6.,
            hole_diameter_mm: Some(1.)
        }
    );
    let after = doc.clone();
    history.undo(&mut doc).unwrap();
    assert_eq!(doc, before);
    history.redo(&mut doc).unwrap();
    assert_eq!(doc, after);
}

#[test]
fn feature_budget_is_structured_and_identity_is_stable() {
    let huge = object(
        "huge",
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Line {
                        start: p(0., 0.),
                        end: p(1., 0.)
                    };
                    editor_core::grip::MAX_GRIP_FEATURES_PER_OBJECT + 1
                ],
            }],
        },
    );
    assert_eq!(
        grip_features(&huge, None),
        Err(editor_core::edit::EditError::ResourceLimit)
    );
    let line = object(
        "line",
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(2., 0.),
            width_mm: 0.1,
        },
    );
    assert_eq!(grip_features(&line, None), grip_features(&line, None));
}
