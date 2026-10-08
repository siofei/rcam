mod array_support;
use array_support::Run;
use editor_core::{SemanticGeometry, geometries_bounds_with_blocks};
use editor_service::*;
use serde_json::json;

#[test]
fn json_dispatch_noop_permissions_and_revision_atomicity() {
    let mut r = Run::new(3);
    let p = r.params(1, 1);
    let before = r.info();
    let noop = r.array(p).unwrap();
    assert_eq!(noop.revision, before.revision);
    assert_eq!(noop.undo_entries_added, 0);
    assert_eq!(r.info().dirty, before.dirty);
    for category in [false, true] {
        for kind in 0..3 {
            let mut patch = LayerPatch {
                layer_id: r.layer.clone(),
                ..Default::default()
            };
            if category {
                let mut c = ClassStyleUpdate {
                    class: Some(DisplayClass::FlashCircle),
                    ..Default::default()
                };
                match kind {
                    0 => c.visible = Some(false),
                    1 => c.selectable = Some(false),
                    _ => c.locked = Some(true),
                };
                patch.classes.push(c);
            } else {
                match kind {
                    0 => patch.visible = Some(false),
                    1 => patch.selectable = Some(false),
                    _ => patch.locked = Some(true),
                }
            }
            r.patch(patch);
            let before = r.snapshot();
            let info = r.info();
            let p = r.params(2, 2);
            assert!(r.array(p).is_err());
            assert_eq!(r.snapshot().layers, before.layers);
            assert_eq!(r.info().revision, info.revision);
            assert_eq!(r.info().undo_entries, info.undo_entries);
            r.patch(LayerPatch {
                layer_id: r.layer.clone(),
                visible: Some(true),
                selectable: Some(true),
                locked: Some(false),
                reset_classes: true,
                ..Default::default()
            });
        }
    }
    let p = r.params(3, 4);
    let request = json!({"api_version":1,"request_id":"array-json","op":"objects.array_rectangular","document_id":r.document,"expected_revision":r.info().revision,"params":p});
    let result = r.service.execute_json(&request.to_string());
    assert_eq!(result["status"], "completed", "{result}");
    assert_eq!(result["result"]["undo_entries_added"], 1);
    assert_eq!(r.ids().len(), 36);
    let before = r.snapshot();
    let stale = r.service.execute_json(&request.to_string());
    assert_eq!(stale["error"]["code"], "REVISION_CONFLICT");
    assert_eq!(r.snapshot().layers, before.layers);
    let mut invalid = request.clone();
    invalid["expected_revision"] = json!(r.info().revision);
    invalid["params"]["surprise"] = json!(true);
    assert_eq!(
        r.service.execute_json(&invalid.to_string())["status"],
        "error"
    );
}

#[test]
fn missing_duplicate_cross_layer_and_noncontiguous_are_rejected_whole() {
    let mut r = Run::new(3);
    let original = r.ids();
    for ids in [
        vec![],
        vec!["missing".into()],
        vec![original[0].clone(), original[0].clone()],
        vec![original[0].clone(), original[2].clone()],
    ] {
        let mut p = r.params(2, 2);
        p.object_ids = ids;
        let before = r.snapshot();
        let info = r.info();
        assert!(r.array(p).is_err());
        assert_eq!(r.snapshot().layers, before.layers);
        assert_eq!(r.info().revision, info.revision);
    }
    let other = r
        .service
        .import_gerber_layer(
            &r.document,
            &r.info().revision,
            ImportGerberLayerParams {
                path: r.dir.join("source.gbr").to_string_lossy().into(),
            },
        )
        .unwrap();
    let _ = other;
    let snapshot = r.snapshot();
    let other_id = snapshot.layers[1].objects[0].object_id.clone();
    let mut p = r.params(2, 2);
    p.object_ids = vec![original[0].clone(), other_id];
    assert_eq!(r.array(p).unwrap_err().code, "CROSS_LAYER_EDIT_UNSUPPORTED");
}

