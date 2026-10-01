use crate::selection::SelectionMode::{Add, Remove, Replace};
use crate::state::{Action, MirrorDirection, Model, PivotInput};
use editor_core::{MmPoint, SemanticGeometry};
use editor_service::LayerUpdateParams;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn display_work_keeps_controls_enabled_and_accepts_one_user_command() {
    let mut app = crate::modal::tests::app();
    let (tx, requests) = std::sync::mpsc::sync_channel(2);
    app.tx = tx;
    let bounds = editor_core::BoundsMm {
        min_x_mm: 0.,
        min_y_mm: 0.,
        max_x_mm: 10.,
        max_y_mm: 10.,
    };
    app.send(Action::Viewport(MmPoint::new(5., 5.), bounds, 16.));
    assert!(!app.busy);
    assert_eq!(app.viewport_sequence, Some(1));
    app.send(Action::Viewport(MmPoint::new(5., 5.), bounds, 16.));
    assert_eq!(app.sequence, 1, "one pending display request");
    app.send(Action::Select(MmPoint::new(5., 5.), 0.01, Replace));
    assert!(app.busy);
    assert_eq!(app.sequence, 2);
    assert!(matches!(
        requests.try_recv().unwrap().2,
        Action::Viewport(..)
    ));
    assert!(matches!(requests.try_recv().unwrap().2, Action::Select(..)));
    assert!(requests.try_recv().is_err());
}

#[test]
fn unchanged_workspace_reuses_scene_and_failed_camera_attempt_settles() {
    let (mut m, _) = setup();
    let scene = m.view.scene.clone().unwrap();
    let id = m.view.layers[0].layer_id.clone();
    m.run(Action::SetActiveLayer(Some(id)));
    patch(&mut m, None, Some(true), Some("renamed".into()));
    assert!(std::sync::Arc::ptr_eq(
        &scene,
        m.view.scene.as_ref().unwrap()
    ));
    let bounds = m.view.bounds.unwrap();
    m.run(Action::Viewport(MmPoint::new(1e18, 1e18), bounds, 128.));
    assert!(m.view.display_transient.is_some());
    let (attempt, ppm) = m.view.display_attempt.unwrap();
    let lo = MmPoint::new(bounds.min_x_mm, bounds.min_y_mm);
    let hi = MmPoint::new(bounds.max_x_mm, bounds.max_y_mm);
    assert!(crate::display::covers_view(attempt, ppm, lo, hi, 64.));
    assert!(!crate::display::covers_view(attempt, ppm, lo, hi, 256.));
    assert!(!crate::display::covers_view(
        attempt,
        ppm,
        MmPoint::new(lo.x_mm - 1., lo.y_mm),
        hi,
        64.
    ));
    m.run(Action::Viewport(bounds.center(), bounds, 32.));
    assert!(m.view.blocked.is_none());
    assert!(m.view.display_transient.is_none());
}

#[test]
#[ignore = "requires private project"]
fn real_large_point_selection_regression() {
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("RCAM_LARGE_INPUTS").unwrap()).unwrap(),
    )
    .unwrap();
    let mut m = Model::default();
    m.open_project(
        std::path::Path::new(manifest["project"].as_str().unwrap()),
        true,
    )
    .unwrap();
    let before = m.view.info.clone();
    for (x, y) in [
        (110.964, 199.782),
        (112.754, 244.521),
        (102.950, 244.444),
        (78.5394, 90.9010),
    ] {
        for tol in [0., 0.05447, 0.5898355194907762] {
            m.run(Action::ProbeDrag(MmPoint::new(x, y), tol));
            eprintln!("PRESS_REPRO {x} {y} tol={tol} error={:?}", m.view.error);
            assert!(m.view.error.is_none());
            m.run(Action::Select(MmPoint::new(x, y), tol, Replace));
            eprintln!(
                "POINT_REPRO {x} {y} selected={:?} error={:?}",
                m.view
                    .selected
                    .ordered
                    .iter()
                    .map(|o| &o.object.object_id)
                    .collect::<Vec<_>>(),
                m.view.error
            );
            assert!(m.view.error.is_none());
            assert_eq!(m.view.info, before);
            assert!(!m.view.info.as_ref().unwrap().project_dirty);
        }
    }
}

/// Private inputs stay outside the source tree; exercise the same worker model
/// as the UI and retain phase timings rather than timing a substitute parser.
#[test]
#[ignore = "requires explicit private input manifest and release profiling"]
fn real_large_workspace_profile() {
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("RCAM_LARGE_INPUTS").expect("RCAM_LARGE_INPUTS")).unwrap(),
    )
    .unwrap();
    for kind in ["set", "project"] {
        let mut m = Model::default();
        let start = std::time::Instant::now();
        let result = if kind == "set" {
            let paths: Vec<_> = manifest["set"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| PathBuf::from(p.as_str().unwrap()))
                .collect();
            m.import_gerbers(&paths)
        } else {
            m.open_project(
                std::path::Path::new(manifest["project"].as_str().unwrap()),
                true,
            )
        };
        eprintln!(
            "PROFILE {kind} load_ms={} result={result:?} blocked={:?}",
            start.elapsed().as_secs_f64() * 1000.,
            m.view.blocked
        );
        assert!(result.is_ok(), "real input must load: {result:?}");
        assert!(m.view.blocked.is_none(), "{:?}", m.view.blocked);
        let doc = m.view.info.as_ref().unwrap().document_id.clone();
        let start = std::time::Instant::now();
        let snapshot = m.service.render_snapshot(&doc).unwrap();
        eprintln!(
            "PROFILE {kind} snapshot_ms={} objects={} apertures={}",
            start.elapsed().as_secs_f64() * 1000.,
            snapshot
                .layers
                .iter()
                .map(|l| l.objects.len())
                .sum::<usize>(),
            snapshot.apertures.len()
        );
        for layer in &snapshot.layers {
            let mut single = snapshot.clone();
            single.layers = vec![layer.clone()];
            let start = std::time::Instant::now();
            let scene =
                crate::display::Scene::build(&single, &m.view.layers, MmPoint::new(0., 0.), 20., 0);
            eprintln!(
                "PROFILE {kind} layer={} objects={} scene_ms={} counts={:?}",
                layer.id,
                layer.objects.len(),
                start.elapsed().as_secs_f64() * 1000.,
                scene.as_ref().map(|s| (
                    s.primitives.len(),
                    s.points.len(),
                    s.primitives
                        .iter()
                        .filter(|p| matches!(p.meta[0], 1 | 3))
                        .map(|p| p.meta[3] as usize)
                        .sum::<usize>()
                ))
            );
        }
        let rect = eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(770., 713.),
        );
        let mut camera = crate::camera::Camera::default();
        camera.fit(m.view.bounds, rect);
        for zoom in [1., 2., 4., 8., 16.] {
            let c = crate::camera::Camera {
                scale: camera.scale * zoom,
                ..camera
            };
            let lo = c.world(rect.left_bottom(), rect);
            let hi = c.world(rect.right_top(), rect);
            let margin_x = (hi.x_mm - lo.x_mm) * 0.5 + 4. / c.scale;
            let margin_y = (hi.y_mm - lo.y_mm) * 0.5 + 4. / c.scale;
            let coverage = editor_core::BoundsMm {
                min_x_mm: lo.x_mm - margin_x,
                min_y_mm: lo.y_mm - margin_y,
                max_x_mm: hi.x_mm + margin_x,
                max_y_mm: hi.y_mm + margin_y,
            };
            let start = std::time::Instant::now();
            m.run(Action::Viewport(
                c.center,
                coverage,
                2f64.powf(c.scale.log2().ceil()),
            ));
            eprintln!(
                "PROFILE {kind} camera_scene zoom={zoom} ms={} blocked={:?}",
                start.elapsed().as_secs_f64() * 1000.,
                m.view.blocked
            );
            let scene = m.view.scene.as_ref().expect("complete real camera scene");
            let start = std::time::Instant::now();
            let prepared = crate::gpu::prepare_measured(
                scene,
                c,
                rect,
                2.,
                &vec![0; scene.objects.len()],
                MmPoint::new(0., 0.),
            );
            eprintln!(
                "PROFILE {kind} gpu_prepare zoom={zoom} ms={} result={:?}",
                start.elapsed().as_secs_f64() * 1000.,
                prepared.as_ref().map(|p| &p.stats)
            );
            if std::env::var_os("RCAM_LARGE_REQUIRE_PASS").is_some() {
                assert!(prepared.is_ok(), "{kind} zoom={zoom} {:?}", prepared.err());
            }
        }
        let start = std::time::Instant::now();
        let _ = m.service.visible_bounds(&doc).unwrap();
        eprintln!(
            "PROFILE {kind} bounds_ms={}",
            start.elapsed().as_secs_f64() * 1000.
        );
        for ppm in [8., 16., 8.] {
            let start = std::time::Instant::now();
            m.run(Action::Viewport(
                MmPoint::new(0., 0.),
                m.view.bounds.unwrap(),
                ppm,
            ));
            eprintln!(
                "PROFILE {kind} viewport ppm={ppm} ms={} blocked={:?}",
                start.elapsed().as_secs_f64() * 1000.,
                m.view.blocked
            );
        }
        for _ in 0..3 {
            let start = std::time::Instant::now();
            m.run(Action::SetActiveLayer(Some(
                m.view.layers[0].layer_id.clone(),
            )));
            eprintln!(
                "PROFILE {kind} active_ms={}",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
        if let Some(point) = snapshot
            .layers
            .iter()
            .flat_map(|l| &l.objects)
            .find_map(|o| {
                if let SemanticGeometry::Flash {
                    center,
                    aperture_id,
                    ..
                } = &o.geometry
                    && snapshot.apertures.iter().any(|a| {
                        a.id == *aperture_id
                            && matches!(
                                a.shape,
                                editor_core::ApertureShape::Rectangle {
                                    hole_diameter_mm: None,
                                    ..
                                }
                            )
                    })
                {
                    return Some(*center);
                }
                None
            })
        {
            let start = std::time::Instant::now();
            m.run(Action::Select(point, 0.01, Replace));
            eprintln!(
                "PROFILE {kind} select_ms={} count={} error={:?}",
                start.elapsed().as_secs_f64() * 1000.,
                m.view.selected.ordered.len(),
                m.view.error
            );
            if !m.view.selected.ordered.is_empty() {
                let start = std::time::Instant::now();
                let selected = m.view.selected.ordered[0].clone();
                let revision = m.view.info.as_ref().unwrap().revision.clone();
                m.service
                    .objects_move(
                        &doc,
                        &revision,
                        editor_service::MoveParams {
                            layer_id: selected.layer_id,
                            object_ids: m
                                .view
                                .selected
                                .ordered
                                .iter()
                                .map(|o| o.object.object_id.clone())
                                .collect(),
                            dx_mm: 0.1,
                            dy_mm: 0.,
                        },
                    )
                    .unwrap();
                eprintln!(
                    "PROFILE {kind} move_service_ms={}",
                    start.elapsed().as_secs_f64() * 1000.
                );
                let refresh = std::time::Instant::now();
                m.refresh(true).unwrap();
                eprintln!(
                    "PROFILE {kind} move_refresh_ms={} move_ms={}",
                    refresh.elapsed().as_secs_f64() * 1000.,
                    start.elapsed().as_secs_f64() * 1000.
                );
                let start = std::time::Instant::now();
                let revision = m.view.info.as_ref().unwrap().revision.clone();
                m.service.history_undo(&doc, &revision).unwrap();
                eprintln!(
                    "PROFILE {kind} undo_service_ms={}",
                    start.elapsed().as_secs_f64() * 1000.
                );
                let refresh = std::time::Instant::now();
                m.refresh(true).unwrap();
                eprintln!(
                    "PROFILE {kind} undo_refresh_ms={} undo_ms={}",
                    refresh.elapsed().as_secs_f64() * 1000.,
                    start.elapsed().as_secs_f64() * 1000.
                );
                assert!(m.view.error.is_none(), "{:?}", m.view.error);
                let after = m.service.render_snapshot(&doc).unwrap();
                assert_eq!(
                    snapshot.layers, after.layers,
                    "Undo restores all geometry exactly"
                );
                assert_eq!(snapshot.apertures, after.apertures);
                assert_eq!(snapshot.block_definitions, after.block_definitions);
            }
        }
    }
}
#[test]
fn incremental_scene_and_bounds_equal_full_rebuild_after_move_undo() {
    let source = SOURCE
        .replace("%ADD11C,4X2*%", "%ADD11C,4X2*%\n%ADD12R,4X2*%")
        .replace("M02*\n", "D12*\nX30000000Y20000000D03*\nM02*\n");
    let (mut m, _) = setup_source("patch-circle-hole-rectangle.gbr", source.as_bytes());
    let coverage = editor_core::BoundsMm {
        min_x_mm: -100.,
        min_y_mm: -100.,
        max_x_mm: 100.,
        max_y_mm: 100.,
    };
    m.run(Action::Viewport(MmPoint::new(0., 0.), coverage, 32.));
    // The existing C,4X2 fixture has a transparent 2 mm hole at its centre.
    m.run(Action::Select(MmPoint::new(20., 20.), 0., Replace));
    assert!(m.view.selected.ordered.is_empty());
    for point in [
        MmPoint::new(10., 20.),
        MmPoint::new(21.5, 20.),
        MmPoint::new(30., 20.),
    ] {
        m.run(Action::Select(point, 0., Replace));
        let before = m.view.snap_snapshot.clone().unwrap();
        let scene = m.view.scene.clone().unwrap();
        m.run(Action::Move("0.25".into(), "0.125".into()));
        assert!(m.view.error.is_none());
        let after = m.view.snap_snapshot.clone().unwrap();
        let changed = crate::world_index::changed_objects(&after, &before).unwrap();
        let filtered = m.view.snap_index.query(&after, &m.view.layers, coverage);
        let patched = crate::display::Scene::patch(
            &scene,
            &filtered,
            &after,
            &changed,
            &m.view.layers,
            scene.anchor,
            scene.ppm,
            scene.serial + 1,
        )
        .unwrap()
        .unwrap();
        let fresh = crate::display::Scene::build(
            &filtered,
            &m.view.layers,
            scene.anchor,
            scene.ppm,
            patched.serial,
        )
        .unwrap();
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&patched.primitives),
            bytemuck::cast_slice::<_, u8>(&fresh.primitives)
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&patched.objects),
            bytemuck::cast_slice::<_, u8>(&fresh.objects)
        );
        assert_eq!(patched.points, fresh.points);
        assert_eq!(patched.index.data, fresh.index.data);
        assert_eq!(
            m.view.bounds,
            m.service.visible_bounds(&after.document_id).unwrap().bounds
        );
        let rebuilt = crate::world_index::WorldIndex::build(&after).unwrap();
        assert_eq!(
            m.view.snap_index.query_indices(coverage),
            rebuilt.query_indices(coverage)
        );
        m.run(Action::History(false));
        assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
        assert!(!m.view.info.as_ref().unwrap().dirty);
    }
}

