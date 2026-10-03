use editor_core::edit::SelectionGroup;
use editor_core::hit_test::selection_geometry::{CompositeMaterial, QueryError, calculate};
use editor_core::*;
use std::f64::consts::PI;
fn document() -> SemanticDocument {
    SemanticDocument {
        id: "d".into(),
        unit: "mm".into(),
        format: SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: vec![SemanticLayer {
            id: "l".into(),
            objects: vec![],
        }],
        apertures: vec![],
        source: SourceMetadata::default(),
        block_definitions: vec![],
    }
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> SemanticGeometry {
    let p = [
        MmPoint::new(x, y),
        MmPoint::new(x + w, y),
        MmPoint::new(x + w, y + h),
        MmPoint::new(x, y + h),
    ];
    SemanticGeometry::Region {
        contours: vec![RegionContour {
            role: RegionRole::Solid,
            edges: p
                .iter()
                .zip(p.iter().cycle().skip(1))
                .take(4)
                .map(|(a, b)| RegionEdge::Line { start: *a, end: *b })
                .collect(),
        }],
    }
}
fn add(d: &mut SemanticDocument, g: SemanticGeometry, exposure: Exposure) {
    let id = format!("o{}", d.layers[0].objects.len());
    d.layers[0].objects.push(SemanticObject {
        object_id: id,
        geometry: g,
        exposure,
        origin: ObjectOrigin::Imported { command_index: 0 },
    });
}
fn groups(d: &SemanticDocument) -> Vec<SelectionGroup> {
    d.layers
        .iter()
        .filter(|l| !l.objects.is_empty())
        .map(|l| SelectionGroup {
            layer_id: l.id.clone(),
            object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
        })
        .collect()
}
fn ready(d: &SemanticDocument, a: f64, p: f64, c: MmPoint) {
    eprintln!("oracle a={a} p={p} c={c:?}");
    let before = d.clone();
    let q = calculate(d, &groups(d), 1e-6, || false).unwrap();
    let CompositeMaterial::Ready {
        area_mm2,
        perimeter_mm,
        centroid_mm,
        centroid_error_mm,
        area_error_mm2,
        perimeter_error_mm,
        ..
    } = q.material
    else {
        panic!("zero area")
    };
    assert!(
        (area_mm2 - a).abs() <= area_error_mm2,
        "area {area_mm2} expected {a} error {area_error_mm2}"
    );
    assert!(
        (perimeter_mm - p).abs() <= perimeter_error_mm,
        "perimeter {perimeter_mm} expected {p} error {perimeter_error_mm}"
    );
    assert!(
        (centroid_mm.x_mm - c.x_mm).abs() <= centroid_error_mm,
        "centroid {centroid_mm:?} expected {c:?} error {centroid_error_mm}"
    );
    assert!((centroid_mm.y_mm - c.y_mm).abs() <= centroid_error_mm);
    assert!(centroid_error_mm <= 1e-7);
    assert_eq!(&before, d);
}
#[test]
fn separated_rectangles_and_envelope_center_differ() {
    let mut d = document();
    add(&mut d, rect(0., 0., 2., 2.), Exposure::Dark);
    add(&mut d, rect(4., 0., 1., 1.), Exposure::Dark);
    ready(&d, 5., 12., MmPoint::new(1.7, 0.9));
    assert_eq!(
        calculate(&d, &groups(&d), 1e-4, || false)
            .unwrap()
            .bounding_center_mm,
        Some(MmPoint::new(2.5, 1.))
    );
}
#[test]
fn overlapping_material_union_removes_internal_edges() {
    let mut d = document();
    add(&mut d, rect(0., 0., 2., 2.), Exposure::Dark);
    add(&mut d, rect(1., 0., 2., 1.), Exposure::Dark);
    ready(&d, 5., 10., MmPoint::new(1.3, 0.9));
}
#[test]
fn ordered_clear_refill_and_unselected_exclusion() {
    let mut d = document();
    add(&mut d, rect(0., 0., 4., 2.), Exposure::Dark);
    add(&mut d, rect(0., 0., 1., 1.), Exposure::Clear);
    ready(&d, 7., 12., MmPoint::new(15.5 / 7., 7.5 / 7.));
    let selected = vec![SelectionGroup {
        layer_id: "l".into(),
        object_ids: vec!["o0".into()],
    }];
    let q = calculate(&d, &selected, 1e-6, || false).unwrap();
    assert!(matches!(
        q.material,
        CompositeMaterial::Ready { area_mm2: 8., .. }
    ));
    add(&mut d, rect(0., 0., 1., 1.), Exposure::Dark);
    ready(&d, 8., 12., MmPoint::new(2., 1.));
}
#[test]
fn cross_layer_overlap_is_counted_independently() {
    let mut d = document();
    add(&mut d, rect(0., 0., 1., 1.), Exposure::Dark);
    add(&mut d, rect(0., 0., 1., 1.), Exposure::Dark);
    add(&mut d, rect(4., 0., 1., 1.), Exposure::Dark);
    ready(&d, 2., 8., MmPoint::new(2.5, 0.5));
    let o = d.layers[0].objects.remove(1);
    d.layers.push(SemanticLayer {
        id: "l2".into(),
        objects: vec![o],
    });
    ready(&d, 3., 12., MmPoint::new(11. / 6., 0.5));
}
#[test]
fn empty_clear_and_total_erasure_are_proved_zero() {
    let mut d = document();
    add(&mut d, rect(0., 0., 2., 2.), Exposure::Clear);
    assert_eq!(
        calculate(&d, &groups(&d), 1e-6, || false).unwrap().material,
        CompositeMaterial::ZeroArea
    );
    d.layers[0].objects[0].exposure = Exposure::Dark;
    add(&mut d, rect(0., 0., 2., 2.), Exposure::Clear);
    assert_eq!(
        calculate(&d, &groups(&d), 1e-6, || false).unwrap().material,
        CompositeMaterial::ZeroArea
    );
}
fn flash(shape: ApertureShape, center: MmPoint) -> SemanticDocument {
    let mut d = document();
    d.apertures.push(ApertureDefinition {
        id: "a".into(),
        source_dcode: 10,
        shape,
    });
    add(
        &mut d,
        SemanticGeometry::Flash {
            center,
            aperture_id: "a".into(),
            transform: LocalTransform::default(),
        },
        Exposure::Dark,
    );
    d
}
#[test]
fn circle_annulus_and_polygon_holes() {
    let c = MmPoint::new(10., -7.);
    ready(
        &flash(
            ApertureShape::Circle {
                diameter_mm: 4.,
                hole_diameter_mm: Some(2.),
            },
            c,
        ),
        3. * PI,
        6. * PI,
        c,
    );
    ready(
        &flash(
            ApertureShape::Rectangle {
                width_mm: 4.,
                height_mm: 3.,
                hole_diameter_mm: Some(2.),
            },
            c,
        ),
        12. - PI,
        14. + 2. * PI,
        c,
    );
    ready(
        &flash(
            ApertureShape::Polygon {
                diameter_mm: 4.,
                vertices: 4,
                rotation_deg: 0.,
                hole_diameter_mm: Some(1.),
            },
            c,
        ),
        8. - PI / 4.,
        8. * 2_f64.sqrt() + PI,
        c,
    );
}

#[test]
fn independently_reviewed_near_tangent_perimeter_is_enclosed() {
    let mut d = flash(
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: None,
        },
        MmPoint::new(0., 0.),
    );
    d.apertures.push(ApertureDefinition {
        id: "b".into(),
        source_dcode: 11,
        shape: ApertureShape::Circle {
            diameter_mm: 0.5,
            hole_diameter_mm: None,
        },
    });
    add(
        &mut d,
        SemanticGeometry::Flash {
            center: MmPoint::new(1.249999999875, 0.),
            aperture_id: "b".into(),
            transform: LocalTransform::default(),
        },
        Exposure::Dark,
    );
    let before = d.clone();
    let CompositeMaterial::Ready {
        perimeter_mm,
        perimeter_error_mm,
        centroid_error_mm,
        ..
    } = calculate(&d, &groups(&d), 1e-4, || false).unwrap().material
    else {
        panic!("practical regression rejected")
    };
    // 120-digit independent Decimal lens truth, rounded to adjacent-f64 enclosure.
    let truth = 7.853953349702066_f64;
    assert!((perimeter_mm - truth).abs() + (truth.next_up() - truth) <= perimeter_error_mm);
    assert!(centroid_error_mm <= 1e-5);
    assert_eq!(before, d);
}

