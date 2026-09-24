use editor_core::{SemanticGeometry, units::DisplayUnit};
use editor_service::*;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn policy_is_separate_atomic_and_normalizes_only_export_snapshot() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s1a/ordered_local_hole.gbr");
    let dir = std::env::temp_dir().join(format!("rcam-units-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = ApplicationService::new();
    s.grant_file_access(&source, false).unwrap();
    s.grant_file_access(&dir, true).unwrap();
    let info = s.open(source.to_str().unwrap()).unwrap();
    let id = &info.document_id;
    assert_eq!(info.manufacturing_precision.resolution_mm, 0.0001);
    assert!(!info.dirty && !info.export_policy_dirty);
    let snapshot = s.render_snapshot(id).unwrap();
    let baseline = serde_json::to_value(&snapshot).unwrap();
    for _ in 0..100 {
        for u in DisplayUnit::ALL {
            let _ = u.format_length(25.4, info.manufacturing_precision.resolution_mm);
            let _ = u.format_area(25.4 * 25.4, info.manufacturing_precision.resolution_mm);
            assert_eq!(s.document_get(id).unwrap(), info);
            assert_eq!(
                serde_json::to_value(s.render_snapshot(id).unwrap()).unwrap(),
                baseline
            );
        }
    }
    for bad in [0., -1., f64::NAN, 0.0000001] {
        assert!(
            s.set_manufacturing_precision(id, "0", ManufacturingPrecision { resolution_mm: bad })
                .is_err()
        );
        assert_eq!(s.document_get(id).unwrap(), info);
    }
    let reply = s.execute_json(&json!({"api_version":1,"request_id":"precision","op":"document.set_manufacturing_precision","document_id":id,"expected_revision":"0","params":{"resolution_mm":0.0005}}).to_string());
    assert_eq!(reply["status"], "completed", "{reply}");
    let updated = s.document_get(id).unwrap();
    assert_eq!(updated.revision, "1");
    assert!(!updated.dirty && updated.export_policy_dirty);
    assert_eq!((updated.undo_entries, updated.redo_entries), (0, 0));
    assert!(s.close(id, "1", false).is_err());
    assert!(
        s.set_manufacturing_precision(id, "0", ManufacturingPrecision::default())
            .is_err()
    );
    assert_eq!(
        s.set_manufacturing_precision(id, "1", updated.manufacturing_precision)
            .unwrap(),
        updated
    );
    let snapshot = s.render_snapshot(id).unwrap();
    assert_eq!(
        snapshot.layers,
        serde_json::from_value::<RenderSnapshot>(baseline)
            .unwrap()
            .layers
    );
    let layer = &info.layer_ids[0];
    // The normalizer's independent numeric oracle and idempotence are tested in IO.
    let target = dir.join("policy.gbr");
    if target.exists() {
        std::fs::remove_file(&target).unwrap();
    }
    let exported = s.export_layer(
        id,
        "1",
        ExportParams {
            layer_id: layer.clone(),
            path: target.to_string_lossy().into(),
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
    );
    assert!(exported.is_ok(), "{exported:?}");
    assert!(!s.document_get(id).unwrap().export_policy_dirty);
    s.grant_file_access(&target, false).unwrap();
    let reopened = s.open(target.to_str().unwrap()).unwrap();
    assert_eq!(
        s.render_snapshot(&reopened.document_id).unwrap().layers[0]
            .objects
            .len(),
        3
    );
    assert!(matches!(
        snapshot.layers[0].objects[0].geometry,
        SemanticGeometry::Flash { .. }
    ));
}

#[test]
fn subgrid_arc_export_rejects_without_touching_files_or_state_and_finer_policy_works() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s1a1");
    let out = std::env::temp_dir().join(format!("rcam-units-arcs-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    for name in ["g75_near_full", "g74_least_deviation"] {
        let source = root.join(format!("{name}.gbr"));
        let bytes = std::fs::read(&source).unwrap();
        let mut s = ApplicationService::new();
        s.grant_file_access(&source, false).unwrap();
        s.grant_file_access(&out, true).unwrap();
        let info = s.open(source.to_str().unwrap()).unwrap();
        let target = out.join(format!("{name}.gbr"));
        let params = ExportParams {
            layer_id: info.layer_ids[0].clone(),
            path: target.to_string_lossy().into(),
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
        assert!(
            s.export_layer(&info.document_id, "0", params.clone())
                .is_err()
        );
        assert!(!target.exists());
        assert_eq!(s.document_get(&info.document_id).unwrap(), info);
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        s.set_manufacturing_precision(
            &info.document_id,
            "0",
            ManufacturingPrecision {
                resolution_mm: 0.000001,
            },
        )
        .unwrap();
        s.export_layer(&info.document_id, "1", params).unwrap();
        let reopened = gerber_io::parse_s1(&std::fs::read(target).unwrap(), "arc").unwrap();
        assert!(matches!(
            reopened.document.layers[0].objects[0].geometry,
            SemanticGeometry::Arc { .. }
        ));
    }
}

#[test]
#[ignore = "native system font and release manufacturing policy matrix"]
fn text_policy_matrix() {
    let out = PathBuf::from(std::env::var_os("RCAM_UNITS_MATRIX").expect("new evidence directory"));
    std::fs::create_dir_all(&out).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s1a/ordered_local_hole.gbr");
    let font_path = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
    let mut rows = Vec::new();
    for (pi, resolution) in [0.0001, 0.0005, 0.001, 0.0002].into_iter().enumerate() {
        for (ti, text) in ["钢网测试口回", "中文ABC123"].into_iter().enumerate() {
            let mut s = ApplicationService::new();
            s.grant_file_access(&root, false).unwrap();
            s.grant_file_access(std::path::Path::new(font_path), false)
                .unwrap();
            s.grant_file_access(&out, true).unwrap();
            let d = s.open(root.to_str().unwrap()).unwrap();
            let policy = ManufacturingPrecision {
                resolution_mm: resolution,
            };
            let d = s
                .set_manufacturing_precision(&d.document_id, &d.revision, policy)
                .unwrap();
            let font = s.font_inspect(font_path, 0).unwrap();
            let params = TextParams {
                layer_id: d.layer_ids[0].clone(),
                font: font.identity.clone(),
                layout: TextLayout {
                    text: text.into(),
                    height_mm: 3.,
                    rotation_deg: 37.,
                    curve_tolerance_mm: policy.text_tolerance_mm(),
                    x_mm: 12.34567,
                    y_mm: -6.78901,
                    tracking_mm: 0.,
                    baseline_spacing_mm: 0.,
                    stroke_width_mm: 0.15,
                    outline_offset_mm: 0.,
                    h_align: HorizontalAlign::Left,
                    v_align: VerticalAlign::Bottom,
                },
            };
            let start = std::time::Instant::now();
            let result = (|| -> Result<(usize, usize, usize), ServiceError> {
                let p = s.text_preview(&d.document_id, &d.revision, params.clone())?;
                let objects = p.geometries.len();
                let edges: usize = p
                    .geometries
                    .iter()
                    .map(|g| {
                        if let SemanticGeometry::Region { contours } = g {
                            contours.iter().map(|c| c.edges.len()).sum()
                        } else {
                            1
                        }
                    })
                    .sum();
                let created = s.text_create(&d.document_id, &d.revision, params.clone())?;
                let path = out.join(format!("{pi}-{ti}.gbr"));
                let export = s.export_layer(
                    &d.document_id,
                    &created.revision,
                    ExportParams {
                        layer_id: params.layer_id,
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
                    },
                )?;
                Ok((objects, edges, export.bytes))
            })();
            rows.push(json!({"text":text,"resolution_mm":resolution,"curve_tolerance_mm":policy.text_tolerance_mm(),"font_sha256":font.identity.sha256,"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"result":result,"status":if result.is_ok(){"PASS"}else{"FAIL"}}));
            std::fs::write(
                out.join("matrix.json"),
                serde_json::to_vec_pretty(&json!({"schema_version":2,"rows":rows})).unwrap(),
            )
            .unwrap();
        }
    }
    assert!(
        rows.iter().all(|r| r["status"] == "PASS"),
        "see matrix.json"
    );
}