#[test]
#[ignore = "private samples and release edit timing"]
fn real_large_edit_speed_profile() {
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("RCAM_LARGE_INPUTS").unwrap()).unwrap(),
    )
    .unwrap();
    for kind in ["set", "project"] {
        let mut m = Model::default();
        if kind == "set" {
            m.import_gerbers(
                &manifest["set"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| PathBuf::from(p.as_str().unwrap()))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        } else {
            m.open_project(
                std::path::Path::new(manifest["project"].as_str().unwrap()),
                true,
            )
            .unwrap();
        }
        let baseline = m.view.snap_snapshot.clone().unwrap();
        let baseline_dirty = m.view.info.as_ref().unwrap().dirty;
        let mut coverage = m.view.bounds.unwrap();
        coverage.min_x_mm -= 10.;
        coverage.min_y_mm -= 10.;
        coverage.max_x_mm += 10.;
        coverage.max_y_mm += 10.;
        m.run(Action::Viewport(coverage.center(), coverage, 8.));
        let layer = baseline
            .layers
            .iter()
            .find(|l| {
                l.objects
                    .iter()
                    .filter(|o| matches!(o.geometry, SemanticGeometry::Line { .. }))
                    .count()
                    >= 1000
            })
            .unwrap();
        for count in [1, 1000] {
            m.view.selected.ordered = layer
                .objects
                .iter()
                .filter(|o| matches!(o.geometry, SemanticGeometry::Line { .. }))
                .take(count)
                .map(|o| editor_service::ObjectInfo {
                    layer_id: layer.id.clone(),
                    object: o.clone(),
                })
                .collect();
            for iteration in 0..5 {
                let start = std::time::Instant::now();
                m.run(Action::Move("0.1".into(), "0".into()));
                let move_ms = start.elapsed().as_secs_f64() * 1000.;
                assert!(m.view.error.is_none(), "{:?}", m.view.error);
                if iteration == 0 {
                    let snapshot = m.view.snap_snapshot.as_ref().unwrap();
                    let filtered = m.view.snap_index.query(snapshot, &m.view.layers, coverage);
                    let scene = m.view.scene.as_ref().unwrap();
                    let fresh = crate::display::Scene::build(
                        &filtered,
                        &m.view.layers,
                        scene.anchor,
                        scene.ppm,
                        scene.serial,
                    )
                    .unwrap();
                    assert_eq!(
                        bytemuck::cast_slice::<_, u8>(&scene.primitives),
                        bytemuck::cast_slice::<_, u8>(&fresh.primitives)
                    );
                    assert_eq!(
                        bytemuck::cast_slice::<_, u8>(&scene.objects),
                        bytemuck::cast_slice::<_, u8>(&fresh.objects)
                    );
                    assert_eq!(scene.points, fresh.points);
                    assert_eq!(scene.index.data, fresh.index.data);
                }
                let start = std::time::Instant::now();
                m.run(Action::History(false));
                let undo_ms = start.elapsed().as_secs_f64() * 1000.;
                assert!(m.view.error.is_none(), "{:?}", m.view.error);
                assert_eq!(m.view.info.as_ref().unwrap().dirty, baseline_dirty);
                let after = m.view.snap_snapshot.as_ref().unwrap();
                assert_eq!(baseline.layers, after.layers);
                assert_eq!(baseline.apertures, after.apertures);
                assert_eq!(baseline.block_definitions, after.block_definitions);
                eprintln!(
                    "EDIT_SPEED kind={kind} objects={count} iteration={iteration} move_ms={move_ms} undo_ms={undo_ms} exact_restore=true"
                );
            }
        }
    }
}

const SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\n%ADD11C,4X2*%\nD10*\nX10000000Y20000000D03*\nD11*\nX20000000Y20000000D03*\nM02*\n";
fn setup_source(name: &str, source: &[u8]) -> (Model, PathBuf) {
    let base = std::env::var_os("RCAM_GUI_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "gui-state-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let mut m = Model::default();
    m.open(&path).unwrap();
    (m, dir)
}
fn setup() -> (Model, PathBuf) {
    setup_source("中文 # source.gbx", SOURCE.as_bytes())
}
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic")
            .join(name),
    )
    .unwrap()
}
fn select_all(m: &mut Model) {
    let document = m.view.info.as_ref().unwrap().document_id.clone();
    let snapshot = m.service.render_snapshot(&document).unwrap();
    let layer = &snapshot.layers[0];
    m.view.selected.ordered = layer
        .objects
        .iter()
        .cloned()
        .map(|object| editor_service::ObjectInfo {
            layer_id: layer.id.clone(),
            object,
        })
        .collect();
}
fn select(m: &mut Model) {
    m.select(MmPoint::new(10., 20.), 0., Replace).unwrap();
    assert!(m.view.selected.primary().is_some());
}
fn patch(m: &mut Model, visible: Option<bool>, locked: Option<bool>, name: Option<String>) {
    let d = m.view.info.as_ref().unwrap();
    let p = LayerUpdateParams {
        layer_id: d.layer_ids[0].clone(),
        expected_workspace_revision: d.workspace_revision.clone(),
        display_name: name,
        visible,
        locked,
        ..Default::default()
    };
    m.run(Action::Layer(p));
    assert!(m.view.error.is_none());
}
/// Gerber export never clears Workspace dirty, so re-opening the exported file
/// replaces the Workspace explicitly (discarding the edits, as a user would confirm).
fn reopen(m: &mut Model, path: &std::path::Path) {
    m.run(Action::Close(true));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    m.open(path).unwrap();
}
fn center(m: &Model) -> MmPoint {
    match m.view.selected.primary().unwrap().object.geometry {
        SemanticGeometry::Flash { center, .. } => center,
        _ => panic!(),
    }
}
#[test]
fn selection_clear_on_empty_click() {
    let (mut m, _) = setup();
    select(&mut m);
    m.select(MmPoint::new(200., 200.), 0., Replace).unwrap();
    assert!(m.view.selected.primary().is_none());
}
#[test]
fn selection_hole_center_not_selected() {
    let (mut m, _) = setup();
    m.select(MmPoint::new(20., 20.), 0.01, Replace).unwrap();
    assert!(m.view.selected.primary().is_none());
}
#[test]
fn selection_skips_hidden_layers() {
    let (mut m, _) = setup();
    select(&mut m);
    patch(&mut m, Some(false), None, None);
    assert!(m.view.selected.primary().is_none());
    m.select(MmPoint::new(10., 20.), 0., Replace).unwrap();
    assert!(m.view.selected.primary().is_none());
}
#[test]
fn selection_allows_locked_but_move_rejected() {
    let (mut m, _) = setup();
    patch(&mut m, None, Some(true), None);
    select(&mut m);
    let before = m.view.info.clone();
    assert_eq!(m.numeric_move("5", "-3").unwrap_err().code, "LAYER_LOCKED");
    assert_eq!(m.view.info, before);
}
#[test]
fn selection_survives_move_undo_redo() {
    let (mut m, _) = setup();
    select(&mut m);
    let id = m.view.selected.primary().unwrap().object.object_id.clone();
    m.numeric_move("5", "-3").unwrap();
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    m.run(Action::History(false));
    assert_eq!(center(&m), MmPoint::new(10., 20.));
    m.run(Action::History(true));
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(m.view.selected.primary().unwrap().object.object_id, id);
}
#[test]
fn workspace_changes_do_not_mark_dirty() {
    let (mut m, _) = setup();
    let before = m.view.info.clone().unwrap();
    patch(&mut m, Some(false), Some(true), Some("钢网 # 1".into()));
    let d = m.view.info.unwrap();
    assert!(!d.dirty);
    assert_eq!(d.revision, before.revision);
    assert_eq!(d.undo_entries, 0);
    assert_eq!(d.workspace_revision, "1");
}
#[test]
fn numeric_move_is_one_history_entry() {
    let (mut m, _) = setup();
    select(&mut m);
    m.numeric_move("5", "-3").unwrap();
    let d = m.view.info.unwrap();
    assert_eq!(d.undo_entries, 1);
    assert_eq!(d.revision, "1");
    assert!(d.dirty);
}
#[test]
fn numeric_zero_move_not_submitted() {
    let (mut m, _) = setup();
    select(&mut m);
    let before = m.view.info.clone();
    m.numeric_move("0", "-0").unwrap();
    assert_eq!(m.view.info, before);
}
#[test]
fn numeric_invalid_not_submitted() {
    let (mut m, _) = setup();
    select(&mut m);
    let before = m.view.info.clone();
    for x in ["", "NaN", "inf", "1e999", "hello"] {
        assert_eq!(m.numeric_move(x, "3").unwrap_err().code, "INVALID_ARGUMENT");
        assert_eq!(m.view.info, before);
    }
}
#[test]
fn export_is_a_copy_it_keeps_the_workspace_dirty_and_unlinked() {
    let (mut m, dir) = setup();
    select(&mut m);
    let source = m.view.info.as_ref().unwrap().source_path.clone();
    m.numeric_move("5", "-3").unwrap();
    let target = dir.join("输出 # 1.gbr");
    let l = m.view.layers[0].layer_id.clone();
    let revision = m.view.info.as_ref().unwrap().revision.clone();
    m.save(&target, l, None).unwrap();
    let d = m.view.info.clone().unwrap();
    assert!(d.dirty, "Gerber export never clears Workspace dirty");
    assert_eq!(d.revision, revision);
    assert_eq!(d.source_path, source, "export creates no source link");
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    reopen(&mut m, &target);
    m.select(MmPoint::new(15., 17.), 0., Replace).unwrap();
    assert_eq!(center(&m), MmPoint::new(15., 17.));
}
#[test]
fn open_failure_keeps_current_document() {
    let (mut m, dir) = setup();
    select(&mut m);
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    let path = dir.join("bad.gbr");
    std::fs::write(&path, "MALFORMED").unwrap();
    assert!(m.open(&path).is_err());
    assert_eq!(m.view.info, before);
    assert_eq!(m.view.selected, selected);
}
#[test]
fn dirty_open_refused_and_close_requires_confirmation() {
    let (mut m, dir) = setup();
    select(&mut m);
    m.numeric_move("1", "0").unwrap();
    let before = m.view.info.clone();
    assert_eq!(
        m.open(&dir.join("中文 # source.gbx")).unwrap_err().code,
        "CONFIRMATION_REQUIRED"
    );
    m.run(Action::Close(false));
    assert_eq!(m.view.info, before);
    m.run(Action::Close(true));
    assert!(m.view.info.is_none());
}
#[test]
fn locked_history_still_restores() {
    let (mut m, _) = setup();
    select(&mut m);
    m.numeric_move("1", "0").unwrap();
    patch(&mut m, None, Some(true), None);
    m.run(Action::History(false));
    assert!(m.view.error.is_none());
    assert_eq!(center(&m), MmPoint::new(10., 20.));
}
#[test]
fn save_failure_preserves_dirty_and_selection() {
    let (mut m, dir) = setup();
    select(&mut m);
    m.numeric_move("1", "0").unwrap();
    let before = m.view.info.clone();
    let target = dir.join("exists.gbr");
    std::fs::write(&target, "keep").unwrap();
    let layer = m.view.layers[0].layer_id.clone();
    assert!(m.save(&target, layer, None).is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
    assert_eq!(m.view.info, before);
    assert!(m.view.selected.primary().is_some());
}
#[test]
fn render_snapshot_contract_is_owned_revision_bound() {
    let (mut m, _) = setup();
    let d = m.view.info.clone().unwrap();
    let s = m.service.render_snapshot(&d.document_id).unwrap();
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(s, serde_json::from_value(json).unwrap());
    select(&mut m);
    m.numeric_move("1", "0").unwrap();
    assert_eq!(s.revision, "0");
    let later = m.service.render_snapshot(&d.document_id).unwrap();
    assert_eq!(later.revision, "1");
    assert_ne!(s.layers, later.layers);
    let response=m.service.execute_json(&serde_json::json!({"api_version":1,"request_id":"display","op":"render.snapshot","document_id":d.document_id,"params":{}}).to_string());
    assert_eq!(response["status"], "completed");
    assert_eq!(response["result"]["revision"], "1");
}

#[test]
fn selection_uses_hit_test_order_topmost_layer() {
    use editor_service::LayerInfo;
    let make = |id: &str, visible| LayerInfo {
        layer_id: id.into(),
        display_name: id.into(),
        visible,
        locked: true,
        object_count: 2,
        ..Default::default()
    };
    // The panel lists layers top first; that is also hit-test priority.
    let layers = vec![
        make("top", true),
        make("bottom", true),
        make("hidden", false),
    ];
    let mut calls = vec![];
    let hit = crate::state::topmost_hit(&layers, |id| {
        calls.push(id.to_owned());
        Ok(vec!["first".into(), "last".into()])
    })
    .unwrap();
    assert_eq!(calls, vec!["top"]);
    assert_eq!(hit, Some(("top".into(), "last".into())));
    let hit = crate::state::topmost_hit(&layers, |id| {
        Ok(if id == "top" {
            vec![]
        } else {
            vec!["bottom-hit".into()]
        })
    })
    .unwrap();
    assert_eq!(hit, Some(("bottom".into(), "bottom-hit".into())));
}

fn armed(m: &mut Model, ppp: f32) -> crate::drag::Drag {
    use eframe::egui::{Pos2, Rect, Vec2};
    select(m);
    let mut drag = crate::drag::Drag::arm(
        &m.view,
        Pos2::new(100., 100.),
        crate::camera::Camera::default(),
        Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.)),
        ppp,
    )
    .unwrap();
    m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
    assert!(m.view.drag_hit);
    drag.confirmed = m.view.drag_hit;
    drag
}
#[test]
fn drag_preview_does_not_change_revision_dirty_or_history() {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    let before = m.view.info.clone();
    let scene = m.view.scene.clone().unwrap();
    for x in 101..150 {
        d.update(eframe::egui::pos2(x as f32, 110.));
    }
    assert!(d.dragging);
    assert_eq!(m.view.info, before);
    assert!(std::sync::Arc::ptr_eq(
        &scene,
        m.view.scene.as_ref().unwrap()
    ));
    assert_eq!(center(&m), MmPoint::new(10., 20.));
}
#[test]
fn drag_release_commits_exactly_one_move_transaction() {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    d.update(eframe::egui::pos2(150., 130.));
    m.run(d.release().unwrap());
    assert!(m.view.error.is_none());
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    let info = m.view.info.unwrap();
    assert_eq!(info.revision, "1");
    assert_eq!(info.undo_entries, 1);
}
#[test]
fn drag_zero_delta_is_noop() {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    let before = m.view.info.clone();
    d.update(eframe::egui::pos2(120., 100.));
    d.update(eframe::egui::pos2(100., 100.));
    assert!(d.release().is_none());
    assert_eq!(m.view.info, before);
}
fn cancel_case(escape: bool, focused: bool, gone: bool, down: bool, released: bool) {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    let before = m.view.info.clone();
    d.update(eframe::egui::pos2(150., 130.));
    let mut preview = Some(d);
    if crate::drag::cancelled(escape, focused, gone, down, released) {
        preview = None;
    }
    assert!(preview.is_none());
    assert_eq!(m.view.info, before);
    assert_eq!(center(&m), MmPoint::new(10., 20.));
}
#[test]
fn drag_escape_cancels_without_side_effects() {
    cancel_case(true, true, false, true, false);
}
#[test]
fn drag_capture_loss_cancels_without_side_effects() {
    cancel_case(false, true, true, true, false);
    cancel_case(false, false, false, true, false);
    cancel_case(false, true, false, false, false);
    cancel_case(false, true, true, false, true);
}
#[test]
fn locked_layer_cannot_drag() {
    let (mut m, _) = setup();
    select(&mut m);
    patch(&mut m, None, Some(true), None);
    assert!(!crate::drag::editable_selection(&m.view));
    assert!(
        crate::drag::Drag::arm(
            &m.view,
            eframe::egui::Pos2::ZERO,
            crate::camera::Camera::default(),
            eframe::egui::Rect::EVERYTHING,
            2.
        )
        .is_none()
    );
    m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
    assert!(!m.view.drag_hit);
}
#[test]
fn service_failure_clears_preview_and_preserves_geometry() {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    d.update(eframe::egui::pos2(150., 130.));
    d.revision = "999".into();
    let before = m.view.info.clone();
    let geometry = center(&m);
    let mut preview = Some(d);
    let action = preview.take().unwrap().release().unwrap();
    m.run(action);
    assert!(preview.is_none());
    assert_eq!(m.view.error.as_ref().unwrap().code, "REVISION_CONFLICT");
    assert_eq!(m.view.info, before);
    assert_eq!(center(&m), geometry);
}
#[test]
fn undo_redo_after_drag_preserves_selection() {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    let id = d.objects[0].clone();
    d.update(eframe::egui::pos2(150., 130.));
    m.run(d.release().unwrap());
    m.run(Action::History(false));
    assert_eq!(center(&m), MmPoint::new(10., 20.));
    m.run(Action::History(true));
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(m.view.selected.primary().unwrap().object.object_id, id);
}
#[test]
fn retina_drag_threshold_is_physical_pixel_stable() {
    for ppp in [1., 2., 3.] {
        let (mut m, _) = setup();
        let mut d = armed(&mut m, ppp);
        d.update(eframe::egui::pos2(100. + 3.9 / ppp, 100.));
        assert!(!d.dragging);
        d.update(eframe::egui::pos2(100. + 4.1 / ppp, 100.));
        assert!(d.dragging);
    }
}
#[test]
fn cmd_d_duplicates_in_place_and_selects_new_object() {
    let (mut m, _) = setup();
    select(&mut m);
    let source = m.view.selected.primary().unwrap().clone();
    m.run(Action::Duplicate);
    assert!(m.view.error.is_none());
    assert_ne!(
        m.view.selected.primary().unwrap().object.object_id,
        source.object.object_id
    );
    assert_eq!(center(&m), MmPoint::new(10., 20.));
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    let original = m
        .service
        .objects_get(
            &m.view.info.as_ref().unwrap().document_id,
            editor_service::ObjectParams {
                layer_id: source.layer_id.clone(),
                object_id: source.object.object_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(original, source);
}
#[test]
fn duplicate_undo_redo_preserves_generated_id() {
    let (mut m, _) = setup();
    select(&mut m);
    m.run(Action::Duplicate);
    let copy = m.view.selected.primary().unwrap().clone();
    m.run(Action::History(false));
    assert_eq!(m.view.layers[0].object_count, 2);
    m.run(Action::History(true));
    assert_eq!(m.view.layers[0].object_count, 3);
    let restored = m
        .service
        .objects_get(
            &m.view.info.as_ref().unwrap().document_id,
            editor_service::ObjectParams {
                layer_id: copy.layer_id.clone(),
                object_id: copy.object.object_id.clone(),
            },
        )
        .unwrap();
    assert_eq!(restored, copy);
}
#[test]
fn delete_selected_clears_selection_and_undo_restores() {
    let (mut m, _) = setup();
    select(&mut m);
    let before = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    m.run(Action::Delete);
    assert!(m.view.error.is_none());
    assert!(m.view.selected.primary().is_none());
    m.run(Action::History(false));
    let after = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    assert_eq!(before.layers, after.layers);
    m.run(Action::History(true));
    assert_eq!(m.view.layers[0].object_count, 1);
}
#[test]
fn text_focus_blocks_manufacturing_delete_duplicate_undo() {
    assert!(!crate::drag::shortcuts_allowed(true, false, false));
    assert!(!crate::drag::shortcuts_allowed(false, true, false));
    assert!(!crate::drag::shortcuts_allowed(false, false, true));
    assert!(crate::drag::shortcuts_allowed(false, false, false));
}
#[test]
fn locked_duplicate_delete_fail_without_losing_selection() {
    let (mut m, _) = setup();
    select(&mut m);
    patch(&mut m, None, Some(true), None);
    let selected = m.view.selected.clone();
    let before = m.view.info.clone();
    for action in [Action::Duplicate, Action::Delete] {
        m.run(action);
        assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
        assert_eq!(m.view.selected, selected);
        assert_eq!(m.view.info, before);
    }
}
#[test]
fn drag_probe_uses_exact_hole_without_mutating_selection() {
    let (mut m, _) = setup();
    select(&mut m);
    m.run(Action::ProbeDrag(MmPoint::new(21.5, 20.), 0.));
    assert!(!m.view.drag_hit);
    assert_eq!(center(&m), MmPoint::new(10., 20.));
    m.run(Action::ProbeDrag(MmPoint::new(20., 20.), 0.));
    assert!(!m.view.drag_hit);
    assert_eq!(center(&m), MmPoint::new(10., 20.));
}
#[test]
fn direct_manipulation_save_reopen_preserves_final_geometry() {
    let (mut m, dir) = setup();
    select(&mut m);
    m.run(Action::Duplicate);
    let mut d = crate::drag::Drag::arm(
        &m.view,
        eframe::egui::pos2(100., 100.),
        crate::camera::Camera::default(),
        eframe::egui::Rect::from_min_max(eframe::egui::Pos2::ZERO, eframe::egui::pos2(400., 400.)),
        2.,
    )
    .unwrap();
    m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
    d.confirmed = m.view.drag_hit;
    d.update(eframe::egui::pos2(150., 130.));
    m.run(d.release().unwrap());
    m.run(Action::Delete);
    m.run(Action::History(false));
    let path = dir.join("direct-final.gbr");
    m.save(&path, m.view.layers[0].layer_id.clone(), None)
        .unwrap();
    reopen(&mut m, &path);
    assert_eq!(m.view.layers[0].object_count, 3);
    m.select(MmPoint::new(15., 17.), 0., Replace).unwrap();
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(
        std::fs::read_to_string(dir.join("中文 # source.gbx")).unwrap(),
        SOURCE
    );
}

#[test]
fn drag_release_before_async_hit_retains_endpoint() {
    let (mut m, _) = setup();
    let mut d = armed(&mut m, 2.);
    d.confirmed = false;
    d.update(eframe::egui::pos2(150., 130.));
    assert!(!d.dragging);
    assert_eq!(d.delta, MmPoint::new(0., 0.));
    d.confirmed = true;
    d.update(d.last);
    m.run(d.release().unwrap());
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(m.view.info.unwrap().undo_entries, 1);
}

fn multiselect(m: &mut Model) {
    select(m);
    m.select(MmPoint::new(21.5, 20.), 0., Add).unwrap();
    assert_eq!(m.view.selected.ordered.len(), 2);
}
#[test]
fn ctrl_add_and_shift_remove_are_idempotent_and_stable() {
    let (mut m, _) = setup();
    let before = m.view.info.clone();
    multiselect(&mut m);
    let ids: Vec<_> = m
        .view
        .selected
        .ids()
        .into_iter()
        .map(str::to_owned)
        .collect();
    m.select(MmPoint::new(10., 20.), 0., Remove).unwrap();
    assert_eq!(m.view.selected.ids(), vec![ids[1].as_str()]);
    m.select(MmPoint::new(10., 20.), 0., Remove).unwrap();
    assert_eq!(m.view.selected.ids(), vec![ids[1].as_str()]);
    m.select(MmPoint::new(21.5, 20.), 0., Add).unwrap();
    assert_eq!(m.view.selected.ids(), vec![ids[1].as_str()]);
    m.select(MmPoint::new(10., 20.), 0., Add).unwrap();
    assert_eq!(
        m.view.selected.ids(),
        vec![ids[1].as_str(), ids[0].as_str()]
    );
    m.select(MmPoint::new(21.5, 20.), 0., Replace).unwrap();
    assert_eq!(m.view.selected.ids(), vec![ids[1].as_str()]);
    assert_eq!(m.view.info, before);
}
fn box_gesture(m: &mut Model) -> crate::drag::Gesture {
    use eframe::egui::{Rect, pos2};
    // Camera world x=-20..20, y=-20..20 at scale10. Press is blank.
    let mut g = crate::drag::Gesture::arm(
        &m.view,
        pos2(280., 30.),
        crate::camera::Camera::default(),
        Rect::from_min_max(pos2(0., 0.), pos2(400., 400.)),
        2.,
        Replace,
    );
    m.run(Action::ProbeDrag(MmPoint::new(8., 17.), 0.));
    g.confirm(&m.view);
    assert!(g.box_select);
    g
}
#[test]
fn selection_box_preview_has_no_manufacturing_side_effect() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    let scene = m.view.scene.clone().unwrap();
    let mut g = box_gesture(&mut m);
    g.update(eframe::egui::pos2(450., -30.));
    assert!(g.preview_rect().unwrap().1);
    assert_eq!(m.view.info, before);
    assert_eq!(m.view.selected, selected);
    assert!(std::sync::Arc::ptr_eq(
        &scene,
        m.view.scene.as_ref().unwrap()
    ));
    m.run(g.release().unwrap());
    assert_eq!(m.view.selected.ordered.len(), 2);
    assert_eq!(m.view.info, before);
}
#[test]
fn selection_box_cancel_has_no_side_effect() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    for (esc, focus, gone) in [
        (true, true, false),
        (false, false, false),
        (false, true, true),
    ] {
        let mut g = box_gesture(&mut m);
        g.update(eframe::egui::pos2(450., -30.));
        assert!(crate::drag::cancelled(esc, focus, gone, true, false));
        drop(g);
        assert_eq!(m.view.selected, selected);
        assert_eq!(m.view.info, before);
    }
}
#[test]
fn multi_drag_commits_one_transaction() {
    let (mut m, dir) = setup();
    multiselect(&mut m);
    let before = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    let selection = m
        .view
        .selected
        .ids()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut drag = crate::drag::Drag::arm(
        &m.view,
        eframe::egui::pos2(100., 100.),
        crate::camera::Camera::default(),
        eframe::egui::Rect::from_min_max(eframe::egui::Pos2::ZERO, eframe::egui::pos2(400., 400.)),
        2.,
    )
    .unwrap();
    m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
    assert!(m.view.drag_hit);
    drag.confirmed = true;
    drag.update(eframe::egui::pos2(150., 130.));
    m.run(drag.release().unwrap());
    assert!(m.view.error.is_none());
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    assert_eq!(m.view.selected.ids(), selection);
    for (o, x) in m.view.selected.ordered.iter().zip([15., 25.]) {
        assert!(
            matches!(o.object.geometry,SemanticGeometry::Flash{center,..} if center==MmPoint::new(x,17.))
        );
    }
    m.run(Action::History(false));
    assert_eq!(
        m.service
            .render_snapshot(&before.document_id)
            .unwrap()
            .layers,
        before.layers
    );
    m.run(Action::History(true));
    let target = dir.join("multi.gbr");
    m.save(&target, m.view.layers[0].layer_id.clone(), None)
        .unwrap();
    reopen(&mut m, &target);
    m.select(MmPoint::new(15., 17.), 0., Replace).unwrap();
    m.select(MmPoint::new(26.5, 17.), 0., Add).unwrap();
    assert_eq!(m.view.selected.ordered.len(), 2);
    assert_eq!(
        std::fs::read_to_string(dir.join("中文 # source.gbx")).unwrap(),
        SOURCE
    );
}
#[test]
fn multi_drag_locked_member_rejects_entire_batch() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let mut drag = crate::drag::Drag::arm(
        &m.view,
        eframe::egui::pos2(100., 100.),
        crate::camera::Camera::default(),
        eframe::egui::Rect::from_min_max(eframe::egui::Pos2::ZERO, eframe::egui::pos2(400., 400.)),
        2.,
    )
    .unwrap();
    drag.confirmed = true;
    drag.update(eframe::egui::pos2(150., 130.));
    patch(&mut m, None, Some(true), None);
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    m.run(drag.release().unwrap());
    assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
    assert_eq!(m.view.info, before);
    assert_eq!(m.view.selected, selected);
}
#[test]
fn multi_duplicate_is_one_transaction_and_redo_restores_ids() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let original = m.view.selected.clone();
    m.run(Action::Duplicate);
    assert!(m.view.error.is_none());
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    assert_eq!(m.view.layers[0].object_count, 4);
    let copies = m.view.selected.clone();
    assert_eq!(copies.ordered.len(), 2);
    for (a, b) in original.ordered.iter().zip(&copies.ordered) {
        assert_ne!(a.object.object_id, b.object.object_id);
        assert_eq!(a.object.geometry, b.object.geometry);
    }
    m.run(Action::History(false));
    assert!(m.view.selected.ordered.is_empty());
    m.run(Action::History(true));
    for o in copies.ordered {
        assert_eq!(
            m.service
                .objects_get(
                    &m.view.info.as_ref().unwrap().document_id,
                    editor_service::ObjectParams {
                        layer_id: o.layer_id.clone(),
                        object_id: o.object.object_id.clone()
                    }
                )
                .unwrap(),
            o
        );
    }
}
#[test]
fn multi_delete_is_atomic_and_undo_restores_order() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let before = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    patch(&mut m, None, Some(true), None);
    let selected = m.view.selected.clone();
    m.run(Action::Delete);
    assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
    assert_eq!(m.view.selected, selected);
    patch(&mut m, None, Some(false), None);
    m.run(Action::Delete);
    assert_eq!(m.view.layers[0].object_count, 0);
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    assert!(m.view.selected.ordered.is_empty());
    m.run(Action::History(false));
    assert_eq!(
        m.service
            .render_snapshot(&before.document_id)
            .unwrap()
            .layers,
        before.layers
    );
}
#[test]
fn selection_cleanup_after_delete_undo_redo() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    m.run(Action::Delete);
    assert!(m.view.selected.ordered.is_empty());
    m.run(Action::History(false));
    multiselect(&mut m);
    m.run(Action::History(true));
    assert!(m.view.selected.ordered.is_empty());
    m.run(Action::Close(true));
    assert!(m.view.selected.ordered.is_empty());
}
#[test]
fn cross_layer_selection_refuses_whole_edit() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let mut layer = m.view.layers[0].clone();
    layer.layer_id = "other".into();
    m.view.layers.push(layer);
    m.view.selected.ordered[0].layer_id = "other".into();
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    for action in [
        Action::Move("1".into(), "1".into()),
        Action::Rotate("90".into(), PivotInput::WorldOrigin),
        Action::Mirror(MirrorDirection::Horizontal),
        Action::Duplicate,
        Action::Delete,
    ] {
        m.run(action);
        assert_eq!(m.view.error.as_ref().unwrap().code, "UNSUPPORTED_FEATURE");
        assert_eq!(m.view.info, before);
        assert_eq!(m.view.selected, selected);
    }
}
#[test]
fn selection_gesture_async_release_and_plain_click_on_selected() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let rect =
        eframe::egui::Rect::from_min_max(eframe::egui::Pos2::ZERO, eframe::egui::pos2(400., 400.));
    let mut g = crate::drag::Gesture::arm(
        &m.view,
        eframe::egui::pos2(300., 0.),
        crate::camera::Camera::default(),
        rect,
        2.,
        Replace,
    );
    m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
    g.released = true;
    g.confirm(&m.view);
    m.run(g.release().unwrap());
    assert_eq!(m.view.selected.ordered.len(), 1);
    let mut g = box_gesture(&mut m);
    g.confirmed = false;
    g.update(eframe::egui::pos2(450., -30.));
    g.released = true;
    g.confirm(&m.view);
    m.run(g.release().unwrap());
    assert_eq!(m.view.selected.ordered.len(), 2);
}