#[test]
fn macro_local_cut_uncertainty_survives_world_transform() {
    let primitives = vec![
        MacroPrimitive::Circle {
            center: MmPoint::new(0., 0.),
            diameter_mm: 2.,
            rotation_deg: 0.,
            exposure: Exposure::Dark,
        },
        MacroPrimitive::Circle {
            center: MmPoint::new(1.249999999875, 0.),
            diameter_mm: 0.5,
            rotation_deg: 0.,
            exposure: Exposure::Dark,
        },
    ];
    let mut d = flash(ApertureShape::Macro { primitives }, MmPoint::new(10., -20.));
    let SemanticGeometry::Flash { transform, .. } = &mut d.layers[0].objects[0].geometry else {
        unreachable!()
    };
    transform.rotation_deg = 37.;
    let CompositeMaterial::Ready {
        perimeter_mm,
        perimeter_error_mm,
        ..
    } = calculate(&d, &groups(&d), 1e-4, || false).unwrap().material
    else {
        panic!("practical macro regression rejected")
    };
    let truth = 7.853953349702066_f64;
    assert!((perimeter_mm - truth).abs() + (truth.next_up() - truth) <= perimeter_error_mm);
}

#[test]
fn macro_scale_rotation_mirror_and_composed_block_keep_valid_bounds() {
    let primitives = vec![
        MacroPrimitive::Circle {
            center: MmPoint::new(0., 0.),
            diameter_mm: 2.,
            rotation_deg: 0.,
            exposure: Exposure::Dark,
        },
        MacroPrimitive::Circle {
            center: MmPoint::new(1., 0.),
            diameter_mm: 2.,
            rotation_deg: 0.,
            exposure: Exposure::Dark,
        },
    ];
    let base_area = 4. * PI / 3. + 3_f64.sqrt() / 2.;
    let base_perimeter = 8. * PI / 3.;
    for scale in [0.001, 1., 10., 1000.] {
        for rotation in [0_f64, 37., 90.] {
            for mirror in [Mirror::None, Mirror::X, Mirror::Y, Mirror::Xy] {
                let mut d = flash(
                    ApertureShape::Macro {
                        primitives: primitives.clone(),
                    },
                    MmPoint::new(1000., -1000.),
                );
                let SemanticGeometry::Flash { transform, .. } =
                    &mut d.layers[0].objects[0].geometry
                else {
                    unreachable!()
                };
                *transform = LocalTransform {
                    scale,
                    rotation_deg: rotation,
                    mirror,
                };
                d.validate().unwrap();
                let CompositeMaterial::Ready {
                    area_mm2,
                    perimeter_mm,
                    centroid_mm,
                    area_error_mm2,
                    perimeter_error_mm,
                    centroid_error_mm,
                    ..
                } = calculate(&d, &groups(&d), 1e-4, || false).unwrap().material
                else {
                    panic!("ordinary transformed Macro rejected")
                };
                let sign = if matches!(mirror, Mirror::X | Mirror::Xy) {
                    -1.
                } else {
                    1.
                };
                let x = 1000. + sign * 0.5 * scale * rotation.to_radians().cos();
                let y = -1000. + sign * 0.5 * scale * rotation.to_radians().sin();
                assert!((area_mm2 - base_area * scale * scale).abs() <= area_error_mm2);
                assert!((perimeter_mm - base_perimeter * scale).abs() <= perimeter_error_mm);
                assert!(
                    (centroid_mm.x_mm - x).abs() <= centroid_error_mm
                        && (centroid_mm.y_mm - y).abs() <= centroid_error_mm
                );
                let geometry = d.layers[0].objects[0].geometry.clone();
                let block_id = editor_core::block::BlockDefinitionId("transformed-macro".into());
                d.block_definitions
                    .push(editor_core::block::BlockDefinition {
                        id: block_id.clone(),
                        name: "Macro".into(),
                        local_origin: MmPoint::new(0., 0.),
                        revision: 1,
                        objects: vec![editor_core::block::BlockObject {
                            geometry: geometry.try_into().unwrap(),
                            exposure: Exposure::Dark,
                        }],
                    });
                d.layers[0].objects[0].geometry = SemanticGeometry::BlockInstance {
                    definition_id: block_id,
                    transform: editor_core::block::BlockTransform {
                        translation: MmPoint::new(-10., 20.),
                        rotation_deg: 53.,
                        mirror: true,
                    },
                };
                d.validate().unwrap();
                let CompositeMaterial::Ready {
                    area_mm2,
                    perimeter_mm,
                    area_error_mm2,
                    perimeter_error_mm,
                    ..
                } = calculate(&d, &groups(&d), 1e-4, || false).unwrap().material
                else {
                    panic!("composed Macro rejected")
                };
                assert!((area_mm2 - base_area * scale * scale).abs() <= area_error_mm2);
                assert!((perimeter_mm - base_perimeter * scale).abs() <= perimeter_error_mm);
            }
        }
    }
    let mut d = flash(
        ApertureShape::Macro {
            primitives: vec![
                MacroPrimitive::Circle {
                    center: MmPoint::new(0., 0.),
                    diameter_mm: 2.,
                    rotation_deg: 0.,
                    exposure: Exposure::Dark,
                },
                MacroPrimitive::Circle {
                    center: MmPoint::new(1.249999999875, 0.),
                    diameter_mm: 0.5,
                    rotation_deg: 0.,
                    exposure: Exposure::Dark,
                },
            ],
        },
        MmPoint::new(0., 0.),
    );
    let SemanticGeometry::Flash { transform, .. } = &mut d.layers[0].objects[0].geometry else {
        unreachable!()
    };
    transform.scale = 1000.;
    d.validate().unwrap();
    // Scaled local conditioning exceeds unchanged centroid epsilon; the old
    // nominal Ready response was not certifiable in world millimeters.
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
}

