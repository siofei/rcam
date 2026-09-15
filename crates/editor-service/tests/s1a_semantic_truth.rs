//! Independent manufacturing assertions; no window, mesh or raster truth.
use editor_core::MmPoint;
use gerber_io::{
    S1Budget, S1Error, export_s1_new_path, parse_s1, parse_s1_with_budget, verify_roundtrip,
    write_s1,
};
use serde_json::{Value, json};
use std::f64::consts::FRAC_1_SQRT_2;
use std::path::Path;

#[test]
fn unbound_template_parameter_does_not_hide_invalid_constant_primitive() {
    let source=b"%FSLAX26Y26*%\n%MOMM*%\n%AMBAD*1,1,$99,0,0*4,1,4,0,0,2,2,0,2,2,0,0,0,0*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    assert!(parse_s1(source, "unbound-with-bowtie").is_err());
    let source=b"%FSLAX26Y26*%\n%MOMM*%\n%AMBAD*$9=$99*$1=0*$2=1/$1*1,1,$9,0,0*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    assert!(parse_s1(source, "unbound-with-known-zero-divisor").is_err());
    let source=b"%FSLAX26Y26*%\n%MOMM*%\n%AMBAD*$9=$99+1/0*1,1,$9,0,0*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    assert!(parse_s1(source, "unbound-with-constant-zero-subexpression").is_err());
}

#[test]
fn omitted_absolute_axis_preserves_exact_mm_value_after_inch_offset() {
    let source=b"%FSLAX26Y26*%\n%MOIN*%\n%IOA0B0.2*%\n%ADD10R,0.1X0.1*%\nD10*\nX0Y100000D02*\nX100000D01*\nM02*\n";
    let scene = parse_s1(source, "held-axis").unwrap();
    let editor_core::SemanticGeometry::RectangularSweep { start, end, .. } =
        scene.document.layers[0].objects[0].geometry
    else {
        panic!("expected axis-aligned rectangle sweep")
    };
    assert_eq!(start.y_mm.to_bits(), end.y_mm.to_bits());
    assert!((start.y_mm - 7.62).abs() < 1e-12);
    write_s1(&scene.document).unwrap();
}

#[test]
fn ordinary_comment_may_span_physical_lines() {
    let source=b"G04 public\nmultiline comment*\n%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    let scene = parse_s1(source, "multiline-comment").unwrap();
    assert_eq!(scene.document.object_count(), 1);
}

#[test]
fn legacy_empty_separators_packed_headers_and_multiline_macro_are_supported() {
    for source in [
        "*\n*\n%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX1000000Y2000000D03*\nM02*\n",
        "%FSLAX26Y26*%\n%MOMM*%\n%IR0*IPPOS*OFA0B0*MIA0B0*SFA1B1*%\n%ADD10C,1*%\nD10*\nX1000000Y2000000D03*\nM02*\n",
        "%FSLAX26Y26*%\n%MOMM*%\n%AMMULTI*\n1,1,1,0,0*\n%\n%ADD10MULTI*%\nD10*\nX1000000Y2000000D03*\nM02*\n",
        "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1*%\n%IOA0.5B1*%\nD10*\nX500000Y1000000D03*\nM02*\n",
    ] {
        let scene = parse_s1(source.as_bytes(), "legacy-lexical").unwrap();
        assert_eq!(scene.document.object_count(), 1);
        assert_eq!(
            scene
                .document
                .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(1., 2.)),
            Some(true)
        );
        write_s1(&scene.document).unwrap();
    }
    let invalid = b"%FSLAX26Y26*%%MOMM*%%IR0*IPNEG*%%ADD10C,1*%D10*X0Y0D03*M02*";
    assert!(parse_s1(invalid, "packed-unsupported").is_err());
}

#[test]
fn writer_declares_multi_quadrant_for_region_arcs() {
    let scene = parse_s1(
        include_bytes!("../../../fixtures/synthetic/s1a/region_arc.gbr"),
        "region-quadrant",
    )
    .unwrap();
    let output = String::from_utf8(write_s1(&scene.document).unwrap()).unwrap();
    let first_arc = output.find("G03").unwrap();
    assert!(
        output[..first_arc].contains("G75*"),
        "signed region I/J requires explicit G75 for independent readers"
    );
    let missing =
        b"%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*X1000000Y0D02*G03X0Y1000000I-1000000J0D01*M02*";
    assert!(
        parse_s1(missing, "undeclared-multi").is_err(),
        "must not silently assume G75"
    );
}