#[test]
fn press_on_any_selected_member_can_drag_beneath_unselected_overlap() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let original = m.view.selected.clone();
    m.run(Action::Duplicate); // new, unselected copies are above the selected sources.
    m.view.selected = original.clone();
    m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
    assert!(m.view.drag_hit);
    assert!(!original.contains(
        &m.view.press_hit.as_ref().unwrap().layer_id,
        &m.view.press_hit.as_ref().unwrap().object.object_id
    ));
    assert_eq!(m.view.selected, original);
}

#[test]
fn modifier_click_gestures_preserve_press_intent_without_starting_move() {
    use crate::selection::SelectionMode;
    use eframe::egui::{Modifiers, Rect, pos2};
    let (mut m, _) = setup();
    select(&mut m);
    let before = m.view.info.clone();
    for (modifiers, expected) in [
        (Modifiers::CTRL, Add),
        (Modifiers::SHIFT, Remove),
        (
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            },
            Remove,
        ),
    ] {
        let mut g = crate::drag::Gesture::arm(
            &m.view,
            pos2(300., 0.),
            crate::camera::Camera::default(),
            Rect::from_min_max(pos2(0., 0.), pos2(400., 400.)),
            1.,
            SelectionMode::from_modifiers(modifiers),
        );
        m.run(Action::ProbeDrag(MmPoint::new(10., 20.), 0.));
        g.confirm(&m.view);
        g.update(pos2(310., 5.));
        assert!(matches!(g.release(), Some(Action::Select(_, _, mode)) if mode == expected));
        assert_eq!(m.view.info, before);
    }
}

#[test]
fn multi_selection_panel_labels_object_sum_not_layer_area() {
    let (mut m, _) = setup();
    m.run(Action::Select(MmPoint::new(10., 20.), 0., Replace));
    let single = crate::metrics_panel::lines(&m.view, Default::default(), 0.000001).join("\n");
    assert!(single.contains("面积：3.141592653590 mm²"));
    assert!(single.contains("周长：6.283185 mm"));
    m.run(Action::Select(MmPoint::new(21.5, 20.), 0., Add));
    let multiple = crate::metrics_panel::lines(&m.view, Default::default(), 0.000001).join("\n");
    assert!(multiple.contains("已精确：2 / 2"));
    assert!(multiple.contains("对象面积合计：12.566370614359 mm²"));
    assert!(multiple.contains("对象制造边界周长合计（含孔边）：25.132741 mm"));
    for forbidden in ["实际开口面积", "钢网总开口面积", "最终面积"] {
        assert!(!multiple.contains(forbidden));
    }
    m.view.metrics[1].value = editor_service::MetricValue::Unsupported {
        reason: "test boundary".into(),
    };
    let partial = crate::metrics_panel::lines(&m.view, Default::default(), 0.000001).join("\n");
    assert!(partial.contains("已精确：1 / 2"));
    assert!(partial.contains("对象面积合计（已精确项）"));
    assert!(partial.contains("1 个对象暂不可计算"));
    m.view.selected.ordered.remove(0);
    m.view.metrics.remove(0);
    let unsupported = crate::metrics_panel::lines(&m.view, Default::default(), 0.000001).join("\n");
    assert!(unsupported.contains("暂不可精确计算"));
    assert!(!unsupported.contains("0.000000"));
}
#[test]
fn metrics_worker_selection_edit_and_close_identity() {
    let (mut m, _) = setup();
    m.run(Action::Select(MmPoint::new(10., 20.), 0., Replace));
    let initial = m.view.metrics[0].value.clone();
    m.run(Action::Move("5".into(), "-3".into()));
    assert_eq!(m.view.metrics[0].value, initial);
    m.run(Action::Duplicate);
    assert_eq!(m.view.metrics[0].value, initial);
    m.run(Action::Delete);
    assert!(m.view.metrics.is_empty());
    m.run(Action::History(false));
    m.run(Action::Select(MmPoint::new(15., 17.), 0., Replace));
    assert_eq!(m.view.metrics[0].value, initial);
    m.run(Action::Select(MmPoint::new(21.5, 20.), 0., Replace));
    assert_ne!(m.view.metrics[0].value, initial);
    m.run(Action::Close(true));
    assert!(m.view.metrics.is_empty());
}

#[test]
fn s2c1_view_tools_preserve_export_revision_dirty_and_history() {
    let (mut m, dir) = setup();
    let layer = m.view.layers[0].layer_id.clone();
    m.save(&dir.join("baseline.gbr"), layer.clone(), None)
        .unwrap();
    let bytes = std::fs::read(dir.join("baseline.gbr")).unwrap();
    let before = m.view.info.clone();
    let mut grid = crate::tools::GridSettings::default();
    let mut measure = crate::tools::MeasureState::default();
    for step in 0..5 {
        match step {
            0 => grid.visible = true,
            1 => grid.spacing_mm = 0.01,
            2 => grid.snap_enabled = true,
            3 => {
                measure.click(grid.point(MmPoint::new(0., 0.)).unwrap());
                measure.hover(Some(MmPoint::new(3., 4.)));
                measure.click(MmPoint::new(3., 4.));
                assert_eq!(measure.values(), Some((3., 4., 5.)));
            }
            _ => measure.clear(),
        }
        let info = m.view.info.as_ref().unwrap();
        let baseline = before.as_ref().unwrap();
        assert_eq!(
            (
                &info.revision,
                info.dirty,
                info.undo_entries,
                info.redo_entries
            ),
            (
                &baseline.revision,
                baseline.dirty,
                baseline.undo_entries,
                baseline.redo_entries
            )
        );
        m.save(&dir.join(format!("view-{step}.gbr")), layer.clone(), None)
            .unwrap();
        assert_eq!(
            std::fs::read(dir.join(format!("view-{step}.gbr"))).unwrap(),
            bytes
        );
    }
    m.run(Action::Move("0.123".into(), "0".into())); // no selection is rejected
    select(&mut m);
    m.run(Action::Move("0.123".into(), "0".into()));
    assert!(grid.snap_enabled);
    assert_eq!(center(&m), MmPoint::new(10.123, 20.));
}

#[test]
fn s2c1_snap_common_delta_preview_single_transaction_and_undo() {
    for multi in [false, true] {
        let (mut m, dir) = setup();
        select(&mut m);
        if multi {
            multiselect(&mut m);
        }
        let original = m.view.selected.clone();
        let before = m.view.info.clone();
        let mut drag = crate::drag::Drag::arm(
            &m.view,
            eframe::egui::pos2(100., 100.),
            crate::camera::Camera::default(),
            eframe::egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(400., 400.),
            ),
            2.,
        )
        .unwrap();
        let grid = crate::tools::GridSettings {
            visible: false,
            spacing_mm: 0.5,
            snap_enabled: true,
        };
        drag.confirmed = true;
        for n in 0..20 {
            let position = eframe::egui::pos2(110. + n as f32, 107.);
            let target = grid
                .point(crate::camera::Camera::default().world(
                    position,
                    eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        eframe::egui::vec2(400., 400.),
                    ),
                ))
                .unwrap();
            drag.update_snapped(position, Some(target));
        }
        assert_eq!(drag.delta, MmPoint::new(3., -0.5));
        assert_eq!(m.view.info, before);
        m.run(drag.release().unwrap());
        assert!(m.view.error.is_none());
        let info = m.view.info.as_ref().unwrap();
        assert_eq!(info.revision, "1");
        assert_eq!(info.undo_entries, 1);
        assert!(info.dirty);
        for (old, new) in original.ordered.iter().zip(&m.view.selected.ordered) {
            match (&old.object.geometry, &new.object.geometry) {
                (
                    SemanticGeometry::Flash { center: a, .. },
                    SemanticGeometry::Flash { center: b, .. },
                ) => assert_eq!(*b, MmPoint::new(a.x_mm + 3., a.y_mm - 0.5)),
                _ => panic!(),
            }
        }
        let output = dir.join("snapped.gbr");
        m.save(&output, m.view.layers[0].layer_id.clone(), None)
            .unwrap();
        m.run(Action::History(false));
        assert_eq!(m.view.selected, original);
        m.run(Action::Close(true));
        m.open(&output).unwrap();
        m.select(MmPoint::new(13., 19.5), 0., Replace).unwrap();
        assert_eq!(center(&m), MmPoint::new(13., 19.5));
        assert_eq!(
            std::fs::read_to_string(dir.join("中文 # source.gbx")).unwrap(),
            SOURCE
        );
    }
}

#[test]
fn s2c1_invalid_snap_cannot_commit() {
    let (mut m, _) = setup();
    let mut drag = armed(&mut m, 2.);
    let grid = crate::tools::GridSettings {
        visible: false,
        spacing_mm: 1e-300,
        snap_enabled: true,
    };
    let before = m.view.info.clone();
    let error = grid.point(MmPoint::new(1., 1.)).unwrap_err();
    drag.set_error(Some(error));
    drag.update(eframe::egui::pos2(120., 120.));
    assert!(drag.release().is_none());
    assert_eq!(m.view.info, before);
}

const TRANSFORM_SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\n%ADD11R,4X2*%\nD10*\nX1000000Y2000000D03*\nD11*\nX5000000Y8000000D03*\nM02*\n";