fn connected_circle_chain() -> SemanticDocument {
    let mut d = flash(
        ApertureShape::Circle {
            diameter_mm: 1.,
            hole_diameter_mm: None,
        },
        MmPoint::new(0., 0.),
    );
    for i in 1..10000 {
        add(
            &mut d,
            SemanticGeometry::Flash {
                center: MmPoint::new(i as f64 * 0.75, 0.),
                aperture_id: "a".into(),
                transform: LocalTransform::default(),
            },
            Exposure::Dark,
        );
    }
    d
}

#[test]
fn connected_chain_exhausts_work_and_early_cancel_without_mutation() {
    let d = connected_circle_chain();
    let before = d.clone();
    let g = groups(&d);
    let mut polls = 0;
    let result = editor_core::hit_test::selection_geometry::calculate_with_deadline(
        &d,
        &g,
        1e-4,
        std::time::Instant::now() + std::time::Duration::from_secs(60),
        || {
            polls += 1;
            false
        },
    );
    assert_eq!(result, Err(QueryError::ResourceLimit));
    assert!(
        (2_000_000..2_100_000).contains(&polls),
        "actual charged work callbacks {polls}"
    );
    polls = 0;
    let result = calculate(&d, &g, 1e-4, || {
        polls += 1;
        polls == 100000
    });
    assert_eq!(result, Err(QueryError::Cancelled));
    assert_eq!(polls, 100000);
    assert_eq!(d, before);
}

