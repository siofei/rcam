//! Real service transaction + immutable preview regression for S5-M2-B.
use crate::{
    camera::Camera,
    drag::Drag,
    gpu,
    state::{Action, Model},
};
use editor_core::{BoundsMm, MmPoint, SemanticGeometry, hit_test::SelectRectMode};
use eframe::egui::{Pos2, Rect, vec2};
use std::{sync::Arc, time::Instant};

fn selection_bounds(n: usize) -> BoundsMm {
    BoundsMm {
        min_x_mm: 0.5,
        min_y_mm: 0.5,
        max_x_mm: n.min(1000) as f64 + 0.5,
        max_y_mm: n.div_ceil(1000) as f64 + 0.5,
    }
}
fn open() -> Model {
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr"),
    )
    .unwrap();
    m
}
fn matrix(sizes: &[usize]) {
    let mut m = open();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(800., 450.));
    let camera = Camera {
        center: MmPoint::new(500.5, 50.5),
        scale: 0.74,
    };
    for &n in sizes {
        m.run(Action::SelectRect(
            selection_bounds(n),
            SelectRectMode::Window,
        ));
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        assert_eq!(m.view.selected.ordered.len(), n);
        let before = m.view.snap_snapshot.clone().unwrap();
        let info = m.view.info.clone().unwrap();
        let scene = m.view.scene.clone().unwrap();
        let start = camera.screen(MmPoint::new(1., 1.), rect);
        let mut drag = Drag::arm(&m.view, start, camera, rect, 2.).unwrap();
        m.run(Action::ProbeDrag(MmPoint::new(1., 1.), 0.001));
        assert!(m.view.drag_hit);
        drag.confirmed = true;
        let flags = gpu::selection_flags(&scene, &m.view.selected.ids());
        let mut prepare_ms = vec![];
        for frame in 1..=600 {
            drag.update(start + vec2(frame as f32 / 600. * 36., -frame as f32 / 600. * 18.));
            if frame % 20 == 0 {
                let prepared =
                    gpu::prepare_measured(&scene, camera, rect, 2., &flags, drag.delta).unwrap();
                assert!(Arc::ptr_eq(&scene.index, &prepared.index));
                assert_eq!(prepared.stats.preview_index_ms, 0.);
                prepare_ms.push(prepared.stats.cpu_prepare_ms);
            }
        }
        assert_eq!(m.view.info.as_ref().unwrap(), &info);
        assert!(Arc::ptr_eq(&scene, m.view.scene.as_ref().unwrap()));
        assert!(Arc::ptr_eq(&before, m.view.snap_snapshot.as_ref().unwrap()));
        let delta = drag.delta;
        let t = Instant::now();
        m.run(drag.release().unwrap());
        let commit = t.elapsed().as_secs_f64() * 1000.;
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
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
        for (i, (a, b)) in before.layers[0]
            .objects
            .iter()
            .zip(&moved.layers[0].objects)
            .enumerate()
        {
            let mut expected = a.clone();
            if i < n {
                let SemanticGeometry::Flash { center, .. } = &mut expected.geometry else {
                    panic!("fixed circles fixture")
                };
                center.x_mm += delta.x_mm;
                center.y_mm += delta.y_mm;
            }
            assert_eq!(&expected, b, "n={n}, object={i}");
        }
        let t = Instant::now();
        m.run(Action::History(false));
        let undo = t.elapsed().as_secs_f64() * 1000.;
        assert!(m.view.error.is_none());
        assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
        assert_eq!(
            m.view.info.as_ref().unwrap().undo_entries,
            info.undo_entries
        );
        assert_eq!(m.view.info.as_ref().unwrap().redo_entries, 1);
        let t = Instant::now();
        m.run(Action::History(true));
        let redo = t.elapsed().as_secs_f64() * 1000.;
        assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, moved.layers);
        assert_eq!(
            m.view.info.as_ref().unwrap().undo_entries,
            info.undo_entries + 1
        );
        assert_eq!(m.view.info.as_ref().unwrap().redo_entries, 0);
        println!(
            "BATCH_DRAG {}",
            serde_json::json!({"selected":n,"total":100000,"commit_ms":commit,"undo_ms":undo,"redo_ms":redo,"prepare_ms":prepare_ms})
        );
        m.run(Action::History(false));
    }
}
#[test]
fn batch_drag_large_selection_transaction_regression() {
    matrix(&[1000, 5000]);
}
#[test]
#[ignore = "release-only fixed four-size measurement; retain raw log"]
fn batch_drag_release_measurement() {
    assert!(!cfg!(debug_assertions));
    matrix(&[100, 500, 1000, 5000]);
}

