//! Fixed S5-M2-C manufacturing truth and baseline measurements.
use crate::{
    camera::Camera,
    gpu,
    state::{Action, Model},
};
use editor_core::{
    ApertureShape, ArcDirection, BoundsMm, Exposure, MmPoint, RegionEdge, RegionRole,
    SemanticGeometry,
};
use std::{path::PathBuf, sync::Arc, time::Instant};

pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s5m2c")
        .join(name)
}
fn checked(m: &Model) {
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert!(m.view.blocked.is_none(), "{:?}", m.view.blocked);
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn point(p: MmPoint, x: f64, y: f64) {
    near(p.x_mm, x);
    near(p.y_mm, y);
}
const CONVEX: [(i32, i32); 16] = [
    (320, 0),
    (296, 122),
    (226, 226),
    (122, 296),
    (0, 320),
    (-122, 296),
    (-226, 226),
    (-296, 122),
    (-320, 0),
    (-296, -122),
    (-226, -226),
    (-122, -296),
    (0, -320),
    (122, -296),
    (226, -226),
    (296, -122),
];
const CONCAVE: [(i32, i32); 16] = [
    (-300, -300),
    (-100, -300),
    (-100, -200),
    (100, -200),
    (100, -300),
    (300, -300),
    (300, 300),
    (100, 300),
    (100, 200),
    (-100, 200),
    (-100, 300),
    (-300, 300),
    (-300, 100),
    (-200, 100),
    (-200, -100),
    (-300, -100),
];

fn open() -> Model {
    let mut m = Model::default();
    m.open(&fixture("PMIX.gbr")).unwrap();
    checked(&m);
    m
}

#[test]
fn pmix_full_fixture_has_independent_type_coordinate_and_edge_truth() {
    let protocol: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/synthetic/s5m2c/protocol.json"
    ))
    .unwrap();
    let bytes = std::fs::read(fixture("PMIX.gbr")).unwrap();
    assert_eq!(
        editor_core::hash::sha256_hex(&bytes),
        protocol["fixture"]["sha256"]
    );
    let start = Instant::now();
    let m = open();
    let load_ms = start.elapsed().as_secs_f64() * 1000.;
    let s = m.view.snap_snapshot.as_ref().unwrap();
    assert_eq!(s.layers.len(), 1);
    let objects = &s.layers[0].objects;
    assert_eq!(objects.len(), 100000);
    let mut counts = [0usize; 4];
    let mut edges = 0;
    for (i, o) in objects.iter().enumerate() {
        assert_eq!(o.object_id, format!("object-{}", i + 1));
        assert_eq!(o.exposure, Exposure::Dark);
        let x = (i % 400 + 1) as f64;
        let y = (i / 400 + 1) as f64;
        match (&o.geometry, i % 10) {
            (
                SemanticGeometry::Flash {
                    center,
                    aperture_id,
                    transform,
                },
                k @ 0..=3,
            ) => {
                counts[0] += 1;
                point(*center, x, y);
                assert_eq!(*transform, Default::default());
                let shape = &s
                    .apertures
                    .iter()
                    .find(|a| &a.id == aperture_id)
                    .unwrap()
                    .shape;
                let expected = match k {
                    0 => ApertureShape::Circle {
                        diameter_mm: 0.5,
                        hole_diameter_mm: None,
                    },
                    1 => ApertureShape::Rectangle {
                        width_mm: 0.6,
                        height_mm: 0.4,
                        hole_diameter_mm: None,
                    },
                    2 => ApertureShape::Obround {
                        width_mm: 0.6,
                        height_mm: 0.3,
                        hole_diameter_mm: None,
                    },
                    _ => ApertureShape::Polygon {
                        diameter_mm: 0.6,
                        vertices: 6,
                        rotation_deg: 0.,
                        hole_diameter_mm: None,
                    },
                };
                assert_eq!(*shape, expected);
            }
            (
                SemanticGeometry::Line {
                    start,
                    end,
                    width_mm,
                },
                k @ 4..=6,
            ) => {
                counts[1] += 1;
                near(*width_mm, 0.1);
                let (dx, dy) = [(0.3, 0.), (0., 0.3), (0.25, 0.2)][k - 4];
                point(*start, x - dx, y - dy);
                point(*end, x + dx, y + dy);
            }
            (SemanticGeometry::Arc { path, width_mm }, k @ 7..=8) => {
                counts[2] += 1;
                near(*width_mm, 0.1);
                point(path.start, x + 0.3, y);
                point(path.end, x, y + 0.3);
                point(path.center, x, y);
                assert_eq!(
                    path.direction,
                    if k == 7 {
                        ArcDirection::CounterClockwise
                    } else {
                        ArcDirection::Clockwise
                    }
                );
                assert!(!path.full_circle);
                assert!(!path.source.unwrap().single_quadrant);
            }
            (SemanticGeometry::Region { contours }, 9) => {
                counts[3] += 1;
                assert_eq!(contours.len(), 1);
                assert_eq!(contours[0].role, RegionRole::Solid);
                assert_eq!(contours[0].edges.len(), 16);
                let template = if (i / 10) % 2 == 0 { CONVEX } else { CONCAVE };
                for (j, e) in contours[0].edges.iter().enumerate() {
                    let RegionEdge::Line { start, end } = e else {
                        panic!("fixed straight Region edge")
                    };
                    let (a, b) = template[j];
                    let (c, d) = template[(j + 1) % 16];
                    point(*start, x + f64::from(a) / 1000., y + f64::from(b) / 1000.);
                    point(*end, x + f64::from(c) / 1000., y + f64::from(d) / 1000.);
                    edges += 1;
                }
            }
            _ => panic!("wrong frozen geometry at {i}: {:?}", o.geometry),
        }
    }
    assert_eq!(counts, [40000, 30000, 20000, 10000]);
    assert_eq!(edges, 160000);
    let b = m.view.bounds.unwrap();
    for (a, b) in [b.min_x_mm, b.min_y_mm, b.max_x_mm, b.max_y_mm]
        .into_iter()
        .zip([0.75, 0.65, 400.3, 250.35])
    {
        near(a, b);
    }
    let scene = m.view.scene.as_ref().unwrap();
    assert_eq!(scene.ids.len(), 100000);
    println!(
        "PMIX_BASELINE {}",
        serde_json::json!({"load_model_ms":load_ms,"objects":counts,"manufacturing_region_edges":edges,"display_primitives":scene.primitives.len(),"display_points":scene.points.len(),"object_mesh_triangles":0,"production_fullscreen_triangles_per_draw":1,"note":"CPU model baseline, not native load-to-visible or frame rate"})
    );
}