#[test]
#[ignore = "release connected Circle10K work/callback/external cancellation observation; explicit gate"]
fn release_connected_chain_budget_and_external_cancel_observation() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    let d = connected_circle_chain();
    let before = d.clone();
    let g = groups(&d);
    let started = Instant::now();
    let mut last = started;
    let mut gap = Duration::ZERO;
    let mut polls = 0;
    let result = calculate(&d, &g, 1e-4, || {
        let now = Instant::now();
        gap = gap.max(now - last);
        last = now;
        polls += 1;
        false
    });
    assert_eq!(result, Err(QueryError::ResourceLimit));
    assert!((2_000_000..2_100_000).contains(&polls));
    assert!(gap <= Duration::from_millis(500));
    println!(
        "{{\"chain10k\":\"budget\",\"polls\":{polls},\"elapsed_ms\":{},\"max_unchecked_ms\":{},\"outcome\":\"ResourceLimit\"}}",
        started.elapsed().as_secs_f64() * 1000.,
        gap.as_secs_f64() * 1000.
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let (send, recv) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        recv.recv().unwrap();
        let requested = Instant::now();
        flag.store(true, Ordering::Release);
        requested
    });
    polls = 0;
    let started = Instant::now();
    let mut last = started;
    let mut gap = Duration::ZERO;
    let result = calculate(&d, &g, 1e-4, || {
        let now = Instant::now();
        gap = gap.max(now - last);
        last = now;
        polls += 1;
        if polls == 120000 {
            send.send(()).unwrap();
        }
        cancelled.load(Ordering::Acquire)
    });
    let stopped = Instant::now();
    let requested = thread.join().unwrap();
    assert_eq!(result, Err(QueryError::Cancelled));
    let latency = stopped.duration_since(requested);
    assert!(latency <= Duration::from_secs(2));
    assert_eq!(d, before);
    println!(
        "{{\"chain10k\":\"external_cancel\",\"polls\":{polls},\"cancel_request_to_return_ms\":{},\"max_unchecked_ms\":{},\"elapsed_ms\":{},\"outcome\":\"Cancelled\"}}",
        latency.as_secs_f64() * 1000.,
        gap.as_secs_f64() * 1000.,
        stopped.duration_since(started).as_secs_f64() * 1000.
    );
}

#[test]
fn translated_thin_annulus_is_uncertain_not_false_zero() {
    let d = flash(
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: Some(1.99999999998),
        },
        MmPoint::new(1e6, 0.),
    );
    let before = d.clone();
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
    assert_eq!(before, d);
    let d = flash(
        ApertureShape::Macro {
            primitives: vec![
                MacroPrimitive::Circle {
                    center: MmPoint::new(1e6, 0.),
                    diameter_mm: 2.,
                    rotation_deg: 0.,
                    exposure: Exposure::Dark,
                },
                MacroPrimitive::Circle {
                    center: MmPoint::new(1e6, 0.),
                    diameter_mm: 1.99999999998,
                    rotation_deg: 0.,
                    exposure: Exposure::Clear,
                },
            ],
        },
        MmPoint::new(0., 0.),
    );
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
    let mut d = flash(
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: None,
        },
        MmPoint::new(1e6, 0.),
    );
    let geometry = d.layers[0].objects[0].geometry.clone();
    add(&mut d, geometry, Exposure::Clear);
    assert_eq!(
        calculate(&d, &groups(&d), 1e-4, || false).unwrap().material,
        CompositeMaterial::ZeroArea
    );
}

#[test]
fn shallow_region_arc_with_distant_center_is_explicitly_uncertain() {
    let mut d = document();
    let start = MmPoint::new(-1., 0.);
    let end = MmPoint::new(1., 0.);
    add(
        &mut d,
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Arc(ArcGeometry {
                        start,
                        end,
                        center: MmPoint::new(0., -1e6),
                        direction: ArcDirection::Clockwise,
                        full_circle: false,
                        source: None,
                    }),
                    RegionEdge::Line {
                        start: end,
                        end: start,
                    },
                ],
            }],
        },
        Exposure::Dark,
    );
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
}