fn transformed_point(point: MmPoint, angle_deg: f64, pivot: MmPoint) -> MmPoint {
    let angle = angle_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = point.x_mm - pivot.x_mm;
    let y = point.y_mm - pivot.y_mm;
    MmPoint::new(
        pivot.x_mm + x * cos - y * sin,
        pivot.y_mm + x * sin + y * cos,
    )
}

fn geometry_list(m: &Model) -> Vec<SemanticGeometry> {
    m.service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap()
        .layers[0]
        .objects
        .iter()
        .map(|object| object.geometry.clone())
        .collect()
}

fn flash_centers(m: &Model) -> Vec<MmPoint> {
    geometry_list(m)
        .into_iter()
        .map(|geometry| match geometry {
            SemanticGeometry::Flash { center, .. } => center,
            _ => panic!("not flash"),
        })
        .collect()
}

#[test]
fn s2c2_selection_center_uses_union_manufacturing_bounds() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    let bounds = crate::state::selected_bounds(&m.view).unwrap();
    assert_eq!(bounds.min_x_mm, 9.);
    assert_eq!(bounds.max_x_mm, 22.);
    assert_eq!(bounds.min_y_mm, 18.);
    assert_eq!(bounds.max_y_mm, 22.);
    assert_eq!(
        crate::state::selected_center(&m.view).unwrap(),
        MmPoint::new(15.5, 20.)
    );
    assert_ne!(crate::state::selected_center(&m.view).unwrap().x_mm, 15.);
}

#[test]
fn s2c2_single_flash_quarter_turn_and_custom_pivot_use_service() {
    let (mut m, _) = setup();
    select(&mut m);
    m.run(Action::Rotate("90".into(), PivotInput::WorldOrigin));
    assert!(m.view.error.is_none());
    assert_eq!(center(&m), MmPoint::new(-20., 10.));
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    m.run(Action::History(false));
    let pivot = MmPoint::new(1.25, -2.5);
    m.run(Action::Rotate(
        "37".into(),
        PivotInput::Custom(pivot.x_mm.to_string(), pivot.y_mm.to_string()),
    ));
    let expected = transformed_point(MmPoint::new(10., 20.), 37., pivot);
    assert!(center(&m).distance_mm(expected) < 1e-9);
}

#[test]
fn s2c2_multi_rotate_uses_one_pivot_and_one_transaction() {
    let (mut m, _) = setup();
    multiselect(&mut m);
    m.run(Action::Rotate("90".into(), PivotInput::SelectionCenter));
    assert!(m.view.error.is_none());
    assert_eq!(
        flash_centers(&m),
        vec![MmPoint::new(15.5, 14.5), MmPoint::new(15.5, 24.5)]
    );
    let info = m.view.info.as_ref().unwrap();
    assert_eq!(info.revision, "1");
    assert_eq!(info.undo_entries, 1);
    m.run(Action::History(false));
    assert_eq!(
        flash_centers(&m),
        vec![MmPoint::new(10., 20.), MmPoint::new(20., 20.)]
    );
}

#[test]
fn s2c2_horizontal_and_vertical_mirror_use_explicit_selection_axes() {
    let (mut horizontal, _) = setup_source("mirror-horizontal.gbr", TRANSFORM_SOURCE.as_bytes());
    select_all(&mut horizontal);
    assert_eq!(
        crate::state::selected_center(&horizontal.view).unwrap(),
        MmPoint::new(3.5, 5.)
    );
    horizontal.run(Action::Mirror(MirrorDirection::Horizontal));
    assert_eq!(
        flash_centers(&horizontal),
        vec![MmPoint::new(1., 8.), MmPoint::new(5., 2.)]
    );

    let (mut vertical, _) = setup_source("mirror-vertical.gbr", TRANSFORM_SOURCE.as_bytes());
    select_all(&mut vertical);
    vertical.run(Action::Mirror(MirrorDirection::Vertical));
    assert_eq!(
        flash_centers(&vertical),
        vec![MmPoint::new(6., 2.), MmPoint::new(2., 8.)]
    );
}

#[test]
fn s2c2_arc_arbitrary_rotate_and_mirror_direction_are_preserved() {
    let source = fixture("s1a1/g74_cw_quarter.gbr");
    let (mut rotate, _) = setup_source("arc-rotate.gbr", &source);
    select_all(&mut rotate);
    let SemanticGeometry::Arc { path: before, .. } = geometry_list(&rotate)[0] else {
        panic!("not arc")
    };
    let pivot = MmPoint::new(2., 3.);
    rotate.run(Action::Rotate(
        "37".into(),
        PivotInput::Custom("2".into(), "3".into()),
    ));
    let SemanticGeometry::Arc { path: after, .. } = geometry_list(&rotate)[0] else {
        panic!("not arc")
    };
    assert!(
        after
            .start
            .distance_mm(transformed_point(before.start, 37., pivot))
            < 1e-9
    );
    assert!(
        after
            .end
            .distance_mm(transformed_point(before.end, 37., pivot))
            < 1e-9
    );
    assert!(
        after
            .center
            .distance_mm(transformed_point(before.center, 37., pivot))
            < 1e-9
    );
    assert_eq!(after.direction, before.direction);
    assert_eq!(after.full_circle, before.full_circle);

    let (mut mirror, _) = setup_source("arc-mirror.gbr", &source);
    select_all(&mut mirror);
    let axis = crate::state::selected_center(&mirror.view).unwrap().y_mm;
    mirror.run(Action::Mirror(MirrorDirection::Horizontal));
    let SemanticGeometry::Arc {
        path: reflected, ..
    } = geometry_list(&mirror)[0]
    else {
        panic!("not arc")
    };
    assert_eq!(reflected.start.x_mm, before.start.x_mm);
    assert!((reflected.start.y_mm - (2. * axis - before.start.y_mm)).abs() < 1e-9);
    assert_ne!(reflected.direction, before.direction);
}

#[test]
fn s2c2_region_rotate_preserves_contours_and_edge_order() {
    let (mut m, _) = setup_source("region.gbr", &fixture("s1a/region_arc.gbr"));
    select_all(&mut m);
    let SemanticGeometry::Region { contours: before } = geometry_list(&m)[0].clone() else {
        panic!("not region")
    };
    m.run(Action::Rotate(
        "37".into(),
        PivotInput::Custom("2".into(), "3".into()),
    ));
    let SemanticGeometry::Region { contours: after } = geometry_list(&m)[0].clone() else {
        panic!("not region")
    };
    assert_eq!(after.len(), before.len());
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(new.role, old.role);
        assert_eq!(new.edges.len(), old.edges.len());
    }
}

#[test]
fn s2c2_rectangular_sweep_quarter_turns_and_37_rejection_are_atomic() {
    let (mut m, _) = setup_source(
        "rectangular-sweep.gbr",
        &fixture("s0c/rectangular_draw.gbr"),
    );
    select_all(&mut m);
    let original = geometry_list(&m);
    for angle in ["90", "-90"] {
        m.run(Action::Rotate(angle.into(), PivotInput::WorldOrigin));
        assert!(m.view.error.is_none());
        for geometry in geometry_list(&m) {
            assert!(matches!(
                geometry,
                SemanticGeometry::RectangularSweep {
                    width_mm: 1.,
                    height_mm: 2.,
                    ..
                }
            ));
        }
        assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
        m.run(Action::History(false));
        assert_eq!(geometry_list(&m), original);
    }
    let before = m.view.info.clone();
    m.run(Action::Rotate("37".into(), PivotInput::WorldOrigin));
    assert_eq!(m.view.error.as_ref().unwrap().code, "UNSUPPORTED_FEATURE");
    assert_eq!(m.view.info, before);
    assert_eq!(geometry_list(&m), original);
}

#[test]
fn s2c2_locked_layer_rejects_rotate_and_mirror_without_partial_change() {
    let (mut m, _) = setup_source("locked.gbr", TRANSFORM_SOURCE.as_bytes());
    select_all(&mut m);
    patch(&mut m, None, Some(true), None);
    let before = m.view.info.clone();
    let geometry = geometry_list(&m);
    for action in [
        Action::Rotate("90".into(), PivotInput::WorldOrigin),
        Action::Mirror(MirrorDirection::Vertical),
    ] {
        m.run(action);
        assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
        assert_eq!(m.view.info, before);
        assert_eq!(geometry_list(&m), geometry);
    }
}

#[test]
fn s2c2_arbitrary_angle_and_custom_pivot_are_not_quantized() {
    let (mut m, _) = setup_source("grid-independent.gbr", TRANSFORM_SOURCE.as_bytes());
    m.select(MmPoint::new(1., 2.), 0., Replace).unwrap();
    let pivot = MmPoint::new(0.123456789, -0.456789123);
    m.run(Action::Rotate(
        "37.125".into(),
        PivotInput::Custom(pivot.x_mm.to_string(), pivot.y_mm.to_string()),
    ));
    let expected = transformed_point(MmPoint::new(1., 2.), 37.125, pivot);
    assert!(center(&m).distance_mm(expected) < 1e-9);
}

#[test]
fn s2c2_rotate_mirror_keep_cached_metrics_exactly_unchanged() {
    let (mut m, _) = setup_source("metrics.gbr", TRANSFORM_SOURCE.as_bytes());
    m.run(Action::Select(MmPoint::new(1., 2.), 0., Replace));
    m.run(Action::Select(MmPoint::new(5., 8.), 0., Add));
    let before: Vec<_> = m
        .view
        .metrics
        .iter()
        .map(|item| item.value.clone())
        .collect();
    assert_eq!(before.len(), 2);
    m.run(Action::Rotate("37".into(), PivotInput::SelectionCenter));
    assert_eq!(
        m.view
            .metrics
            .iter()
            .map(|item| item.value.clone())
            .collect::<Vec<_>>(),
        before
    );
    m.run(Action::Mirror(MirrorDirection::Vertical));
    assert_eq!(
        m.view
            .metrics
            .iter()
            .map(|item| item.value.clone())
            .collect::<Vec<_>>(),
        before
    );
}

#[test]
fn s2c2_undo_redo_save_and_reopen_preserve_transformed_geometry() {
    let (mut m, dir) = setup_source("transform-source.gbr", TRANSFORM_SOURCE.as_bytes());
    select_all(&mut m);
    let original = geometry_list(&m);
    m.run(Action::Rotate("90".into(), PivotInput::WorldOrigin));
    m.run(Action::Mirror(MirrorDirection::Vertical));
    let transformed = geometry_list(&m);
    assert_ne!(transformed, original);
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 2);
    m.run(Action::History(false));
    m.run(Action::History(false));
    assert_eq!(geometry_list(&m), original);
    m.run(Action::History(true));
    m.run(Action::History(true));
    assert_eq!(geometry_list(&m), transformed);
    let output = dir.join("transform-output.gbr");
    m.save(&output, m.view.layers[0].layer_id.clone(), None)
        .unwrap();
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 2);
    reopen(&mut m, &output);
    assert_eq!(geometry_list(&m), transformed);
}

#[test]
fn s2c2_zero_and_non_finite_inputs_fail_without_revision_or_history() {
    let (mut m, _) = setup();
    select(&mut m);
    let before = m.view.info.clone();
    for action in [
        Action::Rotate("0".into(), PivotInput::WorldOrigin),
        Action::Rotate("NaN".into(), PivotInput::WorldOrigin),
        Action::Rotate("inf".into(), PivotInput::WorldOrigin),
        Action::Rotate("37".into(), PivotInput::Custom("NaN".into(), "0".into())),
    ] {
        m.run(action);
        assert_eq!(m.view.error.as_ref().unwrap().code, "INVALID_ARGUMENT");
        assert_eq!(m.view.info, before);
        assert_eq!(center(&m), MmPoint::new(10., 20.));
    }
}

#[test]
fn s2c2_transform_text_fields_share_existing_focus_shortcut_guard() {
    assert!(!crate::drag::shortcuts_allowed(true, false, false));
    assert!(crate::drag::shortcuts_allowed(false, false, false));
}

#[test]
fn s3_flash_size_gui_path_is_cow_undoable_and_roundtrips() {
    let source = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\nD10*\nX10000000Y20000000D03*\nX20000000Y20000000D03*\nM02*\n";
    let (mut m, dir) = setup_source("shared-aperture.gbr", source.as_bytes());
    select(&mut m);
    let before = geometry_list(&m);
    let before_apertures = m.view.apertures.clone();
    m.run(Action::SetFlashSize("4".into(), None));
    assert!(m.view.error.is_none());
    let after = geometry_list(&m);
    let (
        SemanticGeometry::Flash {
            aperture_id: changed,
            ..
        },
        SemanticGeometry::Flash {
            aperture_id: unchanged,
            ..
        },
    ) = (&after[0], &after[1])
    else {
        panic!()
    };
    assert_ne!(changed, unchanged);
    assert_eq!(unchanged, "aperture-10");
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    m.run(Action::History(false));
    assert_eq!(geometry_list(&m), before);
    assert_eq!(m.view.apertures, before_apertures);
    m.run(Action::History(true));
    assert_eq!(geometry_list(&m), after);
    let output = dir.join("resized-flash.gbr");
    m.save(&output, m.view.layers[0].layer_id.clone(), None)
        .unwrap();
    reopen(&mut m, &output);
    assert_eq!(geometry_list(&m).len(), 2);
    let shapes: Vec<_> = geometry_list(&m)
        .iter()
        .map(|geometry| match geometry {
            SemanticGeometry::Flash { aperture_id, .. } => m
                .view
                .apertures
                .iter()
                .find(|aperture| &aperture.id == aperture_id)
                .unwrap()
                .shape
                .clone(),
            _ => panic!(),
        })
        .collect();
    assert!(matches!(
        shapes[0],
        editor_core::ApertureShape::Circle {
            diameter_mm: 4.,
            ..
        }
    ));
}

