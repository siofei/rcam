//! Analytic manufacturing probes: expected coverage is calculated from the
//! primitive definitions, not from RCam export/reimport or renderer meshes.
use editor_core::{ApertureShape, Exposure, MacroPrimitive, MmPoint, SemanticDocument};
use gerber_io::{parse_s1, parse_s1_compat};

fn scene(macro_body: &str, args: &str) -> SemanticDocument {
    let source =
        format!("%FSLAX26Y26*%%MOMM*%%AMTEST*{macro_body}*%%ADD10TEST{args}*%D10*X0Y0D03*M02*");
    parse_s1_compat(source.as_bytes(), "truth")
        .unwrap()
        .document
}

fn covered(document: &SemanticDocument, x: f64, y: f64) -> bool {
    document
        .layer_coverage_at(&document.layers[0].id, MmPoint::new(x, y))
        .unwrap()
}

fn rotate(x: f64, y: f64, degrees: f64) -> (f64, f64) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (cos * x - sin * y, sin * x + cos * y)
}

#[test]
fn am20_flat_ended_rectangle_matches_analytic_coverage() {
    for ((x0, y0), (x1, y1), width, rotation) in [
        ((1., 2.), (4., 2.), 1., 0.),
        ((-2., 1.), (-2., 5.), 0.8, 0.),
        ((1., -2.), (4., 1.), 0.6, 37.),
    ] {
        let body = format!("20,1,{width},{x0},{y0},{x1},{y1},{rotation}");
        let document = scene(&body, "");
        let ApertureShape::Macro { primitives } = &document.apertures[0].shape else {
            panic!()
        };
        let MacroPrimitive::Outline {
            exposure: Exposure::Dark,
            points,
            ..
        } = &primitives[0]
        else {
            panic!()
        };
        let dx: f64 = x1 - x0;
        let dy: f64 = y1 - y0;
        let length = dx.hypot(dy);
        let nx = -dy / length;
        let ny = dx / length;
        let expected = [
            (x0 + nx * width / 2., y0 + ny * width / 2.),
            (x1 + nx * width / 2., y1 + ny * width / 2.),
            (x1 - nx * width / 2., y1 - ny * width / 2.),
            (x0 - nx * width / 2., y0 - ny * width / 2.),
        ];
        for (actual, (x, y)) in points.iter().zip(expected) {
            assert!((actual.x_mm - x).abs() < 1e-9 && (actual.y_mm - y).abs() < 1e-9);
        }
        for (along, across, expected) in [
            (0.5, 0., true),
            (0.5, width * 0.4, true),
            (0.5, width * 0.6, false),
            (-0.05, 0., false),
            (1.05, 0., false),
        ] {
            let x = x0 + along * dx + across * nx;
            let y = y0 + along * dy + across * ny;
            let (x, y) = rotate(x, y, rotation);
            assert_eq!(
                covered(&document, x, y),
                expected,
                "AM20 {body} at ({x},{y})"
            );
        }
    }
    let clear = scene("1,1,10,0,0*20,0,1,1,0,3,0,0", "");
    assert!(!covered(&clear, 2., 0.));
    assert!(covered(&clear, 0., 2.));
}

#[test]
fn am22_lower_left_expression_and_rotation_match_center_rectangle() {
    let document = scene("22,1,$1,$2,-3,-2,30", ",2X1");
    let ApertureShape::Macro { primitives } = &document.apertures[0].shape else {
        panic!()
    };
    assert!(matches!(
        &primitives[0],
        MacroPrimitive::CenterLine {
            exposure: Exposure::Dark,
            width_mm: 2.,
            height_mm: 1.,
            center: MmPoint {
                x_mm: -2.,
                y_mm: -1.5
            },
            rotation_deg: 30.,
        }
    ));
    for (x, y, expected) in [
        (-2., -1.5, true),
        (-2.8, -1.5, true),
        (-3.2, -1.5, false),
        (-2., -0.9, false),
    ] {
        let (x, y) = rotate(x, y, 30.);
        assert_eq!(covered(&document, x, y), expected, "AM22 at ({x},{y})");
    }
}

