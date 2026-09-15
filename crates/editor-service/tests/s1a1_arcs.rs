//! S1-A.1 independent physical assertions, separate from the round-trip comparator.
use editor_core::{MmPoint, SemanticGeometry};
use gerber_io::{parse_s1, verify_roundtrip, write_s1};
use serde_json::{Value, json};
use std::path::Path;

#[test]
fn public_arc_fixture_truth_and_reopen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../fixtures/synthetic/s1a1/manifest.json"
    ))
    .unwrap();
    let mut results = Vec::new();
    for case in manifest["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let bytes = std::fs::read(root.join(case["path"].as_str().unwrap())).unwrap();
        let parsed = parse_s1(&bytes, name);
        if case["accept"] == false {
            assert!(parsed.is_err(), "{name} must reject");
            results.push(json!({"case":name,"status":"passed","expected":"rejected"}));
            continue;
        }
        let scene = parsed.unwrap_or_else(|e| panic!("{name}: {e}"));
        let path = match &scene.document.layers[0].objects[0].geometry {
            SemanticGeometry::Arc { path, .. } => *path,
            SemanticGeometry::Region { contours } => {
                let editor_core::RegionEdge::Arc(path) = contours[0].edges[0] else {
                    panic!("Region arc expected")
                };
                path
            }
            _ => panic!("arc expected"),
        };
        assert_eq!(path.source.unwrap().resolution_mm, 1e-6);
        if name == "g74_least_deviation" {
            assert_eq!(path.center, MmPoint::new(0.1, 1.0));
        }
        if name == "g75_large_deviation" {
            assert_eq!(path.radius(), 1.0);
            assert_eq!(path.end_radius(), 2.0);
            assert_eq!(path.arc_deviation(), 1.0);
            assert_eq!(path.canonical_circle().radius(), 1.5);
        }
        if name.starts_with("g74_zero") {
            assert!(!path.full_circle);
            assert_eq!(path.sweep_radians(), Some(0.0));
        }
        if name == "g75_full" {
            assert!(path.full_circle);
        }
        if name == "g75_near_full" {
            assert!(!path.full_circle);
            assert!(path.sweep_radians().unwrap() > std::f64::consts::TAU - 0.001);
        }
        let output = write_s1(&scene.document).unwrap_or_else(|e| panic!("{name} writer: {e}"));
        verify_roundtrip(&scene.document, &output).unwrap();
        let reopened = parse_s1(&output, "reopen").unwrap();
        for p in case["coverage_mm"].as_array().unwrap() {
            let point = MmPoint::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
            for doc in [&scene.document, &reopened.document] {
                assert_eq!(
                    doc.layer_coverage_at(&doc.layers[0].id, point),
                    p[2].as_bool(),
                    "{name}: {point:?}"
                );
            }
        }
        if let Ok(dir) = std::env::var("RCAM_ARC_OUTPUT") {
            std::fs::create_dir_all(&dir).unwrap();
            let file = Path::new(&dir).join(format!("{name}.gbr"));
            assert!(!file.exists());
            std::fs::write(file, &output).unwrap();
        }
        results.push(json!({"case":name,"status":"passed","arc_deviation_mm":path.arc_deviation(),"probes":case["coverage_mm"]}));
    }
    if let Ok(out) = std::env::var("RCAM_ARC_RESULTS") {
        assert!(!Path::new(&out).exists());
        std::fs::write(
            out,
            serde_json::to_vec_pretty(&json!({"schema_version":2,"results":results})).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn source_resolution_and_declared_center_survive_units_and_normalization() {
    let bytes =
        b"%FSLAX25Y25*%%MOIN*%%ADD10C,0.01*%D10*G75*X100000Y0D02*G03X0Y100001I-100000J0D01*M02*";
    let scene = parse_s1(bytes, "inch").unwrap();
    let SemanticGeometry::Arc { path, .. } = scene.document.layers[0].objects[0].geometry else {
        panic!()
    };
    assert!((path.source.unwrap().resolution_mm - 0.000254).abs() < 1e-15);
    assert_eq!(path.center, MmPoint::new(0., 0.));
    assert!((path.arc_deviation() - 0.000254).abs() < 1e-12);
    write_s1(&scene.document).unwrap();
}

#[test]
fn region_fuzziness_and_quantization_fail_closed() {
    let fuzzy_region = b"%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*G75*G36*X1000000Y0D02*G03X0Y2000000I-1000000J0D01*G01X0Y0D01*X1000000Y0D01*G37*M02*";
    assert!(
        parse_s1(fuzzy_region, "fuzzy-region")
            .unwrap_err()
            .to_string()
            .contains("uncertainty envelope")
    );
    let mut scene = parse_s1(
        include_bytes!("../../../fixtures/synthetic/s1a1/g75_near_full.gbr"),
        "quantization",
    )
    .unwrap();
    let SemanticGeometry::Arc { ref mut path, .. } = scene.document.layers[0].objects[0].geometry
    else {
        panic!()
    };
    let mut far_path = *path;
    far_path.start.x_mm += 100_000_000.0;
    far_path.end.x_mm += 100_000_000.0;
    far_path.center.x_mm += 100_000_000.0;
    assert!(
        !far_path.is_valid(),
        "f64 uncertainty must not relax the core tolerance"
    );
    path.end.y_mm = -0.1e-6;
    assert!(
        write_s1(&scene.document).is_err(),
        "quantization must not turn near-full into full-circle"
    );
}

#[test]
fn g74_roundoff_at_large_origin_and_real_over_quarter_are_distinct() {
    for (end, accepted) in [
        ("X100000000Y100050000", true),
        ("X99999990Y100050000", false),
    ] {
        let text = format!(
            "%FSLAX36Y36*%%MOMM*%%ADD10C,0.2*%D10*G74*X100050000Y100000000D02*G03{end}I50000J0D01*M02*"
        );
        assert_eq!(
            parse_s1(text.as_bytes(), "quarter-roundoff").is_ok(),
            accepted
        );
    }
}

#[test]
fn capabilities_describe_tested_arc_boundaries() {
    use editor_service::{ApplicationService, FileAccessPolicy};
    let service = ApplicationService::with_file_access(FileAccessPolicy::new(".", [], []));
    let capabilities = service.capabilities();
    assert!(
        capabilities
            .supported_gerber_subset
            .iter()
            .any(|s| s.contains("G75 strokes with nonzero"))
    );
    assert!(
        capabilities
            .supported_gerber_subset
            .iter()
            .any(|s| s.contains("G74 strokes with least-deviation"))
    );
    assert!(
        capabilities
            .unsupported_gerber_features
            .iter()
            .any(|s| s.contains("uncertainty envelopes"))
    );
    assert!(
        !capabilities
            .supported_gerber_subset
            .iter()
            .any(|s| s == "G74/G75 circular interpolation")
    );
}

#[test]
fn isolated_fuzzy_region_uses_a_verified_circle_inside_the_annulus() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*G75*G36*X1000000Y0D02*G03X0Y2000000I-1000000J0D01*G01X-3000000Y2000000D01*Y-3000000D01*X1000000D01*Y0D01*G37*M02*";
    let scene = parse_s1(source, "isolated-fuzzy-region").unwrap();
    let output = write_s1(&scene.document).unwrap();
    let reopened = parse_s1(&output, "reopen").unwrap();
    for doc in [&scene.document, &reopened.document] {
        for (x, y, covered) in [
            (0., 0., true),
            (1., 1., true),
            (1.5, 1.5, false),
            (-3.1, 0., false),
        ] {
            assert_eq!(
                doc.layer_coverage_at(&doc.layers[0].id, MmPoint::new(x, y)),
                Some(covered)
            );
        }
    }
}