#[test]
fn macro_expression_recursion_is_bounded_including_unary_operators() {
    for expression in [
        format!("{}1", "-".repeat(258)),
        format!("{}1{}", "(".repeat(257), ")".repeat(257)),
    ] {
        let source = format!(
            "%FSLAX26Y26*%\n%MOMM*%\n%AMDEPTH*$1={expression}*1,1,$1,0,0*%\n%ADD10DEPTH*%\nD10*\nX0Y0D03*\nM02*\n"
        );
        let error = parse_s1(source.as_bytes(), "expression-depth").unwrap_err();
        assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
    }
}

#[test]
fn first_incremental_missing_axis_is_zero() {
    let source = b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG91*\nX1000000D03*\nM02*\n";
    let scene = parse_s1(source, "first-increment").unwrap();
    assert_eq!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(1., 0.)),
        Some(true)
    );
}

#[test]
fn trailing_incremental_full_circle_reencodes_offset_only_record() {
    let source=b"%FSTAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG75*\nG91*\nX01Y0D02*\nG03I-01J0D01*\nM02*\n";
    let scene = parse_s1(source, "offset-only").unwrap();
    assert_eq!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(-1., 0.)),
        Some(true)
    );
    write_s1(&scene.document).unwrap();
}

#[test]
fn incremental_trailing_arc_offsets_are_not_reinterpreted_or_accumulated() {
    let source=b"%FSTAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG75*\nG91*\nX01Y0D02*\nG03X-01Y01I-01J0D01*\nM02*\n";
    let scene = parse_s1(source, "incremental-arc").unwrap();
    assert_eq!(
        scene.document.layer_coverage_at(
            &scene.document.layers[0].id,
            MmPoint::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2)
        ),
        Some(true)
    );
    let bytes = write_s1(&scene.document).unwrap();
    verify_roundtrip(&scene.document, &bytes).unwrap();
}

#[test]
fn incremental_io_is_applied_once() {
    let source=b"%FSLIX26Y26*%\n%MOMM*%\n%IOA2B3*%\n%ADD10C,0.2*%\nD10*\nG91*\nX1000000Y2000000D03*\nM02*\n";
    let scene = parse_s1(source, "incremental-io").unwrap();
    assert_eq!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(3., 5.)),
        Some(true)
    );
    assert_eq!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(5., 8.)),
        Some(false)
    );
}

#[test]
fn g74_ambiguous_centers_and_full_circle_are_rejected() {
    // Both (+0.1,+1) and (-0.1,+1) satisfy radius tolerance and a small
    // counterclockwise sweep. Choosing the first candidate is unsafe.
    for operation in ["G03X3Y0I100000J1000000D01*", "G03X0Y0I1000000J0D01*"] {
        let source = format!(
            "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG74*\nX0Y0D02*\n{operation}\nM02*\n"
        );
        let error = parse_s1(source.as_bytes(), "g74-reject").unwrap_err();
        assert!(matches!(error, S1Error::Semantic { .. }), "{error:?}");
    }
}

#[test]
fn source_metadata_is_preserved_as_diagnostics() {
    let source = include_bytes!("../../../fixtures/synthetic/s0c/legacy_identity.gbr");
    let scene = parse_s1(source, "metadata").unwrap();
    assert_eq!(scene.metadata.image_name.as_deref(), Some("PUBLIC"));
    assert_eq!(scene.metadata.layer_name.as_deref(), Some("PUBLIC"));
    assert_eq!(scene.document.source, scene.metadata);
    assert_ne!(scene.document.layers[0].id, "PUBLIC");
    let metadata = serde_json::to_value(&scene.metadata).unwrap();
    assert_eq!(metadata["coordinate_format"], "%FSLAX26Y26*%");
    assert_eq!(metadata["unit_declarations"], json!(["%MOMM*%", "G71*"]));
    let incremental = parse_s1(
        include_bytes!("../../../fixtures/synthetic/s0c/fs_dix34y34.gbr"),
        "incremental-metadata",
    )
    .unwrap();
    let metadata = serde_json::to_value(&incremental.metadata).unwrap();
    assert_eq!(metadata["coordinate_format"], "%FSDIX34Y34*%");
    assert_eq!(metadata["coordinate_modes"], json!(["G91*"]));
}

