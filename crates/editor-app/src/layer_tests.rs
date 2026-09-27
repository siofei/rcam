//! S4-B1 GUI state tests: multi-layer workspace, view styles and delete flow.
//! They drive `Model` (the same code the GUI worker thread runs) and inspect the
//! display scene the renderer consumes; native Metal parity is a separate ignored test.
use crate::display::{MODE_CENTERLINE, MODE_EDGE, MODE_FILLED, pack_color};
use crate::selection::SelectionMode::Replace;
use crate::state::{Action, Model};
use editor_core::MmPoint;
use editor_core::workspace::{ColorMode, DisplayClass, LayerDisplayMode};
use editor_service::{ClassStyleUpdate, LayerUpdateParams};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Two circles (D10) and one stroke (D11).
const A: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,1.0*%\n%ADD11C,0.2*%\nD10*\nX10000000Y10000000D03*\nX20000000Y10000000D03*\nD11*\nX0Y0D02*\nX5000000Y0D01*\nM02*\n";
/// Two rectangles (D10 again: same DCode as A, different shape).
const B: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,2.0X1.0*%\nD10*\nX30000000Y10000000D03*\nX40000000Y10000000D03*\nM02*\n";
const C: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,3.0*%\nD10*\nX50000000Y10000000D03*\nM02*\n";

struct Fixture {
    m: Model,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gui-layers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in [("A.gbr", A), ("B.gbr", B), ("C.gbr", C)] {
            std::fs::write(dir.join(name), text).unwrap();
        }
        let mut m = Model::default();
        m.run(Action::NewWorkspace);
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        Self { m, dir }
    }
    fn import(&mut self, names: &[&str]) -> Vec<String> {
        let paths = names.iter().map(|n| self.dir.join(n)).collect();
        self.m.run(Action::ImportGerbers(paths));
        assert!(self.m.view.error.is_none(), "{:?}", self.m.view.error);
        self.m
            .view
            .import
            .as_ref()
            .expect("import result")
            .layers
            .iter()
            .map(|l| l.layer_id.clone())
            .collect()
    }
    fn patch(&self, layer: &str) -> LayerUpdateParams {
        LayerUpdateParams {
            layer_id: layer.into(),
            expected_workspace_revision: self
                .m
                .view
                .info
                .as_ref()
                .unwrap()
                .workspace_revision
                .clone(),
            ..Default::default()
        }
    }
    fn update(&mut self, patch: LayerUpdateParams) {
        self.m.run(Action::Layer(patch));
        assert!(self.m.view.error.is_none(), "{:?}", self.m.view.error);
    }
    fn scene(&self) -> &crate::display::Scene {
        self.m.view.scene.as_ref().expect("scene")
    }
    /// Scene object indices of one layer, in composite order.
    fn layer_objects(&self, layer: &str) -> Vec<usize> {
        let snapshot = self
            .m
            .service
            .render_snapshot(&self.m.view.info.as_ref().unwrap().document_id)
            .unwrap();
        let mut start = 0;
        for l in &snapshot.layers {
            if l.id == layer {
                return (start..start + l.objects.len()).collect();
            }
            start += l.objects.len();
        }
        panic!("layer {layer} not in snapshot");
    }
    fn export_bytes(&mut self, layer: &str, name: &str) -> Vec<u8> {
        let path = self.dir.join(name);
        self.m.run(Action::Save(path.clone(), layer.into(), None));
        assert!(self.m.view.error.is_none(), "{:?}", self.m.view.error);
        std::fs::read(path).unwrap()
    }
}