#[test]
fn s3_snap_candidates_come_from_visible_manufacturing_geometry() {
    let (mut m, _) = setup();
    let mut runtime = crate::object_snap::Runtime::default();
    let settings = crate::object_snap::Settings {
        enabled: true,
        ..Default::default()
    };
    let result = runtime
        .resolve(
            MmPoint::new(10., 20.),
            &settings,
            crate::tools::GridSettings::default(),
            crate::camera::Camera {
                scale: 20.,
                ..Default::default()
            },
            2.,
            m.view.snap_snapshot.as_deref(),
            &m.view.snap_index,
            &m.view.layers,
            None,
            false,
        )
        .unwrap();
    assert_eq!(result.kind, Some(editor_core::snap::SnapKind::Center));
    patch(&mut m, Some(false), None, None);
    let result = runtime
        .resolve(
            MmPoint::new(10.01, 20.01),
            &settings,
            crate::tools::GridSettings::default(),
            crate::camera::Camera {
                scale: 20.,
                ..Default::default()
            },
            2.,
            m.view.snap_snapshot.as_deref(),
            &m.view.snap_index,
            &m.view.layers,
            None,
            false,
        )
        .unwrap();
    assert_eq!(result.kind, None);

    patch(&mut m, Some(true), Some(true), None);
    let result = runtime
        .resolve(
            MmPoint::new(10.01, 20.01),
            &settings,
            crate::tools::GridSettings::default(),
            crate::camera::Camera {
                scale: 20.,
                ..Default::default()
            },
            2.,
            m.view.snap_snapshot.as_deref(),
            &m.view.snap_index,
            &m.view.layers,
            None,
            false,
        )
        .unwrap();
    assert_eq!(
        result.kind,
        Some(editor_core::snap::SnapKind::Center),
        "locked but selectable reference layers remain snappable"
    );

    let info = m.view.info.as_ref().unwrap();
    m.run(Action::Layer(LayerUpdateParams {
        layer_id: info.layer_ids[0].clone(),
        expected_workspace_revision: info.workspace_revision.clone(),
        selectable: Some(false),
        ..Default::default()
    }));
    let raw = MmPoint::new(10.01, 20.01);
    let result = runtime
        .resolve(
            raw,
            &settings,
            crate::tools::GridSettings::default(),
            crate::camera::Camera {
                scale: 20.,
                ..Default::default()
            },
            2.,
            m.view.snap_snapshot.as_deref(),
            &m.view.snap_index,
            &m.view.layers,
            None,
            false,
        )
        .unwrap();
    assert_eq!(result.kind, None, "nonselectable layers are excluded");

    let result = runtime
        .resolve(
            raw,
            &settings,
            crate::tools::GridSettings {
                visible: true,
                spacing_mm: 1.,
                snap_enabled: true,
            },
            crate::camera::Camera {
                scale: 20.,
                ..Default::default()
            },
            2.,
            m.view.snap_snapshot.as_deref(),
            &m.view.snap_index,
            &m.view.layers,
            None,
            true,
        )
        .unwrap();
    assert_eq!(
        result.point, raw,
        "Alt temporarily disables object and grid snap"
    );
    assert!(!result.from_grid);
}

fn text_draft(m: &mut Model) -> crate::text_tool::Draft {
    let path = std::path::PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf");
    m.run(Action::TextFont(1, path, 0));
    let reply = m.view.text_reply.as_ref().unwrap();
    let crate::text_tool::Reply::Font { result, .. } = reply.as_ref() else {
        panic!()
    };
    let mut draft = crate::text_tool::Draft {
        text: "口8".into(),
        ..Default::default()
    };
    draft.accept_font(result.as_ref().unwrap().clone());
    draft
}
fn text_request(m: &Model, d: &crate::text_tool::Draft) -> crate::text_tool::Request {
    let info = m.view.info.as_ref().unwrap();
    crate::text_tool::Request {
        generation: d.generation,
        document: info.document_id.clone(),
        revision: info.revision.clone(),
        params: d.params(&m.view.layers[0].layer_id).unwrap(),
    }
}
#[test]
fn text_typed_worker_preview_fencing_apply_group_and_history() {
    let (mut m, _) = setup();
    let mut d = text_draft(&mut m);
    let r = text_request(&m, &d);
    let info = m.view.info.clone();
    m.run(Action::TextPreview(r.clone()));
    assert_eq!(m.view.info, info);
    let crate::text_tool::Reply::Preview { result, .. } =
        m.view.text_reply.as_ref().unwrap().as_ref()
    else {
        panic!()
    };
    let p = result.as_ref().unwrap();
    assert!(!p.geometries.is_empty());
    assert!(d.matches(&r, &m.view, Some(&r.params.layer_id), true));
    assert!(!d.matches(&r, &m.view, Some(&r.params.layer_id), false));
    assert!(!d.matches(&r, &m.view, Some("other-layer"), true));
    let mut other = m.view.clone();
    other.info.as_mut().unwrap().document_id = "other".into();
    assert!(!d.matches(&r, &other, Some(&r.params.layer_id), true));
    other = m.view.clone();
    other.info.as_mut().unwrap().revision = "999".into();
    assert!(!d.matches(&r, &other, Some(&r.params.layer_id), true));
    other.info = None;
    assert!(!d.matches(&r, &other, Some(&r.params.layer_id), true));
    d.changed();
    assert!(!d.matches(&r, &m.view, Some(&r.params.layer_id), true));
    let before = m.view.info.clone();
    d.cancel();
    assert_eq!(m.view.info, before);
    let expected = p.geometries.len();
    m.run(Action::TextCreate(r.clone()));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.selected.ordered.len(), expected);
    let selected = m.view.selected.clone();
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    m.run(Action::TextCreate(r));
    assert_eq!(m.view.error.as_ref().unwrap().code, "REVISION_CONFLICT");
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    m.run(Action::Move("1".into(), "2".into()));
    assert!(m.view.error.is_none());
    m.run(Action::Rotate("37".into(), PivotInput::WorldOrigin));
    assert!(m.view.error.is_none());
    m.run(Action::Mirror(MirrorDirection::Horizontal));
    assert!(m.view.error.is_none());
    m.run(Action::History(false));
    m.run(Action::History(false));
    m.run(Action::History(false));
    assert_eq!(m.view.selected, selected);
    m.run(Action::Delete);
    assert!(m.view.error.is_none());
    m.run(Action::History(false));
    assert!(m.view.error.is_none());
    // Change the current draft font; committed manufacturing snapshot is unchanged.
    let before = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    m.run(Action::TextFont(
        2,
        "/System/Library/Fonts/STHeiti Light.ttc".into(),
        0,
    ));
    assert_eq!(
        m.service
            .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
            .unwrap(),
        before
    );
}

#[test]
#[ignore = "release-only multi-font text preview matrix; writes explicit evidence"]
fn text_preview_performance_matrix() {
    let (mut m, _) = setup();
    let mut rows = Vec::new();
    for path in [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
    ] {
        m.service
            .grant_file_access(std::path::Path::new(path), false)
            .unwrap();
        let font = m.service.font_inspect(path, 0).unwrap();
        for (text, offset, tolerance) in [
            ("ABCD12348O", 0., 0.00025),
            ("中文口回", 0., 0.000125),
            ("中文测试钢网口回", 0., 0.00025),
            ("中文AB1234口回CD", 0.01, 0.00025),
            ("O8中回", -0.01, 0.00025),
            ("口8", 0., 0.0000625),
            ("口8", 0.01, 0.000125),
            ("口8", -0.01, 0.000125),
            ("口", 0., 0.00001),
        ] {
            let mut draft = crate::text_tool::Draft {
                text: text.into(),
                offset: offset.to_string(),
                tolerance: tolerance.to_string(),
                rotation: "37".into(),
                ..Default::default()
            };
            draft.accept_font(font.clone());
            let request = text_request(&m, &draft);
            let start = std::time::Instant::now();
            m.run(Action::TextPreview(request));
            let crate::text_tool::Reply::Preview {
                result,
                worker_ms,
                finished,
                ..
            } = m.view.text_reply.as_ref().unwrap().as_ref()
            else {
                panic!()
            };
            let publish_ms = finished.elapsed().as_secs_f64() * 1000.;
            let row = match result {
                Ok(p) => {
                    let geometry_bounds = editor_core::geometries_bounds(p.geometries.iter(), &[])
                        .unwrap()
                        .unwrap();
                    let ctx = eframe::egui::Context::default();
                    let mut camera = crate::camera::Camera::default();
                    let rect = eframe::egui::Rect::from_min_size(
                        eframe::egui::Pos2::ZERO,
                        eframe::egui::vec2(800., 600.),
                    );
                    camera.fit(Some(geometry_bounds), rect);
                    draft.preview = Some(p.clone());
                    let paint = std::time::Instant::now();
                    let _ = ctx.run(eframe::egui::RawInput::default(), |ctx| {
                        draft.paint(
                            &ctx.layer_painter(eframe::egui::LayerId::background()),
                            camera,
                            rect,
                        )
                    });
                    serde_json::json!({"status":"PASS","objects":p.geometries.len(),"timings":p.timings,"worker_ms":worker_ms,"publish_latency_ms":publish_ms,"overlay_prepare_ms":paint.elapsed().as_secs_f64()*1000.,"renderer_candidates":draft.candidates})
                }
                Err(e) => {
                    serde_json::json!({"status":"REJECTED","error":e.code,"reason":e.message,"worker_ms":worker_ms,"publish_latency_ms":publish_ms})
                }
            };
            rows.push(serde_json::json!({"font":std::path::Path::new(path).file_name().unwrap().to_string_lossy(),"family":font.family,"sha256":font.identity.sha256,"face_index":0,"text":text,"offset_mm":offset,"tolerance_mm":tolerance,"total_ms":start.elapsed().as_secs_f64()*1000.,"result":row}));
        }
    }
    let out = std::env::var("RCAM_TEXT_MATRIX").expect("set RCAM_TEXT_MATRIX to a new output path");
    std::fs::write(out, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    assert!(rows.iter().all(|r| {
        r["result"]["status"] == "PASS"
            || ["RESOURCE_LIMIT", "VALIDATION_FAILED", "UNSUPPORTED_FEATURE"]
                .iter()
                .any(|code| r["result"]["error"] == *code)
    }));
}

#[test]
fn committed_text_fits_retina_viewport_sample_budget() {
    use eframe::egui;
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2a3/gui_primitives.gbr"),
    )
    .unwrap();
    let mut draft = text_draft(&mut m);
    draft.text = "中间".into();
    draft.offset = "0.01".into();
    draft.rotation = "37".into();
    draft.x = "24.36119473474698".into();
    draft.y = "30.628727186858097".into();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(784., 658.));
    let mut camera = crate::camera::Camera::default();
    camera.fit(m.view.bounds, rect);
    let lo = camera.world(rect.left_bottom(), rect);
    let hi = camera.world(rect.right_top(), rect);
    m.run(Action::Viewport(
        camera.center,
        editor_core::BoundsMm {
            min_x_mm: lo.x_mm - 30.,
            min_y_mm: lo.y_mm - 30.,
            max_x_mm: hi.x_mm + 30.,
            max_y_mm: hi.y_mm + 30.,
        },
        64.,
    ));
    m.run(Action::TextCreate(text_request(&m, &draft)));
    assert!(m.view.error.is_none());
    let scene = m.view.scene.as_ref().unwrap();
    let flags = crate::gpu::selection_flags(scene, &m.view.selected.ids());
    let prepared =
        crate::gpu::prepare_measured(scene, camera, rect, 2., &flags, MmPoint::new(0., 0.))
            .unwrap();
    assert!(prepared.stats.estimated_work < 2_000_000_000.);
    // The frozen text no longer contributes hundreds of slab candidates.
    assert!(prepared.stats.candidate_count > 0 && prepared.stats.candidate_count < 700);
    assert_eq!(m.view.selected.ordered.len(), 5);
}

#[cfg(target_os = "macos")]
#[test]
fn lisong_light_sdf_point_committed_text_zoom_stays_bounded() {
    use eframe::egui;
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2a3/gui_primitives.gbr"),
    )
    .unwrap();
    let path = std::path::PathBuf::from("/System/Library/Fonts/Supplemental/Songti.ttc");
    m.run(Action::SystemFont(1, path, "STSongti-SC-Light".into()));
    let crate::text_tool::Reply::Font { result, .. } = m.view.text_reply.as_ref().unwrap().as_ref()
    else {
        panic!()
    };
    let mut draft = crate::text_tool::Draft {
        text: "sdf 点".into(),
        height: "3".into(),
        offset: "0".into(),
        x: "0".into(),
        y: "0".into(),
        ..Default::default()
    };
    draft.accept_font(result.as_ref().unwrap().clone());
    m.run(Action::TextCreate(text_request(&m, &draft)));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);

    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 450.));
    let ppp = 2.;
    // 800 logical points / 69.565 pt/mm = 11.5 mm across the visible canvas.
    let initial = m.view.scene.as_ref().unwrap();
    let selected = m.view.selected.ids();
    let object = initial
        .objects
        .iter()
        .zip(&initial.ids)
        .find(|(_, id)| selected.iter().any(|selected| selected == id))
        .map(|(object, _)| object)
        .unwrap();
    let center = MmPoint::new(
        initial.anchor.x_mm + f64::from((object.bounds[0] + object.bounds[2]) * 0.5),
        initial.anchor.y_mm + f64::from((object.bounds[1] + object.bounds[3]) * 0.5),
    );
    let mut at_11_5_mm = None;
    for visible_width_mm in [11.5, 2., 0.5, 0.1] {
        let camera = crate::camera::Camera {
            center,
            scale: 800. / visible_width_mm,
        };
        let ppm = camera.scale * f64::from(ppp);
        let lo = camera.world(rect.left_bottom(), rect);
        let hi = camera.world(rect.right_top(), rect);
        m.run(Action::Viewport(
            camera.center,
            editor_core::BoundsMm {
                min_x_mm: lo.x_mm,
                min_y_mm: lo.y_mm,
                max_x_mm: hi.x_mm,
                max_y_mm: hi.y_mm,
            },
            ppm,
        ));
        let scene = m.view.scene.as_ref().unwrap();
        let flags = crate::gpu::selection_flags(scene, &m.view.selected.ids());
        let prepared =
            crate::gpu::prepare_measured(scene, camera, rect, ppp, &flags, MmPoint::new(0., 0.))
                .unwrap();
        assert!(prepared.stats.estimated_work < 1_000_000_000.);
        if visible_width_mm == 11.5 {
            at_11_5_mm = Some((
                scene.points.len(),
                prepared.stats.candidate_count,
                prepared.stats.estimated_work,
            ));
        }
    }
    let (points, candidates, work) = at_11_5_mm.unwrap();
    eprintln!("lisong_sdf_point points={points} candidates={candidates} work_at_11_5mm={work}");
    assert!(work < 500_000_000.);
}

