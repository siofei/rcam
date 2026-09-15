use editor_core::*;

fn point(x: f64, y: f64) -> MmPoint {
    MmPoint::new(x, y)
}
fn document(shape: Option<ApertureShape>, geometry: SemanticGeometry) -> SemanticDocument {
    SemanticDocument {
        id: "independent".into(),
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
            locked: false,
            id: "layer".into(),
            name: "truth".into(),
            objects: vec![SemanticObject {
                object_id: "object".into(),
                exposure: Exposure::Dark,
                source_command: 1,
                geometry,
            }],
        }],
    }
}
fn flash(shape: ApertureShape) -> SemanticDocument {
    document(
        Some(shape),
        SemanticGeometry::Flash {
            center: point(0., 0.),
            aperture_id: "ap".into(),
            transform: LocalTransform::default(),
        },
    )
}

#[test]
fn macro_rotation_moves_off_origin_primitive_centres() {
    for primitive in [
        MacroPrimitive::Circle {
            exposure: Exposure::Dark,
            diameter_mm: 1.,
            center: point(2., 0.),
            rotation_deg: 90.,
        },
        MacroPrimitive::CenterLine {
            exposure: Exposure::Dark,
            width_mm: 1.,
            height_mm: 0.5,
            center: point(2., 0.),
            rotation_deg: 90.,
        },
    ] {
        let doc = flash(ApertureShape::Macro {
            primitives: vec![primitive],
        });
        doc.validate().unwrap();
        assert_eq!(doc.layer_coverage_at("layer", point(0., 2.)), Some(true));
        assert_eq!(doc.layer_coverage_at("layer", point(2., 0.)), Some(false));
    }
}

#[test]
fn tiny_local_holes_remain_transparent() {
    for shape in [
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: Some(1e-7),
        },
        ApertureShape::Rectangle {
            width_mm: 2.,
            height_mm: 1.,
            hole_diameter_mm: Some(1e-7),
        },
        ApertureShape::Obround {
            width_mm: 2.,
            height_mm: 1.,
            hole_diameter_mm: Some(1e-7),
        },
        ApertureShape::Polygon {
            diameter_mm: 2.,
            vertices: 6,
            rotation_deg: 0.,
            hole_diameter_mm: Some(1e-7),
        },
    ] {
        let doc = flash(shape);
        doc.validate().unwrap();
        assert_eq!(doc.layer_coverage_at("layer", point(0., 0.)), Some(false));
    }
}

#[test]
fn swept_arcs_include_round_endpoint_caps() {
    let arc = ArcGeometry {
        start: point(1., 0.),
        end: point(0., 1.),
        center: point(0., 0.),
        direction: ArcDirection::CounterClockwise,
        full_circle: false,
        source: None,
    };
    let doc = document(
        None,
        SemanticGeometry::Arc {
            path: arc,
            width_mm: 0.2,
        },
    );
    doc.validate().unwrap();
    assert_eq!(doc.layer_coverage_at("layer", point(1., -0.05)), Some(true));
    assert_eq!(doc.layer_coverage_at("layer", point(-0.05, 1.)), Some(true));
    assert_eq!(doc.layer_coverage_at("layer", point(1., -0.2)), Some(false));
}

#[test]
fn full_circle_flag_cannot_hide_invalid_end_point() {
    let arc = ArcGeometry {
        start: point(1., 0.),
        end: point(9., 0.),
        center: point(0., 0.),
        direction: ArcDirection::CounterClockwise,
        full_circle: true,
        source: None,
    };
    assert!(
        document(
            None,
            SemanticGeometry::Arc {
                path: arc,
                width_mm: 0.2
            }
        )
        .validate()
        .is_err()
    );
}

