use editor_core::hit_test::SelectRectMode;
use editor_core::*;
fn p(x: f64, y: f64) -> MmPoint {
    MmPoint::new(x, y)
}
fn doc(geometry: SemanticGeometry, shape: Option<ApertureShape>) -> SemanticDocument {
    SemanticDocument {
        id: "test".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        source: SourceMetadata::default(),
        block_definitions: vec![],
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
            center: p(0., 0.),
            aperture_id: "ap".into(),
            transform,
        },
        Some(shape),
    )
}
fn circle(exposure: Exposure, r: f64, c: MmPoint) -> MacroPrimitive {
    MacroPrimitive::Circle {
        exposure,
        diameter_mm: 2. * r,
        center: c,
        rotation_deg: 0.,
    }
}
fn rectangle(exposure: Exposure, w: f64, h: f64, c: MmPoint) -> MacroPrimitive {
    MacroPrimitive::CenterLine {
        exposure,
        width_mm: w,
        height_mm: h,
        center: c,
        rotation_deg: 0.,
    }
}
fn macro_doc(primitives: Vec<MacroPrimitive>) -> SemanticDocument {
    flash(
        ApertureShape::Macro { primitives },
        LocalTransform::default(),
    )
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

fn rect(a: f64, b: f64, c: f64, d: f64) -> BoundsMm {
    BoundsMm {
        min_x_mm: a,
        min_y_mm: b,
        max_x_mm: c,
        max_y_mm: d,
    }
}
fn check(d: &SemanticDocument, r: BoundsMm, m: SelectRectMode, hit: bool) {
    assert_eq!(
        !d.select_rect("layer", r, m).unwrap().is_empty(),
        hit,
        "{r:?} {m:?}"
    );
}
#[test]
fn window_select_requires_exact_full_containment() {
    let d = macro_doc(vec![
        rectangle(Exposure::Dark, 20., 4., p(0., 0.)),
        rectangle(Exposure::Clear, 12., 6., p(5., 0.)),
    ]);
    check(&d, rect(-10., -2., -1., 2.), SelectRectMode::Window, true);
    check(
        &d,
        rect(-10., -2., -1.01, 2.),
        SelectRectMode::Window,
        false,
    );
}
#[test]
fn crossing_select_uses_exact_geometry_intersection() {
    let d = flash(
        ApertureShape::Circle {
            diameter_mm: 10.,
            hole_diameter_mm: Some(6.),
        },
        LocalTransform::default(),
    );
    check(&d, rect(-1., -1., 1., 1.), SelectRectMode::Crossing, false);
    check(
        &d,
        rect(4.9, -0.1, 5.1, 0.1),
        SelectRectMode::Crossing,
        true,
    );
    check(&d, rect(4.8, 4.8, 5., 5.), SelectRectMode::Crossing, false);
}
#[test]
fn rectangular_sweep_does_not_select_by_aabb_only() {
    let d = doc(
        SemanticGeometry::RectangularSweep {
            start: p(0., 0.),
            end: p(10., 10.),
            width_mm: 2.,
            height_mm: 2.,
        },
        None,
    );
    check(&d, rect(-1., 9., 1., 11.), SelectRectMode::Crossing, false);
    check(&d, rect(4., 4., 6., 6.), SelectRectMode::Crossing, true);
}
#[test]
fn arc_window_and_crossing_use_real_sweep() {
    let d = doc(
        SemanticGeometry::Arc {
            path: arc(ArcDirection::CounterClockwise),
            width_mm: 2.,
        },
        None,
    );
    check(&d, rect(-1., -1., 11., 11.), SelectRectMode::Window, true);
    check(&d, rect(0., 0., 2., 2.), SelectRectMode::Crossing, false);
    check(&d, rect(6., 6., 8., 8.), SelectRectMode::Crossing, true);
}
#[test]
fn macro_rectangle_relation_is_not_bounds_only() {
    let d = macro_doc(vec![
        circle(Exposure::Dark, 5., p(0., 0.)),
        circle(Exposure::Clear, 3., p(0., 0.)),
    ]);
    check(&d, rect(-1., -1., 1., 1.), SelectRectMode::Crossing, false);
    check(&d, rect(-5., -5., 5., 5.), SelectRectMode::Window, true);
}
#[test]
fn region_hole_and_cutin_rectangle_relations() {
    let points = [
        p(0., 2.),
        p(1., 2.),
        p(1., 3.),
        p(3., 3.),
        p(3., 1.),
        p(1., 1.),
        p(1., 2.),
        p(0., 2.),
        p(0., 0.),
        p(4., 0.),
        p(4., 4.),
        p(0., 4.),
        p(0., 2.),
    ];
    let d = doc(
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: points
                    .windows(2)
                    .map(|v| RegionEdge::Line {
                        start: v[0],
                        end: v[1],
                    })
                    .collect(),
            }],
        },
        None,
    );
    d.validate().unwrap();
    check(
        &d,
        rect(1.1, 1.1, 2.9, 2.9),
        SelectRectMode::Crossing,
        false,
    );
    check(&d, rect(0.2, 1.9, 0.8, 2.1), SelectRectMode::Crossing, true);
    check(&d, rect(0., 0., 4., 4.), SelectRectMode::Window, true);
    check(&d, rect(0., 0., 3.9, 4.), SelectRectMode::Window, false);
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
    d.validate().unwrap();
    check(&d, rect(-1., 3., 1., 4.), SelectRectMode::Crossing, true);
    check(&d, rect(8., 8., 9., 9.), SelectRectMode::Crossing, false);
}
#[test]
fn transformed_standard_flash_holes_and_exact_extrema() {
    for shape in [
        ApertureShape::Circle {
            diameter_mm: 6.,
            hole_diameter_mm: Some(1.),
        },
        ApertureShape::Rectangle {
            width_mm: 8.,
            height_mm: 4.,
            hole_diameter_mm: Some(1.),
        },
        ApertureShape::Obround {
            width_mm: 8.,
            height_mm: 4.,
            hole_diameter_mm: Some(1.),
        },
        ApertureShape::Polygon {
            diameter_mm: 6.,
            vertices: 5,
            rotation_deg: 17.,
            hole_diameter_mm: Some(1.),
        },
    ] {
        let d = flash(
            shape,
            LocalTransform {
                rotation_deg: 37.,
                scale: 2.,
                mirror: Mirror::X,
            },
        );
        d.validate().unwrap();
        check(
            &d,
            rect(-0.1, -0.1, 0.1, 0.1),
            SelectRectMode::Crossing,
            false,
        );
        check(&d, rect(-20., -20., 20., 20.), SelectRectMode::Window, true);
        check(&d, rect(-1., -1., 1., 1.), SelectRectMode::Window, false);
    }
    let d = flash(
        ApertureShape::Obround {
            width_mm: 8.,
            height_mm: 4.,
            hole_diameter_mm: None,
        },
        LocalTransform {
            rotation_deg: 90.,
            ..Default::default()
        },
    );
    check(&d, rect(-2., -4., 2., 4.), SelectRectMode::Window, true);
    check(&d, rect(1.9, 3.9, 2., 4.), SelectRectMode::Crossing, false);
    let d = macro_doc(vec![MacroPrimitive::Outline {
        exposure: Exposure::Dark,
        points: vec![p(0., 0.), p(6., 0.), p(0., 6.)],
        rotation_deg: 0.,
    }]);
    check(&d, rect(4., 4., 5., 5.), SelectRectMode::Crossing, false);
    check(&d, rect(1., 1., 2., 2.), SelectRectMode::Crossing, true);
}
#[test]
fn arc_directions_full_zero_and_deviation() {
    for direction in [ArcDirection::Clockwise, ArcDirection::CounterClockwise] {
        let d = doc(
            SemanticGeometry::Arc {
                path: arc(direction),
                width_mm: 2.,
            },
            None,
        );
        check(
            &d,
            rect(-8., -8., -6., -6.),
            SelectRectMode::Crossing,
            direction == ArcDirection::Clockwise,
        );
    }
    for full_circle in [false, true] {
        let a = ArcGeometry {
            end: p(10., 0.),
            full_circle,
            ..arc(ArcDirection::CounterClockwise)
        };
        let d = doc(
            SemanticGeometry::Arc {
                path: a,
                width_mm: 2.,
            },
            None,
        );
        d.validate().unwrap();
        check(
            &d,
            rect(-11., -1., -9., 1.),
            SelectRectMode::Crossing,
            full_circle,
        );
        check(
            &d,
            rect(9., -1., 11., 1.),
            SelectRectMode::Window,
            !full_circle,
        );
    }
    let a = ArcGeometry {
        end: p(0., 10.000001),
        source: Some(ArcSource {
            resolution_mm: 0.000001,
            single_quadrant: false,
        }),
        ..arc(ArcDirection::CounterClockwise)
    };
    let d = doc(
        SemanticGeometry::Arc {
            path: a,
            width_mm: 0.2,
        },
        None,
    );
    d.validate().unwrap();
    check(
        &d,
        rect(-0.1, 9.999999, 0.1, 10.100002),
        SelectRectMode::Crossing,
        true,
    );
    check(
        &d,
        rect(-0.100002, -0.100002, 10.100002, 10.100002),
        SelectRectMode::Window,
        true,
    );
}
#[test]
fn closed_rect_contact_degenerate_rect_and_submicron_gap() {
    let d = flash(
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: None,
        },
        LocalTransform::default(),
    );
    check(&d, rect(1., 0., 2., 1.), SelectRectMode::Crossing, true);
    check(
        &d,
        rect(1.0000001, 0., 2., 1.),
        SelectRectMode::Crossing,
        false,
    );
    check(&d, rect(0., 0., 0., 0.), SelectRectMode::Crossing, true);
    check(&d, rect(0., 0., 0., 0.), SelectRectMode::Window, false);
    let d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(1e-8, 0.),
            width_mm: 1e-9,
        },
        None,
    );
    check(
        &d,
        rect(5e-9, -1e-9, 6e-9, 1e-9),
        SelectRectMode::Crossing,
        true,
    );
    check(
        &d,
        rect(-1e-9, -1e-9, 11e-9, 1e-9),
        SelectRectMode::Window,
        true,
    );
}
#[test]
fn empty_macro_and_clear_objects_have_stable_selection_order() {
    let d = macro_doc(vec![
        circle(Exposure::Dark, 2., p(0., 0.)),
        circle(Exposure::Clear, 3., p(0., 0.)),
    ]);
    for mode in [SelectRectMode::Window, SelectRectMode::Crossing] {
        check(&d, rect(-10., -10., 10., 10.), mode, false);
    }
    let mut d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(2., 0.),
            width_mm: 2.,
        },
        None,
    );
    let mut clear = d.layers[0].objects[0].clone();
    clear.object_id = "clear".into();
    clear.exposure = Exposure::Clear;
    d.layers[0].objects.push(clear);
    assert_eq!(
        d.select_rect("layer", rect(-1., -1., 3., 1.), SelectRectMode::Window)
            .unwrap(),
        vec!["object", "clear"]
    );
}
#[test]
fn rect_invalid_numerical_failure_and_unlimited_macro_query_are_atomic() {
    use editor_core::hit_test::HitTestError;
    let mut d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(1., 0.),
            width_mm: 1.,
        },
        None,
    );
    let before = d.clone();
    for r in [
        rect(1., 0., 0., 1.),
        rect(f64::NAN, 0., 1., 1.),
        rect(0., 0., f64::INFINITY, 1.),
    ] {
        assert!(d.select_rect("layer", r, SelectRectMode::Crossing).is_err());
    }
    assert_eq!(d, before);
    assert!(matches!(
        d.select_rect("missing", rect(0., 0., 1., 1.), SelectRectMode::Window),
        Err(HitTestError::MissingLayer(_))
    ));
    let expensive = macro_doc(
        (0..1500)
            .map(|i| circle(Exposure::Dark, 1., p(f64::from(i) * 3., 0.)))
            .collect(),
    );
    d.apertures = expensive.apertures;
    d.layers[0]
        .objects
        .extend(expensive.layers[0].objects.clone());
    // Keep the same expensive fixture formerly rejected by the work cap.
    d.layers[0].objects[1].object_id = "expensive-macro".into();
    let expensive_before = d.clone();
    for mode in [SelectRectMode::Window, SelectRectMode::Crossing] {
        assert_eq!(
            d.select_rect("layer", rect(-1., -1., 5000., 2.), mode)
                .unwrap(),
            vec!["object", "expensive-macro"]
        );
        assert_eq!(d, expensive_before);
    }
    assert!(matches!(
        before.select_rect("layer", rect(0., 0., 1e9, 1e9), SelectRectMode::Window),
        Err(HitTestError::Unsupported(_))
    ));
}
#[test]
fn macro_rectangles_match_independent_cell_relation_truth() {
    let boxes = [
        (-5., -4., 5., 4., true),
        (-1., -5., 6., 2., false),
        (2., -1., 4., 1., true),
    ];
    let d = macro_doc(
        boxes
            .iter()
            .map(|&(x, y, u, v, dark)| {
                rectangle(
                    if dark {
                        Exposure::Dark
                    } else {
                        Exposure::Clear
                    },
                    u - x,
                    v - y,
                    p((x + u) / 2., (y + v) / 2.),
                )
            })
            .collect(),
    );
    let xs = [-5., -1., 2., 4., 5., 6.];
    let ys = [-5., -4., -1., 1., 2., 4.];
    let mut cells = Vec::new();
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            let (cx, cy) = ((x[0] + x[1]) / 2., (y[0] + y[1]) / 2.);
            let mut material = false;
            for &(a, b, c, e, dark) in &boxes {
                if cx > a && cx < c && cy > b && cy < e {
                    material = dark;
                }
            }
            if material {
                cells.push(rect(x[0], y[0], x[1], y[1]));
            }
        }
    }
    for x in -7..7 {
        for y in -7..7 {
            let r = rect(
                f64::from(x) + 0.13,
                f64::from(y) + 0.17,
                f64::from(x) + 2.53,
                f64::from(y) + 2.71,
            );
            let overlap = cells.iter().any(|c| {
                c.min_x_mm <= r.max_x_mm
                    && c.max_x_mm >= r.min_x_mm
                    && c.min_y_mm <= r.max_y_mm
                    && c.max_y_mm >= r.min_y_mm
            });
            let contained = cells.iter().all(|c| {
                c.min_x_mm >= r.min_x_mm
                    && c.max_x_mm <= r.max_x_mm
                    && c.min_y_mm >= r.min_y_mm
                    && c.max_y_mm <= r.max_y_mm
            });
            check(&d, r, SelectRectMode::Crossing, overlap);
            check(&d, r, SelectRectMode::Window, contained);
        }
    }
}

