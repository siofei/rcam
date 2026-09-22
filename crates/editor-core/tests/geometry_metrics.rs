use editor_core::metrics::*;
use editor_core::*;
use std::f64::consts::PI;
fn p(x: f64, y: f64) -> MmPoint {
    MmPoint::new(x, y)
}
fn document(shape: Option<ApertureShape>) -> SemanticDocument {
    SemanticDocument {
        id: "d".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![],
        apertures: shape
            .into_iter()
            .map(|shape| ApertureDefinition {
                id: "a".into(),
                source_dcode: 10,
                shape,
            })
            .collect(),
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}
fn flash(shape: ApertureShape, t: LocalTransform) -> Result<GeometryMetrics, MetricsError> {
    calculate(
        &document(Some(shape)),
        &SemanticGeometry::Flash {
            center: p(100., -40.),
            aperture_id: "a".into(),
            transform: t,
        },
        &mut 0,
    )
}
fn metric(g: SemanticGeometry) -> Result<GeometryMetrics, MetricsError> {
    calculate(&document(None), &g, &mut 0)
}
fn check(m: GeometryMetrics, a: f64, p: f64) {
    assert!((m.area_mm2 - a).abs() < 1e-9, "{m:?} expected {a}");
    assert!((m.perimeter_mm - p).abs() < 1e-9, "{m:?} expected {p}");
}
#[test]
fn circle_flash_metrics_exact() {
    check(
        flash(
            ApertureShape::Circle {
                diameter_mm: 4.,
                hole_diameter_mm: None,
            },
            LocalTransform::default(),
        )
        .unwrap(),
        4. * PI,
        4. * PI,
    );
}
#[test]
fn circle_flash_hole_counts_inner_perimeter() {
    check(
        flash(
            ApertureShape::Circle {
                diameter_mm: 4.,
                hole_diameter_mm: Some(2.),
            },
            LocalTransform::default(),
        )
        .unwrap(),
        3. * PI,
        6. * PI,
    );
}
#[test]
fn rectangle_flash_metrics_exact() {
    check(
        flash(
            ApertureShape::Rectangle {
                width_mm: 4.,
                height_mm: 3.,
                hole_diameter_mm: Some(2.),
            },
            LocalTransform::default(),
        )
        .unwrap(),
        12. - PI,
        14. + 2. * PI,
    );
}
#[test]
fn obround_metrics_exact() {
    for (w, h) in [(6., 2.), (2., 6.)] {
        check(
            flash(
                ApertureShape::Obround {
                    width_mm: w,
                    height_mm: h,
                    hole_diameter_mm: Some(1.),
                },
                LocalTransform::default(),
            )
            .unwrap(),
            8. + 0.75 * PI,
            8. + 3. * PI,
        );
    }
}
#[test]
fn polygon_metrics_exact() {
    check(
        flash(
            ApertureShape::Polygon {
                diameter_mm: 4.,
                vertices: 4,
                rotation_deg: 37.,
                hole_diameter_mm: Some(2.),
            },
            LocalTransform::default(),
        )
        .unwrap(),
        8. - PI,
        8. * 2_f64.sqrt() + 2. * PI,
    );
}
#[test]
fn local_rotation_and_mirror_preserve_metrics() {
    for mirror in [Mirror::None, Mirror::X, Mirror::Y, Mirror::Xy] {
        check(
            flash(
                ApertureShape::Rectangle {
                    width_mm: 4.,
                    height_mm: 3.,
                    hole_diameter_mm: None,
                },
                LocalTransform {
                    mirror,
                    rotation_deg: 37.,
                    scale: 1.,
                },
            )
            .unwrap(),
            12.,
            14.,
        );
    }
}
#[test]
fn local_scale_updates_metrics_by_s2_and_abs_s() {
    check(
        flash(
            ApertureShape::Circle {
                diameter_mm: 4.,
                hole_diameter_mm: Some(2.),
            },
            LocalTransform {
                scale: 3.,
                ..Default::default()
            },
        )
        .unwrap(),
        27. * PI,
        18. * PI,
    );
}
#[test]
fn invalid_scale_and_hole_fail_closed() {
    for scale in [0., -1., f64::INFINITY, f64::NAN] {
        assert!(
            flash(
                ApertureShape::Circle {
                    diameter_mm: 4.,
                    hole_diameter_mm: None
                },
                LocalTransform {
                    scale,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert!(
        flash(
            ApertureShape::Polygon {
                diameter_mm: 4.,
                vertices: 4,
                rotation_deg: 0.,
                hole_diameter_mm: Some(3.)
            },
            Default::default()
        )
        .is_err()
    );
}
#[test]
fn line_stroke_metrics_exact() {
    check(
        metric(SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(3., 4.),
            width_mm: 2.,
        })
        .unwrap(),
        10. + PI,
        10. + 2. * PI,
    );
}
#[test]
fn zero_length_line_is_circle_metrics() {
    check(
        metric(SemanticGeometry::Line {
            start: p(4., 4.),
            end: p(4., 4.),
            width_mm: 2.,
        })
        .unwrap(),
        PI,
        2. * PI,
    );
}
#[test]
fn rectangular_sweep_uses_real_polygon_not_aabb() {
    check(
        metric(SemanticGeometry::RectangularSweep {
            start: p(0., 0.),
            end: p(3., 4.),
            width_mm: 2.,
            height_mm: 1.,
        })
        .unwrap(),
        13.,
        16.,
    );
}
fn arc(end: MmPoint, full_circle: bool, direction: ArcDirection) -> ArcGeometry {
    ArcGeometry {
        start: p(5., 0.),
        end,
        center: p(0., 0.),
        direction,
        full_circle,
        source: None,
    }
}
#[test]
fn arc_zero_sweep_metrics_exact() {
    check(
        metric(SemanticGeometry::Arc {
            path: arc(p(5., 0.), false, ArcDirection::Clockwise),
            width_mm: 2.,
        })
        .unwrap(),
        PI,
        2. * PI,
    );
}
#[test]
fn arc_full_circle_metrics_exact() {
    for (w, a, perimeter) in [
        (2., 20. * PI, 20. * PI),
        (10., 100. * PI, 20. * PI),
        (12., 121. * PI, 22. * PI),
    ] {
        check(
            metric(SemanticGeometry::Arc {
                path: arc(p(5., 0.), true, ArcDirection::Clockwise),
                width_mm: w,
            })
            .unwrap(),
            a,
            perimeter,
        );
    }
}
#[test]
fn arc_safe_open_metrics_exact() {
    for direction in [ArcDirection::Clockwise, ArcDirection::CounterClockwise] {
        check(
            metric(SemanticGeometry::Arc {
                path: arc(p(-5., 0.), false, direction),
                width_mm: 2.,
            })
            .unwrap(),
            11. * PI,
            12. * PI,
        );
    }
}
#[test]
fn arc_unsafe_overlap_is_unsupported_not_approximate() {
    for (end, width_mm) in [
        (p(5. * 0.01_f64.cos(), -5. * 0.01_f64.sin()), 2.),
        (p(0., 5.), 12.),
        (p(0., 5.01), 2.),
    ] {
        assert!(matches!(
            metric(SemanticGeometry::Arc {
                path: arc(end, false, ArcDirection::CounterClockwise),
                width_mm
            }),
            Err(MetricsError::Unsupported(_))
        ));
    }
}
fn polygon(points: &[(f64, f64)]) -> RegionContour {
    RegionContour {
        role: RegionRole::Solid,
        edges: points
            .windows(2)
            .map(|w| RegionEdge::Line {
                start: p(w[0].0, w[0].1),
                end: p(w[1].0, w[1].1),
            })
            .collect(),
    }
}
#[test]
fn region_line_arc_green_area_exact() {
    let a = arc(p(-5., 0.), false, ArcDirection::CounterClockwise);
    check(
        metric(SemanticGeometry::Region {
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
        })
        .unwrap(),
        12.5 * PI,
        5. * PI + 10.,
    );
}
fn hole() -> RegionContour {
    polygon(&[
        (0., 0.),
        (10., 0.),
        (10., 10.),
        (0., 10.),
        (0., 0.),
        (3., 0.),
        (3., 3.),
        (3., 7.),
        (7., 7.),
        (7., 3.),
        (3., 3.),
        (3., 0.),
        (0., 0.),
    ])
}
#[test]
fn region_hole_area_and_perimeter() {
    // Frozen cut-in shape uses a single parallel connector, split outer bottom edge.
    let c = polygon(&[
        (0., 0.),
        (3., 0.),
        (3., 3.),
        (3., 7.),
        (7., 7.),
        (7., 3.),
        (3., 3.),
        (3., 0.),
        (10., 0.),
        (10., 10.),
        (0., 10.),
        (0., 0.),
    ]);
    check(
        metric(SemanticGeometry::Region { contours: vec![c] }).unwrap(),
        84.,
        56.,
    );
}
#[test]
fn region_cutin_does_not_double_count_perimeter() {
    let c = polygon(&[
        (0., 0.),
        (3., 0.),
        (3., 3.),
        (3., 7.),
        (7., 7.),
        (7., 3.),
        (3., 3.),
        (3., 0.),
        (10., 0.),
        (10., 10.),
        (0., 10.),
        (0., 0.),
    ]);
    let mut reverse = c.clone();
    reverse.edges = c
        .edges
        .iter()
        .rev()
        .map(|e| match e {
            RegionEdge::Line { start, end } => RegionEdge::Line {
                start: *end,
                end: *start,
            },
            _ => unreachable!(),
        })
        .collect();
    check(
        metric(SemanticGeometry::Region {
            contours: vec![reverse],
        })
        .unwrap(),
        84.,
        56.,
    );
}
#[test]
fn multiple_disjoint_region_contours_and_overlap_refusal() {
    let a = polygon(&[(0., 0.), (2., 0.), (2., 2.), (0., 2.), (0., 0.)]);
    let b = polygon(&[(4., 0.), (6., 0.), (6., 2.), (4., 2.), (4., 0.)]);
    check(
        metric(SemanticGeometry::Region {
            contours: vec![a.clone(), b],
        })
        .unwrap(),
        8.,
        16.,
    );
    assert!(
        metric(SemanticGeometry::Region {
            contours: vec![a.clone(), a]
        })
        .is_err()
    );
}
#[test]
fn unsupported_macro_is_not_zero() {
    assert!(matches!(
        flash(
            ApertureShape::Macro { primitives: vec![] },
            Default::default()
        ),
        Err(MetricsError::Unsupported(_))
    ));
}
#[test]
fn region_budget_is_bounded() {
    let mut c = hole();
    c.edges = (0..1000).flat_map(|_| c.edges.clone()).collect();
    assert_eq!(
        metric(SemanticGeometry::Region { contours: vec![c] }),
        Err(MetricsError::ResourceLimit)
    );
}
