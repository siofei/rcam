use crate::state::{Action, Model};
use editor_core::{MmPoint, SemanticGeometry};
use editor_service::LayerUpdateParams;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2*%\n%ADD11C,4X2*%\nD10*\nX10000000Y20000000D03*\nD11*\nX20000000Y20000000D03*\nM02*\n";
fn setup() -> (Model, PathBuf) {
    let base = std::env::var_os("RCAM_GUI_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "gui-state-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("中文 # source.gbx");
    std::fs::write(&path, SOURCE).unwrap();
    let mut m = Model::default();
    m.open(&path).unwrap();
    (m, dir)
}
fn select(m: &mut Model) {
    m.select(MmPoint::new(10., 20.), 0.).unwrap();
    assert!(m.view.selected.is_some());
}
fn patch(m: &mut Model, visible: Option<bool>, locked: Option<bool>, name: Option<String>) {
    let d = m.view.info.as_ref().unwrap();
    let p = LayerUpdateParams {
        layer_id: d.layer_ids[0].clone(),
        expected_workspace_revision: d.workspace_revision.clone(),
        display_name: name,
        visible,
        locked,
    };
    m.run(Action::Layer(p));
    assert!(m.view.error.is_none());
}
fn center(m: &Model) -> MmPoint {
    match m.view.selected.as_ref().unwrap().object.geometry {
        SemanticGeometry::Flash { center, .. } => center,
        _ => panic!(),
    }
}
#[test]
fn selection_clear_on_empty_click() {
    let (mut m, _) = setup();
    select(&mut m);
    m.select(MmPoint::new(200., 200.), 0.).unwrap();
    assert!(m.view.selected.is_none());
}
#[test]
fn selection_hole_center_not_selected() {
    let (mut m, _) = setup();
    m.select(MmPoint::new(20., 20.), 0.01).unwrap();
    assert!(m.view.selected.is_none());
}
#[test]
fn selection_skips_hidden_layers() {
    let (mut m, _) = setup();
    select(&mut m);
    patch(&mut m, Some(false), None, None);
    assert!(m.view.selected.is_none());
    m.select(MmPoint::new(10., 20.), 0.).unwrap();
    assert!(m.view.selected.is_none());
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
    let id = m.view.selected.as_ref().unwrap().object.object_id.clone();
    m.numeric_move("5", "-3").unwrap();
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    m.run(Action::History(false));
    assert_eq!(center(&m), MmPoint::new(10., 20.));
    m.run(Action::History(true));
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(m.view.selected.unwrap().object.object_id, id);
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
fn save_as_keeps_source_and_last_saved_identity() {
    let (mut m, dir) = setup();
    select(&mut m);
    let source = m.view.info.as_ref().unwrap().source_path.clone();
    m.numeric_move("5", "-3").unwrap();
    let target = dir.join("输出 # 1.gbr");
    let l = m.view.layers[0].layer_id.clone();
    m.save(&target, l, None).unwrap();
    let d = m.view.info.clone().unwrap();
    assert!(!d.dirty);
    assert_eq!(d.source_path, source);
    assert_eq!(
        d.last_saved_path.as_deref(),
        std::fs::canonicalize(&target).unwrap().to_str()
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), SOURCE);
    m.open(&target).unwrap();
    m.select(MmPoint::new(15., 17.), 0.).unwrap();
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
    assert!(m.view.selected.is_some());
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
    };
    let layers = vec![
        make("bottom", true),
        make("top", true),
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
    let id = d.object.clone();
    d.update(eframe::egui::pos2(150., 130.));
    m.run(d.release().unwrap());
    m.run(Action::History(false));
    assert_eq!(center(&m), MmPoint::new(10., 20.));
    m.run(Action::History(true));
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(m.view.selected.unwrap().object.object_id, id);
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
    let source = m.view.selected.clone().unwrap();
    m.run(Action::Duplicate);
    assert!(m.view.error.is_none());
    assert_ne!(
        m.view.selected.as_ref().unwrap().object.object_id,
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
    let copy = m.view.selected.clone().unwrap();
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
    assert!(m.view.selected.is_none());
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
fn drag_probe_uses_exact_hole_and_selects_other_without_arming() {
    let (mut m, _) = setup();
    select(&mut m);
    m.run(Action::ProbeDrag(MmPoint::new(21.5, 20.), 0.));
    assert!(!m.view.drag_hit);
    assert_eq!(center(&m), MmPoint::new(20., 20.));
    m.run(Action::ProbeDrag(MmPoint::new(20., 20.), 0.));
    assert!(!m.view.drag_hit);
    assert!(m.view.selected.is_none());
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
    m.open(&path).unwrap();
    assert_eq!(m.view.layers[0].object_count, 3);
    m.select(MmPoint::new(15., 17.), 0.).unwrap();
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
    d.released = true;
    assert!(!d.dragging);
    assert_eq!(d.delta, MmPoint::new(0., 0.));
    d.confirmed = true;
    d.update(d.last);
    m.run(d.release().unwrap());
    assert_eq!(center(&m), MmPoint::new(15., 17.));
    assert_eq!(m.view.info.unwrap().undo_entries, 1);
}