#[test]
#[ignore = "release PMIX baseline; actual service move/undo/redo and immutable GPU preparation"]
fn pmix_release_baseline_1000_mixed_transaction() {
    let mut m = open();
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: 0.5,
            min_y_mm: 0.5,
            max_x_mm: 20.5,
            max_y_mm: 50.5,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    checked(&m);
    assert_eq!(m.view.selected.ordered.len(), 1000);
    let expected: Vec<_> = (0..50)
        .flat_map(|row| (0..20).map(move |col| format!("object-{}", row * 400 + col + 1)))
        .collect();
    assert_eq!(
        m.view
            .selected
            .ordered
            .iter()
            .map(|x| x.object.object_id.clone())
            .collect::<Vec<_>>(),
        expected
    );
    let before = m.view.snap_snapshot.clone().unwrap();
    let info = m.view.info.clone().unwrap();
    let scene = m.view.scene.clone().unwrap();
    let rect =
        eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(800., 450.));
    let camera = Camera {
        center: MmPoint::new(200.5, 125.5),
        scale: 1.656,
    };
    let flags = gpu::selection_flags(&scene, &m.view.selected.ids());
    let mut timings = vec![];
    for i in 0..30 {
        let r = gpu::prepare_measured(
            &scene,
            camera,
            rect,
            2.,
            &flags,
            MmPoint::new(i as f64 / 3., -2.),
        )
        .unwrap();
        assert!(Arc::ptr_eq(&r.index, &scene.index));
        timings.push(r.stats.cpu_prepare_ms);
    }
    assert_eq!(m.view.info.as_ref().unwrap(), &info);
    assert!(Arc::ptr_eq(m.view.snap_snapshot.as_ref().unwrap(), &before));
    let start = Instant::now();
    m.run(Action::Move("1.25".into(), "-0.75".into()));
    checked(&m);
    let move_ms = start.elapsed().as_secs_f64() * 1000.;
    let moved = m.view.snap_snapshot.clone().unwrap();
    assert_eq!(
        m.view.info.as_ref().unwrap().undo_entries,
        info.undo_entries + 1
    );
    for (i, (a, b)) in before.layers[0]
        .objects
        .iter()
        .zip(&moved.layers[0].objects)
        .enumerate()
    {
        let mut expected = a.clone();
        if i / 400 < 50 && i % 400 < 20 {
            translate(&mut expected.geometry, 1.25, -0.75);
        }
        assert_eq!(&expected, b, "object {i}");
    }
    let start = Instant::now();
    m.run(Action::History(false));
    checked(&m);
    let undo_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().apertures,
        before.apertures
    );
    let start = Instant::now();
    m.run(Action::History(true));
    checked(&m);
    let redo_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, moved.layers);
    println!(
        "PMIX_BASELINE {}",
        serde_json::json!({"move_worker_ms":move_ms,"undo_worker_ms":undo_ms,"redo_worker_ms":redo_ms,"prepare_ms":timings,"note":"CPU/worker measurements only; native E2E gates still required"})
    );
}
fn translate(g: &mut SemanticGeometry, dx: f64, dy: f64) {
    let p = |p: &mut MmPoint| {
        p.x_mm += dx;
        p.y_mm += dy;
    };
    match g {
        SemanticGeometry::Flash { center, .. } => p(center),
        SemanticGeometry::Line { start, end, .. } => {
            p(start);
            p(end);
        }
        SemanticGeometry::Arc { path, .. } => {
            p(&mut path.start);
            p(&mut path.end);
            p(&mut path.center);
        }
        SemanticGeometry::Region { contours } => {
            for c in contours {
                for e in &mut c.edges {
                    match e {
                        RegionEdge::Line { start, end } => {
                            p(start);
                            p(end);
                        }
                        _ => panic!("fixed straight Region"),
                    }
                }
            }
        }
        _ => panic!("fixed PMIX types"),
    }
}