#[test]
fn source_aperture_alias_and_collapse_cannot_claim_zero_material() {
    let rectangle = |width_mm| ApertureShape::Rectangle {
        width_mm,
        height_mm: 2.,
        hole_diameter_mm: None,
    };
    let mut d = flash(rectangle(2.), MmPoint::new(1e6, 0.));
    d.apertures.push(ApertureDefinition {
        id: "b".into(),
        source_dcode: 11,
        shape: rectangle(1.99999999998),
    });
    add(
        &mut d,
        SemanticGeometry::Flash {
            center: MmPoint::new(1e6, 0.),
            aperture_id: "b".into(),
            transform: LocalTransform::default(),
        },
        Exposure::Clear,
    );
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
    let d = flash(rectangle(1e-11), MmPoint::new(1e6, 0.));
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
    let d = flash(
        ApertureShape::Macro {
            primitives: vec![MacroPrimitive::CenterLine {
                center: MmPoint::new(1e6, 0.),
                width_mm: 1e-11,
                height_mm: 2.,
                rotation_deg: 0.,
                exposure: Exposure::Dark,
            }],
        },
        MmPoint::new(0., 0.),
    );
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
}
#[test]
fn obround_and_capsule_exact_arc_moments() {
    let c = MmPoint::new(0., 0.);
    ready(
        &flash(
            ApertureShape::Obround {
                width_mm: 4.,
                height_mm: 2.,
                hole_diameter_mm: None,
            },
            c,
        ),
        4. + PI,
        4. + 2. * PI,
        c,
    );
    let mut d = document();
    add(
        &mut d,
        SemanticGeometry::Line {
            start: MmPoint::new(0., 0.),
            end: MmPoint::new(2., 0.),
            width_mm: 2.,
        },
        Exposure::Dark,
    );
    ready(&d, 4. + PI, 4. + 2. * PI, MmPoint::new(1., 0.));
}
#[test]
fn macro_hole_is_local_transparent_over_background() {
    let mut d = flash(
        ApertureShape::Macro {
            primitives: vec![
                MacroPrimitive::Circle {
                    center: MmPoint::new(0., 0.),
                    diameter_mm: 4.,
                    rotation_deg: 0.,
                    exposure: Exposure::Dark,
                },
                MacroPrimitive::Circle {
                    center: MmPoint::new(0., 0.),
                    diameter_mm: 2.,
                    rotation_deg: 0.,
                    exposure: Exposure::Clear,
                },
            ],
        },
        MmPoint::new(0., 0.),
    );
    ready(&d, 3. * PI, 6. * PI, MmPoint::new(0., 0.));
    let mut outer = d.layers[0].objects.remove(0);
    outer.object_id = "macro".into();
    add(&mut d, rect(-0.5, -0.5, 1., 1.), Exposure::Dark);
    d.layers[0].objects.push(outer);
    ready(&d, 3. * PI + 1., 6. * PI + 4., MmPoint::new(0., 0.));
}
#[test]
fn curved_region_semicircle_centroid() {
    let mut d = document();
    add(
        &mut d,
        SemanticGeometry::Region {
            contours: vec![RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Arc(ArcGeometry {
                        start: MmPoint::new(2., 0.),
                        end: MmPoint::new(-2., 0.),
                        center: MmPoint::new(0., 0.),
                        direction: ArcDirection::CounterClockwise,
                        full_circle: false,
                        source: None,
                    }),
                    RegionEdge::Line {
                        start: MmPoint::new(-2., 0.),
                        end: MmPoint::new(2., 0.),
                    },
                ],
            }],
        },
        Exposure::Dark,
    );
    ready(&d, 2. * PI, 2. * PI + 4., MmPoint::new(0., 8. / (3. * PI)));
}
#[test]
fn arc_tube_full_and_half() {
    let mut d = document();
    let path = ArcGeometry {
        start: MmPoint::new(2., 0.),
        end: MmPoint::new(-2., 0.),
        center: MmPoint::new(0., 0.),
        direction: ArcDirection::CounterClockwise,
        full_circle: false,
        source: None,
    };
    add(
        &mut d,
        SemanticGeometry::Arc { path, width_mm: 1. },
        Exposure::Dark,
    );
    // Analytic half-annulus plus two nonoverlapping half disks at y<0.
    let a = 2. * PI + PI / 4.;
    let my = (2. / 3.) * (2.5_f64.powi(3) - 1.5_f64.powi(3)) - 4. * 0.5_f64.powi(3) / 3.;
    ready(&d, a, 5. * PI, MmPoint::new(0., my / a));
    d.layers[0].objects[0].geometry = SemanticGeometry::Arc {
        path: ArcGeometry {
            end: path.start,
            full_circle: true,
            ..path
        },
        width_mm: 1.,
    };
    ready(&d, 4. * PI, 8. * PI, MmPoint::new(0., 0.));
}
#[test]
fn invalid_ids_cancellation_numeric_limit_no_partial_query() {
    let mut d = document();
    add(&mut d, rect(0., 0., 2., 2.), Exposure::Dark);
    let g = groups(&d);
    assert_eq!(calculate(&d, &g, 1e-4, || true), Err(QueryError::Cancelled));
    let mut polls = 0;
    assert_eq!(
        calculate(&d, &g, 1e-4, || {
            polls += 1;
            polls > 20
        }),
        Err(QueryError::Cancelled)
    );
    let mut duplicate = g.clone();
    duplicate[0].object_ids.push("o0".into());
    assert!(matches!(
        calculate(&d, &duplicate, 1e-4, || false),
        Err(QueryError::InvalidArgument(_))
    ));
    d.layers[0].objects[0].geometry = rect(1e8, 1e8, 2., 2.);
    assert!(matches!(
        calculate(&d, &g, 1e-6, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
}
#[test]
fn block_child_clear_acts_on_previous_layer_material_and_outer_exposure_is_not_substitute() {
    use editor_core::block::*;
    let mut d = document();
    add(&mut d, rect(0., 0., 4., 2.), Exposure::Dark);
    d.block_definitions.push(BlockDefinition {
        id: BlockDefinitionId("b".into()),
        name: "clear insert".into(),
        revision: 1,
        local_origin: MmPoint::new(0., 0.),
        objects: vec![BlockObject {
            geometry: rect(0., 0., 1., 1.).try_into().unwrap(),
            exposure: Exposure::Clear,
        }],
    });
    add(
        &mut d,
        SemanticGeometry::BlockInstance {
            definition_id: BlockDefinitionId("b".into()),
            transform: BlockTransform::IDENTITY,
        },
        Exposure::Dark,
    );
    ready(&d, 7., 12., MmPoint::new(15.5 / 7., 7.5 / 7.));
    d.layers[0].objects[1].exposure = Exposure::Clear;
    ready(&d, 7., 12., MmPoint::new(15.5 / 7., 7.5 / 7.));
}
#[test]
fn multi_contour_union_and_cutin_holes_are_not_svg_sibling_subtraction() {
    let mut d = document();
    let SemanticGeometry::Region { mut contours } = rect(0., 0., 2., 2.) else {
        unreachable!()
    };
    let SemanticGeometry::Region { contours: other } = rect(1., 0., 2., 1.) else {
        unreachable!()
    };
    contours.extend(other);
    add(
        &mut d,
        SemanticGeometry::Region { contours },
        Exposure::Dark,
    );
    ready(&d, 5., 10., MmPoint::new(1.3, 0.9));
    let points = [
        (0., 0.),
        (4., 0.),
        (4., 4.),
        (0., 4.),
        (0., 0.),
        (1., 1.),
        (1., 3.),
        (3., 3.),
        (3., 1.),
        (1., 1.),
        (0., 0.),
    ]
    .map(|(x, y)| MmPoint::new(x, y));
    d.layers[0].objects[0].geometry = SemanticGeometry::Region {
        contours: vec![RegionContour {
            role: RegionRole::Solid,
            edges: points
                .windows(2)
                .map(|p| RegionEdge::Line {
                    start: p[0],
                    end: p[1],
                })
                .collect(),
        }],
    };
    ready(&d, 12., 24., MmPoint::new(2., 2.));
}
#[test]
fn circle_overlap_lens_has_no_double_count_and_true_arc_perimeter() {
    let mut d = flash(
        ApertureShape::Circle {
            diameter_mm: 2.,
            hole_diameter_mm: None,
        },
        MmPoint::new(-0.5, 0.),
    );
    add(
        &mut d,
        SemanticGeometry::Flash {
            center: MmPoint::new(0.5, 0.),
            aperture_id: "a".into(),
            transform: LocalTransform::default(),
        },
        Exposure::Dark,
    );
    let lens = 2. * PI / 3. - 3_f64.sqrt() / 2.;
    ready(&d, 2. * PI - lens, 8. * PI / 3., MmPoint::new(0., 0.));
}
#[test]
fn rectangular_sweep_macro_polygon_and_rigid_invariance() {
    let mut d = document();
    add(
        &mut d,
        SemanticGeometry::RectangularSweep {
            start: MmPoint::new(0., 0.),
            end: MmPoint::new(2., 1.),
            width_mm: 2.,
            height_mm: 2.,
        },
        Exposure::Dark,
    );
    ready(&d, 10., 8. + 2. * 5_f64.sqrt(), MmPoint::new(1., 0.5));
    let mut d = flash(
        ApertureShape::Macro {
            primitives: vec![
                MacroPrimitive::CenterLine {
                    center: MmPoint::new(1., 0.5),
                    width_mm: 2.,
                    height_mm: 1.,
                    rotation_deg: 0.,
                    exposure: Exposure::Dark,
                },
                MacroPrimitive::Outline {
                    points: vec![
                        MmPoint::new(1., 0.),
                        MmPoint::new(3., 0.),
                        MmPoint::new(3., 1.),
                        MmPoint::new(1., 1.),
                    ],
                    rotation_deg: 0.,
                    exposure: Exposure::Dark,
                },
            ],
        },
        MmPoint::new(0., 0.),
    );
    ready(&d, 3., 8., MmPoint::new(1.5, 0.5));
    editor_core::edit::EditHistory::default()
        .rotate_objects(&mut d, "l", &["o0".into()], 37., MmPoint::new(0., 0.))
        .unwrap();
    let a = 37_f64.to_radians();
    ready(
        &d,
        3.,
        8.,
        MmPoint::new(1.5 * a.cos() - 0.5 * a.sin(), 1.5 * a.sin() + 0.5 * a.cos()),
    );
}
#[test]
fn near_zero_sliver_fails_precision_without_bounding_center_fallback() {
    let mut d = document();
    add(&mut d, rect(0., 0., 1., 1.), Exposure::Dark);
    add(&mut d, rect(0., 0., 1. - 1e-12, 1.), Exposure::Clear);
    assert!(matches!(
        calculate(&d, &groups(&d), 1e-6, || false),
        Err(QueryError::PrecisionUncertain(_))
    ));
}
#[test]
fn zero_width_line_is_zero_material_not_centerline_perimeter() {
    let mut d = document();
    add(
        &mut d,
        SemanticGeometry::Line {
            start: MmPoint::new(0., 0.),
            end: MmPoint::new(2., 0.),
            width_mm: 0.,
        },
        Exposure::Dark,
    );
    assert_eq!(
        calculate(&d, &groups(&d), 1e-4, || false).unwrap().material,
        CompositeMaterial::ZeroArea
    );
}
#[test]
#[ignore = "release performance and cancellation observation; explicit gate"]
fn release_thirty_cold_queries_and_long_segment_cancellation() {
    use std::time::{Duration, Instant};
    let mut d = flash(
        ApertureShape::Circle {
            diameter_mm: 0.4,
            hole_diameter_mm: None,
        },
        MmPoint::new(0., 0.),
    );
    d.layers[0].objects.clear();
    for i in 0..10_000 {
        add(
            &mut d,
            SemanticGeometry::Flash {
                center: MmPoint::new((i % 100) as f64 * 2., (i / 100) as f64 * 2.),
                aperture_id: "a".into(),
                transform: LocalTransform::default(),
            },
            Exposure::Dark,
        );
    }
    let g = groups(&d);
    let mut cold = Vec::new();
    let mut bbox = Vec::new();
    let mut longest_gap = Duration::ZERO;
    for i in 0..30 {
        let start = Instant::now();
        let bounds = geometries_bounds(
            d.layers[0].objects.iter().map(|o| &o.geometry),
            &d.apertures,
        )
        .unwrap()
        .unwrap();
        bbox.push(start.elapsed().as_secs_f64() * 1000.);
        assert!((bounds.center().x_mm - 99.).abs() < 1e-12);
        assert!((bounds.center().y_mm - 99.).abs() < 1e-12);
        let start = Instant::now();
        let mut last = start;
        let q = calculate(&d, &g, 1e-4, || {
            let now = Instant::now();
            longest_gap = longest_gap.max(now.duration_since(last));
            last = now;
            false
        })
        .unwrap();
        cold.push(start.elapsed().as_secs_f64() * 1000.);
        let CompositeMaterial::Ready {
            area_mm2,
            centroid_mm,
            centroid_error_mm,
            ..
        } = q.material
        else {
            panic!("zero")
        };
        assert!((area_mm2 - 400. * PI).abs() < 1e-7);
        assert!((centroid_mm.x_mm - 99.).abs() <= centroid_error_mm);
        assert!((centroid_mm.y_mm - 99.).abs() <= centroid_error_mm);
        println!(
            "{{\"sample\":{i},\"cold_ms\":{},\"bbox_ms\":{},\"work\":{}}}",
            cold[i], bbox[i], q.work
        );
    }
    cold.sort_by(f64::total_cmp);
    bbox.sort_by(f64::total_cmp);
    println!(
        "{{\"cold_p95_ms\":{},\"bbox_p95_ms\":{},\"max_unchecked_ms\":{}}}",
        cold[28],
        bbox[28],
        longest_gap.as_secs_f64() * 1000.
    );
    assert!(cold[28] <= 200.);
    assert!(bbox[28] <= 100.);
    let mut dense = document();
    for _ in 0..2000 {
        add(&mut dense, rect(0., 0., 10., 10.), Exposure::Dark);
    }
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = cancel.clone();
    let start = Instant::now();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2));
        flag.store(true, std::sync::atomic::Ordering::Release);
        Instant::now()
    });
    let result = calculate(&dense, &groups(&dense), 1e-4, || {
        cancel.load(std::sync::atomic::Ordering::Acquire)
    });
    let stopped = Instant::now();
    let requested = thread.join().unwrap();
    assert_eq!(result, Err(QueryError::Cancelled));
    let latency = stopped.duration_since(requested).as_secs_f64() * 1000.;
    println!(
        "{{\"cancel_to_termination_ms\":{latency},\"dense_total_ms\":{}}}",
        start.elapsed().as_secs_f64() * 1000.
    );
    assert!(latency <= 2000.);
}