#[test]
fn seventy_thousand_capsules_have_exact_ordered_rectangle_results_without_work_cap() {
    let mut d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(1., 0.),
            width_mm: 1.,
        },
        None,
    );
    d.layers[0].objects = (0..70000)
        .map(|i| SemanticObject {
            object_id: format!("line-{i}"),
            exposure: Exposure::Dark,
            origin: ObjectOrigin::Imported { command_index: i },
            geometry: SemanticGeometry::Line {
                start: p(i as f64 * 3., 0.),
                end: p(i as f64 * 3. + 1., 0.),
                width_mm: 1.,
            },
        })
        .collect();
    let before = d.clone();
    for mode in [SelectRectMode::Window, SelectRectMode::Crossing] {
        let all = d
            .select_rect("layer", rect(-1., -1., 210000., 1.), mode)
            .unwrap();
        assert_eq!(
            all,
            (0..70000).map(|i| format!("line-{i}")).collect::<Vec<_>>()
        );
    }
    // Analytic capsule extents: left -0.5, right +1.5, y +/-0.5.
    let r = rect(300., -1., 600., 1.);
    assert_eq!(
        d.select_rect("layer", r, SelectRectMode::Window).unwrap(),
        (101..200).map(|i| format!("line-{i}")).collect::<Vec<_>>()
    );
    assert_eq!(
        d.select_rect("layer", r, SelectRectMode::Crossing).unwrap(),
        (100..=200).map(|i| format!("line-{i}")).collect::<Vec<_>>()
    );
    assert_eq!(d, before);
}
