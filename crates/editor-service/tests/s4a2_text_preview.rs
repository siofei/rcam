//! S4-A2 service foundation; this does not claim native GUI/IME evidence.
use editor_service::*;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Instant;

fn setup() -> (ApplicationService, DocumentInfo, TextParams) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s1a/ordered_local_hole.gbr");
    let path = std::env::var("RCAM_TEXT_FONT")
        .unwrap_or_else(|_| "/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into());
    let hash = std::env::var("RCAM_TEXT_FONT_SHA256").unwrap_or_else(|_| {
        "876af2cd4854644e7f3e7feb2f688997fdb3343c6df6693611209c9dfb47ccec".into()
    });
    let mut service = ApplicationService::new();
    service.grant_file_access(&source, false).unwrap();
    service
        .grant_file_access(std::path::Path::new(&path), false)
        .unwrap();
    let info = service.open(source.to_str().unwrap()).unwrap();
    // This is intentionally the old S4-A1 wire shape: no precision field.
    let params: TextParams = serde_json::from_value(json!({
        "layer_id": info.layer_ids[0],
        "font": {"path":path,"sha256":hash,"face_index":0,
            "license_status":"local OS font; no redistribution","redistribution_allowed":false},
        "layout": {"text":"口8","height_mm":3.,"tracking_mm":0.1,
            "x_mm":12.34567,"y_mm":-6.78901,"rotation_deg":37.,
            "h_align":"center","v_align":"middle"}
    }))
    .unwrap();
    (service, info, params)
}
fn state(service: &ApplicationService, id: &str) -> Value {
    json!({"info":service.document_get(id).unwrap(),
        "snapshot":service.render_snapshot(id).unwrap()})
}

#[test]
fn preview_is_read_only_and_matches_one_create_at_each_precision() {
    for tolerance in [0.00025, 0.000125] {
        let (mut service, info, mut params) = setup();
        assert_eq!(params.layout.curve_tolerance_mm, editor_text::TOLERANCE_MM);
        params.layout.curve_tolerance_mm = tolerance;
        let before = state(&service, &info.document_id);
        let started = Instant::now();
        let preview = service
            .text_preview(&info.document_id, &info.revision, params.clone())
            .unwrap();
        eprintln!(
            "precision_mm={tolerance} generation_ms={} object_count={}",
            started.elapsed().as_secs_f64() * 1000.,
            preview.geometries.len()
        );
        let decoded: TextPreviewResult =
            serde_json::from_value(serde_json::to_value(&preview).unwrap()).unwrap();
        assert_eq!(preview.geometries, decoded.geometries);
        assert_eq!(preview.document_id, info.document_id);
        assert_eq!(preview.revision, info.revision);
        assert_eq!(preview.params.layout.curve_tolerance_mm, tolerance);
        assert_eq!(state(&service, &info.document_id), before);
        // Repeating a preview cannot consume IDs, produce history, or dirty the document.
        assert_eq!(
            service
                .text_preview(&info.document_id, &info.revision, params.clone())
                .unwrap()
                .geometries,
            preview.geometries
        );
        assert_eq!(state(&service, &info.document_id), before);
        let created = service
            .text_create(&info.document_id, &info.revision, params.clone())
            .unwrap();
        assert_eq!(created.undo_entries_added, 1);
        assert!(created.generated_object_ids[0].ends_with("-generated-object-0"));
        let current = service.document_get(&info.document_id).unwrap();
        assert_eq!(current.undo_entries, info.undo_entries + 1);
        assert!(current.dirty);
        assert_eq!(
            current.revision.parse::<u64>().unwrap(),
            info.revision.parse::<u64>().unwrap() + 1
        );
        let actual: Vec<_> = created
            .generated_object_ids
            .iter()
            .map(|id| {
                let object = service
                    .objects_get(
                        &info.document_id,
                        ObjectParams {
                            layer_id: params.layer_id.clone(),
                            object_id: id.clone(),
                        },
                    )
                    .unwrap()
                    .object;
                assert_eq!(object.exposure, editor_core::Exposure::Dark);
                object.geometry
            })
            .collect();
        assert_eq!(actual, preview.geometries);
        assert_eq!(
            service
                .text_preview(&info.document_id, &info.revision, params.clone())
                .unwrap_err()
                .code,
            "REVISION_CONFLICT"
        );
        service
            .history_undo(&info.document_id, &current.revision)
            .unwrap();
        let undone = service.document_get(&info.document_id).unwrap();
        assert!(!undone.dirty);
        let saved = state(&service, &info.document_id);
        service
            .text_preview(&info.document_id, &undone.revision, params)
            .unwrap();
        assert_eq!(saved, state(&service, &info.document_id));
        service
            .history_redo(&info.document_id, &undone.revision)
            .unwrap();
        assert_eq!(
            service
                .document_get(&info.document_id)
                .unwrap()
                .undo_entries,
            current.undo_entries
        );
    }
}