#[cfg(target_os = "macos")]
#[test]
fn system_font_catalog_resolves_named_ttc_faces_without_document_mutation() {
    let (mut m, _) = setup();
    let before = m.view.info.clone();
    m.run(Action::FontCatalog);
    let crate::text_tool::Reply::Catalog(result) = m.view.text_reply.as_ref().unwrap().as_ref()
    else {
        panic!()
    };
    let fonts = result.as_ref().unwrap().clone();
    assert!(!fonts.is_empty());
    let arial = fonts
        .iter()
        .find(|f| f.path.ends_with("Arial Unicode.ttf"))
        .unwrap();
    m.run(Action::SystemFont(
        1,
        arial.path.clone(),
        arial.postscript.clone(),
    ));
    let crate::text_tool::Reply::Font { result, .. } = m.view.text_reply.as_ref().unwrap().as_ref()
    else {
        panic!()
    };
    assert_eq!(result.as_ref().unwrap().identity.face_index, 0);
    let collection: Vec<_> = fonts
        .iter()
        .filter(|f| f.path.ends_with("STHeiti Light.ttc"))
        .collect();
    assert!(collection.len() >= 2);
    let mut indices = std::collections::BTreeSet::new();
    for font in collection {
        m.run(Action::SystemFont(
            2,
            font.path.clone(),
            font.postscript.clone(),
        ));
        let crate::text_tool::Reply::Font { result, .. } =
            m.view.text_reply.as_ref().unwrap().as_ref()
        else {
            panic!()
        };
        indices.insert(
            result
                .as_ref()
                .unwrap_or_else(|e| panic!("{}: {e:?}", font.postscript))
                .identity
                .face_index,
        );
    }
    assert!(indices.len() >= 2);
    assert_eq!(m.view.info, before);
    eprintln!(
        "system_font_catalog_count={} resolved_ttc_faces={indices:?}",
        fonts.len()
    );
}

#[test]
fn floating_text_translation_snap_cancel_commit_and_camera_parity() {
    let (mut m, _) = setup();
    let mut d = text_draft(&mut m);
    let request = text_request(&m, &d);
    m.run(Action::TextPreview(request));
    let crate::text_tool::Reply::Preview { result, .. } =
        m.view.text_reply.as_ref().unwrap().as_ref()
    else {
        panic!()
    };
    let preview = result.as_ref().unwrap().clone();
    d.preview = Some(preview.clone());
    assert!(d.start_placement());
    let generation = d.generation;
    let original_text = d.text.clone();
    d.resume_dialog();
    assert!(d.floating.is_none());
    assert_eq!(d.text, original_text);
    assert!(std::sync::Arc::ptr_eq(
        d.preview.as_ref().unwrap(),
        &preview
    ));
    assert_eq!(d.generation, generation);
    assert!(d.start_placement());
    let initial = m.view.info.clone();
    let rect =
        eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(800., 600.));
    let mut camera = crate::camera::Camera::default();
    for i in 0..7 {
        let raw = camera.world(
            eframe::egui::pos2(100. + i as f32 * 37., 200. + i as f32 * 13.),
            rect,
        );
        let point = crate::tools::GridSettings {
            snap_enabled: true,
            ..Default::default()
        }
        .point(raw)
        .unwrap();
        d.floating = Some(point);
        assert_eq!(d.generation, generation);
        assert!(std::sync::Arc::ptr_eq(
            d.preview.as_ref().unwrap(),
            &preview
        ));
        assert!(!d.ready(std::time::Instant::now()));
        let request = d.placement_request().unwrap();
        assert_eq!(request.params.layout.x_mm, point.x_mm);
        assert_eq!(request.params.layout.y_mm, point.y_mm);
        camera.scale *= 1.4;
    }
    assert_eq!(m.view.info, initial);
    let request = d.placement_request().unwrap();
    let anchor = d.floating.unwrap();
    m.run(Action::TextCreate(request));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.info.as_ref().unwrap().undo_entries, 1);
    assert_eq!(m.view.selected.ordered.len(), preview.geometries.len());
    let translated: Vec<_> = m
        .view
        .selected
        .ordered
        .iter()
        .map(|o| o.object.geometry.clone())
        .collect();
    let actual = editor_core::geometries_bounds(&translated, &[])
        .unwrap()
        .unwrap();
    let expected = editor_core::geometries_bounds(&preview.geometries, &[])
        .unwrap()
        .unwrap();
    assert!(
        (actual.min_x_mm - expected.min_x_mm - anchor.x_mm + preview.params.layout.x_mm).abs()
            < 1e-10
    );
    assert!(
        (actual.min_y_mm - expected.min_y_mm - anchor.y_mm + preview.params.layout.y_mm).abs()
            < 1e-10
    );
    m.run(Action::History(false));
    m.run(Action::History(true));
    let state = m.view.info.clone();
    d.cancel();
    assert!(d.floating.is_none() && d.preview.is_none());
    assert_eq!(m.view.info, state);
}

#[test]
fn multiline_stroke_undo_redo_then_chinese_remains_renderable() {
    use eframe::egui;
    let (mut m, _) = setup_source("text-display.gbr", &fixture("s0_polarity.gbr"));
    let rect = egui::Rect::from_min_size(egui::pos2(156., 90.), egui::vec2(786., 658.));
    let camera = crate::camera::Camera {
        center: MmPoint::new(0., 0.),
        scale: 64.17,
    };
    m.run(Action::Viewport(
        camera.center,
        editor_core::BoundsMm {
            min_x_mm: -13.,
            min_y_mm: -11.,
            max_x_mm: 13.,
            max_y_mm: 11.,
        },
        256.,
    ));
    let draft = crate::text_tool::Draft {
        text: "abcABC\n0123".into(),
        height: "1".into(),
        x: "-2.890759".into(),
        y: "-5.003636".into(),
        ..Default::default()
    };
    m.run(Action::TextCreate(text_request(&m, &draft)));
    m.run(Action::History(false));
    m.run(Action::History(true));
    let mut draft = text_draft(&mut m);
    draft.text = "中文\nAB".into();
    draft.height = "1".into();
    draft.x = "2.545322".into();
    draft.y = "-5.003636".into();
    m.run(Action::TextCreate(text_request(&m, &draft)));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let scene = m.view.scene.as_ref().unwrap();
    assert_eq!(scene.objects.len(), 77);
    // All four 1 mm glyphs must have tight sweep bounds, including shallow arcs.
    for o in &scene.objects[73..] {
        assert!(o.bounds[2] - o.bounds[0] < 2.);
        assert!(o.bounds[3] - o.bounds[1] < 2.);
    }
    let flags = crate::gpu::selection_flags(scene, &m.view.selected.ids());
    let prepared =
        crate::gpu::prepare_measured(scene, camera, rect, 2., &flags, MmPoint::new(0., 0.))
            .unwrap();
    eprintln!("multiline display work {}", prepared.stats.estimated_work);
}

#[test]
fn compatibility_open_shows_warning_and_keeps_canvas_editable() {
    let path = std::env::temp_dir().join(format!("rcam-compat-ui-{}.gbr", std::process::id()));
    std::fs::write(
        &path,
        b"%FSLAX26Y26*%%MOMM*%G36*X0Y0D02*X1000000Y0D01*X1000000Y1000000D01*X0Y1000000D01*X0Y10D01*G37*M02*",
    )
    .unwrap();
    let mut model = Model::default();
    model.open(&path).unwrap();
    assert!(model.view.scene.is_some());
    assert!(model.view.blocked.is_none());
    assert!(model.view.message.contains("⚠"));
    assert!(
        model.view.layers[0]
            .import_diagnostics
            .iter()
            .any(|line| line.starts_with("兼容导入："))
    );
    std::fs::remove_file(path).unwrap();
}

fn c3_edit(m: &mut Model, edit: crate::block_ui::Edit) {
    let context = crate::block_ui::Context::capture(&m.view).unwrap();
    m.run(Action::BlockEdit(Box::new(crate::block_ui::Request {
        context,
        edit,
    })));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
}
fn c3_create(m: &mut Model) -> (String, String, String) {
    select_all(m);
    let (layer, ids) = crate::block_ui::create_targets(&m.view).unwrap();
    c3_edit(
        m,
        crate::block_ui::Edit::Create(editor_service::CreateBlockDefinitionParams {
            layer_id: layer.clone(),
            object_ids: ids,
            local_origin_mm: editor_service::PivotMm {
                x_mm: 10.,
                y_mm: 20.,
            },
            name: "中文 Block".into(),
        }),
    );
    (
        layer,
        m.view.block_definitions[0].id.0.clone(),
        m.view.selected.primary().unwrap().object.object_id.clone(),
    )
}
#[test]
fn c3_create_preview_cancel_place_transform_rename_explode_and_delete_workflow() {
    use crate::block_ui::{Context, Edit, Session, SessionKind};
    use editor_service::*;
    let (mut m, dir) = setup();
    let before = m.view.snap_snapshot.clone().unwrap();
    let original = m.view.info.clone().unwrap();
    let (layer, id, instance) = c3_create(&mut m);
    assert_eq!(
        m.view.info.as_ref().unwrap().undo_entries,
        original.undo_entries + 1
    );
    assert_eq!(m.view.apertures, before.apertures);
    assert_eq!(m.view.block_counts[&id], 1);
    let created = m.view.snap_snapshot.clone().unwrap();
    m.run(Action::History(false));
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
    assert!(m.view.block_definitions.is_empty());
    m.run(Action::History(true));
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers,
        created.layers
    );
    assert_eq!(m.view.block_definitions, created.block_definitions);
    let context = Context::capture(&m.view).unwrap();
    let pre = m.view.info.clone();
    m.run(Action::BlockPreview(context.clone(), id.clone(), 100.));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let preview = m.view.block_preview.clone().unwrap();
    assert!(!preview.paths.is_empty());
    let mut session = Session {
        context: context.clone(),
        layer: layer.clone(),
        kind: SessionKind::Place {
            definition: id.clone(),
        },
        point: None,
        preview: Some(preview.clone()),
    };
    for i in 0..100 {
        session.point = Some(MmPoint::new(i as f64, 8.));
        assert!(session.valid(&m.view));
    }
    drop(session);
    assert_eq!(m.view.info, pre);
    assert!(std::sync::Arc::ptr_eq(
        &preview,
        m.view.block_preview.as_ref().unwrap()
    ));
    for x in [30., 40., 50.] {
        c3_edit(
            &mut m,
            Edit::Place(CreateBlockInstanceParams {
                layer_id: layer.clone(),
                definition_id: id.clone(),
                transform: BlockTransformParams {
                    translation_mm: PivotMm { x_mm: x, y_mm: 20. },
                    rotation_deg: 0.,
                    mirror: false,
                },
            }),
        );
    }
    assert_eq!(m.view.block_counts[&id], 4);
    assert_eq!(m.view.block_definitions[0], created.block_definitions[0]);
    c3_edit(
        &mut m,
        Edit::Rename(RenameBlockDefinitionParams {
            definition_id: id.clone(),
            name: "重命名 μ".into(),
        }),
    );
    let transform = BlockTransformParams {
        translation_mm: PivotMm { x_mm: 5., y_mm: 6. },
        rotation_deg: 37.,
        mirror: true,
    };
    c3_edit(
        &mut m,
        Edit::Transform(UpdateBlockInstanceTransformParams {
            layer_id: layer.clone(),
            object_id: instance.clone(),
            transform,
        }),
    );
    let project = dir.join("blocks.rcam");
    m.run(Action::SaveProject(Some(project.clone()), false, None));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let saved = m.view.snap_snapshot.clone().unwrap();
    m.run(Action::OpenProject(project, true));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, saved.layers);
    assert_eq!(m.view.block_definitions, saved.block_definitions);
    let ctx = Context::capture(&m.view).unwrap();
    let info = m.view.info.clone();
    m.run(Action::BlockEdit(Box::new(crate::block_ui::Request {
        context: ctx,
        edit: Edit::Delete(BlockDefinitionIdParams {
            definition_id: id.clone(),
        }),
    })));
    assert_eq!(
        m.view.error.as_ref().unwrap().code,
        "BLOCK_DEFINITION_REFERENCED"
    );
    assert_eq!(m.view.info, info);
    m.run(Action::BlockSelect(id.clone()));
    assert_eq!(m.view.selected.ordered.len(), 4);
    let targets: Vec<_> = m
        .view
        .selected
        .ordered
        .iter()
        .map(|o| o.object.object_id.clone())
        .collect();
    for object_id in targets {
        c3_edit(
            &mut m,
            Edit::Explode(ExplodeBlockInstanceParams {
                layer_id: layer.clone(),
                object_id,
            }),
        );
    }
    assert_eq!(m.view.block_counts.get(&id), None);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers[0]
            .objects
            .len(),
        8
    );
    c3_edit(
        &mut m,
        Edit::Delete(BlockDefinitionIdParams {
            definition_id: id.clone(),
        }),
    );
    assert!(m.view.block_definitions.is_empty());
    m.run(Action::History(false));
    assert_eq!(m.view.block_definitions[0].id.0, id);
}
#[test]
fn c3_session_rejects_stale_context_and_nested_selection() {
    use crate::block_ui::{Context, Edit, Request, Session, SessionKind};
    let (mut m, _) = setup();
    let (layer, id, _) = c3_create(&mut m);
    assert!(
        crate::block_ui::create_targets(&m.view)
            .unwrap_err()
            .contains("嵌套")
    );
    let context = Context::capture(&m.view).unwrap();
    let session = Session {
        context: context.clone(),
        layer,
        kind: SessionKind::Place {
            definition: id.clone(),
        },
        point: Some(MmPoint::new(0., 0.)),
        preview: None,
    };
    assert!(session.valid(&m.view));
    m.run(Action::History(false));
    assert!(!session.valid(&m.view));
    let before = m.view.info.clone();
    m.run(Action::BlockEdit(Box::new(Request {
        context,
        edit: Edit::Delete(editor_service::BlockDefinitionIdParams { definition_id: id }),
    })));
    assert!(m.view.error.is_some());
    assert_eq!(m.view.info, before);
}

