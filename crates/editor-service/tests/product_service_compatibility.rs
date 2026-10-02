use editor_core::{ApertureShape, RegionRole, SemanticGeometry};
use editor_service::{
    ApplicationService, ExportParams, FileAccessPolicy, ImportGerberLayersParams, MetadataPolicy,
    OverwritePolicy,
};
use gerber_io::{S1Error, parse_s1, parse_s1_compat};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn fixture_import(source: &[u8]) -> (ApplicationService, String, String, Vec<String>) {
    let root = std::env::temp_dir().join(format!(
        "rcam-product-compat-{}-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("source.gbr");
    std::fs::write(&path, source).unwrap();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        &root,
        [root.clone()],
        [root.clone()],
    ));
    let doc = service.document_new().unwrap();
    let imported = service
        .import_gerber_layers(
            &doc.document_id,
            &doc.revision,
            ImportGerberLayersParams {
                paths: vec![path.to_str().unwrap().into()],
            },
        )
        .unwrap();
    let diagnostics = imported.layers[0].diagnostics.clone();
    (
        service,
        doc.document_id,
        root.to_str().unwrap().into(),
        diagnostics,
    )
}

#[test]
fn unsupported_classes_reach_product_compatibility() {
    let inputs: &[(&str, &[u8])] = &[
        (
            "MI",
            b"%FSLAX24Y24*%%MOMM*%%ADD10R,2X1*%%MIA1*%D10*X100000Y200000D03*M02*",
        ),
        (
            "Thermal",
            b"%FSLAX26Y26*%%MOMM*%%AMTHERM*7,0,0,1,0.5,0.1,0*%%ADD10THERM*%D10*X0Y0D03*M02*",
        ),
        (
            "diagonal sweep",
            b"%FSLAX26Y26*%%MOMM*%%ADD10R,2X2*%D10*X0Y0D02*X10000000Y10000000D01*M02*",
        ),
    ];
    for (name, source) in inputs {
        assert!(
            matches!(parse_s1(source, name), Err(S1Error::Unsupported { .. })),
            "{name}"
        );
        let (service, id, root, diagnostics) = fixture_import(source);
        assert!(
            diagnostics
                .iter()
                .any(|line| line.starts_with("兼容导入：")),
            "{name}: {diagnostics:?}"
        );
        assert_eq!(service.document_get(&id).unwrap().layer_ids.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn am5_and_am6_remain_fail_closed_in_product_service() {
    for body in ["5,1,3,0,0,1,0", "6,0,0,4,0.2,0.2,3,0.2,3,0"] {
        let source = format!(
            "%FSLAX26Y26*%%MOMM*%%AMUNSUPPORTED*{body}*%%ADD10UNSUPPORTED*%D10*X0Y0D03*M02*"
        );
        let parsed = parse_s1_compat(source.as_bytes(), "unsupported");
        assert!(
            matches!(parsed, Err(S1Error::Unsupported { .. })),
            "{body}: {parsed:?}"
        );
        let root = std::env::temp_dir().join(format!(
            "rcam-am-unsupported-{}-{}",
            std::process::id(),
            body.len()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.gbr");
        std::fs::write(&path, source).unwrap();
        let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
            &root,
            [root.clone()],
            Vec::<PathBuf>::new(),
        ));
        let doc = service.document_new().unwrap();
        let error = service
            .import_gerber_layers(
                &doc.document_id,
                &doc.revision,
                ImportGerberLayersParams {
                    paths: vec![path.to_string_lossy().into()],
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "UNSUPPORTED_FEATURE", "{body}: {error:?}");
        assert!(
            service
                .document_get(&doc.document_id)
                .unwrap()
                .layer_ids
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn non_utf8_comment_does_not_change_ascii_geometry() {
    let mut source = b"G04 old comment ".to_vec();
    source.extend_from_slice(&[0xce, 0xc4]);
    source.extend_from_slice(b"*%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X1234567Y7654321D03*M02*");
    assert!(matches!(
        parse_s1(&source, "strict"),
        Err(S1Error::InvalidUtf8)
    ));
    let expected = parse_s1_compat(&source, "expected").unwrap().document;
    let (service, id, root, diagnostics) = fixture_import(&source);
    assert!(diagnostics.iter().any(|line| line.contains("UTF-8")));
    let actual = service.project_snapshot(&id).unwrap();
    let (
        SemanticGeometry::Flash { center: actual, .. },
        SemanticGeometry::Flash {
            center: expected, ..
        },
    ) = (
        &actual.layers[0].layer.objects[0].geometry,
        &expected.layers[0].objects[0].geometry,
    )
    else {
        panic!("expected one preserved flash")
    };
    assert_eq!(actual, expected);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lossy_zero_aperture_warning_survives_project_roundtrip_and_export_gate() {
    let source = b"%FSLAX26Y26*%%MOMM*%%ADD10C,0*%D10*X0Y0D03*M02*";
    let (mut service, id, root, diagnostics) = fixture_import(source);
    assert!(diagnostics.iter().any(|line| line.contains("2 µm")));
    let project = service.project_snapshot(&id).unwrap();
    assert!(
        project.layers[0]
            .compatibility_issues
            .iter()
            .any(|line| line.contains("零直径"))
    );
    assert!(matches!(project.apertures[0].shape,
        ApertureShape::Circle { diameter_mm, .. } if (diameter_mm - 0.002).abs() < 1e-12));
    let path = PathBuf::from(&root).join("compatible.rcam");
    let revision = service.document_get(&id).unwrap().revision;
    service
        .project_save(&id, &revision, Some(path.to_str().unwrap()), false)
        .unwrap();
    let reopened = service.project_open(path.to_str().unwrap()).unwrap();
    let issues = &service
        .project_snapshot(&reopened.document_id)
        .unwrap()
        .layers[0]
        .compatibility_issues;
    assert_eq!(issues, &project.layers[0].compatibility_issues);
    let error = service
        .export_layer(
            &reopened.document_id,
            &reopened.revision,
            ExportParams {
                layer_id: reopened.layer_ids[0].clone(),
                path: PathBuf::from(&root)
                    .join("converted.gbr")
                    .to_string_lossy()
                    .into(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
                compatibility_precision_override_mm: None,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "CONFIRMATION_REQUIRED");
    assert_eq!(
        error.details["compatibility_warning"]["contains_lossy_zero_aperture_conversion"],
        true
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn compatibility_precision_requires_explicit_per_export_override() {
    let mut source = b"G04 legacy ".to_vec();
    source.push(0xff);
    source.extend_from_slice(
        b"*%FSLAX26Y26*%%MOMM*%G36*X0Y0D02*X20Y0D01*X20Y20D01*X0Y20D01*X0Y0D01*G37*M02*",
    );
    let (mut service, id, root, _) = fixture_import(&source);
    let info = service.document_get(&id).unwrap();
    let params = ExportParams {
        layer_id: info.layer_ids[0].clone(),
        path: PathBuf::from(&root)
            .join("fine.gbr")
            .to_string_lossy()
            .into(),
        overwrite: OverwritePolicy {
            mode: "deny".into(),
            expected_sha256: None,
        },
        metadata_policy: MetadataPolicy {
            mode: "drop_listed".into(),
            categories: Some(vec!["compatibility_issues".into()]),
        },
        compatibility_precision_override_mm: None,
    };
    let error = service
        .export_layer(&id, &info.revision, params.clone())
        .unwrap_err();
    assert_eq!(error.code, "CONFIRMATION_REQUIRED", "{error:?}");
    assert_eq!(error.details["reason"], "compatibility_precision_override");
    assert_eq!(error.details["project_resolution_mm"], 0.0001);
    let required = error.details["required_resolution_mm"].as_f64().unwrap();
    assert!(required < 0.0001);
    assert!(!PathBuf::from(&params.path).exists());
    let mut approved = params;
    approved.compatibility_precision_override_mm = Some(required);
    service.export_layer(&id, &info.revision, approved).unwrap();
    assert_eq!(
        service
            .document_get(&id)
            .unwrap()
            .manufacturing_precision
            .resolution_mm,
        0.0001
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn nonstandard_region_role_and_export_gate_survive_project_roundtrip() {
    let source = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/synthetic/s1a/region_bowtie.gbr"
    ))
    .unwrap();
    let parsed = parse_s1_compat(&source, "bowtie").unwrap();
    assert!(matches!(&parsed.document.layers[0].objects[0].geometry,
        SemanticGeometry::Region { contours } if contours.iter().any(|c| c.role == RegionRole::CompatibilitySolid)));
    let (mut service, id, root, _) = fixture_import(&source);
    let revision = service.document_get(&id).unwrap().revision;
    let path = PathBuf::from(&root).join("bowtie.rcam");
    service
        .project_save(&id, &revision, Some(path.to_str().unwrap()), false)
        .unwrap();
    let reopened = service.project_open(path.to_str().unwrap()).unwrap();
    let project = service.project_snapshot(&reopened.document_id).unwrap();
    assert!(matches!(&project.layers[0].layer.objects[0].geometry,
        SemanticGeometry::Region { contours } if contours.iter().any(|c| c.role == RegionRole::CompatibilitySolid)));
    let error = service
        .export_layer(
            &reopened.document_id,
            &reopened.revision,
            ExportParams {
                layer_id: reopened.layer_ids[0].clone(),
                path: PathBuf::from(&root)
                    .join("bowtie.gbr")
                    .to_string_lossy()
                    .into(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
                compatibility_precision_override_mm: None,
            },
        )
        .unwrap_err();
    assert_eq!(
        error.details["compatibility_warning"]["contains_nonstandard_compatibility_region"],
        true
    );
    assert!(
        error.details["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|category| category == "nonstandard_compatibility_region")
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Same frozen candidate list as the parser inventory. Emits a per-file,
/// machine-readable product result and fails on any parser/product divergence.
#[test]
#[ignore = "requires local RCAM_ROOT and RCAM_CANDIDATE_PATHS"]
fn product_corpus_matches_parser_policy() {
    let root = PathBuf::from(std::env::var("RCAM_ROOT").unwrap());
    let paths = std::fs::read_to_string(std::env::var("RCAM_CANDIDATE_PATHS").unwrap()).unwrap();
    println!("parser\tproduct\tbytes\tpath\terror");
    let mut mismatches = Vec::new();
    for relative in paths.lines() {
        let path = root.join(relative);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("IO\tIO\t0\t{relative}\t{error}");
                continue;
            }
        };
        let parser = match parse_s1(&bytes, relative) {
            Ok(_) => "STRICT",
            Err(error) if error.allows_compatibility_fallback() => {
                if parse_s1_compat(&bytes, relative).is_ok() {
                    "COMPAT"
                } else {
                    "REJECTED"
                }
            }
            Err(_) => "REJECTED",
        };
        let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
            &root,
            [root.clone()],
            Vec::<PathBuf>::new(),
        ));
        // This inventory uses the product's initial document.open path. Adding
        // an 803k-object layer to an existing document has a separate Undo
        // history budget, so that operation cannot stand in for this gate.
        let (product, error) = match service.open(path.to_str().unwrap()) {
            Ok(doc) => {
                let compatible =
                    service
                        .layers_list(&doc.document_id)
                        .unwrap()
                        .iter()
                        .any(|layer| {
                            layer
                                .import_diagnostics
                                .iter()
                                .any(|line| line.starts_with("兼容导入："))
                        });
                (if compatible { "COMPAT" } else { "STRICT" }, String::new())
            }
            Err(error) => ("REJECTED", format!("{}: {}", error.code, error.message)),
        };
        println!("{parser}\t{product}\t{}\t{relative}\t{error}", bytes.len());
        if parser != product {
            mismatches.push(relative.to_owned());
        }
    }
    assert!(
        mismatches.is_empty(),
        "parser/product mismatch: {mismatches:?}"
    );
}

/// Produces local-only normalized Gerbers for independent CAM comparison.
#[test]
#[ignore = "requires RCAM_CAM_OUT and local EP11BAM/art08 files"]
fn real_compatibility_cam_artifacts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = PathBuf::from(std::env::var("RCAM_CAM_OUT").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    let inputs = [
        (
            "ep11-top",
            "tests/GERBER/13/EP11BAM-A_top_0mm_202607241511.gbr",
        ),
        (
            "ep11-bottom",
            "tests/GERBER/13/EP11BAM-A_bot_0mm_202607241511.gbr",
        ),
        ("art08", "0727SMT/75/ea1hs2m01MAN_VB/art08.art"),
    ];
    for (name, relative) in inputs {
        let source = root.join(relative);
        let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
            &root,
            [root.clone(), out.clone()],
            [out.clone()],
        ));
        let doc = service.open(source.to_str().unwrap()).unwrap();
        let path = out.join(format!("{name}-normalized.gbr"));
        let mut params = ExportParams {
            layer_id: doc.layer_ids[0].clone(),
            path: path.to_string_lossy().into(),
            overwrite: OverwritePolicy {
                mode: "deny".into(),
                expected_sha256: None,
            },
            metadata_policy: MetadataPolicy {
                mode: "require_confirmation".into(),
                categories: None,
            },
            compatibility_precision_override_mm: None,
        };
        let pending = service
            .export_layer(&doc.document_id, &doc.revision, params.clone())
            .unwrap_err();
        assert_eq!(pending.code, "CONFIRMATION_REQUIRED", "{name}: {pending:?}");
        params.metadata_policy.mode = "drop_listed".into();
        params.metadata_policy.categories =
            Some(serde_json::from_value(pending.details["categories"].clone()).unwrap());
        let exported = match service.export_layer(&doc.document_id, &doc.revision, params.clone()) {
            Ok(result) => result,
            Err(error) if error.details["reason"] == "compatibility_precision_override" => {
                params.compatibility_precision_override_mm =
                    error.details["required_resolution_mm"].as_f64();
                service
                    .export_layer(&doc.document_id, &doc.revision, params)
                    .unwrap()
            }
            Err(error) => panic!("{name}: {error:?}"),
        };
        let bounds = service.document_bounds(&doc.document_id).unwrap();
        println!(
            "{name}\tsource={relative}\tsha256={}\tbytes={}\tobjects={}\tbounds={:?}\tissues={:?}",
            exported.sha256,
            exported.bytes,
            service.project_snapshot(&doc.document_id).unwrap().layers[0]
                .layer
                .objects
                .len(),
            bounds.bounds,
            service.project_snapshot(&doc.document_id).unwrap().layers[0].compatibility_issues
        );
    }
}