#[test]
fn batch_import_creates_one_layer_per_file_and_one_undo() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "B.gbr", "C.gbr"]);
    assert_eq!(ids.len(), 3);
    assert_eq!(f.m.view.layers.len(), 3);
    assert!(f.m.view.layers.iter().all(|l| l.object_count > 0));
    assert_eq!(
        f.m.view.layers.iter().filter(|l| l.is_active).count(),
        1,
        "exactly one active layer"
    );
    let colors: std::collections::HashSet<_> =
        f.m.view.layers.iter().map(|l| l.base_color).collect();
    assert_eq!(colors.len(), 3, "auto colours are distinct");
    let info = f.m.view.info.clone().unwrap();
    assert_eq!(info.undo_entries, 1, "the whole batch is one transaction");
    f.m.run(Action::History(false));
    assert!(f.m.view.layers.is_empty(), "one Undo removes the batch");
    f.m.run(Action::History(true));
    assert_eq!(f.m.view.layers.len(), 3);
}

#[test]
fn a_bad_file_in_the_batch_adds_nothing() {
    let mut f = Fixture::new();
    std::fs::write(f.dir.join("bad.gbr"), "not gerber at all").unwrap();
    let paths = vec![f.dir.join("A.gbr"), f.dir.join("bad.gbr")];
    f.m.run(Action::ImportGerbers(paths));
    assert!(f.m.view.error.is_some());
    assert!(f.m.view.layers.is_empty());
    assert_eq!(f.m.view.info.as_ref().unwrap().undo_entries, 0);
}

#[test]
fn hidden_layers_and_categories_leave_the_render_index() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "B.gbr"]);
    let (a, b) = (ids[0].clone(), ids[1].clone());
    let visible = |f: &Fixture| f.scene().objects.iter().filter(|o| o.meta[3] != 0).count();
    let candidates = |f: &Fixture| {
        f.scene()
            .index
            .viewport([-1e6, -1e6, 1e6, 1e6])
            .ordered_candidate_ids
            .len()
    };
    assert_eq!(visible(&f), 5);
    assert_eq!(candidates(&f), 5);
    // Case B: only one layer stays visible.
    f.update(LayerUpdateParams {
        visible: Some(false),
        ..f.patch(&b)
    });
    assert_eq!(visible(&f), 3);
    assert_eq!(candidates(&f), 3, "hidden layer is not a render candidate");
    // Case C: a whole category disappears from the render candidates.
    f.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::FlashCircle),
            visible: Some(false),
            ..Default::default()
        }],
        ..f.patch(&a)
    });
    assert_eq!(visible(&f), 1);
    assert_eq!(
        candidates(&f),
        1,
        "hidden category is not a render candidate"
    );
    // The hidden objects still exist in the manufacturing scene and export.
    assert_eq!(f.scene().objects.len(), 5);
    assert_eq!(
        f.m.view
            .layers
            .iter()
            .map(|l| l.object_count)
            .sum::<usize>(),
        5
    );
}

#[test]
fn display_modes_style_objects_without_changing_exported_bytes() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr"]);
    let a = ids[0].clone();
    let revision_before = f.m.view.info.as_ref().unwrap().revision.clone();
    let filled_bytes = f.export_bytes(&a, "filled.gbr");
    let objects = f.layer_objects(&a);
    assert!(
        objects
            .iter()
            .all(|i| f.scene().objects[*i].style[1] == MODE_FILLED)
    );
    let mut modes = Vec::new();
    for mode in [LayerDisplayMode::Outline, LayerDisplayMode::ZeroWidth] {
        f.update(LayerUpdateParams {
            display_mode: Some(mode),
            ..f.patch(&a)
        });
        modes.push(
            objects
                .iter()
                .map(|i| f.scene().objects[*i].style[1])
                .collect::<Vec<_>>(),
        );
    }
    // Outline: every object is a boundary hairline.
    assert!(modes[0].iter().all(|m| *m == MODE_EDGE));
    // ZeroWidth: the stroke becomes a centre line, flashes fall back to their contour.
    assert_eq!(
        modes[1].iter().filter(|m| **m == MODE_CENTERLINE).count(),
        1
    );
    assert_eq!(modes[1].iter().filter(|m| **m == MODE_EDGE).count(), 2);
    let hairlines = f
        .scene()
        .primitives
        .iter()
        .filter(|p| p.meta[0] == 0 && p.meta[3] == crate::display::HAIRLINE)
        .count();
    assert_eq!(
        hairlines, 1,
        "one centre-line hairline for the single stroke"
    );
    // Selection identity and exposure are untouched by display mode.
    assert!(f.scene().objects.iter().all(|o| o.meta[2] == 1));
    f.update(LayerUpdateParams {
        display_mode: Some(LayerDisplayMode::Filled),
        ..f.patch(&a)
    });
    let after = f.export_bytes(&a, "after.gbr");
    assert_eq!(
        filled_bytes, after,
        "writer bytes never depend on display mode"
    );
    let revision = f.m.view.info.as_ref().unwrap().revision.clone();
    assert_eq!(
        revision, revision_before,
        "display changes are not manufacturing revisions"
    );
}