#[test]
fn parameterized_macro_uses_real_ad_arguments() {
    let source=b"%FSLAX26Y26*%\n%MOMM*%\n%AMPARAM*1,1,$3,0,0*%\n%ADD10PARAM,0X0X2*%\nD10*\nX0Y0D03*\nM02*\n";
    let scene = parse_s1(source, "arguments").unwrap();
    assert_eq!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(0.75, 0.)),
        Some(true)
    );
    let written = write_s1(&scene.document).unwrap();
    let reopened = parse_s1(&written, "again").unwrap();
    assert_eq!(
        reopened
            .document
            .layer_coverage_at(&reopened.document.layers[0].id, MmPoint::new(0.75, 0.)),
        Some(true)
    );
}

#[test]
fn writer_preserves_transform_precision_in_physical_space() {
    let source = b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,100X1*%\n%LS1.0000004*%\nD10*\nX0Y0D03*\nM02*\n";
    let scene = parse_s1(source, "precision").unwrap();
    let point = MmPoint::new(50.00001, 0.);
    assert_eq!(
        scene
            .document
            .layer_coverage_at(&scene.document.layers[0].id, point),
        Some(true)
    );
    let written = write_s1(&scene.document).unwrap();
    let reopened = parse_s1(&written, "reopened").unwrap();
    assert_eq!(
        reopened
            .document
            .layer_coverage_at(&reopened.document.layers[0].id, point),
        Some(true),
        "dimensionless rounding must not amplify beyond physical output tolerance"
    );
}

