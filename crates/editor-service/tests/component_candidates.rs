mod candidate_support;
use candidate_support::*;
use editor_core::{Exposure, MmPoint, SemanticGeometry, board::*};
use editor_service::*;
use serde_json::json;
#[test]
fn registration_transforms_bottom_revision_json_and_zero_mutation() {
    let mut r = Run::new(20);
    import(&mut r);
    let before = r.info();
    let q = query(&r);
    assert_eq!(nearby(&r, &q).unwrap_err().code, "REGISTRATION_REQUIRED");
    assert_eq!(r.info(), before);
    for reflect_x in [false, true] {
        for rotation_deg in [0., 37., 90.] {
            let t = CoordinateTransform2D {
                reflect_x,
                rotation_deg,
                translation: MmPoint::new(2., 3.),
            };
            register(&mut r, t);
            let mut q = query(&r);
            let before = r.info();
            r.export(&format!("before-{reflect_x}-{rotation_deg}.gbr"));
            let page = nearby(&r, &q).unwrap();
            let component = r
                .service
                .components_get(&r.document, &q.component_id)
                .unwrap();
            assert_eq!(page.window.center, component.world_position.unwrap());
            assert_eq!(
                page.window.rotation_deg,
                component.world_rotation_deg.unwrap()
            );
            let local = CoordinateTransform2D {
                rotation_deg: 37.,
                ..CoordinateTransform2D::IDENTITY
            };
            for p in [
                MmPoint::new(-5., -5.),
                MmPoint::new(5., -5.),
                MmPoint::new(5., 5.),
                MmPoint::new(-5., 5.),
            ] {
                let actual = page.window.corners();
                let expected = t.apply(local.apply(p));
                assert!(actual.iter().any(|a| a.distance_mm(expected) < 1e-10));
            }
            q.component_id = r
                .service
                .board_state(&r.document)
                .unwrap()
                .unwrap()
                .components[1]
                .id
                .0
                .clone();
            let bottom = nearby(&r, &q).unwrap();
            assert_eq!(bottom.window, page.window);
            assert_eq!(bottom.items, page.items);
            assert_eq!(r.info(), before);
            let response=r.service.execute_json(&json!({"api_version":1,"request_id":"nearby","op":"components.nearby_manufacturing","document_id":r.document,"params":q}).to_string());
            let decoded = response;
            assert_eq!(decoded["status"], "completed", "{decoded}");
            let json_page: ManufacturingCandidatePage =
                serde_json::from_value(decoded["result"].clone()).unwrap();
            assert_eq!(json_page, bottom);
            assert_eq!(r.info(), before);
            r.export(&format!("after-{reflect_x}-{rotation_deg}.gbr"));
            assert_eq!(
                std::fs::read(r.dir.join(format!("before-{reflect_x}-{rotation_deg}.gbr")))
                    .unwrap(),
                std::fs::read(r.dir.join(format!("after-{reflect_x}-{rotation_deg}.gbr"))).unwrap()
            );
        }
    }
    let q = query(&r);
    r.service
        .history_undo(&r.document, &r.info().revision)
        .unwrap();
    assert_eq!(nearby(&r, &q).unwrap_err().code, "REVISION_CONFLICT");
    r.service
        .history_redo(&r.document, &r.info().revision)
        .unwrap();
    assert_eq!(nearby(&r, &q).unwrap_err().code, "REVISION_CONFLICT");
}
#[test]
fn deterministic_ranking_pagination_clear_and_explicit_hidden_layers() {
    let mut r=Run::from_bytes(b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.8*%\nD10*\nX0Y0D03*\nX2000000Y0D03*\nX-2000000Y0D03*\n%LPC*%\nX4800000Y0D03*\nM02*\n");
    import(&mut r);
    register(&mut r, CoordinateTransform2D::IDENTITY);
    register(
        &mut r,
        CoordinateTransform2D {
            rotation_deg: -37.,
            ..CoordinateTransform2D::IDENTITY
        },
    );
    let q = query(&r);
    let all = nearby(&r, &q).unwrap();
    assert_eq!(all.total, 4);
    assert!(all.items.last().is_some_and(|c| !c.fully_inside_window));
    assert!(all.items.iter().any(|c| c.exposure == Exposure::Clear));
    assert!(all.items[0].center_distance_mm < 1e-9);
    let mut q = q;
    q.limit = 1;
    for i in 0..4 {
        q.offset = i;
        assert_eq!(nearby(&r, &q).unwrap().items, all.items[i..i + 1]);
    }
    q.offset = 100;
    assert!(nearby(&r, &q).unwrap().items.is_empty());
    r.patch(LayerPatch {
        layer_id: r.layer.clone(),
        visible: Some(false),
        selectable: Some(false),
        ..Default::default()
    });
    assert_eq!(nearby(&r, &query(&r)).unwrap().items, all.items);
    let source = r.dir.join("source.gbr").to_string_lossy().into_owned();
    r.service
        .import_gerber_layers(
            &r.document,
            &r.info().revision,
            ImportGerberLayersParams {
                paths: vec![source],
            },
        )
        .unwrap();
    let mut q = query(&r);
    q.layer_ids = r.info().layer_ids;
    let page = nearby(&r, &q).unwrap();
    assert_eq!(page.total, 8);
    let ids: std::collections::HashSet<_> = page
        .items
        .iter()
        .map(|c| (&c.layer_id, &c.object_id))
        .collect();
    assert_eq!(ids.len(), 8);
}
#[test]
fn invalid_and_budget_queries_reject_whole_result() {
    let r = setup(3);
    let base = query(&r);
    let before = r.info();
    for v in [
        0.,
        -1.,
        f64::NAN,
        f64::INFINITY,
        editor_core::pnp::MAX_BOARD_MM + 1.,
    ] {
        let mut q = base.clone();
        q.window = ManufacturingSearchWindow::ComponentLocalRect {
            width_mm: v,
            height_mm: 10.,
        };
        assert!(nearby(&r, &q).is_err());
    }
    for limit in [0, 501, usize::MAX] {
        let mut q = base.clone();
        q.limit = limit;
        assert!(nearby(&r, &q).is_err());
    }
    for layers in [
        vec![],
        vec!["missing".into()],
        vec![r.layer.clone(), r.layer.clone()],
    ] {
        let mut q = base.clone();
        q.layer_ids = layers;
        assert!(nearby(&r, &q).is_err());
    }
    assert_eq!(r.info(), before);
    let mut bytes = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.8*%\nD10*\n");
    for _ in 0..10001 {
        bytes.push_str("X0Y0D03*\n");
    }
    bytes.push_str("M02*\n");
    let mut dense = Run::from_bytes(bytes.as_bytes());
    import(&mut dense);
    register(&mut dense, CoordinateTransform2D::IDENTITY);
    let before = dense.info();
    assert_eq!(
        nearby(&dense, &query(&dense)).unwrap_err().code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(dense.info(), before);
}
#[test]
fn whole_block_and_complete_text_group_bounds_cache_edit_invalidation() {
    let mut r = setup(1);
    r.block();
    let definition = r.service.blocks_list_definitions(&r.document).unwrap()[0]
        .id
        .clone();
    r.service
        .blocks_create_instance(
            &r.document,
            &r.info().revision,
            CreateBlockInstanceParams {
                layer_id: r.layer.clone(),
                definition_id: definition,
                transform: BlockTransformParams {
                    translation_mm: PivotMm { x_mm: 0., y_mm: 0. },
                    rotation_deg: 0.,
                    mirror: false,
                },
            },
        )
        .unwrap();
    let text = r
        .service
        .text_create(
            &r.document,
            &r.info().revision,
            TextParams {
                layer_id: r.layer.clone(),
                font: builtin_stroke_font().identity,
                layout: TextLayout {
                    text: "ABC".into(),
                    x_mm: 1.,
                    y_mm: 1.,
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
    let page = nearby(&r, &query(&r)).unwrap();
    assert!(
        page.items
            .iter()
            .any(|c| c.geometry_kind == "BlockInstance" && c.member_object_ids.len() == 1)
    );
    let t = page
        .items
        .iter()
        .find(|c| c.geometry_kind == "GeneratedText")
        .unwrap();
    assert_eq!(t.member_object_ids.len(), text.generated_object_ids.len());
    let objects = r
        .service
        .candidate_member_objects(&r.document, &page.revision, &page.items)
        .unwrap();
    assert_eq!(
        objects.iter().flatten().count(),
        page.items
            .iter()
            .map(|c| c.member_object_ids.len())
            .sum::<usize>()
    );
    let q = query(&r);
    r.service
        .objects_move(
            &r.document,
            &r.info().revision,
            MoveParams {
                layer_id: r.layer.clone(),
                object_ids: r.ids(),
                dx_mm: 100.,
                dy_mm: 0.,
            },
        )
        .unwrap();
    assert_eq!(nearby(&r, &q).unwrap_err().code, "REVISION_CONFLICT");
    assert_eq!(nearby(&r, &query(&r)).unwrap().total, 0);
    r.service
        .history_undo(&r.document, &r.info().revision)
        .unwrap();
    assert_eq!(nearby(&r, &query(&r)).unwrap().items, page.items);
    assert!(
        r.snapshot().layers[0]
            .objects
            .iter()
            .any(|o| matches!(o.geometry, SemanticGeometry::BlockInstance { .. }))
    );
}
#[test]
#[ignore = "standalone release measurement; not P100K product acceptance"]
fn performance_100k_sparse_and_5000_dense() {
    use std::time::Instant;
    let r = setup(100000);
    let q = query(&r);
    let cold = Instant::now();
    let page = nearby(&r, &q).unwrap();
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.;
    let mut samples = vec![];
    for _ in 0..100 {
        let start = Instant::now();
        assert_eq!(nearby(&r, &q).unwrap().items, page.items);
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    samples.sort_by(f64::total_cmp);
    assert!(page.nearby_object_count < 100);
    assert!(samples[95] < 100.);
    let mut bytes = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.8*%\nD10*\n");
    for _ in 0..5000 {
        bytes.push_str("X0Y0D03*\n");
    }
    bytes.push_str("M02*\n");
    let mut dense = Run::from_bytes(bytes.as_bytes());
    import(&mut dense);
    register(&mut dense, CoordinateTransform2D::IDENTITY);
    let mut qd = query(&dense);
    let mut ids = vec![];
    for offset in (0..5000).step_by(500) {
        qd.offset = offset;
        let p = nearby(&dense, &qd).unwrap();
        assert_eq!(p.total, 5000);
        ids.extend(p.items.into_iter().map(|c| c.object_id));
    }
    assert_eq!(ids.len(), 5000);
    assert!(ids.windows(2).all(|p| p[0] < p[1]));
    let report = json!({"schema_version":2,"manufacturing_objects":100000,"cold_ms":cold_ms,"query_p50_ms":samples[50],"query_p95_ms":samples[95],"nearby_object_count":page.nearby_object_count,"dense_count":5000,"dense_pages":10,"status":"PASS","p100k_product_acceptance":"not claimed"});
    println!("{report}");
    if let Some(out) = std::env::var_os("RCAM_S4D2_EVIDENCE_DIR") {
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            out.join("candidate-performance.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