pub(crate) fn workflow_project() -> rcam_project::RCamProject {
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    checked(&m);
    m.run(Action::ImportGerbers(vec![
        fixture("MIX_BASE.gbr"),
        fixture("MIX_UPPER.gbr"),
    ]));
    checked(&m);
    let base = m
        .view
        .snap_snapshot
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .find(|l| l.objects.len() == 15)
        .unwrap();
    let layer = base.id.clone();
    let ids = base.objects[..10]
        .iter()
        .map(|o| o.object_id.clone())
        .collect();
    m.run(Action::SetActiveLayer(Some(layer.clone())));
    checked(&m);
    let request = crate::block_ui::Request {
        context: crate::block_ui::Context::capture(&m.view).unwrap(),
        edit: crate::block_ui::Edit::Create(editor_service::CreateBlockDefinitionParams {
            layer_id: layer.clone(),
            object_ids: ids,
            local_origin_mm: editor_service::PivotMm { x_mm: 0., y_mm: 0. },
            name: "PMIX tile".into(),
        }),
    };
    m.run(Action::BlockEdit(Box::new(request)));
    checked(&m);
    assert_eq!(m.view.block_definitions.len(), 1);
    let definition = m.view.block_definitions[0].id.clone();
    for (x, angle, mirror) in [(40., 0., false), (80., 37., false), (120., 90., true)] {
        let request = crate::block_ui::Request {
            context: crate::block_ui::Context::capture(&m.view).unwrap(),
            edit: crate::block_ui::Edit::Place(editor_service::CreateBlockInstanceParams {
                layer_id: layer.clone(),
                definition_id: definition.0.clone(),
                transform: editor_service::BlockTransformParams {
                    translation_mm: editor_service::PivotMm { x_mm: x, y_mm: 0. },
                    rotation_deg: angle,
                    mirror,
                },
            }),
        };
        m.run(Action::BlockEdit(Box::new(request)));
        checked(&m);
    }
    let mut draft = crate::text_tool::Draft {
        text: "PMIX 8B".into(),
        x: "0".into(),
        y: "18".into(),
        ..Default::default()
    };
    draft.accept_font(editor_service::builtin_stroke_font());
    let info = m.view.info.as_ref().unwrap();
    let request = crate::text_tool::Request {
        generation: 0,
        document: info.document_id.clone(),
        revision: info.revision.clone(),
        params: draft.params(&layer).unwrap(),
    };
    m.run(Action::TextCreate(request));
    checked(&m);
    let id = &m.view.info.as_ref().unwrap().document_id;
    let mut project = m.service.project_snapshot(id).unwrap();
    // Deterministic test artifact metadata only; never modify a live document.
    project.project_id = rcam_project::ProjectId("project-0000000000000000000000000000c053".into());
    for layer in &mut project.layers {
        if let Some(p) = &mut layer.provenance {
            p.imported_at = "2000-01-01T00:00:00Z".into();
        }
    }
    project
}
#[test]
#[ignore = "explicit deterministic fixture construction; output path required"]
fn pmix_generate_workflow_fixture() {
    let a = workflow_project();
    let b = workflow_project();
    assert_eq!(
        a, b,
        "fixture generation must be deterministic after identity/time normalization only"
    );
    let bytes = rcam_project::encode_v1(&a).unwrap();
    assert_eq!(rcam_project::decode(&bytes).unwrap(), a);
    let output = PathBuf::from(
        std::env::var_os("RCAM_PMIX_FIXTURE_OUT").expect("explicit local fixture path"),
    );
    if output.exists() {
        assert_eq!(
            std::fs::read(&output).unwrap(),
            bytes,
            "frozen fixture differs"
        );
    } else {
        std::fs::write(&output, &bytes).unwrap();
    }
    println!(
        "PMIX_WORKFLOW {}",
        serde_json::json!({"sha256":editor_core::hash::sha256_hex(&bytes),"bytes":bytes.len(),"layers":a.layers.iter().map(|l|(&l.layer.id,l.layer.objects.len())).collect::<Vec<_>>(),"definitions":a.block_definitions.len()})
    );
}

