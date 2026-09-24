use editor_core::{
    ApertureShape, ArcDirection, Exposure, MacroPrimitive, MmPoint, SemanticGeometry,
};
use gerber_io::{parse_s1, parse_s1_compat, write_s1};

#[test]
fn leading_zero_format_integer_overflow_is_explicitly_widened() {
    let source = b"%FSLAX24Y24*%%MOMM*%%ADD10C,1*%D10*X1234567Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("FS 整数位"))
    );
    assert_eq!(scene.document.format.integer, 3);
    assert!(matches!(scene.document.layers[0].objects[0].geometry,
        SemanticGeometry::Flash { center: MmPoint { x_mm, y_mm: 0.0 }, .. }
            if (x_mm - 123.4567).abs() < 1e-9));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn legacy_mi_reflects_coordinates_and_arc_direction_but_not_aperture() {
    let source = b"%FSLAX24Y24*%%MOMM*%%ADD10R,2X1*%%ADD11C,1*%%MIA1*%D10*X100000Y200000D03*D11*G75*X100000Y0D02*G03X0Y100000I-100000J0D01*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("MI 镜像"))
    );
    assert!(matches!(
        scene.document.apertures[0].shape,
        ApertureShape::Rectangle {
            width_mm: 2.0,
            height_mm: 1.0,
            ..
        }
    ));
    assert!(matches!(
        scene.document.layers[0].objects[0].geometry,
        SemanticGeometry::Flash {
            center: MmPoint {
                x_mm: -10.0,
                y_mm: 20.0
            },
            ..
        }
    ));
    assert!(matches!(scene.document.layers[0].objects[1].geometry,
        SemanticGeometry::Arc { path, .. }
            if path.start == MmPoint::new(-10.0, 0.0)
            && path.end == MmPoint::new(0.0, 10.0)
            && path.center == MmPoint::new(0.0, 0.0)
            && path.direction == ArcDirection::Clockwise));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn macro_vector_line_retains_flat_end_rectangle() {
    let source = b"%FSLAX26Y26*%%MOMM*%%AMMACROV*20,1,1,0,0,4,0,0*%%ADD10MACROV*%D10*X0Y0D03*M02*";
    let scene = parse_s1_compat(source, "vector-line").unwrap();
    assert_eq!(scene.document.object_count(), 1);
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn zero_diameter_aperture_is_marked_and_exportable() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10C,0*%D10*X0Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("零直径"))
    );
    assert!(matches!(scene.document.apertures[0].shape,
        ApertureShape::Circle { diameter_mm, .. } if (diameter_mm - 0.000002).abs() < 1e-12));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn legacy_non_utf8_comment_preserves_ascii_geometry() {
    let mut source = b"G04 path ".to_vec();
    source.extend_from_slice(&[0xce, 0xc4]);
    source.extend_from_slice(b"*%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X0Y0D03*M02*");
    assert!(parse_s1(&source, "strict").is_err());
    let scene = parse_s1_compat(&source, "compat").unwrap();
    assert_eq!(scene.document.object_count(), 1);
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("UTF-8"))
    );
}