#[test]
fn array_project_recovery_allocator_and_expanded_gerber_positions() {
    for block in [false, true] {
        let mut r = Run::new(3);
        if block {
            r.block();
        }
        let before = r.snapshot();
        let p = r.params(3, 4);
        let source_count = p.object_ids.len();
        let mut p = p;
        p.pitch_x_mm = -50.;
        p.pitch_y_mm = -60.;
        let result = r.array(p).unwrap();
        assert_eq!(result.undo_entries_added, 1);
        let after = r.snapshot();
        assert_eq!(after.block_definitions, before.block_definitions);
        assert_eq!(after.apertures, before.apertures);
        let undo = r
            .service
            .history_undo(&r.document, &result.revision)
            .unwrap();
        assert_eq!(r.snapshot().layers, before.layers);
        r.service.history_redo(&r.document, &undo.revision).unwrap();
        assert_eq!(r.snapshot().layers, after.layers);
        let bytes = r.service.project_recovery_bytes(&r.document).unwrap();
        let restore = r.service.project_restore(&bytes).unwrap();
        let restored = r.service.render_snapshot(&restore.document_id).unwrap();
        assert_eq!(restored.layers, after.layers);
        assert_eq!(restored.block_definitions, after.block_definitions);
        let path = r.dir.join("array.rcam");
        r.service
            .project_save(
                &r.document,
                &r.info().revision,
                Some(path.to_str().unwrap()),
                false,
            )
            .unwrap();
        let opened = r.service.project_open(path.to_str().unwrap()).unwrap();
        let snapshot = r.service.render_snapshot(&opened.document_id).unwrap();
        assert_eq!(snapshot.layers, after.layers);
        assert_eq!(snapshot.apertures, after.apertures);
        assert_eq!(snapshot.block_definitions, after.block_definitions);
        let exported = r.export("array-export.gbr");
        assert_eq!(exported.layers[0].objects.len(), 36);
        assert!(
            !std::fs::read_to_string(r.dir.join("array-export.gbr"))
                .unwrap()
                .contains("%SR")
        );
        // Independent expected cell positions, not writer-vs-writer comparison.
        for cell in 0..12 {
            for source in 0..3 {
                let o = &exported.layers[0].objects[cell * 3 + source];
                let bounds = geometries_bounds_with_blocks([&o.geometry], &exported.apertures, &[])
                    .unwrap()
                    .unwrap();
                assert!(
                    (bounds.center().x_mm - (source as f64 * 2. - (cell % 4) as f64 * 50.)).abs()
                        < 1e-7
                );
                assert!((bounds.center().y_mm + (cell / 4) as f64 * 60.).abs() < 1e-7);
            }
        }
        assert_eq!(std::fs::read(r.dir.join("source.gbr")).unwrap(), r.source);
        let selected = snapshot.layers[0].objects[..source_count]
            .iter()
            .map(|o| o.object_id.clone())
            .collect();
        let params = ArrayRectangularParams {
            layer_id: snapshot.layers[0].id.clone(),
            object_ids: selected,
            rows: 1,
            columns: 2,
            pitch_x_mm: 500.,
            pitch_y_mm: 0.,
        };
        let created = r
            .service
            .objects_array_rectangular(&opened.document_id, &opened.revision, params)
            .unwrap();
        assert!(created.changed_object_ids.iter().all(|id| {
            !snapshot.layers[0]
                .objects
                .iter()
                .any(|o| &o.object_id == id)
        }));
    }
}