#[test]
#[ignore = "release source preparation and cancellation observations; explicit gate"]
fn release_large_source_preparation_and_cancellation() {
    use std::time::{Duration, Instant};
    let vertices: Vec<_> = (0..10000)
        .map(|i| {
            let a = 2. * PI * i as f64 / 10000.;
            MmPoint::new(10. * a.cos(), 10. * a.sin())
        })
        .collect();
    let region = SemanticGeometry::Region {
        contours: vec![RegionContour {
            role: RegionRole::Solid,
            edges: vertices
                .iter()
                .zip(vertices.iter().cycle().skip(1))
                .take(vertices.len())
                .map(|(a, b)| RegionEdge::Line { start: *a, end: *b })
                .collect(),
        }],
    };
    let mut region_doc = document();
    add(&mut region_doc, region, Exposure::Dark);
    let macro_doc = flash(
        ApertureShape::Macro {
            primitives: vec![MacroPrimitive::Outline {
                points: vertices,
                rotation_deg: 37.,
                exposure: Exposure::Dark,
            }],
        },
        MmPoint::new(0., 0.),
    );
    let mut block_doc = document();
    let block_id = editor_core::block::BlockDefinitionId("prep-block".into());
    block_doc
        .block_definitions
        .push(editor_core::block::BlockDefinition {
            id: block_id.clone(),
            name: "Preparation".into(),
            local_origin: MmPoint::new(0., 0.),
            revision: 1,
            objects: (0..10000)
                .map(|_| editor_core::block::BlockObject {
                    geometry: editor_core::block::BlockObjectGeometry::Line {
                        start: MmPoint::new(0., 0.),
                        end: MmPoint::new(0., 1.),
                        width_mm: 0.2,
                    },
                    exposure: Exposure::Dark,
                })
                .collect(),
        });
    add(
        &mut block_doc,
        SemanticGeometry::BlockInstance {
            definition_id: block_id,
            transform: editor_core::block::BlockTransform {
                translation: MmPoint::new(0., 0.),
                rotation_deg: 0.,
                mirror: false,
            },
        },
        Exposure::Dark,
    );
    for (name, d) in [
        ("REGION10K", region_doc),
        ("MACRO10K", macro_doc),
        ("BLOCK10K", block_doc),
    ] {
        let started = Instant::now();
        let mut last = started;
        let mut gap = Duration::ZERO;
        let result = calculate(&d, &groups(&d), 1e-4, || {
            let now = Instant::now();
            gap = gap.max(now.duration_since(last));
            last = now;
            false
        });
        println!(
            "source={name} elapsed_ms={} longest_unchecked_ms={} outcome={result:?}",
            started.elapsed().as_secs_f64() * 1000.,
            gap.as_secs_f64() * 1000.
        );
        assert!(
            gap <= Duration::from_millis(500),
            "{name}: uncontrolled preparation segment {gap:?}"
        );
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = cancel.clone();
        let request = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2));
            flag.store(true, std::sync::atomic::Ordering::Release);
            Instant::now()
        });
        let result = calculate(&d, &groups(&d), 1e-4, || {
            cancel.load(std::sync::atomic::Ordering::Acquire)
        });
        let stopped = Instant::now();
        let requested = request.join().unwrap();
        assert_eq!(result, Err(QueryError::Cancelled), "{name}");
        let latency = stopped.saturating_duration_since(requested);
        assert!(latency <= Duration::from_secs(2));
        println!(
            "source={name} cancel_to_termination_ms={} outcome=Cancelled",
            latency.as_secs_f64() * 1000.
        );
    }
}