#[test]
fn precision_rejection_and_font_failure_leave_everything_unchanged() {
    let (mut service, info, params) = setup();
    let before = state(&service, &info.document_id);
    for (tolerance, code) in [
        (f64::NAN, "INVALID_ARGUMENT"),
        (f64::INFINITY, "INVALID_ARGUMENT"),
        (0., "INVALID_ARGUMENT"),
        (-0.1, "INVALID_ARGUMENT"),
        (0.00025001, "INVALID_ARGUMENT"),
        (1., "INVALID_ARGUMENT"),
        (0.00000999, "RESOURCE_LIMIT"),
        (f64::MIN_POSITIVE, "RESOURCE_LIMIT"),
    ] {
        let mut p = params.clone();
        p.layout.curve_tolerance_mm = tolerance;
        assert_eq!(
            service
                .text_preview(&info.document_id, &info.revision, p.clone())
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(
            service
                .text_create(&info.document_id, &info.revision, p)
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(state(&service, &info.document_id), before);
    }
    for field in 0..4 {
        let mut p = params.clone();
        match field {
            0 => p.font.sha256 = "0".repeat(64),
            1 => p.font.face_index = u32::MAX,
            2 => p.layout.text = "A".repeat(129),
            _ => p.layer_id = "missing-layer".into(),
        }
        let preview = service
            .text_preview(&info.document_id, &info.revision, p.clone())
            .unwrap_err();
        let apply = service
            .text_create(&info.document_id, &info.revision, p)
            .unwrap_err();
        assert_eq!(preview, apply);
        assert_eq!(state(&service, &info.document_id), before);
    }
}

#[test]
fn json_preview_requires_revision_and_strict_params() {
    let (mut service, info, params) = setup();
    let before = state(&service, &info.document_id);
    let mut request = json!({"api_version":1,"request_id":"preview-1","op":"text.preview",
        "document_id":info.document_id,"expected_revision":info.revision,"params":params});
    let result = service.execute_json(&request.to_string());
    assert!(
        result["result"]["geometries"].as_array().is_some(),
        "{result}"
    );
    assert_eq!(
        result["result"]["params"]["layout"]["curve_tolerance_mm"],
        0.00025
    );
    request["params"]["layout"]["unknown_offset"] = json!(0.1);
    // Unknown fields must never be silently ignored.
    assert_eq!(
        service.execute_json(&request.to_string())["error"]["code"],
        "INVALID_ARGUMENT"
    );
    request["params"]["layout"]
        .as_object_mut()
        .unwrap()
        .remove("unknown_offset");
    request.as_object_mut().unwrap().remove("expected_revision");
    assert_eq!(
        service.execute_json(&request.to_string())["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(before, state(&service, &info.document_id));
    service
        .close(&info.document_id, &info.revision, false)
        .unwrap();
    assert_eq!(
        service
            .text_preview(&info.document_id, &info.revision, params)
            .unwrap_err()
            .code,
        "NOT_FOUND"
    );
}

#[test]
fn frozen_thin_slab_preview_create_parity() {
    for tolerance in [0.00025, 0.000125, 0.0000625] {
        let (mut service, info, mut params) = setup();
        params.layout.curve_tolerance_mm = tolerance;
        let before = state(&service, &info.document_id);
        let preview = service
            .text_preview(&info.document_id, &info.revision, params.clone())
            .unwrap();
        assert_eq!(state(&service, &info.document_id), before);
        let result = service
            .text_create(&info.document_id, &info.revision, params.clone())
            .unwrap();
        let actual: Vec<_> = result
            .generated_object_ids
            .into_iter()
            .map(|id| {
                service
                    .objects_get(
                        &info.document_id,
                        ObjectParams {
                            layer_id: params.layer_id.clone(),
                            object_id: id,
                        },
                    )
                    .unwrap()
                    .object
                    .geometry
            })
            .collect();
        assert_eq!(actual, preview.geometries);
    }
}

#[test]
fn locked_layer_is_rejected_and_does_not_modify_manufacturing_state() {
    let (mut service, info, params) = setup();
    service
        .layer_update(
            &info.document_id,
            &info.revision,
            LayerUpdateParams {
                layer_id: params.layer_id.clone(),
                expected_workspace_revision: info.workspace_revision.clone(),
                display_name: None,
                visible: None,
                locked: Some(true),
            },
        )
        .unwrap();
    let before = state(&service, &info.document_id);
    assert_eq!(
        service
            .text_preview(&info.document_id, &info.revision, params.clone())
            .unwrap_err()
            .code,
        "LAYER_LOCKED"
    );
    assert_eq!(
        service
            .text_create(&info.document_id, &info.revision, params)
            .unwrap_err()
            .code,
        "LAYER_LOCKED"
    );
    assert_eq!(state(&service, &info.document_id), before);
}

#[test]
fn preview_preflights_history_budget_instead_of_promising_an_uncommittable_edit() {
    let (_, info, params) = setup();
    let source = PathBuf::from(&info.source_path);
    let font = PathBuf::from(&params.font.path);
    let policy = FileAccessPolicy::new(
        source.parent().unwrap(),
        [source.clone(), font],
        std::iter::empty::<PathBuf>(),
    );
    let mut service =
        ApplicationService::with_file_access_and_history_limits(policy, 100, 1024).unwrap();
    let opened = service.open(source.to_str().unwrap()).unwrap();
    let mut params = params;
    params.layer_id = opened.layer_ids[0].clone();
    let before = state(&service, &opened.document_id);
    let preview = service
        .text_preview(&opened.document_id, &opened.revision, params.clone())
        .unwrap_err();
    assert_eq!(preview.code, "RESOURCE_LIMIT");
    assert_eq!(
        service
            .text_create(&opened.document_id, &opened.revision, params)
            .unwrap_err(),
        preview
    );
    assert_eq!(before, state(&service, &opened.document_id));
}

#[test]
fn preview_does_not_authorize_changed_font_bytes() {
    let (mut service, info, mut params) = setup();
    let dir = std::env::temp_dir().join(format!("rcam-s4a2-font-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let local = dir.join("local.ttf");
    std::fs::copy(&params.font.path, &local).unwrap();
    service.grant_file_access(&local, false).unwrap();
    params.font.path = local.to_str().unwrap().into();
    let before = state(&service, &info.document_id);
    service
        .text_preview(&info.document_id, &info.revision, params.clone())
        .unwrap();
    std::fs::write(&local, b"externally changed font").unwrap();
    let result = service.text_create(&info.document_id, &info.revision, params);
    std::fs::remove_dir_all(&dir).unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENT");
    assert!(error.message.contains("hash mismatch"));
    assert_eq!(state(&service, &info.document_id), before);
}

#[test]
fn accepted_custom_precision_exports_and_reopens_within_writer_budget() {
    let (mut service, info, mut params) = setup();
    params.layout.curve_tolerance_mm = 0.000125;
    let preview = service
        .text_preview(&info.document_id, &info.revision, params.clone())
        .unwrap();
    let created = service
        .text_create(&info.document_id, &info.revision, params.clone())
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rcam-s4a2-reopen-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    service.grant_file_access(&dir, true).unwrap();
    let path = dir.join("custom.gbr");
    let exported = service.export_layer(
        &info.document_id,
        &created.revision,
        ExportParams {
            layer_id: params.layer_id,
            path: path.to_str().unwrap().into(),
            overwrite: OverwritePolicy {
                mode: "deny".into(),
                expected_sha256: None,
            },
            metadata_policy: MetadataPolicy {
                mode: "require_confirmation".into(),
                categories: None,
            },
        },
    );
    if let Err(error) = exported {
        std::fs::remove_dir_all(&dir).unwrap();
        panic!("custom precision writer rejected: {error:?}");
    }
    let bytes = std::fs::read(&path).unwrap();
    let reopened = gerber_io::parse_s1(&bytes, "reopened").unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let objects = &reopened.document.layers[0].objects;
    let generated = &objects[objects.len() - preview.geometries.len()..];
    for (object, expected) in generated.iter().zip(&preview.geometries) {
        assert_eq!(object.exposure, editor_core::Exposure::Dark);
        let (
            editor_core::SemanticGeometry::Region { contours: a },
            editor_core::SemanticGeometry::Region { contours: b },
        ) = (&object.geometry, expected)
        else {
            panic!("expected ordinary manufacturing Region after reopen")
        };
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.edges.len(), b.edges.len());
            for (a, b) in a.edges.iter().zip(&b.edges) {
                match (a, b) {
                    (
                        editor_core::RegionEdge::Line { start: a, end: b },
                        editor_core::RegionEdge::Line { start: c, end: d },
                    ) => assert!(
                        a.distance_mm(output_grid(*c)) < 1e-6
                            && b.distance_mm(output_grid(*d)) < 1e-6
                    ),
                    (editor_core::RegionEdge::Arc(a), editor_core::RegionEdge::Arc(b)) => {
                        assert!(
                            a.start.distance_mm(output_grid(b.start)) < 1e-6
                                && a.end.distance_mm(output_grid(b.end)) < 1e-6
                        );
                        assert!(a.center.distance_mm(output_grid(b.center)) < 1e-6);
                        assert_eq!(a.direction, b.direction);
                        assert_eq!(a.full_circle, b.full_circle);
                    }
                    _ => panic!("manufacturing edge kind changed on reopen"),
                }
            }
        }
    }
}

#[test]
fn material_offset_real_glyphs_are_dark_and_atomic() {
    for text in ["O", "8", "中", "回"] {
        for offset in [0., 0.01, -0.01] {
            let (mut service, info, mut params) = setup();
            params.layout.text = text.into();
            params.layout.outline_offset_mm = offset;
            let preview = service
                .text_preview(&info.document_id, &info.revision, params.clone())
                .unwrap_or_else(|e| panic!("{text} {offset}: {e:?}"));
            let result = service
                .text_create(&info.document_id, &info.revision, params.clone())
                .unwrap();
            let actual: Vec<_> = result
                .generated_object_ids
                .into_iter()
                .map(|id| {
                    let o = service
                        .objects_get(
                            &info.document_id,
                            ObjectParams {
                                layer_id: params.layer_id.clone(),
                                object_id: id,
                            },
                        )
                        .unwrap()
                        .object;
                    assert_eq!(o.exposure, editor_core::Exposure::Dark);
                    o.geometry
                })
                .collect();
            assert_eq!(actual, preview.geometries);
        }
    }
    let (mut service, info, mut params) = setup();
    params.layout.text = "O".into();
    params.layout.outline_offset_mm = -0.7;
    let before = state(&service, &info.document_id);
    assert_eq!(
        service
            .text_create(&info.document_id, &info.revision, params)
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    assert_eq!(state(&service, &info.document_id), before);
}

// Independent default manufacturing-grid oracle; encoding comparison stays 1 nm.
fn output_grid(p: editor_core::MmPoint) -> editor_core::MmPoint {
    editor_core::MmPoint::new(
        (p.x_mm * 10000.).round() / 10000.,
        (p.y_mm * 10000.).round() / 10000.,
    )
}