#[test]
fn category_color_mode_gives_each_class_its_own_colour() {
    let mut f = Fixture::new();
    let a = f.import(&["A.gbr"])[0].clone();
    let objects = f.layer_objects(&a);
    let base = f.m.view.layers[0].base_color;
    assert!(
        objects
            .iter()
            .all(|i| f.scene().objects[*i].style[0] == pack_color(base)),
        "LayerColor mode paints the layer colour"
    );
    f.update(LayerUpdateParams {
        color_mode: Some(ColorMode::CategoryColor),
        ..f.patch(&a)
    });
    let colors: std::collections::HashSet<_> = objects
        .iter()
        .map(|i| f.scene().objects[*i].style[0])
        .collect();
    assert_eq!(colors.len(), 2, "circles and strokes get different colours");
    f.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::Stroke),
            color_override: Some("#123456".into()),
            ..Default::default()
        }],
        ..f.patch(&a)
    });
    assert!(
        objects
            .iter()
            .any(|i| f.scene().objects[*i].style[0] == 0x123456),
        "explicit category colour wins"
    );
}

#[test]
fn selection_follows_visibility_selectability_but_not_lock() {
    let mut f = Fixture::new();
    let a = f.import(&["A.gbr"])[0].clone();
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    assert_eq!(f.m.view.selected.ordered.len(), 1);
    // Lock keeps the selection but refuses edits through validation.
    f.update(LayerUpdateParams {
        locked: Some(true),
        ..f.patch(&a)
    });
    assert_eq!(f.m.view.selected.ordered.len(), 1);
    f.m.run(Action::Delete);
    assert_eq!(f.m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
    f.update(LayerUpdateParams {
        locked: Some(false),
        ..f.patch(&a)
    });
    // A hidden layer drops its objects from the selection.
    f.update(LayerUpdateParams {
        visible: Some(false),
        ..f.patch(&a)
    });
    assert!(f.m.view.selected.ordered.is_empty());
    f.update(LayerUpdateParams {
        visible: Some(true),
        selectable: Some(false),
        ..f.patch(&a)
    });
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    assert!(
        f.m.view.selected.ordered.is_empty(),
        "non-selectable layers cannot be picked"
    );
    // A locked category refuses the edit even though the layer is unlocked.
    f.update(LayerUpdateParams {
        selectable: Some(true),
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::FlashCircle),
            locked: Some(true),
            ..Default::default()
        }],
        ..f.patch(&a)
    });
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    assert_eq!(f.m.view.selected.ordered.len(), 1);
    f.m.run(Action::Delete);
    assert_eq!(f.m.view.error.as_ref().unwrap().code, "OBJECT_CLASS_LOCKED");
    // Hiding the category leaves the selection too.
    f.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::FlashCircle),
            visible: Some(false),
            ..Default::default()
        }],
        ..f.patch(&a)
    });
    assert!(f.m.view.selected.ordered.is_empty());
}

#[test]
fn topmost_visible_selectable_layer_wins_the_pick() {
    let mut f = Fixture::new();
    // The same file twice: two overlapping layers, the later batch entry is lower.
    let ids = f.import(&["A.gbr", "A.gbr"]);
    let top = f.m.view.layers[0].layer_id.clone();
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    assert_eq!(f.m.view.selected.primary().unwrap().layer_id, top);
    // Making the top layer non-selectable passes the pick through to the layer below.
    f.update(LayerUpdateParams {
        selectable: Some(false),
        ..f.patch(&top)
    });
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    let picked = f.m.view.selected.primary().unwrap().layer_id.clone();
    assert_ne!(picked, top);
    assert!(ids.contains(&picked));
}

