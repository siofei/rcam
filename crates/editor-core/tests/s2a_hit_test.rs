use editor_core::hit_test::HitTestError;
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
fn check(d: &SemanticDocument, point: MmPoint, tol: f64, hit: bool) {
    assert_eq!(
        !d.hit_test("layer", point, tol).unwrap().is_empty(),
        hit,
        "point={point:?} tol={tol}"
    );
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

#[test]
fn hit_test_flash_circle_hole() {
    let d = flash(
        ApertureShape::Circle {
            diameter_mm: 10.,
            hole_diameter_mm: Some(6.),
        },
        LocalTransform::default(),
    );
    d.validate().unwrap();
    for (point, tol, hit) in [
        (p(4., 0.), 0., true),
        (p(0., 0.), 0.2, false),
        (p(2.8, 0.), 0.19, false),
        (p(2.8, 0.), 0.21, true),
        (p(3., 0.), 0., true),
        (p(5.2, 0.), 0.19, false),
        (p(5.2, 0.), 0.21, true),
    ] {
        check(&d, point, tol, hit);
    }
}
#[test]
fn hit_test_rectangle_obround_polygon_local_transform() {
    for shape in [
        ApertureShape::Rectangle {
            width_mm: 6.,
            height_mm: 2.,
            hole_diameter_mm: Some(1.),
        },
        ApertureShape::Obround {
            width_mm: 6.,
            height_mm: 2.,
            hole_diameter_mm: Some(1.),
        },
        ApertureShape::Polygon {
            diameter_mm: 6.,
            vertices: 4,
            rotation_deg: 0.,
            hole_diameter_mm: Some(1.),
        },
    ] {
        for mirror in [Mirror::None, Mirror::X, Mirror::Y, Mirror::Xy] {
            let d = flash(
                shape.clone(),
                LocalTransform {
                    mirror,
                    rotation_deg: 90.,
                    scale: 2.,
                },
            );
            d.validate().unwrap();
            check(&d, p(0., 4.), 0., true);
            check(&d, p(0., 0.), 0.1, false);
            check(&d, p(0., 0.8), 0.19, false);
            check(&d, p(0., 0.8), 0.21, true);
            check(&d, p(5., 5.), 0., false);
        }
    }
    let d = flash(
        ApertureShape::Rectangle {
            width_mm: 2.,
            height_mm: 2.,
            hole_diameter_mm: None,
        },
        LocalTransform::default(),
    );
    check(&d, p(1.1, 1.1), 0.12, false);
    check(&d, p(1.1, 1.1), 0.15, true); // Euclidean corner distance, not inflated rectangle.
    let d = flash(
        ApertureShape::Obround {
            width_mm: 6.,
            height_mm: 2.,
            hole_diameter_mm: None,
        },
        LocalTransform::default(),
    );
    check(&d, p(2.9, 0.9), 0., false);
    let d = flash(
        ApertureShape::Polygon {
            diameter_mm: 4.,
            vertices: 3,
            rotation_deg: 0.,
            hole_diameter_mm: None,
        },
        LocalTransform::default(),
    );
    check(&d, p(-1.5, 0.), 0., false);
}
#[test]
fn hit_test_macro_dark_clear_primitives() {
    let d = macro_doc(vec![
        circle(Exposure::Dark, 5., p(0., 0.)),
        circle(Exposure::Clear, 3., p(0., 0.)),
        rectangle(Exposure::Dark, 1., 1., p(0., 0.)),
    ]);
    d.validate().unwrap();
    check(&d, p(4., 0.), 0., true);
    check(&d, p(2., 0.), 0.1, false);
    check(&d, p(2.8, 0.), 0.21, true);
    check(&d, p(0., 0.), 0., true);
    check(&d, p(0.8, 0.8), 0.4, false);
    let d = macro_doc(vec![
        rectangle(Exposure::Dark, 2., 2., p(3., 0.)),
        rectangle(Exposure::Clear, 2., 2., p(3., 0.)),
    ]);
    for point in [p(3., 0.), p(2., 0.), p(4., 1.)] {
        check(&d, point, 1., false);
    }
}
#[test]
fn macro_overlap_corner_tolerance_uses_final_exposed_edges() {
    // Remaining material is lower-left quadrant of the square. Distance from
    // (1,1) to final material is sqrt(2), not distance 1 to an erased boundary.
    let d = macro_doc(vec![
        rectangle(Exposure::Dark, 4., 4., p(0., 0.)),
        rectangle(Exposure::Clear, 4., 8., p(2., 0.)),
        rectangle(Exposure::Clear, 8., 4., p(0., 2.)),
    ]);
    d.validate().unwrap();
    check(&d, p(1., 1.), 1.1, false);
    check(&d, p(1., 1.), 1.5, true);
    check(&d, p(-1., -1.), 0., true);
    check(&d, p(1., -1.), 0., false);
}
#[test]
fn macro_overlapping_circles_and_complete_multi_clear_erasure() {
    let d = macro_doc(vec![
        circle(Exposure::Dark, 2., p(0., 0.)),
        circle(Exposure::Clear, 2., p(2., 0.)),
    ]);
    check(&d, p(-1., 0.), 0., true);
    check(&d, p(1., 0.), 0.9, false);
    check(&d, p(1., 0.), 1.01, true);
    let erased = macro_doc(vec![
        rectangle(Exposure::Dark, 4., 4., p(0., 0.)),
        rectangle(Exposure::Clear, 4., 8., p(-2., 0.)),
        rectangle(Exposure::Clear, 4., 8., p(2., 0.)),
    ]);
    check(&erased, p(0., 0.), 10., false);
}
#[test]
fn macro_rotated_outline_offset_and_local_mirror() {
    let primitive = MacroPrimitive::Outline {
        exposure: Exposure::Dark,
        points: vec![p(1., 0.), p(3., 0.), p(1., 2.), p(1., 0.)],
        rotation_deg: 90.,
    };
    let d = flash(
        ApertureShape::Macro {
            primitives: vec![primitive],
        },
        LocalTransform {
            mirror: Mirror::Y,
            rotation_deg: 90.,
            scale: 2.,
        },
    );
    d.validate().unwrap();
    check(&d, p(3., -1.), 0., true);
    check(&d, p(5., -3.), 0., false);
    check(&d, p(0., 0.), 0., false);
    let d = flash(
        ApertureShape::Macro {
            primitives: vec![MacroPrimitive::Circle {
                exposure: Exposure::Dark,
                diameter_mm: 2.,
                center: p(3., 0.),
                rotation_deg: 90.,
            }],
        },
        LocalTransform {
            mirror: Mirror::X,
            rotation_deg: 90.,
            scale: 2.,
        },
    );
    check(&d, p(-6., 0.), 0., true);
    check(&d, p(0., 6.), 0., false);
}
#[test]
fn hit_test_line_width_and_tolerance() {
    let d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(4., 4.),
            width_mm: 1.,
        },
        None,
    );
    check(&d, p(0., 4.), 0., false);
    check(&d, p(2., 2.5), 0., true);
    check(&d, p(4.6, 4.), 0.09, false);
    check(&d, p(4.6, 4.), 0.11, true);
}
#[test]
fn hit_test_zero_length_line_and_tiny_nonzero_segment() {
    let d = doc(
        SemanticGeometry::Line {
            start: p(1., 2.),
            end: p(1., 2.),
            width_mm: 2.,
        },
        None,
    );
    check(&d, p(1., 2.), 0., true);
    check(&d, p(2.2, 2.), 0.19, false);
    check(&d, p(2.2, 2.), 0.21, true);
    let d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(1e-8, 0.),
            width_mm: 1e-10,
        },
        None,
    );
    check(&d, p(1e-8, 0.), 0., true);
    check(&d, p(5e-9, 1e-9), 0., false);
}
#[test]
fn hit_test_rectangular_sweep_exact_not_aabb() {
    for end in [p(4., 0.), p(0., 4.), p(4., 4.), p(0., 0.)] {
        let d = doc(
            SemanticGeometry::RectangularSweep {
                start: p(0., 0.),
                end,
                width_mm: 2.,
                height_mm: 1.,
            },
            None,
        );
        check(&d, p(end.x_mm / 2., end.y_mm / 2.), 0., true);
        check(&d, p(10., 10.), 0., false);
    }
    // Diagonal algorithm support does not extend parser/writer permissions.
    let d = doc(
        SemanticGeometry::RectangularSweep {
            start: p(0., 0.),
            end: p(4., 4.),
            width_mm: 2.,
            height_mm: 1.,
        },
        None,
    );
    check(&d, p(-0.9, 4.4), 0., false);
    check(&d, p(1.8, 2.), 0., true);
    check(&d, p(5.1, 4.6), 0.12, false);
    check(&d, p(5.1, 4.6), 0.15, true);
}
#[test]
fn hit_test_arc_direction_and_sweep() {
    let ccw = doc(
        SemanticGeometry::Arc {
            path: arc(ArcDirection::CounterClockwise),
            width_mm: 2.,
        },
        None,
    );
    check(&ccw, p(0., -10.), 0., false);
    check(&ccw, p(10., -0.8), 0., true);
    check(&ccw, p(10., -1.2), 0.1, false);
    check(&ccw, p(10., -1.2), 0.21, true);
    check(&ccw, p(7.5, 7.5), 0., true);
    check(&ccw, p(8., 8.), 0., false);
    let cw = doc(
        SemanticGeometry::Arc {
            path: arc(ArcDirection::Clockwise),
            width_mm: 2.,
        },
        None,
    );
    check(&cw, p(0., -10.), 0., true);
    check(&cw, p(7., 7.), 0., false);
}
#[test]
fn hit_test_arc_full_zero_and_deviation() {
    let a = arc(ArcDirection::CounterClockwise);
    let full = doc(
        SemanticGeometry::Arc {
            path: ArcGeometry {
                end: a.start,
                full_circle: true,
                ..a
            },
            width_mm: 2.,
        },
        None,
    );
    check(&full, p(-10., 0.), 0., true);
    check(&full, p(0., 0.), 0., false);
    let dot = doc(
        SemanticGeometry::Arc {
            path: ArcGeometry { end: a.start, ..a },
            width_mm: 2.,
        },
        None,
    );
    check(&dot, p(10., 0.), 0., true);
    check(&dot, p(-10., 0.), 0., false);
    let fuzzy = doc(
        SemanticGeometry::Arc {
            path: ArcGeometry {
                end: p(0., 12.),
                ..a
            },
            width_mm: 0.2,
        },
        None,
    );
    check(&fuzzy, p(10.5, 0.), 0., true);
    check(&fuzzy, p(0., 11.5), 0., true);
    check(&fuzzy, p(11. / 2_f64.sqrt(), 11. / 2_f64.sqrt()), 0., true);
    check(&fuzzy, p(10. / 2_f64.sqrt(), 10. / 2_f64.sqrt()), 0., false);
}
#[test]
fn hit_test_region_inside_outside_hole_cutin() {
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
                    .map(|pair| RegionEdge::Line {
                        start: pair[0],
                        end: pair[1],
                    })
                    .collect(),
            }],
        },
        None,
    );
    d.validate().unwrap();
    check(&d, p(0.5, 0.5), 0., true);
    check(&d, p(2., 2.), 0.2, false);
    check(&d, p(1.1, 2.), 0.09, false);
    check(&d, p(1.1, 2.), 0.11, true);
    check(&d, p(0.5, 2.), 0., true);
    check(&d, p(-0.1, -0.1), 0.12, false);
    check(&d, p(-0.1, -0.1), 0.15, true);
}
#[test]
fn region_real_arc_edges_and_independent_contour_union() {
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
    check(&d, p(0., 5.), 0., true);
    check(&d, p(0., -1.), 0., false);
    check(&d, p(0., 10.2), 0.19, false);
    check(&d, p(0., 10.2), 0.21, true);
}
#[test]
fn hit_test_clear_object_returns_object_geometry_and_order() {
    let mut d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(2., 0.),
            width_mm: 2.,
        },
        None,
    );
    for (id, exposure) in [("z", Exposure::Clear), ("a", Exposure::Dark)] {
        let mut o = d.layers[0].objects[0].clone();
        o.object_id = id.into();
        o.exposure = exposure;
        d.layers[0].objects.push(o);
    }
    assert_eq!(
        d.hit_test("layer", p(1., 0.), 0.).unwrap(),
        vec!["object", "z", "a"]
    );
}
#[test]
fn hit_test_invalid_params_and_budget_are_fail_closed() {
    let d = doc(
        SemanticGeometry::Line {
            start: p(0., 0.),
            end: p(1., 0.),
            width_mm: 1.,
        },
        None,
    );
    let before = d.clone();
    for (point, tol) in [
        (p(f64::NAN, 0.), 0.),
        (p(0., f64::INFINITY), 0.),
        (p(1e9 + 1., 0.), 0.),
        (p(0., 0.), -1.),
        (p(0., 0.), f64::INFINITY),
        (p(0., 0.), 1e9 + 1.),
    ] {
        assert!(matches!(
            d.hit_test("layer", point, tol),
            Err(HitTestError::InvalidArgument(_))
        ));
    }
    assert_eq!(d, before);
    assert!(matches!(
        d.hit_test("missing", p(0., 0.), 0.),
        Err(HitTestError::MissingLayer(_))
    ));
    let expensive = macro_doc(
        (0..1500)
            .map(|i| circle(Exposure::Dark, 1., p(f64::from(i) * 3., 0.)))
            .collect(),
    );
    assert!(matches!(
        expensive.hit_test("layer", p(0., 0.), 0.),
        Err(HitTestError::ResourceLimit { limit: 2_000_000, attempted }) if attempted > 2_000_000
    ));
}