#[test]
fn expired_deadline_and_source_admission_reject_before_partial_result() {
    let mut d = document();
    add(&mut d, rect(0., 0., 1., 1.), Exposure::Dark);
    let before = d.clone();
    assert_eq!(
        editor_core::hit_test::selection_geometry::calculate_with_deadline(
            &d,
            &groups(&d),
            1e-4,
            std::time::Instant::now(),
            || false
        ),
        Err(QueryError::ResourceLimit)
    );
    assert_eq!(d, before);
    let edge = RegionEdge::Line {
        start: MmPoint::new(0., 0.),
        end: MmPoint::new(1., 0.),
    };
    d.layers[0].objects[0].geometry = SemanticGeometry::Region {
        contours: vec![RegionContour {
            role: RegionRole::Solid,
            edges: vec![edge; 140000],
        }],
    };
    let before = d.clone();
    assert_eq!(
        calculate(&d, &groups(&d), 1e-4, || false),
        Err(QueryError::ResourceLimit)
    );
    assert_eq!(d, before);
}
#[test]
fn straight_round_cap_sweep_has_material_moments_not_centerline_area() {
    let mut d = document();
    add(
        &mut d,
        SemanticGeometry::Line {
            start: MmPoint::new(2., -3.),
            end: MmPoint::new(5., 1.),
            width_mm: 2.,
        },
        Exposure::Dark,
    );
    ready(&d, 10. + PI, 10. + 2. * PI, MmPoint::new(3.5, -1.));
    for angle in [13_f64, 37., 90., 180., 270.] {
        let mut rotated = d.clone();
        editor_core::edit::EditHistory::default()
            .rotate_objects(
                &mut rotated,
                "l",
                &["o0".into()],
                angle,
                MmPoint::new(0., 0.),
            )
            .unwrap();
        let a = angle.to_radians();
        ready(
            &rotated,
            10. + PI,
            10. + 2. * PI,
            MmPoint::new(3.5 * a.cos() + a.sin(), 3.5 * a.sin() - a.cos()),
        );
    }
}