#[test]
fn delete_layer_tiers_and_undo_restore_identity() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "B.gbr"]);
    let b = ids[1].clone();
    let before_order: Vec<String> = f.m.view.layers.iter().map(|l| l.layer_id.clone()).collect();
    // Non-empty: the service refuses without explicit intent and changes nothing.
    f.m.run(Action::RemoveLayer(b.clone(), false));
    assert_eq!(
        f.m.view.error.as_ref().unwrap().code,
        "CONFIRMATION_REQUIRED"
    );
    assert_eq!(f.m.view.layers.len(), 2);
    // The GUI asks the service for the summary first.
    f.m.run(Action::LayerSummary(b.clone()));
    let summary = f.m.view.layer_summary.clone().unwrap();
    assert_eq!(summary.summary.object_count, 2);
    assert_eq!(
        summary.risk,
        editor_core::workspace::DeleteRisk::NonEmptyClean
    );
    f.m.run(Action::RemoveLayer(b.clone(), true));
    assert!(f.m.view.error.is_none());
    assert_eq!(f.m.view.layers.len(), 1);
    f.m.run(Action::History(false));
    let after_order: Vec<String> = f.m.view.layers.iter().map(|l| l.layer_id.clone()).collect();
    assert_eq!(after_order, before_order, "Undo restores id and position");
    // Empty layers delete directly (no allow_non_empty) and Undo restores them.
    f.m.run(Action::CreateEmptyLayer(Some("scratch".into())));
    let empty =
        f.m.view
            .layers
            .iter()
            .find(|l| l.display_name == "scratch")
            .unwrap()
            .layer_id
            .clone();
    f.m.run(Action::LayerSummary(empty.clone()));
    assert_eq!(
        f.m.view.layer_summary.as_ref().unwrap().risk,
        editor_core::workspace::DeleteRisk::Empty
    );
    f.m.run(Action::RemoveLayer(empty.clone(), false));
    assert!(f.m.view.error.is_none(), "{:?}", f.m.view.error);
    assert!(f.m.view.layers.iter().all(|l| l.layer_id != empty));
    f.m.run(Action::History(false));
    assert!(f.m.view.layers.iter().any(|l| l.layer_id == empty));
}

#[test]
fn deleting_a_generated_or_modified_layer_reports_dirty_risk() {
    let mut f = Fixture::new();
    let a = f.import(&["A.gbr"])[0].clone();
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    f.m.numeric_move("1", "0").unwrap();
    f.m.run(Action::LayerSummary(a));
    let summary = f.m.view.layer_summary.clone().unwrap();
    assert_eq!(
        summary.risk,
        editor_core::workspace::DeleteRisk::NonEmptyDirty
    );
    assert!(summary.summary.modified_object_count >= 1);
}

#[test]
fn reorder_changes_composite_order_only() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "B.gbr", "C.gbr"]);
    let ids_before: Vec<String> = f.scene().ids.clone();
    let revision = f.m.view.info.as_ref().unwrap().revision.clone();
    let mut order: Vec<String> = f.m.view.layers.iter().map(|l| l.layer_id.clone()).collect();
    order.reverse();
    f.m.run(Action::ReorderLayers(order.clone()));
    assert!(f.m.view.error.is_none(), "{:?}", f.m.view.error);
    let panel: Vec<String> = f.m.view.layers.iter().map(|l| l.layer_id.clone()).collect();
    assert_eq!(panel, order);
    assert_eq!(f.m.view.info.as_ref().unwrap().revision, revision);
    assert_eq!(
        f.m.view.info.as_ref().unwrap().undo_entries,
        1,
        "reorder is not a history entry"
    );
    let mut ids_after = f.scene().ids.clone();
    assert_ne!(ids_before, ids_after, "composite order followed the panel");
    ids_after.sort();
    let mut sorted_before = ids_before;
    sorted_before.sort();
    assert_eq!(sorted_before, ids_after, "same objects, different order");
    assert_eq!(ids.len(), 3);
}