#[test]
fn thermal7_dark_outer_minus_inner_and_rotated_gaps() {
    let document = scene("7,1,-2,4,2,0.5,45", "");
    let ApertureShape::Macro { primitives } = &document.apertures[0].shape else {
        panic!()
    };
    assert_eq!(primitives.len(), 4);
    assert!(matches!(
        primitives[0],
        MacroPrimitive::Circle {
            exposure: Exposure::Dark,
            diameter_mm: 4.,
            ..
        }
    ));
    assert!(matches!(
        primitives[1],
        MacroPrimitive::Circle {
            exposure: Exposure::Clear,
            diameter_mm: 2.,
            ..
        }
    ));
    assert!(matches!(
        primitives[2],
        MacroPrimitive::CenterLine {
            exposure: Exposure::Clear,
            ..
        }
    ));
    assert!(matches!(
        primitives[3],
        MacroPrimitive::CenterLine {
            exposure: Exposure::Clear,
            ..
        }
    ));
    for (dx, dy, expected) in [
        (0., 0., false),
        (1.2, 0., false),
        (0., 1.8, false),
        (1.2, 1.2, true),
        (1.8, 1.8, false),
    ] {
        let (x, y) = rotate(1. + dx, -2. + dy, 45.);
        assert_eq!(covered(&document, x, y), expected, "Thermal7 at ({x},{y})");
    }
}

#[test]
fn diagonal_rectangle_sweep_matches_minkowski_interval_oracle() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10R,2X2*%D10*X0Y0D02*X10000000Y10000000D01*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let document = parse_s1_compat(source, "compat").unwrap().document;
    for xi in -3..25 {
        for yi in -3..25 {
            let x = xi as f64 * 0.47 + 0.13;
            let y = yi as f64 * 0.47 + 0.21;
            let expected = 0_f64.max(x - 1.).max(y - 1.) <= 10_f64.min(x + 1.).min(y + 1.);
            assert_eq!(covered(&document, x, y), expected, "sweep at ({x},{y})");
        }
    }
}

#[test]
fn repaired_and_explicitly_closed_region_match_square_oracle() {
    for end in ["X0Y1D01*", "X0Y0D01*"] {
        let source = format!(
            "%FSLAX26Y26*%%MOMM*%G36*X0Y0D02*X1000000Y0D01*X1000000Y1000000D01*X0Y1000000D01*{end}G37*M02*"
        );
        let document = parse_s1_compat(source.as_bytes(), "region")
            .unwrap()
            .document;
        for (x, y, expected) in [
            (0.5, 0.5, true),
            (-0.1, 0.5, false),
            (1.1, 0.5, false),
            (0.5, 1.1, false),
        ] {
            assert_eq!(
                covered(&document, x, y),
                expected,
                "region {end} at ({x},{y})"
            );
        }
    }
}

#[test]
fn g74_quantized_over_quadrant_has_expected_arc_coverage() {
    let source =
        b"%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*G74*X1000000Y0D02*G03X-1Y1000000I1000000J0D01*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|line| line.contains("G75"))
    );
    let document = scene.document;
    assert!(covered(
        &document,
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2
    ));
    assert!(!covered(
        &document,
        -std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2
    ));
    assert!(!covered(&document, 0., 0.));
}

#[test]
fn mi_mirrors_flash_position_without_mirroring_local_aperture() {
    let source = b"%FSLAX24Y24*%%MOMM*%%ADD10R,2X1*%%MIA1*%D10*X100000Y200000D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let document = parse_s1_compat(source, "compat").unwrap().document;
    for (x, y, expected) in [
        (-10., 20., true),
        (-10.8, 20.4, true),
        (-10., 20.6, false),
        (10., 20., false),
    ] {
        assert_eq!(covered(&document, x, y), expected, "MI at ({x},{y})");
    }
}
