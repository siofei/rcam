use super::*;

fn shapes() -> Vec<Shape> {
    let edge = RegionEdge::Line {
        start: MmPoint::new(0., 0.),
        end: MmPoint::new(1., 0.),
    };
    (0..2)
        .map(|_| Shape {
            geometry: SemanticGeometry::Flash {
                center: MmPoint::new(0., 0.),
                aperture_id: "a".into(),
                transform: LocalTransform::default(),
            },
            exposure: Exposure::Dark,
            edges: vec![edge.clone()],
            bounds: BoundsMm {
                min_x_mm: 0.,
                max_x_mm: 1.,
                min_y_mm: 0.,
                max_y_mm: 1.,
            },
            endpoint_errors: vec![(0., 0.)],
            boundary_error_mm: 0.,
            circle_sources: vec![None],
        })
        .collect()
}

#[test]
fn alias_skips_charge_budget_before_circle_and_non_flash_branches() {
    let circle = ApertureShape::Circle {
        diameter_mm: 1.,
        hole_diameter_mm: None,
    };
    let apertures = HashMap::from([("a", &circle)]);
    for non_flash in [false, true] {
        let mut shapes = shapes();
        if non_flash {
            for s in &mut shapes {
                s.geometry = SemanticGeometry::Line {
                    start: MmPoint::new(0., 0.),
                    end: MmPoint::new(1., 0.),
                    width_mm: 1.,
                };
            }
        }
        let mut cancelled = || false;
        let mut w = Work {
            count: MAX_WORK,
            deadline: Instant::now() + Duration::from_secs(2),
            cancelled: &mut cancelled,
        };
        assert_eq!(
            check_source_aliases(&shapes, &[0, 1], &apertures, &mut w),
            Err(QueryError::ResourceLimit)
        );
        assert_eq!(w.count, MAX_WORK + 1);
    }
}

#[test]
fn alias_skips_check_cancellation_before_type_or_identity_work() {
    let shapes = shapes();
    // Missing apertures would panic if lookup occurred before the common check.
    let mut cancelled = || true;
    let mut w = Work {
        count: 0,
        deadline: Instant::now() + Duration::from_secs(2),
        cancelled: &mut cancelled,
    };
    assert_eq!(
        check_source_aliases(&shapes, &[0, 1], &HashMap::new(), &mut w),
        Err(QueryError::Cancelled)
    );
    assert_eq!(w.count, 0);
}

#[test]
fn edge_comparisons_charge_only_actual_steps_and_stop_at_budget() {
    let line = |x| RegionEdge::Line {
        start: MmPoint::new(0., 0.),
        end: MmPoint::new(x, 0.),
    };
    let a = [line(1.), line(2.)];
    let b = [line(-1.), line(2.)];
    let mut cancelled = || false;
    let mut w = Work {
        count: 0,
        deadline: Instant::now() + Duration::from_secs(2),
        cancelled: &mut cancelled,
    };
    assert!(!same_edges(&a, &b, &mut w).unwrap());
    assert_eq!(w.count, 2);
    w.count = MAX_WORK - 2;
    assert_eq!(same_edges(&a, &a, &mut w), Err(QueryError::ResourceLimit));
    assert_eq!(w.count, MAX_WORK + 1);
}