#[test]
fn new_workspace_refuses_unexported_edits_until_discard() {
    let mut f = Fixture::new();
    f.import(&["A.gbr"]);
    let a = f.m.view.layers[0].layer_id.clone();
    // Adding layers changes the Workspace content, so it is unexported work.
    assert!(f.m.view.info.as_ref().unwrap().dirty);
    f.m.run(Action::NewWorkspace);
    assert_eq!(
        f.m.view.error.as_ref().unwrap().code,
        "CONFIRMATION_REQUIRED"
    );
    assert!(f.m.view.layers.iter().any(|l| l.layer_id == a));
    f.m.run(Action::DiscardNewWorkspace);
    assert!(f.m.view.error.is_none());
    assert!(f.m.view.layers.is_empty());
}

#[test]
fn export_of_one_layer_never_marks_the_workspace_clean() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "B.gbr"]);
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    f.m.numeric_move("1", "0").unwrap();
    let dirty_layer = f.m.view.selected.primary().unwrap().layer_id.clone();
    let bytes = f.export_bytes(&dirty_layer, "moved.gbr");
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("%ADD10"), "{text}");
    let info = f.m.view.info.as_ref().unwrap();
    assert!(info.dirty, "export does not save the Workspace");
    // The layer keeps its id; no source link appears.
    assert!(f.m.view.layers.iter().any(|l| l.layer_id == dirty_layer));
    assert!(ids.contains(&dirty_layer));
}

#[test]
fn text_target_rules_follow_layer_and_category_state() {
    let mut f = Fixture::new();
    let a = f.import(&["A.gbr"])[0].clone();
    let ok = |f: &Fixture| crate::state::text_target_ok(&f.m.view.layers[0]);
    assert!(ok(&f));
    f.update(LayerUpdateParams {
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::GeneratedText),
            locked: Some(true),
            ..Default::default()
        }],
        ..f.patch(&a)
    });
    assert!(!ok(&f), "locked text category refuses new text");
    f.update(LayerUpdateParams {
        reset_classes: true,
        locked: Some(true),
        ..f.patch(&a)
    });
    assert!(!ok(&f), "locked layer refuses new text");
}

#[test]
fn fit_layer_returns_only_that_layers_bounds() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "C.gbr"]);
    f.m.run(Action::FitLayer(ids[1].clone()));
    let c = f.m.view.focus_bounds.expect("bounds of layer C");
    assert!(c.min_x_mm > 40., "C sits at x=50: {c:?}");
    let all = f.m.view.bounds.unwrap();
    assert!(all.min_x_mm < c.min_x_mm);
}

#[test]
fn show_all_and_hide_all_are_one_workspace_revision_and_show_all_ends_solo() {
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr", "B.gbr", "C.gbr"]);
    let manufacturing = f.m.view.info.as_ref().unwrap().revision.clone();
    let before = f.m.view.info.as_ref().unwrap().workspace_revision.clone();

    f.m.run(Action::SetAllLayersVisible(false));
    assert!(f.m.view.error.is_none(), "{:?}", f.m.view.error);
    assert!(
        f.m.view
            .layers
            .iter()
            .all(|l| !l.visible && !l.effective_visible)
    );
    let hidden = f.m.view.info.as_ref().unwrap().workspace_revision.clone();
    assert_ne!(hidden, before);
    assert_eq!(
        f.m.view.info.as_ref().unwrap().revision,
        manufacturing,
        "view state never changes the manufacturing revision"
    );

    // Solo one layer, then "show all": everything visible and Solo gone.
    f.m.run(Action::SetSoloLayer(Some(ids[0].clone())));
    assert!(f.m.view.layers.iter().any(|l| l.is_solo));
    f.m.run(Action::SetAllLayersVisible(true));
    assert!(f.m.view.error.is_none(), "{:?}", f.m.view.error);
    assert!(
        f.m.view
            .layers
            .iter()
            .all(|l| l.visible && l.effective_visible)
    );
    assert!(f.m.view.layers.iter().all(|l| !l.is_solo));
    assert_eq!(f.m.view.info.as_ref().unwrap().revision, manufacturing);
}