#[test]
fn pmix_frozen_point_witnesses_full_box_and_region_area() {
    let mut m = open();
    let before = m.view.info.clone().unwrap();
    let scene = m.view.scene.clone().unwrap();
    let protocol: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/synthetic/s5m2c/protocol.json"
    ))
    .unwrap();
    let mut timings = vec![];
    for (i, p) in protocol["points"].as_array().unwrap().iter().enumerate() {
        let a = &p["position_mm"];
        let now = Instant::now();
        m.run(Action::Select(
            MmPoint::new(a[0].as_f64().unwrap(), a[1].as_f64().unwrap()),
            0.,
            crate::selection::SelectionMode::Replace,
        ));
        timings.push(now.elapsed().as_secs_f64() * 1000.);
        checked(&m);
        let expected: Vec<&str> = p["expected_id"].as_str().into_iter().collect();
        assert_eq!(m.view.selected.ids(), expected, "point witness {i}");
        assert!(Arc::ptr_eq(&scene, m.view.scene.as_ref().unwrap()));
        assert_eq!(m.view.info.as_ref().unwrap().revision, before.revision);
        assert_eq!(
            m.view.info.as_ref().unwrap().undo_entries,
            before.undo_entries
        );
    }
    let now = Instant::now();
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: 0.,
            min_y_mm: 0.,
            max_x_mm: 401.,
            max_y_mm: 251.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    let box_ms = now.elapsed().as_secs_f64() * 1000.;
    checked(&m);
    assert_eq!(m.view.selected.ordered.len(), 100000);
    for (i, id) in m.view.selected.ids().iter().enumerate() {
        assert_eq!(*id, format!("object-{}", i + 1));
    }
    // Integer shoelace oracle, independent of the production Region area function.
    let area = |v: &[(i32, i32); 16]| -> i64 {
        v.iter()
            .zip(v.iter().cycle().skip(1))
            .map(|(&(a, b), &(c, d))| i64::from(a) * i64::from(d) - i64::from(b) * i64::from(c))
            .sum()
    };
    assert_eq!(area(&CONVEX), 626912);
    assert_eq!(area(&CONCAVE), 600000);
    println!(
        "PMIX_QUERY_BASELINE {}",
        serde_json::json!({"model_selection_ms":timings,"box_model_ms":box_ms,"scope":"real synchronous Model worker, not CPU query-only or input-to-highlight"})
    );
}

fn temp(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rcam-pmix-test-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    dir
}

