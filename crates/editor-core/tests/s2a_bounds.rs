use editor_core::*;

fn p(x: f64, y: f64) -> MmPoint {
    MmPoint::new(x, y)
}
fn doc(geometry: SemanticGeometry, shape: Option<ApertureShape>) -> SemanticDocument {
    SemanticDocument {
        id: "bounds".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        source: SourceMetadata::default(),
        apertures: shape
            .into_iter()
            .map(|shape| ApertureDefinition {
                id: "ap".into(),
                source_dcode: 10,
                shape,
            })
            .collect(),
        layers: vec![SemanticLayer {
            id: "layer".into(),
            objects: vec![SemanticObject {
                object_id: "object".into(),
                exposure: Exposure::Dark,
                origin: ObjectOrigin::Imported { command_index: 1 },
                geometry,
            }],
        }],
    }
}
fn flash(shape: ApertureShape, transform: LocalTransform) -> SemanticDocument {
    doc(
        SemanticGeometry::Flash {
            center: p(10., 20.),
            aperture_id: "ap".into(),
            transform,
        },
        Some(shape),
    )
}
fn assert_bounds(document: &SemanticDocument, expected: [f64; 4]) {
    document.validate().unwrap();
    let b = document.manufacturing_bounds(None).unwrap().unwrap();
    let actual = [b.min_x_mm, b.min_y_mm, b.max_x_mm, b.max_y_mm];
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-9, "{actual:?} != {expected:?}");
    }
    assert_eq!(
        document.manufacturing_bounds(Some("layer")).unwrap(),
        Some(b)
    );
}
fn arc(direction: ArcDirection) -> ArcGeometry {
    ArcGeometry {
        start: p(10., 0.),
        end: p(0., 10.),
        center: p(0., 0.),
        direction,
        full_circle: false,
        source: None,
    }
}

#[test]
fn flash_standard_shapes_respect_transform_and_holes() {
    for mirror in [Mirror::None, Mirror::X, Mirror::Y, Mirror::Xy] {
        let transform = LocalTransform {
            mirror,
            rotation_deg: 90.,
            scale: 2.,
        };
        assert_bounds(
            &flash(
                ApertureShape::Circle {
                    diameter_mm: 4.,
                    hole_diameter_mm: Some(2.),
                },
                transform,
            ),
            [6., 16., 14., 24.],
        );
        assert_bounds(
            &flash(
                ApertureShape::Rectangle {
                    width_mm: 6.,
                    height_mm: 2.,
                    hole_diameter_mm: Some(1.),
                },
                transform,
            ),
            [8., 14., 12., 26.],
        );
        assert_bounds(
            &flash(
                ApertureShape::Obround {
                    width_mm: 6.,
                    height_mm: 2.,
                    hole_diameter_mm: Some(1.),
                },
                transform,
            ),
            [8., 14., 12., 26.],
        );
        assert_bounds(
            &flash(
                ApertureShape::Polygon {
                    diameter_mm: 4.,
                    vertices: 4,
                    rotation_deg: 0.,
                    hole_diameter_mm: Some(1.),
                },
                transform,
            ),
            [6., 16., 14., 24.],
        );
    }
}

#[test]
fn obround_diagonal_bounds_are_not_rotated_local_aabb() {
    let d = flash(
        ApertureShape::Obround {
            width_mm: 6.,
            height_mm: 2.,
            hole_diameter_mm: None,
        },
        LocalTransform {
            rotation_deg: 45.,
            ..Default::default()
        },
    );
    let extent = 1. + 2_f64.sqrt();
    assert_bounds(&d, [10. - extent, 20. - extent, 10. + extent, 20. + extent]);
}

#[test]
fn polygon_uses_vertices_instead_of_circumcircle() {
    let d = flash(
        ApertureShape::Polygon {
            diameter_mm: 4.,
            vertices: 3,
            rotation_deg: 0.,
            hole_diameter_mm: None,
        },
        LocalTransform {
            mirror: Mirror::X,
            ..Default::default()
        },
    );
    assert_bounds(&d, [8., 20. - 3_f64.sqrt(), 11., 20. + 3_f64.sqrt()]);
}

#[test]
fn macro_offset_circle_and_centerline_compose_local_transform() {
    for (primitive, expected) in [
        (
            MacroPrimitive::Circle {
                exposure: Exposure::Dark,
                diameter_mm: 2.,
                center: p(3., 0.),
                rotation_deg: 90.,
            },
            [2., 18., 6., 22.],
        ),
        (
            MacroPrimitive::CenterLine {
                exposure: Exposure::Dark,
                width_mm: 4.,
                height_mm: 2.,
                center: p(3., 0.),
                rotation_deg: 90.,
            },
            [0., 18., 8., 22.],
        ),
    ] {
        let d = flash(
            ApertureShape::Macro {
                primitives: vec![primitive],
            },
            LocalTransform {
                mirror: Mirror::X,
                rotation_deg: 90.,
                scale: 2.,
            },
        );
        assert_bounds(&d, expected);
    }
}