#[test]
fn recent_colors_are_bounded_deduplicated_newest_first_and_ui_only() {
    use crate::layer_panel::{RECENT_COLOR_LIMIT, push_recent_color};
    let mut list = Vec::new();
    for i in 0..12u8 {
        push_recent_color(&mut list, &format!("#0000{i:02x}"));
    }
    assert_eq!(list.len(), RECENT_COLOR_LIMIT);
    assert_eq!(list[0], "#00000b", "newest first");
    push_recent_color(&mut list, "#000008");
    assert_eq!(list[0], "#000008", "re-picking moves to the front");
    assert_eq!(list.iter().filter(|c| *c == "#000008").count(), 1);
    push_recent_color(&mut list, "inherit");
    push_recent_color(&mut list, "not-a-colour");
    assert_eq!(
        list.len(),
        RECENT_COLOR_LIMIT,
        "non-colours are not remembered"
    );
    push_recent_color(&mut list, " #FF00AA ");
    assert_eq!(list[0], "#ff00aa", "normalised to lower case");

    // Committing a colour is workspace state only: the manufacturing revision
    // and the exported bytes do not move.
    let mut f = Fixture::new();
    let ids = f.import(&["A.gbr"]);
    let manufacturing = f.m.view.info.as_ref().unwrap().revision.clone();
    let mut patch = f.patch(&ids[0]);
    patch.base_color = Some("#ff00aa".into());
    f.m.run(Action::Layer(patch));
    assert!(f.m.view.error.is_none(), "{:?}", f.m.view.error);
    assert_eq!(f.m.view.info.as_ref().unwrap().revision, manufacturing);
}

#[test]
fn delete_dialog_numbers_are_grouped_and_display_glyphs_are_distinct() {
    use crate::layer_panel::{display_mode_glyph, group_digits};
    assert_eq!(group_digits(12438usize), "12,438");
    assert_eq!(group_digits(125usize), "125");
    assert_eq!(group_digits(0usize), "0");
    assert_eq!(group_digits(1000usize), "1,000");
    let glyphs: std::collections::HashSet<_> = LayerDisplayMode::ALL
        .iter()
        .map(|m| display_mode_glyph(*m))
        .collect();
    assert_eq!(glyphs.len(), 3);
}

#[test]
fn nonselectable_retains_selection_but_refuses_grip_and_delete() {
    let mut f = Fixture::new();
    let layer = f.import(&["A.gbr"])[0].clone();
    f.m.select(MmPoint::new(10., 10.), 0.1, Replace).unwrap();
    let ids: Vec<String> =
        f.m.view
            .selected
            .ids()
            .into_iter()
            .map(str::to_owned)
            .collect();
    let before = f.m.view.info.clone().unwrap();
    assert!(!crate::grip::features(&f.m.view).unwrap().is_empty());
    f.update(LayerUpdateParams {
        selectable: Some(false),
        ..f.patch(&layer)
    });
    assert_eq!(f.m.view.selected.ids(), ids);
    assert!(crate::grip::features(&f.m.view).unwrap().is_empty());
    f.m.run(Action::Delete);
    assert_eq!(f.m.view.error.as_ref().unwrap().code, "INVALID_ARGUMENT");
    let after = f.m.view.info.as_ref().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.undo_entries, before.undo_entries);
    assert_eq!(after.redo_entries, before.redo_entries);
    assert_eq!(after.dirty, before.dirty);
    f.update(LayerUpdateParams {
        selectable: Some(true),
        ..f.patch(&layer)
    });
    assert_eq!(f.m.view.selected.ids(), ids);
    assert!(!crate::grip::features(&f.m.view).unwrap().is_empty());
}