#[test]
fn pmix_workflow_project_view_edit_history_and_safe_export_roundtrip() {
    let freeze: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/synthetic/s5m2c/workflow-freeze.json"
    ))
    .unwrap();
    let file = fixture("MIX_WORKFLOW.rcam");
    assert_eq!(
        editor_core::hash::sha256_hex(&std::fs::read(&file).unwrap()),
        freeze["sha256"]
    );
    let mut m = Model::default();
    m.run(Action::OpenProject(file, true));
    checked(&m);
    let initial = m.view.snap_snapshot.clone().unwrap();
    assert_eq!(
        initial
            .layers
            .iter()
            .map(|l| l.objects.len())
            .collect::<Vec<_>>(),
        vec![2, 49]
    );
    assert_eq!(initial.block_definitions.len(), 1);
    let base = initial
        .layers
        .iter()
        .find(|l| l.id == "layer-1")
        .unwrap()
        .id
        .clone();
    let dir = temp("workflow");
    m.run(Action::Save(dir.join("before.gbr"), base.clone(), None));
    checked(&m);
    let export_before = std::fs::read(dir.join("before.gbr")).unwrap();
    let revision = m.view.info.as_ref().unwrap().revision.clone();
    for action in [
        Action::SetAllLayersVisible(false),
        Action::SetAllLayersVisible(true),
        Action::SetSoloLayer(Some(base.clone())),
        Action::SetSoloLayer(None),
        Action::FitLayer(base.clone()),
    ] {
        m.run(action);
        checked(&m);
        assert_eq!(m.view.info.as_ref().unwrap().revision, revision);
        assert_eq!(
            m.view.snap_snapshot.as_ref().unwrap().layers,
            initial.layers
        );
    }
    m.run(Action::Save(dir.join("after-view.gbr"), base.clone(), None));
    checked(&m);
    assert_eq!(
        std::fs::read(dir.join("after-view.gbr")).unwrap(),
        export_before
    );
    m.run(Action::SetActiveLayer(Some(base.clone())));
    checked(&m);
    let definition = initial.block_definitions[0].id.0.clone();
    m.run(Action::BlockSelect(definition));
    checked(&m);
    assert_eq!(m.view.selected.ordered.len(), 4);
    let before = m.view.snap_snapshot.clone().unwrap();
    let undo = m.view.info.as_ref().unwrap().undo_entries;
    m.run(Action::Move("2".into(), "-1".into()));
    checked(&m);
    let moved = m.view.snap_snapshot.clone().unwrap();
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, undo + 1);
    for (a, b) in before
        .layers
        .iter()
        .find(|l| l.id == base)
        .unwrap()
        .objects
        .iter()
        .zip(&moved.layers.iter().find(|l| l.id == base).unwrap().objects)
    {
        let mut expected = a.clone();
        if let SemanticGeometry::BlockInstance { transform, .. } = &mut expected.geometry {
            transform.translation.x_mm += 2.;
            transform.translation.y_mm -= 1.;
        }
        assert_eq!(&expected, b);
    }
    assert_eq!(moved.block_definitions, before.block_definitions);
    m.run(Action::Rotate(
        "37".into(),
        crate::state::PivotInput::WorldOrigin,
    ));
    checked(&m);
    let rotated = m.view.snap_snapshot.clone().unwrap();
    assert_eq!(rotated.block_definitions, before.block_definitions);
    m.run(Action::History(false));
    checked(&m);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, moved.layers);
    m.run(Action::History(false));
    checked(&m);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
    m.run(Action::History(true));
    checked(&m);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, moved.layers);
    m.run(Action::History(true));
    checked(&m);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers,
        rotated.layers
    );
    let info = m.view.info.as_ref().unwrap();
    m.run(Action::Layer(editor_service::LayerUpdateParams {
        layer_id: base.clone(),
        expected_workspace_revision: info.workspace_revision.clone(),
        locked: Some(true),
        ..Default::default()
    }));
    checked(&m);
    let locked = m.view.snap_snapshot.clone().unwrap();
    let info = m.view.info.clone().unwrap();
    m.run(Action::Move("10".into(), "0".into()));
    assert!(m.view.error.is_some());
    assert_eq!(m.view.info.as_ref().unwrap(), &info);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, locked.layers);
    m.run(Action::Layer(editor_service::LayerUpdateParams {
        layer_id: base.clone(),
        expected_workspace_revision: info.workspace_revision,
        locked: Some(false),
        ..Default::default()
    }));
    checked(&m);
    m.run(Action::SaveProject(
        Some(dir.join("edited.rcam")),
        false,
        None,
    ));
    checked(&m);
    let saved = m
        .service
        .project_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    m.run(Action::OpenProject(dir.join("edited.rcam"), true));
    checked(&m);
    let reopened = m
        .service
        .project_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    assert_eq!(saved, reopened);
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 0);
    for (i, l) in reopened.layers.iter().enumerate() {
        let path = dir.join(format!("layer-{i}.gbr"));
        m.run(Action::Save(path.clone(), l.layer.id.clone(), None));
        checked(&m);
        let mut reopened = Model::default();
        reopened.open(&path).unwrap();
        checked(&reopened);
        let snap = reopened.view.snap_snapshot.as_ref().unwrap();
        assert_eq!(snap.layers.len(), 1);
        assert!(snap.block_definitions.is_empty());
        assert!(
            !snap.layers[0]
                .objects
                .iter()
                .any(|o| matches!(o.geometry, SemanticGeometry::BlockInstance { .. }))
        );
        assert_eq!(snap.layers[0].objects.len(), if i == 0 { 85 } else { 2 });
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pmix_native_marquee_margin_has_independent_hit_boundary_and_full_id_oracle() {
    let mut m = open();
    let camera = Camera {
        center: MmPoint::new(200.5, 125.5),
        scale: 1.656,
    };
    let rect =
        eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(800., 450.));
    let tolerance = camera.tolerance(2.);
    // Left boundary of the first C Flash is x=.75. No other cell can be nearer.
    for (x, hit) in [
        (0.75 - tolerance - 0.00001, false),
        (0.75 - tolerance + 0.00001, true),
    ] {
        m.run(Action::ProbeDrag(MmPoint::new(x, 1.), tolerance));
        checked(&m);
        assert_eq!(
            m.view.press_hit.is_some(),
            hit,
            "analytic material+tolerance boundary x={x}"
        );
    }
    let old_start = camera.screen(MmPoint::new(0.5, 0.5), rect);
    let mut gesture = crate::drag::Gesture::arm(
        &m.view,
        old_start,
        camera,
        rect,
        2.,
        crate::selection::SelectionMode::Replace,
    );
    m.run(Action::ProbeDrag(camera.world(old_start, rect), tolerance));
    checked(&m);
    assert!(
        m.view.press_hit.is_some(),
        "the initial protocol's press is correctly an object hit"
    );
    gesture.confirm(&m.view);
    gesture.update(camera.screen(MmPoint::new(400.5, 250.5), rect));
    assert!(
        gesture.preview_rect().is_none(),
        "an object press cannot turn into a blank marquee"
    );
    assert!(
        gesture.release().is_none(),
        "accepted I1 ignores a moved non-dragging object press; no click or marquee is committed"
    );
    let new_start = camera.screen(MmPoint::new(-3., -3.), rect);
    let new_end = camera.screen(MmPoint::new(403., 253.), rect);
    assert!(
        rect.contains(new_start) && rect.contains(new_end),
        "gesture remains in the frozen canvas"
    );
    let mut gesture = crate::drag::Gesture::arm(
        &m.view,
        new_start,
        camera,
        rect,
        2.,
        crate::selection::SelectionMode::Replace,
    );
    m.run(Action::ProbeDrag(camera.world(new_start, rect), tolerance));
    checked(&m);
    assert!(
        m.view.press_hit.is_none(),
        "new press is independent empty material beyond hit radius"
    );
    gesture.confirm(&m.view);
    gesture.update(new_end);
    let action = gesture.release().unwrap();
    assert!(matches!(
        action,
        Action::SelectRect(_, editor_core::hit_test::SelectRectMode::Window)
    ));
    m.run(action);
    checked(&m);
    let expected: Vec<_> = (1..=100000).map(|i| format!("object-{i}")).collect();
    assert_eq!(
        m.view.selected.ids(),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(m.view.info.as_ref().unwrap().revision, "0");
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 0);
}