#[test]
fn macro_rectangles_match_independent_cell_distance_truth() {
    // Independent oracle: split the plane into axis-aligned cells, classify
    // each cell by the exposure program, then measure Euclidean distance to
    // the material cells. It does not reuse boundary splitting or hit helpers.
    let mut seed = 17_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) % 9) as i32 - 4
    };
    for _ in 0..24 {
        let rects: Vec<_> = (0..5)
            .map(|i| {
                let x = f64::from(next());
                let y = f64::from(next());
                let w = f64::from(next().abs() + 1);
                let h = f64::from(next().abs() + 1);
                (x - w / 2., y - h / 2., x + w / 2., y + h / 2., i % 3 != 1)
            })
            .collect();
        let d = macro_doc(
            rects
                .iter()
                .map(|&(l, b, r, t, dark)| {
                    rectangle(
                        if dark {
                            Exposure::Dark
                        } else {
                            Exposure::Clear
                        },
                        r - l,
                        t - b,
                        p((l + r) / 2., (b + t) / 2.),
                    )
                })
                .collect(),
        );
        d.validate().unwrap();
        let mut xs: Vec<_> = rects.iter().flat_map(|r| [r.0, r.2]).collect();
        let mut ys: Vec<_> = rects.iter().flat_map(|r| [r.1, r.3]).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        ys.sort_by(f64::total_cmp);
        ys.dedup();
        let mut cells = Vec::new();
        for x in xs.windows(2) {
            for y in ys.windows(2) {
                let px = (x[0] + x[1]) / 2.;
                let py = (y[0] + y[1]) / 2.;
                let mut material = false;
                for &(l, b, r, t, dark) in &rects {
                    if l < px && px < r && b < py && py < t {
                        material = dark;
                    }
                }
                if material {
                    cells.push((x[0], y[0], x[1], y[1]));
                }
            }
        }
        for _ in 0..12 {
            let point = p(f64::from(next()) + 0.17, f64::from(next()) + 0.23);
            let distance = cells
                .iter()
                .map(|&(l, b, r, t)| {
                    (l - point.x_mm)
                        .max(point.x_mm - r)
                        .max(0.)
                        .hypot((b - point.y_mm).max(point.y_mm - t).max(0.))
                })
                .fold(f64::INFINITY, f64::min);
            for tolerance in [0., 0.15, 0.6, 1.3] {
                check(&d, point, tolerance, distance <= tolerance);
            }
        }
    }
}