#[test]
fn all_frozen_formats_decode_digits_and_signs() {
    let mut failures = Vec::new();
    for (integer, decimal) in [(4, 5), (4, 4), (3, 4), (2, 6), (3, 5), (2, 5), (4, 3)] {
        for omission in ['L', 'T', 'D'] {
            let width = (integer + decimal) as usize;
            let full = format!("{:0width$}", 10_u64.pow(decimal));
            let digits = match omission {
                'L' => full.trim_start_matches('0').to_string(),
                'T' => full.trim_end_matches('0').to_string(),
                _ => full.clone(),
            };
            let source = format!(
                "%FS{omission}AX{integer}{decimal}Y{integer}{decimal}*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nX{digits}Y-{digits}D03*\nM02*\n"
            );
            match parse_s1(source.as_bytes(), "format") {
                Ok(scene) => {
                    if scene
                        .document
                        .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(1., -1.))
                        != Some(true)
                    {
                        failures.push(format!("{omission}{integer}{decimal}: wrong coordinates"));
                    }
                }
                Err(error) => failures.push(format!("{omission}{integer}{decimal}: {error}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn resource_limits_are_checked_at_each_input_expansion_boundary() {
    let simple = b"%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X0Y0D03*M02*";
    for budget in [
        S1Budget {
            max_source_bytes: 1,
            ..S1Budget::default()
        },
        S1Budget {
            max_commands: 2,
            ..S1Budget::default()
        },
        S1Budget {
            max_objects: 0,
            ..S1Budget::default()
        },
    ] {
        let error = parse_s1_with_budget(simple, "limited", budget).unwrap_err();
        assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
    }
    let am = include_bytes!("../../../fixtures/synthetic/s0c/am_1.gbr");
    let error = parse_s1_with_budget(
        am,
        "limited",
        S1Budget {
            max_am_expansions: 0,
            ..S1Budget::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
    let region = include_bytes!("../../../fixtures/synthetic/s0c/region.gbr");
    let error = parse_s1_with_budget(
        region,
        "limited",
        S1Budget {
            max_region_edges: 1,
            ..S1Budget::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
}

#[test]
fn writer_and_validation_copy_have_separate_resource_limits() {
    let scene = parse_s1(
        include_bytes!("../../../fixtures/synthetic/s0c/fs_lax43y43.gbr"),
        "writer-budget",
    )
    .unwrap();
    for budget in [
        S1Budget {
            max_writer_bytes: 1,
            ..S1Budget::default()
        },
        S1Budget {
            max_validation_bytes: 1,
            ..S1Budget::default()
        },
        S1Budget {
            max_objects: 0,
            ..S1Budget::default()
        },
    ] {
        let error = gerber_io::write_s1_with_budget(&scene.document, budget).unwrap_err();
        assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
    }
}

#[test]
fn macro_expansion_budget_is_global_across_apertures() {
    let source=b"%FSLAX26Y26*%\n%MOMM*%\n%AMONE*1,1,1,0,0*%\n%ADD10ONE*%\n%ADD11ONE*%\nD10*\nX0Y0D03*\nD11*\nX2000000Y0D03*\nM02*\n";
    let error = parse_s1_with_budget(
        source,
        "global-am-budget",
        S1Budget {
            max_am_expansions: 1,
            ..S1Budget::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
    let outline = include_bytes!("../../../fixtures/synthetic/s0c/am_4.gbr");
    let budget = S1Budget {
        max_am_expansions: 3,
        ..S1Budget::default()
    };
    let error = parse_s1_with_budget(outline, "outline-points-budget", budget).unwrap_err();
    assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
    let scene = parse_s1(outline, "outline-writer-budget").unwrap();
    let error = gerber_io::write_s1_with_budget(&scene.document, budget).unwrap_err();
    assert!(matches!(error, S1Error::ResourceLimit { .. }), "{error:?}");
}

#[test]
fn writer_comparison_does_not_relax_to_old_input_resolution() {
    let scene = parse_s1(
        include_bytes!("../../../fixtures/synthetic/s0c/fs_lax43y43.gbr"),
        "rounding",
    )
    .unwrap();
    let bytes = write_s1(&scene.document).unwrap();
    let original = String::from_utf8(bytes).unwrap();
    let changed = original.replacen("X1000000Y1000000D03*", "X1000001Y1000000D03*", 1);
    assert_ne!(changed, original, "expected normalized physical coordinate");
    assert!(
        verify_roundtrip(&scene.document, changed.as_bytes()).is_err(),
        "1e-6 mm displacement exceeds output q/2 even with FS4.3 source"
    );
}

#[test]
fn writer_failure_preserves_preexisting_temporary_and_target_files() {
    let scene = parse_s1(
        include_bytes!("../../../fixtures/synthetic/s0c/fs_lax43y43.gbr"),
        "safety",
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("rcam-s1a-writer-safety-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("new.gbr");
    let unrelated = dir.join(format!(".new.gbr.rcam-{}.tmp", std::process::id()));
    std::fs::write(&unrelated, b"another operation owns this temporary file").unwrap();
    let _outcome = export_s1_new_path(&scene.document, &target);
    assert_eq!(
        std::fs::read(&unrelated).unwrap(),
        b"another operation owns this temporary file"
    );
    let existing = dir.join("existing.gbr");
    std::fs::write(&existing, b"existing target sentinel").unwrap();
    assert!(export_s1_new_path(&scene.document, &existing).is_err());
    assert_eq!(
        std::fs::read(&existing).unwrap(),
        b"existing target sentinel"
    );
    let mut invalid = scene.document.clone();
    invalid.layers[0].objects[0].object_id.clear();
    let bad = dir.join("invalid.gbr");
    assert!(export_s1_new_path(&invalid, &bad).is_err());
    assert!(!bad.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn independent_source_geometry_and_rejection_truth() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../fixtures/synthetic/s1a/manifest.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for case in manifest["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let bytes = std::fs::read(root.join(case["path"].as_str().unwrap())).unwrap();
        match parse_s1(&bytes, name) {
            Ok(scene) => {
                if !case["accept"].as_bool().unwrap() {
                    failures.push(format!("{name}: accepted invalid input"));
                    continue;
                }
                scene.document.validate().unwrap();
                for probe in case["coverage_mm"].as_array().unwrap() {
                    let point =
                        MmPoint::new(probe[0].as_f64().unwrap(), probe[1].as_f64().unwrap());
                    let expected = probe[2].as_bool().unwrap();
                    let actual = scene
                        .document
                        .layer_coverage_at(&scene.document.layers[0].id, point);
                    if actual != Some(expected) {
                        failures.push(format!(
                            "{name}: {point:?} expected {expected}, actual {actual:?}"
                        ));
                    }
                }
                match write_s1(&scene.document).and_then(|bytes| parse_s1(&bytes, name)) {
                    Ok(reopened) => {
                        for probe in case["coverage_mm"].as_array().unwrap() {
                            let point = MmPoint::new(
                                probe[0].as_f64().unwrap(),
                                probe[1].as_f64().unwrap(),
                            );
                            let expected = probe[2].as_bool().unwrap();
                            let actual = reopened
                                .document
                                .layer_coverage_at(&reopened.document.layers[0].id, point);
                            if actual != Some(expected) {
                                failures.push(format!("{name}: writer changed {point:?}, expected {expected}, actual {actual:?}"));
                            }
                        }
                    }
                    Err(error) => {
                        failures.push(format!("{name}: normalized roundtrip failed {error}"))
                    }
                }
            }
            Err(error) if case["accept"] == true => failures.push(format!("{name}: {error}")),
            Err(_) => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn original_eighteen_scope_inputs_have_actual_semantics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../fixtures/synthetic/s0c/manifest.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for case in manifest["fixtures"].as_array().unwrap() {
        let path = case["path"].as_str().unwrap();
        if let Err(error) = parse_s1(&std::fs::read(root.join(path)).unwrap(), path) {
            failures.push(format!("{path}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn original_apertures_arcs_and_rectangular_sweeps_have_known_geometry() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s0c");
    type CoverageCase = (&'static str, &'static [(f64, f64, bool)]);
    let cases: &[CoverageCase] = &[
        ("am_1.gbr", &[(0.9, 0., true), (1.1, 0., false)]),
        ("am_4.gbr", &[(1., 0.5, true), (3., 0.5, false)]),
        (
            "am_21_variables.gbr",
            &[(0., 0., false), (0.8, 0., true), (1.1, 0., false)],
        ),
        (
            "g74.gbr",
            &[
                (FRAC_1_SQRT_2, FRAC_1_SQRT_2, true),
                (-FRAC_1_SQRT_2, FRAC_1_SQRT_2, false),
            ],
        ),
        (
            "g75.gbr",
            &[
                (FRAC_1_SQRT_2, FRAC_1_SQRT_2, true),
                (-FRAC_1_SQRT_2, FRAC_1_SQRT_2, false),
            ],
        ),
        ("region.gbr", &[(1., 0.5, true), (3., 0.5, false)]),
        (
            "rectangular_draw.gbr",
            &[(3.95, 2.45, true), (-0.95, -0.45, true), (4.05, 2., false)],
        ),
        ("io_nonzero.gbr", &[(3., 5., true), (1., 2., false)]),
        ("ic_ascii.gbr", &[(1., 2., true), (2., 1., false)]),
        ("sr_identity.gbr", &[(1., 2., true), (2., 1., false)]),
    ];
    let mut failures = Vec::new();
    for (name, probes) in cases {
        let scene = match parse_s1(&std::fs::read(root.join(name)).unwrap(), name) {
            Ok(scene) => scene,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        for &(x, y, expected) in *probes {
            let actual = scene
                .document
                .layer_coverage_at(&scene.document.layers[0].id, MmPoint::new(x, y));
            if actual != Some(expected) {
                failures.push(format!(
                    "{name}: ({x},{y}) expected {expected}, actual {actual:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "requires locally authorized frozen CORE10 manifest; explicit run only"]
fn frozen_core10_semantic_scan() {
    let manifest_path =
        std::env::var("RCAM_CORE10_MANIFEST").expect("explicit authorized manifest");
    let out = std::env::var("RCAM_CORE10_RESULTS").expect("explicit new evidence path");
    assert!(
        !Path::new(&out).exists(),
        "never overwrite earlier evidence"
    );
    let manifest: Value = serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
    let root = Path::new(manifest["root"].as_str().unwrap());
    let mut results = Vec::new();
    for sample in manifest["samples"].as_array().unwrap() {
        let Some(id) = sample["core_id"].as_str() else {
            continue;
        };
        let path = root.join(sample["path"].as_str().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let start = std::time::Instant::now();
        let result = match parse_s1(&bytes, id) {
            Ok(scene) => {
                json!({"core_id":id,"semantic_status":"passed","objects":scene.document.object_count(),"validation":scene.document.validate().unwrap()})
            }
            Err(error) => {
                json!({"core_id":id,"semantic_status":"failed","diagnostic":error.to_string()})
            }
        };
        assert_eq!(bytes, std::fs::read(&path).unwrap(), "source changed");
        let mut result = result;
        result["elapsed_ms"] = json!(start.elapsed().as_millis());
        result["source_sha256"] = sample["sha256"].clone();
        result["source_bytes"] = json!(bytes.len());
        println!("{result}");
        results.push(result);
    }
    assert_eq!(results.len(), 10, "frozen set must contain exactly ten");
    std::fs::write(out,serde_json::to_vec_pretty(&json!({"schema_version":2,"kind":"semantic_scan_not_edit_roundtrip","results":results})).unwrap()).unwrap();
    assert!(
        results.iter().all(|r| r["semantic_status"] == "passed"),
        "one or more frozen samples lacks S1-A semantics"
    );
}
