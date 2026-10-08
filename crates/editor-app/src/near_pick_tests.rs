//! Ordinary public synthetic CPU regressions; no native mouse/GPU evidence.
use crate::{
    camera::Camera,
    selection::{ClickContext, SelectionMode},
    state::{Action, Model},
};
use editor_core::{BoundsMm, MmPoint, SemanticGeometry, workspace::DisplayClass};
use editor_service::{ClassStyleUpdate, LayerUpdateParams};
use eframe::egui::{self, Rect};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    model: Model,
    dir: PathBuf,
}
fn circles(centers: &[(f64, f64)], diameter: f64) -> String {
    let mut s = format!("%FSLAX46Y46*%%MOMM*%%ADD10C,{diameter}*%D10*");
    for (x, y) in centers {
        s.push_str(&format!(
            "X{}Y{}D03*",
            (x * 1e6).round() as i64,
            (y * 1e6).round() as i64
        ));
    }
    s.push_str("M02*");
    s
}
impl Fixture {
    fn new(layers: &[String]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rcam-near-pick-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = layers
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let p = dir.join(format!("layer{i}.gbr"));
                std::fs::write(&p, s).unwrap();
                p
            })
            .collect();
        let mut model = Model::default();
        model.run(Action::ImportGerbers(paths));
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        Self { model, dir }
    }
    fn click(&mut self, context: ClickContext, mode: SelectionMode) {
        self.model.run(Action::CanvasSelect(context, mode));
        assert!(
            self.model.view.error.is_none(),
            "{:?}",
            self.model.view.error
        );
    }
    fn lock(&mut self, index: usize, category: bool) {
        let info = self.model.view.info.as_ref().unwrap();
        let params = LayerUpdateParams {
            layer_id: self.model.view.layers[index].layer_id.clone(),
            expected_workspace_revision: info.workspace_revision.clone(),
            locked: (!category).then_some(true),
            classes: if category {
                vec![ClassStyleUpdate {
                    class: Some(DisplayClass::FlashCircle),
                    locked: Some(true),
                    ..Default::default()
                }]
            } else {
                vec![]
            },
            ..Default::default()
        };
        self.model.run(Action::Layer(params));
        assert!(self.model.view.error.is_none());
    }
    fn selected_layer(&self) -> usize {
        let id = &self.model.view.selected.primary().unwrap().layer_id;
        self.model
            .view
            .layers
            .iter()
            .position(|l| &l.layer_id == id)
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn context(x: f64, y: f64, scale: f64, ppp: f32) -> ClickContext {
    let camera = Camera {
        center: MmPoint::new(0., 0.),
        scale,
    };
    let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    ClickContext::new(camera.screen(MmPoint::new(x, y), rect), camera, rect, ppp)
}

#[test]
fn nearest_exterior_lower_beats_far_upper_and_cycles_all_candidates() {
    let mut f = Fixture::new(&[circles(&[(-0.35, 0.)], 0.2), circles(&[(0., 0.)], 0.2)]);
    let c = context(0.12, 0., 10., 1.);
    let info = f.model.view.info.clone();
    for expected in [1, 0, 1] {
        f.click(c.clone(), SelectionMode::Replace);
        assert_eq!(f.selected_layer(), expected);
        assert_eq!(
            f.model.view.click_cycle.as_ref().unwrap().candidates.len(),
            2
        );
    }
    assert_eq!(f.model.view.info, info);
    f.click(context(2., 0., 10., 1.), SelectionMode::Replace);
    assert!(f.model.view.selected.ordered.is_empty());
    assert!(f.model.view.click_cycle.is_none());
}
#[test]
fn direct_hits_keep_layer_priority_and_near_ties_keep_reverse_exposure() {
    let mut f = Fixture::new(&[circles(&[(-0.35, 0.)], 0.2), circles(&[(0., 0.)], 0.2)]);
    f.click(context(0.09, 0., 10., 1.), SelectionMode::Replace);
    assert_eq!(f.selected_layer(), 1, "direct lower beats exterior upper");
    let mut f = Fixture::new(&[circles(&[(0., 0.)], 0.2), circles(&[(0., 0.)], 0.2)]);
    f.click(context(0., 0., 10., 1.), SelectionMode::Replace);
    assert_eq!(
        f.selected_layer(),
        0,
        "direct overlap keeps existing upper priority"
    );
    let mut f = Fixture::new(&[circles(&[(-0.3, 0.), (0.3, 0.)], 0.2)]);
    f.click(context(0., 0., 10., 1.), SelectionMode::Replace);
    assert!(
        matches!(f.model.view.selected.primary().unwrap().object.geometry,SemanticGeometry::Flash{center,..} if center.x_mm==0.3),
        "distance tie keeps reverse exposure"
    );
    f.click(context(-0.01, 0., 10., 1.), SelectionMode::Replace);
    assert!(
        matches!(f.model.view.selected.primary().unwrap().object.geometry,SemanticGeometry::Flash{center,..} if center.x_mm== -0.3)
    );
}
#[test]
fn anchor_order_survives_distance_crossing_and_resets_for_membership_or_radius() {
    let mut f = Fixture::new(&[circles(&[(-0.15, 0.)], 0.1), circles(&[(0.15, 0.)], 0.1)]);
    let a = context(0.01, 0., 10., 1.);
    let b = context(-0.01, 0., 10., 1.);
    assert!(a.same_place(&b));
    f.click(a.clone(), SelectionMode::Replace);
    assert_eq!(f.selected_layer(), 1);
    let original = f
        .model
        .view
        .click_cycle
        .as_ref()
        .unwrap()
        .candidates
        .clone();
    f.click(b.clone(), SelectionMode::Replace);
    assert_eq!(f.selected_layer(), 0);
    f.click(b.clone(), SelectionMode::Replace);
    assert_eq!(
        f.selected_layer(),
        1,
        "retain anchor ranking despite nearest reversal"
    );
    assert!(original.shares_storage(&f.model.view.click_cycle.as_ref().unwrap().candidates));
    assert_eq!(f.model.view.click_cycle.as_ref().unwrap().context, a);
    let mut radius = b.clone();
    radius.tolerance *= 0.5;
    assert!(!a.same_place(&radius));
    f.click(radius, SelectionMode::Replace);
    assert_eq!(
        f.selected_layer(),
        0,
        "new radius starts fresh nearest ranking"
    );
    let mut f = Fixture::new(&[circles(&[(-0.5, 0.)], 0.2), circles(&[(0., 0.)], 0.2)]);
    let before = context(0.199, 0., 10., 1.);
    let membership = context(0.201, 0., 10., 1.);
    assert!(before.same_place(&membership));
    f.click(before, SelectionMode::Replace);
    assert_eq!(
        f.model.view.click_cycle.as_ref().unwrap().candidates.len(),
        2
    );
    f.click(membership.clone(), SelectionMode::Replace);
    assert_eq!(f.selected_layer(), 1);
    assert_eq!(
        f.model.view.click_cycle.as_ref().unwrap().candidates.len(),
        1
    );
    assert_eq!(
        f.model.view.click_cycle.as_ref().unwrap().context,
        membership
    );
}
#[test]
fn selection_radius_is_six_physical_pixels_across_dpi_and_zoom() {
    let mut f = Fixture::new(&[circles(&[(0., 0.)], 2.)]);
    for scale in [1., 10., 100.] {
        for ppp in [1., 1.25, 1.5, 2., 3., 4.] {
            for gap in [5.9, 6., 6.1, 12.] {
                let c = context(1. + gap / (scale * f64::from(ppp)), 0., scale, ppp);
                // Test acceptance from the actual f32 screen press converted to f64 world.
                let expected = c.world.x_mm - 1. <= c.tolerance + 1e-12;
                f.click(c, SelectionMode::Replace);
                assert_eq!(
                    !f.model.view.selected.ordered.is_empty(),
                    expected,
                    "scale={scale} ppp={ppp} gap={gap}"
                );
                if gap == 6.1 || gap == 12. {
                    assert!(!expected);
                }
            }
        }
    }
}
#[test]
fn add_all_and_box_exclude_locks_but_replace_remove_and_existing_selection_remain_inspectable() {
    for category in [false, true] {
        let mut f = Fixture::new(&[circles(&[(0., 0.)], 0.2), circles(&[(0., 0.)], 0.2)]);
        f.lock(0, category);
        let c = context(0., 0., 10., 1.);
        f.click(c.clone(), SelectionMode::Add);
        assert_eq!(
            f.selected_layer(),
            1,
            "locked first cannot conceal editable Add candidate"
        );
        assert_eq!(f.model.view.selected.ordered.len(), 1);
        f.model.run(Action::SelectAll);
        assert_eq!(f.model.view.selected.ordered.len(), 1);
        assert_eq!(f.selected_layer(), 1);
        f.click(c.clone(), SelectionMode::Replace);
        assert_eq!(f.selected_layer(), 0, "locked Replace remains inspection");
        let readonly = f.model.view.selected.primary().unwrap().clone();
        f.click(c.clone(), SelectionMode::Add);
        assert_eq!(f.model.view.selected.ordered.len(), 2);
        assert!(
            f.model
                .view
                .selected
                .contains(&readonly.layer_id, &readonly.object.object_id)
        );
        let before = f.model.view.info.clone();
        let selection = f.model.view.selected.clone();
        f.model.run(Action::Move("1".into(), "0".into()));
        assert_eq!(f.model.view.info, before);
        assert_eq!(f.model.view.selected, selection);
        assert_eq!(
            f.model.view.error.as_ref().unwrap().code,
            if category {
                "OBJECT_CLASS_LOCKED"
            } else {
                "LAYER_LOCKED"
            }
        );
        f.click(c.clone(), SelectionMode::Remove);
        assert_eq!(f.model.view.selected.ordered.len(), 1);
        assert_eq!(
            f.selected_layer(),
            1,
            "Remove can clear existing readonly target"
        );
        f.model.run(Action::Select(
            MmPoint::new(0., 0.),
            0.6,
            SelectionMode::Replace,
        ));
        f.model.run(Action::CanvasSelectRect(
            BoundsMm {
                min_x_mm: -1.,
                min_y_mm: -1.,
                max_x_mm: 1.,
                max_y_mm: 1.,
            },
            editor_core::hit_test::SelectRectMode::Window,
            SelectionMode::Add,
        ));
        assert!(f.model.view.error.is_none());
        assert_eq!(
            f.model.view.selected.ordered.len(),
            2,
            "box Add preserves earlier readonly selection without adding another"
        );
        f.model.run(Action::SelectAll);
        assert_eq!(f.model.view.selected.ordered.len(), 1);
        f.click(context(50., 0., 10., 1.), SelectionMode::Replace);
        f.model.run(Action::CanvasSelectRect(
            BoundsMm {
                min_x_mm: -1.,
                min_y_mm: -1.,
                max_x_mm: 1.,
                max_y_mm: 1.,
            },
            editor_core::hit_test::SelectRectMode::Window,
            SelectionMode::Add,
        ));
        assert_eq!(
            f.model.view.selected.ordered.len(),
            1,
            "fresh box Add cannot add locked object"
        );
        f.model.run(Action::Select(
            MmPoint::new(0., 0.),
            0.6,
            SelectionMode::Add,
        ));
        assert_eq!(
            f.model.view.selected.ordered.len(),
            1,
            "legacy Add query skips locked target too"
        );
        f.lock(1, category);
        f.model.run(Action::SelectAll);
        assert!(
            f.model.view.selected.ordered.is_empty(),
            "all locked means empty editable all-selection"
        );
    }
}
#[test]
fn cancelled_and_invalid_queries_preserve_selection_and_anchor() {
    let mut f = Fixture::new(&[circles(&[(-0.35, 0.)], 0.2), circles(&[(0., 0.)], 0.2)]);
    let c = context(0.12, 0., 10., 1.);
    f.click(c.clone(), SelectionMode::Replace);
    let before = f.model.view.selected.clone();
    let cycle = f
        .model
        .view
        .click_cycle
        .as_ref()
        .unwrap()
        .candidates
        .clone();
    for tolerance in [f64::NAN, f64::INFINITY, -1.] {
        let mut invalid = c.clone();
        invalid.tolerance = tolerance;
        f.model
            .run(Action::CanvasSelect(invalid, SelectionMode::Replace));
        assert_eq!(
            f.model.view.error.as_ref().unwrap().code,
            "INVALID_ARGUMENT"
        );
        assert_eq!(f.model.view.selected, before);
        assert!(cycle.shares_storage(&f.model.view.click_cycle.as_ref().unwrap().candidates));
    }
    let task = editor_service::task::TaskContext::new(900, f.model.task_version().unwrap());
    task.cancel_token.cancel();
    f.model
        .run_task(task, Action::CanvasSelect(c, SelectionMode::Replace));
    assert_eq!(f.model.view.error.as_ref().unwrap().code, "CANCELLED");
    assert_eq!(f.model.view.selected, before);
    assert!(cycle.shares_storage(&f.model.view.click_cycle.as_ref().unwrap().candidates));
}

#[test]
fn interleaved_text_members_use_first_direct_priority_and_minimum_near_distance() {
    use crate::selection::{merge_click_candidate, rank_click_candidates};
    use editor_core::{Exposure, ObjectOrigin, SemanticObject};
    let object = |id: &str, text: bool| SemanticObject {
        object_id: id.into(),
        exposure: Exposure::Dark,
        origin: if text {
            ObjectOrigin::GeneratedText {
                operation_id: "operation".into(),
            }
        } else {
            ObjectOrigin::Imported { command_index: 1 }
        },
        geometry: SemanticGeometry::Line {
            start: MmPoint::new(0., 0.),
            end: MmPoint::new(1., 0.),
            width_mm: 0.1,
        },
    };
    let mut candidates = Vec::new();
    let mut logical = std::collections::HashMap::new();
    // Reverse exposure: near text C, direct ordinary B, direct text A.
    for (order, (id, text, distance, uncertainty)) in [
        ("C", true, 0.3, 0.),
        ("B", false, 0., 0.),
        ("A", true, 5e-8, 1e-7),
    ]
    .into_iter()
    .enumerate()
    {
        let hit = editor_core::hit_test::HitTestCandidate {
            object_id: id.into(),
            distance_mm: distance,
            uncertainty_mm: uncertainty,
        };
        merge_click_candidate(
            &mut candidates,
            &mut logical,
            "layer",
            &object(id, text),
            &hit,
            order,
        );
    }
    rank_click_candidates(&mut candidates);
    assert_eq!(
        candidates
            .iter()
            .map(|c| c.representative.1.as_str())
            .collect::<Vec<_>>(),
        vec!["B", "A"],
        "positive-distance zero-tolerance text follows its actual first direct hit, not near C"
    );
    let mut candidates = Vec::new();
    let mut logical = std::collections::HashMap::new();
    for (order, (id, text, distance)) in [("C", true, 0.3), ("B", false, 0.2), ("A", true, 0.1)]
        .into_iter()
        .enumerate()
    {
        let hit = editor_core::hit_test::HitTestCandidate {
            object_id: id.into(),
            distance_mm: distance,
            uncertainty_mm: 0.,
        };
        merge_click_candidate(
            &mut candidates,
            &mut logical,
            "layer",
            &object(id, text),
            &hit,
            order,
        );
    }
    rank_click_candidates(&mut candidates);
    assert_eq!(
        candidates
            .iter()
            .map(|c| c.representative.1.as_str())
            .collect::<Vec<_>>(),
        vec!["C", "B"],
        "near group uses minimum distance across members"
    );
    let ordinary = object("operation", false);
    let text = object("glyph", true);
    assert_ne!(
        crate::selection::ClickCandidateKey::new("layer", &ordinary),
        crate::selection::ClickCandidateKey::new("layer", &text)
    );
}
#[test]
fn text_cycle_keeps_anchor_representative_when_eligible_glyphs_change_and_locks_are_whole_group() {
    use editor_service::*;
    let mut f = Fixture::new(&[circles(&[(20., 0.)], 0.2)]);
    let info = f.model.view.info.clone().unwrap();
    let layer = f.model.view.layers[0].layer_id.clone();
    f.model.run(Action::TextCreate(crate::text_tool::Request {
        generation: 1,
        document: info.document_id,
        revision: info.revision,
        params: TextParams {
            layer_id: layer.clone(),
            font: builtin_stroke_font().identity,
            layout: TextLayout {
                text: "A".into(),
                x_mm: 0.,
                y_mm: 0.,
                height_mm: 1.,
                tracking_mm: 0.,
                h_align: HorizontalAlign::Left,
                v_align: VerticalAlign::Bottom,
                rotation_deg: 0.,
                curve_tolerance_mm: 0.00025,
                baseline_spacing_mm: 0.,
                stroke_width_mm: 0.02,
                outline_offset_mm: 0.,
            },
        },
    }));
    assert!(f.model.view.error.is_none(), "{:?}", f.model.view.error);
    let group_size = f.model.view.selected.ordered.len();
    assert_eq!(group_size, 3);
    let first = context(-5.98, 0., 1., 1.);
    let next = context(-5.80, 0., 1., 1.);
    assert!(first.same_place(&next));
    let info = f.model.view.info.clone().unwrap();
    let hits = |c: &ClickContext| {
        f.model
            .service
            .objects_hit_test(
                &info.document_id,
                HitTestParams {
                    layer_id: layer.clone(),
                    point: HitTestPoint {
                        x_mm: c.world.x_mm,
                        y_mm: c.world.y_mm,
                    },
                    tolerance_mm: c.tolerance,
                    selectable_only: true,
                },
            )
            .unwrap()
            .object_ids
    };
    let first_ids = hits(&first);
    let next_ids = hits(&next);
    assert_ne!(
        first_ids.last(),
        next_ids.last(),
        "real glyph hit representative changes inside original screen anchor"
    );
    f.click(first.clone(), SelectionMode::Replace);
    let original = f
        .model
        .view
        .click_cycle
        .as_ref()
        .unwrap()
        .candidates
        .clone();
    assert_eq!(original.len(), 1);
    assert_eq!(f.model.view.selected.ordered.len(), group_size);
    f.click(next, SelectionMode::Replace);
    let cycle = f.model.view.click_cycle.as_ref().unwrap();
    assert!(original.shares_storage(&cycle.candidates));
    assert_eq!(cycle.context, first);
    assert_eq!(f.model.view.selected.ordered.len(), group_size);
    let revision = f
        .model
        .view
        .info
        .as_ref()
        .unwrap()
        .workspace_revision
        .clone();
    f.model.run(Action::Layer(LayerUpdateParams {
        layer_id: layer,
        expected_workspace_revision: revision,
        classes: vec![ClassStyleUpdate {
            class: Some(DisplayClass::GeneratedText),
            locked: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    }));
    f.click(context(50., 0., 1., 1.), SelectionMode::Replace);
    f.click(first.clone(), SelectionMode::Add);
    assert!(
        f.model.view.selected.ordered.is_empty(),
        "no partial locked text Add"
    );
    f.model.run(Action::SelectAll);
    assert_eq!(
        f.model.view.selected.ordered.len(),
        1,
        "Ctrl+A excludes all locked glyphs as one logical group"
    );
    f.click(first.clone(), SelectionMode::Replace);
    assert_eq!(
        f.model.view.selected.ordered.len(),
        group_size,
        "inspection selects whole locked text"
    );
    f.click(first, SelectionMode::Remove);
    assert!(f.model.view.selected.ordered.is_empty());
}
#[test]
fn candidate_cap_and_eighty_thousand_selection_preserve_shared_storage() {
    let limit = editor_core::edit::MAX_MOVE_OBJECTS;
    let mut f = Fixture::new(&[circles(&vec![(0., 0.); limit], 0.2)]);
    f.click(context(0., 0., 10., 1.), SelectionMode::Replace);
    let original = f.model.view.click_cycle.as_ref().unwrap();
    assert_eq!(original.candidates.len(), limit);
    for _ in 0..128 {
        let copy = f.model.view.clone();
        let cycle = copy.click_cycle.as_ref().unwrap();
        assert!(original.candidates.shares_storage(&cycle.candidates));
        assert!(std::sync::Arc::ptr_eq(
            &original.logical_candidates,
            &cycle.logical_candidates
        ));
    }
    let mut over = Fixture::new(&[circles(&vec![(0., 0.); limit + 1], 0.2)]);
    let before = over.model.view.info.clone();
    let selected = over.model.view.selected.clone();
    over.model.run(Action::CanvasSelect(
        context(0., 0., 10., 1.),
        SelectionMode::Replace,
    ));
    assert_eq!(
        over.model.view.error.as_ref().unwrap().code,
        "RESOURCE_LIMIT"
    );
    assert_eq!(over.model.view.info, before);
    assert_eq!(over.model.view.selected, selected);
    assert!(over.model.view.click_cycle.is_none());
    let centers = (0..80_000)
        .map(|n| ((n % 400) as f64, (n / 400) as f64))
        .collect::<Vec<_>>();
    let mut f = Fixture::new(&[circles(&centers, 0.2)]);
    f.model.run(Action::SelectAll);
    assert_eq!(f.model.view.selected.ordered.len(), 80_000);
    assert!(f.model.view.click_cycle.is_none());
    for _ in 0..128 {
        let copy = f.model.view.clone();
        assert!(
            copy.selected
                .ordered
                .shares_storage(&f.model.view.selected.ordered)
        );
    }
}