#[test]
fn macro_near_coincident_boundary_is_rejected_not_guessed() {
    let d = macro_doc(vec![
        circle(Exposure::Dark, 2., p(0., 0.)),
        circle(Exposure::Clear, 2. - 1e-14, p(0., 0.)),
    ]);
    assert!(matches!(
        d.hit_test("layer", p(0., 0.), 0.),
        Err(HitTestError::Unsupported(_))
    ));
}

#[test]
fn tiny_hole_center_is_not_filled_by_a_fixed_numeric_selection_band() {
    let mut d = flash(
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: Some(1e-15),
        },
        LocalTransform::default(),
    );
    check(&d, p(0., 0.), 0., false);
    if let SemanticGeometry::Flash { center, .. } = &mut d.layers[0].objects[0].geometry {
        *center = p(10., 20.);
    }
    check(&d, p(10., 20.), 0., false);
}

#[test]
fn dark_union_rounded_pad_tangency_matches_independent_distance() {
    // A cap sits a representable step below a rectangle's tangent edge.
    // Requiring an arrangement used to reject this harmless Dark union.
    let cy = 1.0_f64.next_down();
    let d = macro_doc(vec![
        rectangle(Exposure::Dark, 4., 2., p(0., 0.)),
        circle(Exposure::Dark, 0.5, p(-1.5, cy)),
        circle(Exposure::Dark, 0.5, p(1.5, cy)),
        rectangle(Exposure::Dark, 3., 1., p(0., 1.)),
    ]);
    d.validate().unwrap();
    for x in [-2.2_f64, -2., -1.75, -1., 0., 1., 1.75, 2., 2.2] {
        for y in [-1.2_f64, -1., 0., 0.5, 1., 1.25, 1.5, 1.7] {
            let rectangle_distance = (x.abs() - 2.).max(0.).hypot((y.abs() - 1.).max(0.));
            let bridge_distance = (x.abs() - 1.5)
                .max(0.)
                .hypot(((y - 1.).abs() - 0.5).max(0.));
            let left_distance = ((x + 1.5).hypot(y - cy) - 0.5).max(0.);
            let right_distance = ((x - 1.5).hypot(y - cy) - 0.5).max(0.);
            let distance = rectangle_distance
                .min(bridge_distance)
                .min(left_distance)
                .min(right_distance);
            for tolerance in [0., 0.05, 0.15, 0.3] {
                check(&d, p(x, y), tolerance, distance <= tolerance + 1e-13);
            }
        }
    }
}