#[test]
fn c3_mixed_capture_keeps_identical_render_input_and_recovery_export_geometry() {
    let (mut m, dir) = setup_source("blocks.gbr", &fixture("s4c3/blocks.gbr"));
    let before = m.view.snap_snapshot.clone().unwrap();
    let scene_before = m.view.scene.clone().unwrap();
    let bounds = m.view.bounds;
    let (layer, _, _) = c3_create(&mut m);
    assert_eq!(m.view.bounds, bounds);
    let after = m.view.scene.as_ref().unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&scene_before.objects),
        bytemuck::cast_slice::<_, u8>(&after.objects)
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&scene_before.primitives),
        bytemuck::cast_slice::<_, u8>(&after.primitives)
    );
    assert_eq!(scene_before.points, after.points);
    let d = m.view.info.clone().unwrap();
    let bytes = m.service.project_recovery_bytes(&d.document_id).unwrap();
    let project = rcam_project::decode(&bytes).unwrap();
    assert_eq!(project.block_definitions, m.view.block_definitions);
    let file = dir.join("flatten.gbr");
    m.run(Action::Save(file.clone(), layer, None));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("%AB"));
    let reopened = gerber_io::parse_s1(&std::fs::read(&file).unwrap(), "reopened")
        .unwrap()
        .document;
    assert_eq!(
        reopened.layers[0].objects.len(),
        before.layers[0].objects.len()
    );
    assert_eq!(reopened.manufacturing_bounds(None).unwrap(), bounds);
    // Independent world probes: two known Flash centers, line midspan and Region interior.
    for point in [
        MmPoint::new(0., 0.),
        MmPoint::new(6., 0.),
        MmPoint::new(3., 4.),
        MmPoint::new(11., 2.),
    ] {
        assert!(
            !reopened
                .hit_test(&reopened.layers[0].id, point, 0.)
                .unwrap()
                .is_empty()
        );
    }
    let snap = m.view.snap_snapshot.clone().unwrap();
    m.run(Action::RestoreProject(bytes));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, snap.layers);
    assert_eq!(m.view.block_definitions, snap.block_definitions);
}

fn selected_flash_center(m: &Model, object_id: &str) -> MmPoint {
    let object = m
        .view
        .selected
        .ordered
        .iter()
        .find(|item| item.object.object_id == object_id)
        .unwrap();
    match object.object.geometry {
        SemanticGeometry::Flash { center, .. } => center,
        _ => panic!("fixture object should be a Flash"),
    }
}

#[test]
fn arrangement_command_dispatch_aligns_one_transaction_and_undo_redoes_anchor_geometry() {
    use editor_core::command::ids;
    use editor_service::AlignmentMode;

    let (mut m, _) = setup();
    select_all(&mut m);
    let ordered: Vec<_> = m
        .view
        .selected
        .ordered
        .iter()
        .map(|item| item.object.object_id.clone())
        .collect();
    let anchor = m.view.selected.primary().unwrap().object.object_id.clone();
    let before = m.view.info.clone().unwrap();

    let action = crate::arrangement_action(ids::OBJECT_ALIGN_LEFT).unwrap();
    assert!(matches!(action, Action::Align(AlignmentMode::Left)));
    m.run(action);
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(
        m.view.info.as_ref().unwrap().undo_entries,
        before.undo_entries + 1
    );
    assert_eq!(m.view.info.as_ref().unwrap().revision, "1");
    assert_eq!(selected_flash_center(&m, &anchor), MmPoint::new(20., 20.));
    assert_eq!(
        selected_flash_center(&m, &ordered[0]),
        MmPoint::new(19., 20.)
    );

    let after_align = m.view.info.clone();
    m.run(Action::History(false));
    assert_eq!(
        selected_flash_center(&m, &ordered[0]),
        MmPoint::new(10., 20.)
    );
    assert_eq!(selected_flash_center(&m, &anchor), MmPoint::new(20., 20.));
    m.run(Action::History(true));
    assert_eq!(
        selected_flash_center(&m, &ordered[0]),
        MmPoint::new(19., 20.)
    );
    assert_eq!(selected_flash_center(&m, &anchor), MmPoint::new(20., 20.));

    let before_noop = m.view.info.clone();
    m.run(crate::arrangement_action(ids::OBJECT_ALIGN_LEFT).unwrap());
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.info, before_noop);
    assert_ne!(after_align, before_noop);
}

#[test]
fn arrangement_anchor_follows_selection_order_and_menu_gates_use_logical_text_groups() {
    let (mut m, _) = setup();
    select_all(&mut m);
    let original_anchor = m.view.selected.primary().unwrap().object.object_id.clone();
    let (anchor, moving) = {
        let selected = &mut m.view.selected.ordered;
        selected.reverse();
        (
            selected[1].object.object_id.clone(),
            selected[0].object.object_id.clone(),
        )
    };
    assert_ne!(anchor, original_anchor);
    assert_eq!(m.view.selected.primary().unwrap().object.object_id, anchor);
    let eligibility = crate::state::arrangement_eligibility(&m.view);
    assert!(eligibility.align);
    assert!(!eligibility.distribute);
    assert_eq!(eligibility.logical_count, 2);
    m.run(crate::arrangement_action(editor_core::command::ids::OBJECT_ALIGN_LEFT).unwrap());
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(selected_flash_center(&m, &anchor), MmPoint::new(10., 20.));
    assert_eq!(selected_flash_center(&m, &moving), MmPoint::new(11., 20.));

    let mut text_model = setup().0;
    select_all(&mut text_model);
    let operation = "synthetic-text-operation";
    let snapshot = std::sync::Arc::make_mut(text_model.view.snap_snapshot.as_mut().unwrap());
    for object in &mut snapshot.layers[0].objects {
        object.origin = editor_core::ObjectOrigin::GeneratedText {
            operation_id: operation.into(),
        };
    }
    for selected in &mut text_model.view.selected.ordered {
        selected.object.origin = editor_core::ObjectOrigin::GeneratedText {
            operation_id: operation.into(),
        };
    }
    let one_text_group = crate::state::arrangement_eligibility(&text_model.view);
    assert_eq!(one_text_group.logical_count, 1);
    assert!(!one_text_group.align);

    let mut extra = text_model.view.selected.ordered[0].clone();
    extra.object.object_id = "synthetic-extra-object".into();
    extra.object.origin = editor_core::ObjectOrigin::Imported { command_index: 0 };
    text_model.view.selected.ordered.push(extra.clone());
    let two_units = crate::state::arrangement_eligibility(&text_model.view);
    assert_eq!(two_units.logical_count, 2);
    assert!(two_units.align);
    assert!(!two_units.distribute);
    extra.object.object_id = "synthetic-extra-object-2".into();
    text_model.view.selected.ordered.push(extra);
    assert!(crate::state::arrangement_eligibility(&text_model.view).distribute);

    text_model.view.selected.ordered.pop();
    text_model.view.selected.ordered.pop();
    text_model.view.selected.ordered.pop();
    let incomplete_text_group = crate::state::arrangement_eligibility(&text_model.view);
    assert_eq!(incomplete_text_group.logical_count, 0);
    assert!(!incomplete_text_group.align);
}

#[test]
fn arrangement_menu_disables_locked_hidden_and_cross_layer_selections() {
    let (mut m, _) = setup();
    select_all(&mut m);
    assert!(crate::state::arrangement_eligibility(&m.view).align);

    patch(&mut m, None, Some(true), None);
    assert!(!crate::state::arrangement_eligibility(&m.view).align);

    let (mut hidden, _) = setup();
    select_all(&mut hidden);
    patch(&mut hidden, Some(false), None, None);
    assert!(!crate::state::arrangement_eligibility(&hidden.view).align);

    let (mut cross_layer, _) = setup();
    select_all(&mut cross_layer);
    cross_layer.view.selected.ordered[1].layer_id = "another-layer".into();
    assert!(!crate::state::arrangement_eligibility(&cross_layer.view).align);
}

#[test]
fn all_arrangement_command_ids_dispatch_to_their_typed_actions() {
    use editor_core::command::ids;
    use editor_service::{AlignmentMode as A, DistributionAxis as D};

    for (command, expected) in [
        (ids::OBJECT_ALIGN_LEFT, "left"),
        (ids::OBJECT_ALIGN_RIGHT, "right"),
        (ids::OBJECT_ALIGN_TOP, "top"),
        (ids::OBJECT_ALIGN_BOTTOM, "bottom"),
        (ids::OBJECT_ALIGN_HCENTER, "hcenter"),
        (ids::OBJECT_ALIGN_VCENTER, "vcenter"),
        (ids::OBJECT_DISTRIBUTE_HORIZONTAL, "horizontal"),
        (ids::OBJECT_DISTRIBUTE_VERTICAL, "vertical"),
    ] {
        let action = crate::arrangement_action(command).unwrap();
        let actual = match action {
            Action::Align(A::Left) => "left",
            Action::Align(A::Right) => "right",
            Action::Align(A::Top) => "top",
            Action::Align(A::Bottom) => "bottom",
            Action::Align(A::HCenter) => "hcenter",
            Action::Align(A::VCenter) => "vcenter",
            Action::Distribute(D::Horizontal) => "horizontal",
            Action::Distribute(D::Vertical) => "vertical",
            _ => panic!("command did not map to an arrangement action"),
        };
        assert_eq!(actual, expected, "wrong mapping for {}", command.0);
    }
}

#[test]
fn estimated_pixel_work_above_old_budget_is_diagnostic_only() {
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2a3/gui_primitives.gbr"),
    )
    .unwrap();
    let rect = eframe::egui::Rect::from_min_size(
        eframe::egui::Pos2::ZERO,
        eframe::egui::vec2(100_000., 100_000.),
    );
    let mut camera = crate::camera::Camera::default();
    camera.fit(m.view.bounds, rect);
    let scene = m.view.scene.as_ref().unwrap();
    let prepared = crate::gpu::prepare_measured(
        scene,
        camera,
        rect,
        1.,
        &vec![0; scene.objects.len()],
        MmPoint::new(0., 0.),
    )
    .unwrap();
    assert!(prepared.stats.estimated_work > 2_000_000_000.);
}

#[test]
fn rectangle_lookup_preserves_order_and_old_selection_on_invalid_query() {
    let (mut m, _) = setup();
    let bounds = m.view.bounds.unwrap();
    let before = m.view.info.clone();
    m.run(Action::SelectRect(
        bounds,
        editor_core::hit_test::SelectRectMode::Crossing,
    ));
    assert!(m.view.error.is_none());
    assert!(!m.view.selected.ordered.is_empty());
    let selected = m.view.selected.ordered.clone();
    assert_eq!(m.view.info, before);
    let invalid = editor_core::BoundsMm {
        min_x_mm: f64::NAN,
        ..bounds
    };
    m.run(Action::SelectRect(
        invalid,
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert!(m.view.error.is_some());
    assert_eq!(m.view.selected.ordered, selected);
    assert_eq!(m.view.info, before);
}

#[test]
#[ignore = "requires explicit private read-only input manifest"]
fn real_large_rectangle_selection_regression() {
    use editor_core::hit_test::SelectRectMode::{Crossing, Window};
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("RCAM_LARGE_INPUTS").unwrap()).unwrap(),
    )
    .unwrap();
    for kind in ["set", "project"] {
        let mut m = Model::default();
        if kind == "set" {
            let paths: Vec<_> = manifest["set"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| PathBuf::from(p.as_str().unwrap()))
                .collect();
            m.import_gerbers(&paths).unwrap();
        } else {
            m.open_project(
                std::path::Path::new(manifest["project"].as_str().unwrap()),
                true,
            )
            .unwrap();
        }
        let before = m.view.info.clone();
        let bounds = m.view.bounds.unwrap();
        let c = bounds.center();
        let small = editor_core::BoundsMm {
            min_x_mm: c.x_mm - 5.,
            max_x_mm: c.x_mm + 5.,
            min_y_mm: c.y_mm - 5.,
            max_y_mm: c.y_mm + 5.,
        };
        for (region, r) in [("small", small), ("full", bounds)] {
            let mut previous = Vec::new();
            for mode in [Window, Crossing] {
                let start = std::time::Instant::now();
                m.run(Action::SelectRect(r, mode));
                eprintln!(
                    "RECT_REPRO kind={kind} region={region} mode={mode:?} ms={:.3} selected={} error={:?}",
                    start.elapsed().as_secs_f64() * 1000.,
                    m.view.selected.ordered.len(),
                    m.view.error
                );
                assert!(m.view.error.is_none(), "{:?}", m.view.error);
                assert!(m.view.blocked.is_none());
                assert_eq!(m.view.info, before);
                let ids: Vec<_> = m
                    .view
                    .selected
                    .ordered
                    .iter()
                    .map(|o| (o.layer_id.clone(), o.object.object_id.clone()))
                    .collect();
                if mode == Window {
                    previous = ids;
                } else {
                    let crossing: std::collections::HashSet<_> = ids.into_iter().collect();
                    assert!(previous.iter().all(|id| crossing.contains(id)));
                    if region == "full" {
                        assert!(crossing.len() > 100000);
                    }
                }
            }
        }
    }
}