#[test]
fn curved_region_coverage_is_not_display_tessellation() {
    let arc = ArcGeometry {
        start: point(10., 0.),
        end: point(-10., 0.),
        center: point(0., 0.),
        direction: ArcDirection::CounterClockwise,
        full_circle: false,
        source: None,
    };
    let doc = document(
        None,
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Arc(arc),
                    RegionEdge::Line {
                        start: point(-10., 0.),
                        end: point(10., 0.),
                    },
                ],
            }],
        },
    );
    doc.validate().unwrap();
    let angle = std::f64::consts::PI / 64.;
    assert_eq!(
        doc.layer_coverage_at("layer", point(9.9999 * angle.cos(), 9.9999 * angle.sin())),
        Some(true)
    );
}

#[test]
fn bowtie_is_not_a_valid_region_or_macro_outline() {
    let points = vec![
        point(0., 0.),
        point(2., 2.),
        point(0., 2.),
        point(2., 0.),
        point(0., 0.),
    ];
    let edges = points
        .windows(2)
        .map(|p| RegionEdge::Line {
            start: p[0],
            end: p[1],
        })
        .collect();
    assert!(
        document(
            None,
            SemanticGeometry::Region {
                contours: vec![RegionContour {
                    role: RegionRole::Solid,
                    edges
                }]
            }
        )
        .validate()
        .is_err()
    );
    assert!(
        flash(ApertureShape::Macro {
            primitives: vec![MacroPrimitive::Outline {
                exposure: Exposure::Dark,
                points,
                rotation_deg: 0.
            }]
        })
        .validate()
        .is_err()
    );
}

#[test]
fn polygon_hole_must_fit_inside_edges_not_just_vertex_circle() {
    let doc = flash(ApertureShape::Polygon {
        diameter_mm: 2.,
        vertices: 3,
        rotation_deg: 0.,
        hole_diameter_mm: Some(1.5),
    });
    assert!(doc.validate().is_err());
}

#[test]
fn zero_rectangular_sweep_is_outside_frozen_nonzero_subset() {
    assert!(
        document(
            None,
            SemanticGeometry::RectangularSweep {
                start: point(0., 0.),
                end: point(0., 0.),
                width_mm: 2.,
                height_mm: 1.
            }
        )
        .validate()
        .is_err()
    );
}

fn contour(points: &[(f64, f64)]) -> RegionContour {
    RegionContour {
        role: RegionRole::Solid,
        edges: points
            .windows(2)
            .map(|p| RegionEdge::Line {
                start: point(p[0].0, p[0].1),
                end: point(p[1].0, p[1].1),
            })
            .collect(),
    }
}

#[test]
fn legal_axis_cutin_creates_only_a_local_hole() {
    let outline = contour(&[
        (0., 2.),
        (1., 2.),
        (1., 3.),
        (3., 3.),
        (3., 1.),
        (1., 1.),
        (1., 2.),
        (0., 2.),
        (0., 0.),
        (4., 0.),
        (4., 4.),
        (0., 4.),
        (0., 2.),
    ]);
    let doc = document(
        None,
        SemanticGeometry::Region {
            contours: vec![outline],
        },
    );
    doc.validate().unwrap();
    assert_eq!(doc.layer_coverage_at("layer", point(2., 2.)), Some(false));
    assert_eq!(doc.layer_coverage_at("layer", point(0.5, 0.5)), Some(true));
}

#[test]
fn nested_independent_contours_are_union_not_a_hole() {
    let outer = contour(&[(0., 0.), (4., 0.), (4., 4.), (0., 4.), (0., 0.)]);
    let inner = contour(&[(1., 1.), (3., 1.), (3., 3.), (1., 3.), (1., 1.)]);
    let doc = document(
        None,
        SemanticGeometry::Region {
            contours: vec![outer, inner],
        },
    );
    doc.validate().unwrap();
    assert_eq!(doc.layer_coverage_at("layer", point(2., 2.)), Some(true));
}

#[test]
fn degenerate_backtracking_contour_is_not_a_region() {
    for points in [
        vec![(0., 0.), (1., 0.), (0., 0.)],
        vec![(0., 0.), (2., 0.), (1., 0.), (1., 1.), (0., 1.), (0., 0.)],
    ] {
        assert!(
            document(
                None,
                SemanticGeometry::Region {
                    contours: vec![contour(&points)]
                }
            )
            .validate()
            .is_err()
        );
    }
}