#[test]
fn real_generated_text_full_group_and_operation_identity_roundtrip() {
    let mut r = Run::new(1);
    let created = r
        .service
        .text_create(
            &r.document,
            &r.info().revision,
            TextParams {
                layer_id: r.layer.clone(),
                font: builtin_stroke_font().identity,
                layout: TextLayout {
                    text: "ABC".into(),
                    x_mm: 0.,
                    y_mm: 5.,
                    height_mm: 3.,
                    tracking_mm: 0.,
                    h_align: HorizontalAlign::Left,
                    v_align: VerticalAlign::Bottom,
                    rotation_deg: 0.,
                    curve_tolerance_mm: editor_text::TOLERANCE_MM,
                    baseline_spacing_mm: 0.,
                    stroke_width_mm: 0.15,
                    outline_offset_mm: 0.,
                },
            },
        )
        .unwrap();
    assert!(created.generated_object_ids.len() > 1);
    let original = r.snapshot();
    let mut p = r.params(2, 2);
    p.object_ids = created.generated_object_ids[..1].to_vec();
    assert!(r.array(p).is_err());
    assert_eq!(r.snapshot().layers, original.layers);
    let mut p = r.params(2, 2);
    p.object_ids = created.generated_object_ids.clone();
    let added = r.array(p).unwrap();
    assert_eq!(
        added.changed_object_ids.len(),
        created.generated_object_ids.len() * 3
    );
    let snapshot = r.snapshot();
    let mut groups = std::collections::HashSet::new();
    for group in snapshot.layers[0].objects[1..].chunks(created.generated_object_ids.len()) {
        let editor_core::ObjectOrigin::GeneratedText { operation_id } = &group[0].origin else {
            panic!()
        };
        assert!(groups.insert(operation_id));
        assert!(group.iter().all(|o| o.origin == group[0].origin));
    }
    assert_eq!(groups.len(), 4);
    let bytes = r.service.project_recovery_bytes(&r.document).unwrap();
    let restored = r.service.project_restore(&bytes).unwrap();
    assert_eq!(
        r.service
            .render_snapshot(&restored.document_id)
            .unwrap()
            .layers,
        snapshot.layers
    );
}

#[test]
#[ignore = "release benchmark; raw timings are evidence, not P100K acceptance"]
fn performance_array_10k_block_and_ordinary_and_project_size() {
    for (count, block, rows, columns) in [
        (400, true, 100, 100),
        (1, false, 100, 100),
        (400, false, 5, 5),
        (400, true, 5, 5),
    ] {
        let mut r = Run::new(count);
        if block {
            r.block();
        }
        let p = r.params(rows, columns);
        let t = std::time::Instant::now();
        let estimate = r
            .service
            .estimate_array_rectangular(&r.document, &r.info().revision, &p)
            .unwrap();
        let preflight_us = t.elapsed().as_micros();
        let t = std::time::Instant::now();
        r.array(p).unwrap();
        let transaction_us = t.elapsed().as_micros();
        let snapshot = r.snapshot();
        let path = r.dir.join("benchmark.rcam");
        let t = std::time::Instant::now();
        r.service
            .project_save(
                &r.document,
                &r.info().revision,
                Some(path.to_str().unwrap()),
                false,
            )
            .unwrap();
        let save_us = t.elapsed().as_micros();
        let size = std::fs::metadata(&path).unwrap().len();
        let t = std::time::Instant::now();
        r.service.project_open(path.to_str().unwrap()).unwrap();
        let open_us = t.elapsed().as_micros();
        println!(
            "S4C5_PERF {}",
            json!({"source":count,"block":block,"rows":rows,"columns":columns,"preflight_us":preflight_us,"transaction_us":transaction_us,"history_bytes":estimate.history_bytes,"project_objects":snapshot.layers[0].objects.len(),"definitions":snapshot.block_definitions.len(),"definition_objects":snapshot.block_definitions.iter().map(|d|d.objects.len()).sum::<usize>(),"compressed_bytes":size,"save_us":save_us,"open_us":open_us})
        );
        if block {
            assert_eq!(snapshot.block_definitions.len(), 1);
            assert_eq!(snapshot.block_definitions[0].objects.len(), 400);
            assert!(
                snapshot.layers[0]
                    .objects
                    .iter()
                    .all(|o| matches!(o.geometry, SemanticGeometry::BlockInstance { .. }))
            );
        }
    }
}