fn armed(m: &Model) -> Drag {
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(800., 450.));
    let mut drag = Drag::arm(&m.view, rect.center(), Camera::default(), rect, 2.).unwrap();
    drag.confirmed = true;
    drag.update(rect.center() + vec2(20., -10.));
    drag
}
#[test]
fn batch_drag_mixed_geometry_blocks_and_complete_text_restore_exactly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    for fixture in ["s2a3/gui_primitives.gbr", "s4c4/native-blocks.rcam"] {
        let mut m = Model::default();
        if fixture.ends_with("rcam") {
            m.open_project(&root.join(fixture), true).unwrap();
        } else {
            m.open(&root.join(fixture)).unwrap();
        }
        let d = m.view.info.clone().unwrap();
        let mut params:editor_service::TextParams=serde_json::from_value(serde_json::json!({
            "layer_id":m.view.layers[0].layer_id,"font":editor_service::builtin_stroke_font().identity,
            "layout":{"text":"AB8","height_mm":3.,"tracking_mm":0.1,"x_mm":60.,"y_mm":20.,"rotation_deg":0.,"h_align":"left","v_align":"baseline"}
        })).unwrap();
        params.layout.curve_tolerance_mm = 0.0001;
        let created = m
            .service
            .text_create(&d.document_id, &d.revision, params)
            .unwrap();
        m.refresh(true).unwrap();
        m.run(Action::SelectRect(
            m.view.bounds.unwrap(),
            SelectRectMode::Window,
        ));
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        assert!(
            created.generated_object_ids.iter().all(|id| m
                .view
                .selected
                .ids()
                .contains(&id.as_str()))
        );
        let before = m.view.snap_snapshot.clone().unwrap();
        let info = m.view.info.clone().unwrap();
        let drag = armed(&m);
        m.run(drag.release().unwrap());
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        let after = m.view.snap_snapshot.clone().unwrap();
        assert_ne!(before.layers, after.layers);
        assert_eq!(before.apertures, after.apertures);
        assert_eq!(before.block_definitions, after.block_definitions);
        assert_eq!(
            m.view.info.as_ref().unwrap().undo_entries,
            info.undo_entries + 1
        );
        m.run(Action::History(false));
        assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
        m.run(Action::History(true));
        assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, after.layers);
    }
}
#[test]
fn batch_drag_cancellation_context_and_atomic_refusal() {
    let mut m = open();
    m.run(Action::SelectRect(
        selection_bounds(5000),
        SelectRectMode::Window,
    ));
    let before = m.view.snap_snapshot.clone().unwrap();
    let info = m.view.info.clone();
    for (escape, focused, gone, down, released) in [
        (true, true, false, false, true),
        (false, false, false, false, true),
        (false, true, true, false, true),
        (false, true, false, false, false),
    ] {
        let drag = armed(&m);
        assert!(crate::drag::cancelled(
            escape, focused, gone, down, released
        ));
        drop(drag);
        assert_eq!(m.view.info, info);
        assert!(Arc::ptr_eq(m.view.snap_snapshot.as_ref().unwrap(), &before));
    }
    // Existing permission policies apply to all targets, never a valid subset.
    for altered in [0, 1, 2, 3] {
        let mut view = m.view.clone();
        match altered {
            0 => view.layers[0].locked = true,
            1 => view.layers[0].visible = false,
            2 => view.layers[0].selectable = false,
            _ => view.selected.ordered[1].layer_id = "other-layer".into(),
        }
        assert!(Drag::arm(&view, Pos2::ZERO, Camera::default(), Rect::EVERYTHING, 2.).is_none());
    }
    let mut invalid = armed(&m);
    invalid.delta.x_mm = f64::NAN;
    m.run(invalid.release().unwrap());
    assert!(m.view.error.is_some());
    assert_eq!(m.view.info, info);
    assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
    m.view.error = None;
    let stale = armed(&m);
    let commit = armed(&m);
    m.run(commit.release().unwrap());
    assert!(m.view.error.is_none());
    let edited = m.view.info.clone();
    let geometry = m.view.snap_snapshot.clone().unwrap();
    m.run(stale.release().unwrap());
    assert_eq!(m.view.error.as_ref().unwrap().code, "REVISION_CONFLICT");
    assert_eq!(m.view.info, edited);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().layers,
        geometry.layers
    );
    m.view.error = None;
    let old = armed(&m);
    m.run(Action::DiscardNewWorkspace);
    let empty = m.view.info.clone();
    m.run(old.release().unwrap());
    assert!(m.view.error.is_some());
    assert_eq!(m.view.info, empty);
}
