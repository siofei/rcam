use crate::selection::SelectionMode::{Add, Remove, Replace};
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
    };
    m.run(Action::Layer(p));
    assert!(m.view.error.is_none());
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
    m.open(&path).unwrap();
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
    m.open(&target).unwrap();
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
    let single = crate::metrics_panel::lines(&m.view).join("\n");
    assert!(single.contains("面积：3.141593 mm²"));
    assert!(single.contains("周长：6.283185 mm"));
    m.run(Action::Select(MmPoint::new(21.5, 20.), 0., Add));
    let multiple = crate::metrics_panel::lines(&m.view).join("\n");
    assert!(multiple.contains("已精确：2 / 2"));
    assert!(multiple.contains("对象面积合计：12.566371 mm²"));
    assert!(multiple.contains("对象周长合计：25.132741 mm"));
    for forbidden in ["实际开口面积", "钢网总开口面积", "最终面积"] {
        assert!(!multiple.contains(forbidden));
    }
    m.view.metrics[1].value = editor_service::MetricValue::Unsupported {
        reason: "test boundary".into(),
    };
    let partial = crate::metrics_panel::lines(&m.view).join("\n");
    assert!(partial.contains("已精确：1 / 2"));
    assert!(partial.contains("对象面积合计（已精确项）"));
    assert!(partial.contains("1 个对象暂不可计算"));
    m.view.selected.ordered.remove(0);
    m.view.metrics.remove(0);
    let unsupported = crate::metrics_panel::lines(&m.view).join("\n");
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