#[test]
fn macro_outline_and_clear_have_conservative_dark_envelope() {
    let d = flash(
        ApertureShape::Macro {
            primitives: vec![
                MacroPrimitive::Outline {
                    exposure: Exposure::Dark,
                    points: vec![p(1., 0.), p(3., 0.), p(1., 1.), p(1., 0.)],
                    rotation_deg: 90.,
                },
                MacroPrimitive::Circle {
                    exposure: Exposure::Clear,
                    diameter_mm: 100.,
                    center: p(0., 0.),
                    rotation_deg: 0.,
                },
            ],
        },
        LocalTransform::default(),
    );
    assert_bounds(&d, [9., 21., 10., 23.]);
}

#[test]
fn line_and_rectangular_sweep_include_real_width() {
    assert_bounds(
        &doc(
            SemanticGeometry::Line {
                start: p(-2., 3.),
                end: p(4., 5.),
                width_mm: 2.,
            },
            None,
        ),
        [-3., 2., 5., 6.],
    );
    assert_bounds(
        &doc(
            SemanticGeometry::RectangularSweep {
                start: p(0., 0.),
                end: p(3., 0.),
                width_mm: 2.,
                height_mm: 1.,
            },
            None,
        ),
        [-1., -0.5, 4., 0.5],
    );
}

#[test]
fn arc_bounds_only_include_sweep_extrema_and_direction() {
    assert_bounds(
        &doc(
            SemanticGeometry::Arc {
                path: arc(ArcDirection::CounterClockwise),
                width_mm: 2.,
            },
            None,
        ),
        [-1., -1., 11., 11.],
    );
    assert_bounds(
        &doc(
            SemanticGeometry::Arc {
                path: arc(ArcDirection::Clockwise),
                width_mm: 2.,
            },
            None,
        ),
        [-11., -11., 11., 11.],
    );
    let r = 10. / 2_f64.sqrt();
    let a = ArcGeometry {
        start: p(r, r),
        end: p(-r, r),
        ..arc(ArcDirection::CounterClockwise)
    };
    assert_bounds(
        &doc(
            SemanticGeometry::Arc {
                path: a,
                width_mm: 2.,
            },
            None,
        ),
        [-r - 1., r - 1., r + 1., 11.],
    );
}

#[test]
fn full_circle_zero_sweep_and_deviation_bounds() {
    let a = arc(ArcDirection::CounterClockwise);
    assert_bounds(
        &doc(
            SemanticGeometry::Arc {
                path: ArcGeometry {
                    end: a.start,
                    full_circle: true,
                    ..a
                },
                width_mm: 2.,
            },
            None,
        ),
        [-11., -11., 11., 11.],
    );
    assert_bounds(
        &doc(
            SemanticGeometry::Arc {
                path: ArcGeometry { end: a.start, ..a },
                width_mm: 2.,
            },
            None,
        ),
        [9., -1., 11., 1.],
    );
    // Mean circle r=11 plus declared endpoint joins; not start-radius r=10.
    assert_bounds(
        &doc(
            SemanticGeometry::Arc {
                path: ArcGeometry {
                    end: p(0., 12.),
                    ..a
                },
                width_mm: 2.,
            },
            None,
        ),
        [-1., -1., 12., 13.],
    );
}

#[test]
fn region_bounds_preserve_real_arc_extrema() {
    let a = ArcGeometry {
        end: p(-10., 0.),
        ..arc(ArcDirection::CounterClockwise)
    };
    let d = doc(
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Arc(a),
                    RegionEdge::Line {
                        start: a.end,
                        end: a.start,
                    },
                ],
            }],
        },
        None,
    );
    assert_bounds(&d, [-10., 0., 10., 10.]);
}

#[test]
fn clear_objects_and_multiple_layers_are_included() {
    let mut d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(2., 0.),
            width_mm: 2.,
        },
        None,
    );
    let mut other = d.layers[0].clone();
    other.id = "other".into();
    other.objects[0].object_id = "clear".into();
    other.objects[0].exposure = Exposure::Clear;
    other.objects[0].geometry = SemanticGeometry::Line {
        start: p(10., 0.),
        end: p(11., 0.),
        width_mm: 2.,
    };
    d.layers.push(other);
    d.validate().unwrap();
    assert_eq!(
        d.manufacturing_bounds(None).unwrap(),
        Some(BoundsMm {
            min_x_mm: -1.,
            min_y_mm: -1.,
            max_x_mm: 12.,
            max_y_mm: 1.
        })
    );
    assert_eq!(
        d.manufacturing_bounds(Some("layer"))
            .unwrap()
            .unwrap()
            .max_x_mm,
        3.
    );
}

#[test]
fn empty_layer_and_document_are_null_unknown_layer_is_error() {
    let mut d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(2., 0.),
            width_mm: 2.,
        },
        None,
    );
    d.layers[0].objects.clear();
    assert_eq!(d.manufacturing_bounds(Some("layer")).unwrap(), None);
    assert_eq!(d.manufacturing_bounds(None).unwrap(), None);
    assert!(d.manufacturing_bounds(Some("missing")).is_err());
    d.layers.clear();
    assert_eq!(d.manufacturing_bounds(None).unwrap(), None);
}

#[test]
fn missing_aperture_is_not_silently_omitted() {
    let d = doc(
        SemanticGeometry::Flash {
            center: p(0., 0.),
            aperture_id: "missing".into(),
            transform: LocalTransform::default(),
        },
        None,
    );
    assert_eq!(
        d.manufacturing_bounds(None),
        Err(SemanticError::MissingAperture("missing".into()))
    );
}