fn coverage_document(s: &editor_service::RenderSnapshot) -> editor_core::SemanticDocument {
    editor_core::SemanticDocument {
        id: s.document_id.clone(),
        unit: "mm".into(),
        format: editor_core::SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        },
        layers: s.layers.clone(),
        apertures: s.apertures.clone(),
        source: Default::default(),
        block_definitions: s.block_definitions.clone(),
    }
}
#[test]
fn pmix_local_hole_and_ordered_polarity_have_independent_material_witnesses() {
    let mut m = Model::default();
    m.run(Action::OpenProject(fixture("MIX_WORKFLOW.rcam"), true));
    checked(&m);
    let witnesses = [
        (10., 12., true),
        (11., 12., false),
        (11.8, 12., true),
        (20., 12., true),
        (20., 12.2, false),
        (21., 12., true),
    ];
    let doc = coverage_document(m.view.snap_snapshot.as_ref().unwrap());
    for (x, y, expected) in witnesses {
        assert_eq!(
            doc.layer_coverage_at("layer-1", MmPoint::new(x, y)),
            Some(expected),
            "ordered/local-hole witness {x},{y}"
        );
    }
    assert_eq!(
        doc.layer_coverage_at("layer-2", MmPoint::new(10., 12.)),
        Some(false)
    );
    assert_eq!(
        doc.layer_coverage_at("layer-1", MmPoint::new(10., 12.)),
        Some(true),
        "other layer Clear cannot erase base"
    );
    assert_eq!(
        doc.layer_coverage_at("layer-2", MmPoint::new(45., 12.)),
        Some(true)
    );
    let dir = temp("coverage");
    let path = dir.join("base.gbr");
    m.run(Action::Save(path.clone(), "layer-1".into(), None));
    checked(&m);
    let mut reopened = Model::default();
    reopened.open(&path).unwrap();
    checked(&reopened);
    let exported = coverage_document(reopened.view.snap_snapshot.as_ref().unwrap());
    for (x, y, expected) in witnesses {
        assert_eq!(
            exported.layer_coverage_at(&exported.layers[0].id, MmPoint::new(x, y)),
            Some(expected),
            "independent export witness {x},{y}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pmix_cross_layer_workflow_move_is_atomic_and_undo_exact() {
    let mut m = Model::default();
    m.run(Action::OpenProject(fixture("MIX_WORKFLOW.rcam"), true));
    checked(&m);
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -200.,
            min_y_mm: -200.,
            max_x_mm: 300.,
            max_y_mm: 300.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    checked(&m);
    assert_eq!(m.view.selected.ordered.len(), 51);
    assert_eq!(
        m.view
            .selected
            .ordered
            .iter()
            .map(|o| &o.layer_id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2
    );
    let before = m.view.snap_snapshot.clone().unwrap();
    let info = m.view.info.clone().unwrap();
    m.run(Action::Move("1.25".into(), "-0.75".into()));
    checked(&m);
    let moved = m.view.snap_snapshot.clone().unwrap();
    assert_eq!(
        m.view.info.as_ref().unwrap().undo_entries,
        info.undo_entries + 1
    );
    assert_eq!(
        m.view
            .info
            .as_ref()
            .unwrap()
            .revision
            .parse::<u64>()
            .unwrap(),
        info.revision.parse::<u64>().unwrap() + 1
    );
    assert_eq!(moved.apertures, before.apertures);
    assert_eq!(moved.block_definitions, before.block_definitions);
    for (a, b) in before.layers.iter().zip(&moved.layers) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.objects.len(), b.objects.len());
        for (source, result) in a.objects.iter().zip(&b.objects) {
            let mut expected = source.clone();
            match &mut expected.geometry {
                SemanticGeometry::BlockInstance { transform, .. } => {
                    transform.translation.x_mm += 1.25;
                    transform.translation.y_mm -= 0.75;
                }
                geometry => translate(geometry, 1.25, -0.75),
            }
            assert_eq!(&expected, result);
        }
    }
    m.run(Action::History(false));
    checked(&m);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().block_definitions,
        before.block_definitions
    );
    m.run(Action::History(true));
    checked(&m);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, moved.layers);
    let workspace = m.view.info.as_ref().unwrap().workspace_revision.clone();
    m.run(Action::Layer(editor_service::LayerUpdateParams {
        layer_id: "layer-2".into(),
        expected_workspace_revision: workspace,
        locked: Some(true),
        ..Default::default()
    }));
    checked(&m);
    let locked = m.view.snap_snapshot.clone().unwrap();
    let info = m.view.info.clone().unwrap();
    m.run(Action::Move("10".into(), "0".into()));
    assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
    assert_eq!(m.view.info.as_ref().unwrap(), &info);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, locked.layers);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().block_definitions,
        locked.block_definitions
    );
}