#[test]
fn legacy_macro_22_uses_lower_left_origin() {
    let source = b"%FSLAX26Y26*%%MOMM*%%AMMACRO22*22,1,2,1,1,2,0*%%ADD10MACRO22*%D10*X0Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("primitive 22"))
    );
    assert!(matches!(&scene.document.apertures[0].shape,
    ApertureShape::Macro { primitives }
    if matches!(primitives[0], MacroPrimitive::CenterLine {
        width_mm: 2.0, height_mm: 1.0, center: MmPoint { x_mm: 2.0, y_mm: 2.5 }, ..
    })));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn unclosed_region_gets_explicit_repair_and_warning() {
    let source = b"%FSLAX26Y26*%%MOMM*%G36*X0Y0D02*X1000000Y0D01*X1000000Y1000000D01*X0Y1000000D01*X0Y1D01*G37*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("调整终点"))
    );
    assert!(matches!(&scene.document.layers[0].objects[0].geometry,
        SemanticGeometry::Region { contours } if contours[0].edges.len() == 4));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn region_gap_larger_than_source_precision_gets_a_line() {
    let source = b"%FSLAX26Y26*%%MOMM*%G36*X0Y0D02*X1000000Y0D01*X1000000Y1000000D01*X0Y1000000D01*X0Y10D01*G37*M02*";
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("插入闭合线段"))
    );
    assert!(matches!(&scene.document.layers[0].objects[0].geometry,
        SemanticGeometry::Region { contours } if contours[0].edges.len() == 5));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn empty_region_warns_without_losing_following_image_data() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10C,1*%G36*G37*D10*X0Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("空 Region"))
    );
    assert_eq!(scene.document.object_count(), 1);
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn redundant_pre_image_fs_with_same_decimal_keeps_first_format() {
    let source = b"%FSLAX35Y35*%%MOMM*%%ADD10C,1*%%FSX25Y25*%D10*X12345678Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("重复 FS"))
    );
    assert_eq!(scene.document.format.integer, 3);
    assert_eq!(scene.document.format.decimal, 5);
    assert_eq!(scene.document.object_count(), 1);
}

#[test]
fn trailing_dos_eof_marker_is_reported_and_ignored() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X0Y0D03*M02*\r\n\x1a";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("DOS EOF"))
    );
    assert_eq!(scene.document.object_count(), 1);
}

#[test]
fn macro_outline_near_duplicate_point_is_repaired_with_warning() {
    let source = b"%FSLAX26Y26*%%MOMM*%%AMNEAR*4,1,5,0,0,1,0,1,1,0,1,0,0.000001,0,0,0*%%ADD10NEAR*%D10*X0Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("重复顶点"))
    );
    assert_eq!(scene.document.object_count(), 1);
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn empty_aperture_table_placeholder_is_ignored_with_warning() {
    let source = b"%FSLAX26Y26*%%MOMM*%%AD*%%ADD10C,1*%D10*X0Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("空 AD"))
    );
    assert_eq!(scene.document.object_count(), 1);
}

#[test]
fn diagonal_rectangle_sweep_becomes_exact_convex_region() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10R,2X2*%D10*X0Y0D02*X10000000Y10000000D01*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("斜向矩形"))
    );
    assert!(matches!(&scene.document.layers[0].objects[0].geometry,
        SemanticGeometry::Region { contours } if contours[0].edges.len() == 6));
    assert!(write_s1(&scene.document).is_ok());
}

#[test]
fn thermal_macro_uses_local_dark_clear_geometry() {
    let source = b"%FSLAX26Y26*%%MOMM*%%AMTHERM*7,0,0,1,0.5,0.1,0*%%ADD10THERM*%D10*X0Y0D03*M02*";
    assert!(parse_s1(source, "strict").is_err());
    let scene = parse_s1_compat(source, "compat").unwrap();
    assert!(
        scene
            .metadata
            .compatibility_issues
            .iter()
            .any(|x| x.contains("Thermal"))
    );
    assert!(matches!(&scene.document.apertures[0].shape,
        ApertureShape::Macro { primitives } if primitives.len() == 4
        && matches!(primitives[0], MacroPrimitive::Circle { exposure: Exposure::Dark, diameter_mm: 1.0, .. })
        && matches!(primitives[1], MacroPrimitive::Circle { exposure: Exposure::Clear, diameter_mm: 0.5, .. })
        && matches!(primitives[2], MacroPrimitive::CenterLine { exposure: Exposure::Clear, width_mm: 1.0, height_mm: 0.1, .. })
        && matches!(primitives[3], MacroPrimitive::CenterLine { exposure: Exposure::Clear, width_mm: 0.1, height_mm: 1.0, .. })));
    assert!(write_s1(&scene.document).is_ok());
}
