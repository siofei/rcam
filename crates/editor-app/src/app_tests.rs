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
