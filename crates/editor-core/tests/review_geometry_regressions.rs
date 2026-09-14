use editor_core::{CircleAperture, Geometry, MmPoint};

#[test]
fn sub_tolerance_hole_is_never_silently_filled() {
    let aperture = CircleAperture::new(1.0, Some(1e-7)).unwrap();
    assert!(!aperture.covers(MmPoint::new(0.0, 0.0), MmPoint::new(0.0, 0.0)));
    assert!(!aperture.covers(MmPoint::new(0.0, 2e-8), MmPoint::new(0.0, 0.0)));
}

#[test]
fn short_line_endpoint_and_zero_length_follow_capsule_geometry() {
    let line = Geometry::Line {
        start: MmPoint::new(0.0, 0.0),
        end: MmPoint::new(0.0001, 0.0),
        width_mm: 0.00002,
    };
    assert!(line.covers(MmPoint::new(0.0001, 0.0)));
    assert!(!line.covers(MmPoint::new(0.0001, 0.0001)));
    let point = Geometry::Line {
        start: MmPoint::new(0.0, 0.0),
        end: MmPoint::new(0.0, 0.0),
        width_mm: 0.2,
    };
    assert!(point.covers(MmPoint::new(0.05, 0.0)));
    assert!(!point.covers(MmPoint::new(0.2, 0.0)));
}

#[test]
fn unrepresentable_f64_radius_is_explicitly_rejected() {
    assert!(CircleAperture::new(f64::from_bits(1), None).is_err());
    assert!(CircleAperture::new(1.0, Some(f64::from_bits(1))).is_err());
}