#[test]
fn array_metrics_and_dark_clear_order_remain_manufacturing_based() {
    for block in [false, true] {
        let mut r=Run::from_bytes(b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,10*%\n%ADD11C,6*%\n%ADD12C,2*%\nD10*\nX0Y0D03*\n%LPC*%\nD11*\nX0Y0D03*\n%LPD*%\nD12*\nX0Y0D03*\nM02*\n");
        if block {
            r.block();
        }
        let selected = r.ids();
        let before = r
            .service
            .objects_metrics(
                &r.document,
                MetricsParams {
                    layer_id: r.layer.clone(),
                    object_ids: selected.clone(),
                },
            )
            .unwrap();
        let p = r.params(3, 2);
        r.array(p).unwrap();
        let after = r
            .service
            .objects_metrics(
                &r.document,
                MetricsParams {
                    layer_id: r.layer.clone(),
                    object_ids: r.ids(),
                },
            )
            .unwrap();
        for group in after.items.chunks(selected.len()) {
            for (source, copy) in before.items.iter().zip(group) {
                assert_eq!(source.value, copy.value);
            }
        }
        let exported = r.export("dcd.gbr");
        assert_eq!(exported.layers[0].objects.len(), 18);
        for group in exported.layers[0].objects.chunks(3) {
            assert_eq!(
                group.iter().map(|o| o.exposure).collect::<Vec<_>>(),
                vec![
                    editor_core::Exposure::Dark,
                    editor_core::Exposure::Clear,
                    editor_core::Exposure::Dark
                ]
            );
        }
    }
}

#[test]
#[ignore = "explicit synthetic native fixture generation"]
fn generate_native_array_block_fixture() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s4c5")
        .canonicalize()
        .unwrap();
    let mut service = ApplicationService::with_file_access(FileAccessPolicy::new(
        root.clone(),
        [root.clone()],
        [root.clone()],
    ));
    let opened = service.open("array-400.gbr").unwrap();
    let document = opened.document_id;
    let snapshot = service.render_snapshot(&document).unwrap();
    service
        .blocks_create_definition_from_objects(
            &document,
            "0",
            CreateBlockDefinitionParams {
                layer_id: snapshot.layers[0].id.clone(),
                object_ids: snapshot.layers[0]
                    .objects
                    .iter()
                    .map(|o| o.object_id.clone())
                    .collect(),
                local_origin_mm: PivotMm { x_mm: 0., y_mm: 0. },
                name: "400 openings".into(),
            },
        )
        .unwrap();
    let info = service.document_get(&document).unwrap();
    service
        .project_save(
            &document,
            &info.revision,
            Some(root.join("array-block400.rcam").to_str().unwrap()),
            false,
        )
        .unwrap();
}

#[test]
fn advisory_array_counts_and_resource_error_explain_demand_atomically() {
    let mut r = Run::new(1);
    let p = r.params(1, 500_001);
    let original = r.snapshot();
    let info = r.info();
    let error = r.array(p).unwrap_err();
    assert_eq!(error.code, "RESOURCE_LIMIT");
    assert!(error.message.contains("没有阵列格数硬上限"));
    assert_eq!(error.details["created_object_count"], 500_000);
    assert!(
        error.details["required_history_bytes"].as_u64().unwrap()
            > error.details["max_history_bytes"].as_u64().unwrap()
    );
    assert_eq!(r.snapshot().layers, original.layers);
    assert_eq!(r.info().revision, info.revision);
    assert_eq!(r.info().undo_entries, info.undo_entries);
    let made = r.array(r.params(1, 10_001)).unwrap();
    assert_eq!(made.changed_object_ids.len(), 10_000);
    assert_eq!(made.undo_entries_added, 1);
    assert_eq!(r.snapshot().layers[0].objects.len(), 10_001);
}
