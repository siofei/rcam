mod array_support;
use array_support::Run;
use editor_core::{MmPoint, board::*, pnp::*};
use editor_service::*;
use serde_json::json;
fn mapping() -> PnpMapping {
    serde_json::from_value(json!({"delimiter":"csv","unit":"mm","refdes":0,"x":1,"y":2,"rotation":3,"side":4,"footprint":5,"value":6,"top_token":"Top","bottom_token":"Bottom","clockwise":false,"rotation_offset_deg":0,"invert_y":false})).unwrap()
}
fn setup() -> Run {
    let mut r = Run::new(3);
    std::fs::write(
        r.dir.join("pnp.csv"),
        include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv"),
    )
    .unwrap();
    let path = r.dir.join("baseline.rcam");
    r.service
        .project_save(
            &r.document,
            &r.info().revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    r
}
fn params(r: &Run) -> ImportPnpParams {
    let p = r
        .service
        .components_preview_pnp("pnp.csv", &mapping())
        .unwrap();
    ImportPnpParams {
        path: "pnp.csv".into(),
        mapping: mapping(),
        preview_sha256: p.sha256,
        allow_replace: false,
    }
}
fn import(r: &mut Run) {
    let p = params(r);
    r.service
        .components_import_pnp(&r.document, &r.info().revision, p)
        .unwrap();
}
fn undo(r: &mut Run) {
    r.service
        .history_undo(&r.document, &r.info().revision)
        .unwrap();
}
fn redo(r: &mut Run) {
    r.service
        .history_redo(&r.document, &r.info().revision)
        .unwrap();
}
fn q(r: &Run, query: &str, mode: RefdesMatch) -> ComponentQuery {
    ComponentQuery {
        revision: r.info().revision,
        query: query.into(),
        mode,
        side: None,
        footprint: None,
        offset: 0,
        limit: 500,
    }
}
#[test]
fn import_registration_mixed_history_dirty_exact_restore_and_writer_invariance() {
    let mut r = setup();
    let before = r.info();
    let geometry = r.snapshot();
    r.export("before.gbr");
    let gerber = std::fs::read(r.dir.join("before.gbr")).unwrap();
    import(&mut r);
    assert_eq!(r.info().undo_entries, before.undo_entries + 1);
    assert!(!r.info().dirty);
    assert!(r.info().project_dirty);
    assert_eq!(r.snapshot().layers, geometry.layers);
    let board = r.service.board_state(&r.document).unwrap().unwrap();
    assert!(
        r.service
            .components_get(&r.document, &board.components[0].id.0)
            .unwrap()
            .world_position
            .is_none()
    );
    let input = RegistrationInput::TwoPoint {
        board: [MmPoint::new(0., 0.), MmPoint::new(10., 0.)],
        world: [MmPoint::new(20., 30.), MmPoint::new(20., 40.)],
        reflect_x: false,
    };
    r.service
        .board_set_registration(&r.document, &r.info().revision, input.clone())
        .unwrap();
    let registered = r.service.board_state(&r.document).unwrap().unwrap();
    assert_eq!(
        r.service
            .components_get(&r.document, &board.components[1].id.0)
            .unwrap()
            .world_position,
        Some(MmPoint::new(20., 40.))
    );
    let info = r.info();
    let noop = r
        .service
        .board_set_registration(&r.document, &info.revision, input)
        .unwrap();
    assert_eq!(noop.revision, info.revision);
    assert_eq!(noop.undo_entries_added, 0);
    r.service
        .objects_move(
            &r.document,
            &r.info().revision,
            MoveParams {
                layer_id: r.layer.clone(),
                object_ids: vec![r.ids()[0].clone()],
                dx_mm: 1.,
                dy_mm: 0.,
            },
        )
        .unwrap();
    assert!(r.info().dirty);
    undo(&mut r);
    assert!(!r.info().dirty);
    assert_eq!(
        r.service.board_state(&r.document).unwrap().unwrap(),
        registered
    );
    undo(&mut r);
    assert_eq!(r.service.board_state(&r.document).unwrap().unwrap(), board);
    undo(&mut r);
    assert!(r.service.board_state(&r.document).unwrap().is_none());
    assert!(!r.info().project_dirty);
    assert!(r.info().revision.parse::<u64>().unwrap() > info.revision.parse::<u64>().unwrap());
    redo(&mut r);
    redo(&mut r);
    assert_eq!(
        r.service.board_state(&r.document).unwrap().unwrap(),
        registered
    );
    r.export("after.gbr");
    assert_eq!(std::fs::read(r.dir.join("after.gbr")).unwrap(), gerber);
    // View/query/display units are outside Board/geometry history.
    let info = r.info();
    let page = r
        .service
        .components_search(&r.document, &q(&r, "R", RefdesMatch::Prefix))
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].component.id, board.components[0].id);
    assert_eq!(r.info(), info);
}
#[test]
fn whole_batch_errors_stale_preview_conflicts_permissions_and_budget_preserve_redo_ids() {
    let mut r = setup();
    let p = params(&r);
    std::fs::write(
        r.dir.join("pnp.csv"),
        include_bytes!("../../../fixtures/synthetic/s4d1/pnp-invalid.csv"),
    )
    .unwrap();
    let before = r.info();
    assert_eq!(
        r.service
            .components_import_pnp(&r.document, &before.revision, p)
            .unwrap_err()
            .code,
        "EXTERNAL_MODIFICATION"
    );
    assert_eq!(r.info(), before);
    let p = params(&r);
    let e = r
        .service
        .components_import_pnp(&r.document, &before.revision, p)
        .unwrap_err();
    assert_eq!(e.code, "VALIDATION_FAILED");
    assert_eq!(e.details["diagnostics"][0]["line"], 3);
    assert!(!e.to_string().contains("NaN"));
    assert_eq!(r.info(), before);
    assert!(r.service.board_state(&r.document).unwrap().is_none());
    std::fs::write(
        r.dir.join("pnp.csv"),
        include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv"),
    )
    .unwrap();
    import(&mut r);
    assert_eq!(
        r.service
            .board_state(&r.document)
            .unwrap()
            .unwrap()
            .components[0]
            .id
            .0,
        "component-0"
    );
    undo(&mut r);
    let before = r.info();
    let mut p = params(&r);
    p.mapping.x = 0;
    assert!(
        r.service
            .components_import_pnp(&r.document, &before.revision, p)
            .is_err()
    );
    assert_eq!(r.info(), before);
    redo(&mut r);
    let p = params(&r);
    assert_eq!(
        r.service
            .components_import_pnp(&r.document, &r.info().revision, p.clone())
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let mut p = p;
    p.allow_replace = true;
    let stale = r
        .service
        .components_import_pnp(&r.document, "0", p.clone())
        .unwrap_err();
    assert_eq!(stale.code, "REVISION_CONFLICT");
    r.service
        .components_import_pnp(&r.document, &r.info().revision, p)
        .unwrap();
    assert_eq!(
        r.service
            .board_state(&r.document)
            .unwrap()
            .unwrap()
            .components[0]
            .id
            .0,
        "component-3"
    );
    let before = r.info();
    assert!(
        r.service
            .board_set_registration(
                &r.document,
                &before.revision,
                RegistrationInput::TwoPoint {
                    board: [MmPoint::new(0., 0.); 2],
                    world: [MmPoint::new(1., 2.), MmPoint::new(10., 20.)],
                    reflect_x: false
                }
            )
            .is_err()
    );
    assert_eq!(r.info(), before);
    assert!(
        r.service
            .components_preview_pnp("../outside.csv", &mapping())
            .is_err()
    );
    let mut bounded = ApplicationService::with_file_access_and_history_limits(
        FileAccessPolicy::new(r.dir.clone(), [r.dir.clone()], [r.dir.clone()]),
        100,
        1,
    )
    .unwrap();
    let info = bounded.document_new().unwrap();
    let p = params(&r);
    assert_eq!(
        bounded
            .components_import_pnp(&info.document_id, "0", p)
            .unwrap_err()
            .code,
        "RESOURCE_LIMIT"
    );
    assert!(bounded.board_state(&info.document_id).unwrap().is_none());
    assert_eq!(
        bounded.document_get(&info.document_id).unwrap().revision,
        "0"
    );
}
#[test]
fn typed_json_queries_and_save_open_recovery_ids_and_determinism() {
    let mut r = setup();
    let p = params(&r);
    let req = json!({"api_version":1,"request_id":"d1","op":"components.import_pnp","document_id":r.document,"expected_revision":r.info().revision,"params":p});
    let res = r.service.execute_json(&req.to_string());
    assert_eq!(res["status"], "completed", "{res}");
    assert_eq!(
        r.service.execute_json(&req.to_string())["error"]["code"],
        "REVISION_CONFLICT"
    );
    for (mode, text, total) in [
        (RefdesMatch::Exact, "R123", 1),
        (RefdesMatch::Prefix, "R", 1),
        (RefdesMatch::Substring, "1", 3),
        (RefdesMatch::Exact, "r123", 0),
    ] {
        assert_eq!(
            r.service
                .components_search(&r.document, &q(&r, text, mode))
                .unwrap()
                .total,
            total
        );
    }
    let mut query = q(&r, "", RefdesMatch::Prefix);
    query.side = Some(BoardSide::Bottom);
    query.footprint = Some("0603".into());
    assert_eq!(
        r.service
            .components_list(&r.document, &query)
            .unwrap()
            .total,
        1
    );
    query.limit = 501;
    assert!(r.service.components_search(&r.document, &query).is_err());
    query.limit = 500;
    query.revision = "0".into();
    assert!(r.service.components_search(&r.document, &query).is_err());
    r.service
        .board_set_registration(
            &r.document,
            &r.info().revision,
            RegistrationInput::Manual {
                transform: CoordinateTransform2D {
                    reflect_x: true,
                    rotation_deg: 37.,
                    translation: MmPoint::new(12., -8.),
                },
            },
        )
        .unwrap();
    let original = r.service.board_state(&r.document).unwrap().unwrap();
    let bytes = r.service.project_recovery_bytes(&r.document).unwrap();
    assert_eq!(rcam_project::decode(&bytes).unwrap().format_version, 2);
    assert_eq!(
        r.service.project_recovery_bytes(&r.document).unwrap(),
        bytes
    );
    let restored = r.service.project_restore(&bytes).unwrap();
    assert!(restored.project_dirty);
    assert_eq!(
        r.service
            .board_state(&restored.document_id)
            .unwrap()
            .unwrap(),
        original
    );
    let path = r.dir.join("pnp.rcam");
    let saved = r
        .service
        .project_save(
            &r.document,
            &r.info().revision,
            Some(path.to_str().unwrap()),
            false,
        )
        .unwrap();
    assert!(!saved.project_dirty);
    std::fs::remove_file(r.dir.join("pnp.csv")).unwrap();
    let opened = r.service.project_open(path.to_str().unwrap()).unwrap();
    assert!(!opened.project_dirty);
    assert_eq!(
        r.service.board_state(&opened.document_id).unwrap().unwrap(),
        original
    );
    std::fs::write(
        r.dir.join("pnp.csv"),
        include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv"),
    )
    .unwrap();
    let mut p = params(&r);
    p.allow_replace = true;
    r.service
        .components_import_pnp(&opened.document_id, &opened.revision, p)
        .unwrap();
    assert_eq!(
        r.service
            .board_state(&opened.document_id)
            .unwrap()
            .unwrap()
            .components[0]
            .id
            .0,
        "component-3"
    );
}
#[test]
#[ignore = "release synthetic 100k performance gate"]
fn performance_100k_import_search_list_memory_budget() {
    let mut r = setup();
    let mut text = String::from("RefDes,X,Y,Rotation,Side,Footprint,Value\n");
    for i in 0..MAX_COMPONENTS {
        text += &format!(
            "R{i},{},{},37,{},0603,v\n",
            i % 1000,
            i / 1000,
            if i % 2 == 0 { "Top" } else { "Bottom" }
        );
    }
    assert!(text.len() < MAX_PNP_BYTES);
    std::fs::write(r.dir.join("pnp.csv"), &text).unwrap();
    let start = std::time::Instant::now();
    let p = params(&r);
    let preview_us = start.elapsed().as_micros();
    let start = std::time::Instant::now();
    r.service
        .components_import_pnp(&r.document, &r.info().revision, p)
        .unwrap();
    let import_us = start.elapsed().as_micros();
    let mut searches = vec![];
    for mode in [
        RefdesMatch::Exact,
        RefdesMatch::Prefix,
        RefdesMatch::Substring,
    ] {
        // Build revision/DTO before timing the query. document_get also hashes
        // project content for dirty status; that distinct work is measured below.
        let query = q(&r, "R99999", mode);
        let start = std::time::Instant::now();
        let page = r.service.components_search(&r.document, &query).unwrap();
        assert_eq!(page.total, 1);
        searches.push(start.elapsed().as_micros());
    }
    let query = q(&r, "", RefdesMatch::Prefix);
    let start = std::time::Instant::now();
    let list = r.service.components_list(&r.document, &query).unwrap();
    let list_us = start.elapsed().as_micros();
    assert_eq!(list.total, 100000);
    assert_eq!(list.items.len(), 500);
    let start = std::time::Instant::now();
    let indices = r.service.component_indices(&r.document, &query).unwrap();
    let index_us = start.elapsed().as_micros();
    assert_eq!(indices.len(), 100000);
    let start = std::time::Instant::now();
    let _ = r.info();
    let document_info_us = start.elapsed().as_micros();
    println!(
        "100k raw preview_us={preview_us} import_us={import_us} search_us={searches:?} list_us={list_us} index_us={index_us} document_info_us={document_info_us}"
    );
    assert!(preview_us < 5_000_000);
    assert!(import_us < 5_000_000);
    assert!(searches.iter().all(|us| *us < 100_000));
    assert!(list_us < 100_000);
    assert!(r.info().history_bytes <= editor_core::edit::MAX_HISTORY_BYTES);
    r.service
        .board_set_registration(
            &r.document,
            &r.info().revision,
            RegistrationInput::Manual {
                transform: Default::default(),
            },
        )
        .unwrap();
    let report = json!({"schema_version":2,"components":100000,"source_bytes":text.len(),"preview_us":preview_us,"import_us":import_us,"search_us":searches,"list_us":list_us,"index_us":index_us,"document_info_us":document_info_us,"query_timing":"service query only; revision/DTO construction and dirty-state hashing timed separately","history_bytes":r.info().history_bytes,"index_bytes":indices.capacity()*std::mem::size_of::<usize>(),"component_payload_budget_bytes":r.service.board_state(&r.document).unwrap().unwrap().history_bytes(),"thresholds":{"import_us":5000000,"query_us":100000}});
    println!("{report}");
    if let Some(dir) = std::env::var_os("RCAM_S4D1_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::PathBuf::from(dir).join("performance.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    text += "overflow,0,0,0,Top,a,b\n";
    assert!(!parse_pnp(text.as_bytes(), &mapping()).valid());
}
