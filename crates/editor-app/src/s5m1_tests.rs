//! Manufacturing truth and viewport-cache regression tests for S5-M1.
use crate::{
    selection::SelectionMode,
    state::{Action, Model},
};
use editor_core::{BoundsMm, MmPoint};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn open(source: &[u8]) -> (Model, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "rcam-s5m1-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("synthetic.gbr");
    std::fs::write(&path, source).unwrap();
    let mut model = Model::default();
    model.open(&path).unwrap();
    (model, dir)
}
fn view(model: &mut Model, bounds: BoundsMm) {
    model.run(Action::Viewport(bounds.center(), bounds, 4.));
    assert!(model.view.error.is_none());
    assert!(model.view.blocked.is_none());
}
fn bounds(a: f64, b: f64, c: f64, d: f64) -> BoundsMm {
    BoundsMm {
        min_x_mm: a,
        min_y_mm: b,
        max_x_mm: c,
        max_y_mm: d,
    }
}
const ORDERED:&[u8]=b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\nX1000000Y1000000D03*\n%LPC*%\nX2000000Y1000000D03*\n%LPD*%\nX3000000Y1000000D03*\nM02*\n";
#[test]
fn complete_viewport_preserves_dark_clear_dark_and_partial_query_stays_partial() {
    let (mut m, dir) = open(ORDERED);
    view(&mut m, bounds(0., 0., 4., 2.));
    assert!(m.view.render_coverage_complete);
    let ids = m.view.scene.as_ref().unwrap().ids.clone();
    assert_eq!(ids.len(), 3);
    assert!(
        ids.iter()
            .enumerate()
            .all(|(i, id)| id == &format!("object-{}", i + 1))
    );
    let snapshot = m.view.snap_snapshot.as_ref().unwrap();
    assert_eq!(
        snapshot.layers[0].objects[1].exposure,
        editor_core::Exposure::Clear
    );
    view(&mut m, bounds(0.8, 0.8, 1.2, 1.2));
    assert!(!m.view.render_coverage_complete);
    assert_eq!(m.view.scene.as_ref().unwrap().ids, vec![ids[0].clone()]);
    view(&mut m, bounds(0., 0., 4., 2.));
    assert!(m.view.render_coverage_complete);
    assert_eq!(m.view.scene.as_ref().unwrap().ids, ids);
    m.run(Action::SetAllLayersVisible(false));
    assert!(m.view.error.is_none());
    assert!(!m.view.render_coverage_complete);
    assert!(m.view.scene.as_ref().unwrap().ids.is_empty());
    m.run(Action::SetAllLayersVisible(true));
    assert!(m.view.error.is_none());
    assert!(m.view.render_coverage_complete);
    assert_eq!(m.view.scene.as_ref().unwrap().ids, ids);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn full_scene_selection_reuses_geometry_and_edit_invalidates_complete_coverage() {
    let (mut m, dir) = open(ORDERED);
    view(&mut m, bounds(0., 0., 4., 2.));
    let scene = m.view.scene.clone().unwrap();
    let baseline = m.view.info.clone().unwrap();
    m.run(Action::Select(
        MmPoint::new(1., 1.),
        0.,
        SelectionMode::Replace,
    ));
    assert!(m.view.error.is_none());
    assert_eq!(m.view.selected.ordered.len(), 1);
    assert!(Arc::ptr_eq(&scene, m.view.scene.as_ref().unwrap()));
    assert!(m.view.render_coverage_complete);
    let after = m.view.info.as_ref().unwrap();
    assert_eq!(baseline.revision, after.revision);
    assert_eq!(baseline.project_dirty, after.project_dirty);
    assert_eq!(baseline.undo_entries, after.undo_entries);
    m.run(Action::Move("10".into(), "0".into()));
    assert!(m.view.error.is_none());
    assert!(!m.view.render_coverage_complete);
    assert_ne!(m.view.scene.as_ref().unwrap().serial, scene.serial);
    assert_eq!(m.view.scene.as_ref().unwrap().ids.len(), 2);
    m.run(Action::History(false));
    assert!(m.view.error.is_none());
    assert!(m.view.render_coverage_complete);
    assert_eq!(m.view.scene.as_ref().unwrap().ids, scene.ids);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn frozen_p10k_shapes_and_p100k_point_truth_match_manufacturing_import() {
    let (manifest, protocol): (serde_json::Value, serde_json::Value) = (
        serde_json::from_slice(include_bytes!(
            "../../../fixtures/synthetic/s5m1/manifest.json"
        ))
        .unwrap(),
        serde_json::from_slice(include_bytes!(
            "../../../fixtures/synthetic/s5m1/protocol.json"
        ))
        .unwrap(),
    );
    assert_eq!(
        editor_core::hash::sha256_hex(include_bytes!(
            "../../../fixtures/synthetic/s5m1/protocol.json"
        )),
        manifest["protocol_sha256"]
    );
    for (bytes, count, index) in [
        (
            include_bytes!("../../../fixtures/synthetic/s5m1/P10K_CROP.gbr").as_slice(),
            10000,
            0,
        ),
        (
            include_bytes!("../../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr").as_slice(),
            100000,
            1,
        ),
    ] {
        assert_eq!(
            editor_core::hash::sha256_hex(bytes),
            manifest["fixtures"][index]["sha256"]
        );
        let (m, dir) = open(bytes);
        let s = m.view.snap_snapshot.as_ref().unwrap();
        let objects = &s.layers[0].objects;
        assert_eq!(objects.len(), count);
        assert_eq!(s.apertures.len(), if count == 10000 { 4 } else { 1 });
        let b = m.view.bounds.unwrap();
        assert_eq!(
            [b.min_x_mm, b.min_y_mm, b.max_x_mm, b.max_y_mm],
            if count == 10000 {
                [0.75, 0.75, 100.25, 100.25]
            } else {
                [0.75, 0.75, 1000.25, 100.25]
            }
        );
        if count == 100000 {
            for p in protocol["point_positions"].as_array().unwrap() {
                let ordinal = p["ordinal"].as_u64().unwrap() as usize;
                let object = &objects[ordinal - 1];
                assert_eq!(object.object_id, format!("object-{ordinal}"));
                let editor_core::SemanticGeometry::Flash {
                    center: position, ..
                } = &object.geometry
                else {
                    panic!("frozen Flash")
                };
                assert_eq!(position.x_mm, p["x_mm"].as_f64().unwrap());
                assert_eq!(position.y_mm, p["y_mm"].as_f64().unwrap());
            }
        } else {
            let mut counts = std::collections::BTreeMap::new();
            for o in objects {
                let editor_core::SemanticGeometry::Flash { aperture_id, .. } = &o.geometry else {
                    panic!("frozen Flash")
                };
                *counts.entry(aperture_id).or_insert(0) += 1;
            }
            assert_eq!(counts.len(), 4);
            assert!(counts.values().all(|c| *c == 2500));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn complete_scene_far_pan_rebases_with_existing_precision_guard_without_manufacturing_changes() {
    let (mut m, dir) = open(ORDERED);
    view(&mut m, bounds(0., 0., 4., 2.));
    let baseline = m.view.info.clone().unwrap();
    let original = m.view.snap_snapshot.clone().unwrap();
    let ids = m.view.scene.as_ref().unwrap().ids.clone();
    let near = crate::camera::Camera {
        center: MmPoint::new(80., 8.),
        scale: 2.,
    };
    let far = crate::camera::Camera {
        center: MmPoint::new(100_000.123_45, 10_000.123_45),
        scale: 2.,
    };
    assert!(m.view.render_coverage_complete);
    assert!(!crate::viewport_requires_rebase(
        m.view.scene.as_ref().unwrap(),
        near
    ));
    assert!(crate::viewport_requires_rebase(
        m.view.scene.as_ref().unwrap(),
        far
    ));
    view(
        &mut m,
        bounds(
            far.center.x_mm - 2.,
            far.center.y_mm - 1.,
            far.center.x_mm + 2.,
            far.center.y_mm + 1.,
        ),
    );
    assert!(!m.view.render_coverage_complete);
    assert!(m.view.scene.as_ref().unwrap().ids.is_empty());
    assert!(!crate::viewport_requires_rebase(
        m.view.scene.as_ref().unwrap(),
        far
    ));
    view(&mut m, bounds(0., 0., 4., 2.));
    assert!(m.view.render_coverage_complete);
    assert_eq!(m.view.scene.as_ref().unwrap().ids, ids);
    assert_eq!(
        serde_json::to_vec(m.view.snap_snapshot.as_ref().unwrap().as_ref()).unwrap(),
        serde_json::to_vec(original.as_ref()).unwrap()
    );
    let after = m.view.info.as_ref().unwrap();
    assert_eq!(baseline.revision, after.revision);
    assert_eq!(baseline.workspace_revision, after.workspace_revision);
    assert_eq!(baseline.dirty, after.dirty);
    assert_eq!(baseline.project_dirty, after.project_dirty);
    assert_eq!(baseline.undo_entries, after.undo_entries);
    std::fs::remove_dir_all(dir).unwrap();
}
