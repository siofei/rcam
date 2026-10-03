//! Real two-import regressions, never synthetic layer metadata.
use crate::{
    camera::Camera,
    drag::Gesture,
    selection::SelectionMode,
    state::{Action, Model},
};
use editor_core::{BoundsMm, MmPoint};
use eframe::egui::{Rect, pos2};
fn model() -> Model {
    let dir = std::env::temp_dir().join(format!(
        "rcam-i1-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let source = b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    let paths = ["lower.gbr", "upper.gbr"].map(|n| {
        let p = dir.join(n);
        std::fs::write(&p, source).unwrap();
        p
    });
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    m.run(Action::ImportGerbers(paths.to_vec()));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.layers.len(), 2);
    m
}
#[test]
fn i1_real_two_layer_box_move_one_undo() {
    let mut m = model();
    m.run(Action::SelectRect(
        BoundsMm {
            min_x_mm: -1.,
            min_y_mm: -1.,
            max_x_mm: 1.,
            max_y_mm: 1.,
        },
        editor_core::hit_test::SelectRectMode::Window,
    ));
    assert_eq!(m.view.selected.ordered.len(), 2);
    let before = m.view.selected.clone();
    let revision = m
        .view
        .info
        .as_ref()
        .unwrap()
        .revision
        .parse::<u64>()
        .unwrap();
    assert!(
        crate::drag::editable_selection(&m.view),
        "real cross-layer selection must be draggable"
    );
    m.run(Action::Move("2".into(), "3".into()));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(
        m.view
            .info
            .as_ref()
            .unwrap()
            .revision
            .parse::<u64>()
            .unwrap(),
        revision + 1
    );
    for (a, b) in before.ordered.iter().zip(&m.view.selected.ordered) {
        assert_eq!(a.layer_id, b.layer_id);
        assert_eq!(a.object.object_id, b.object.object_id);
        assert_ne!(a.object.geometry, b.object.geometry);
    }
    m.run(Action::History(false));
    assert_eq!(m.view.selected, before);
}
#[test]
fn i1_real_two_layer_repeated_click_reaches_both() {
    let mut m = model();
    let camera = Camera::default();
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    let point = pos2(200., 200.);
    let mut chosen = Vec::new();
    for _ in 0..3 {
        let mut g = Gesture::arm(&m.view, point, camera, rect, 1., SelectionMode::Replace);
        m.run(Action::ProbeDrag(
            MmPoint::new(0., 0.),
            camera.tolerance(1.),
        ));
        g.confirm(&m.view);
        m.run(g.release().unwrap());
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        chosen.push(m.view.selected.primary().unwrap().layer_id.clone());
    }
    assert_ne!(
        chosen[0], chosen[1],
        "second click must reach covered layer"
    );
    assert_eq!(chosen[0], chosen[2]);
}

fn click(m: &mut Model, camera: Camera, point: eframe::egui::Pos2, mode: SelectionMode) {
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    let mut g = Gesture::arm(&m.view, point, camera, rect, 1., mode);
    m.run(Action::ProbeDrag(
        camera.world(point, rect),
        camera.tolerance(1.),
    ));
    g.confirm(&m.view);
    m.run(g.release().unwrap());
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
}
#[test]
fn i1_cycle_modifiers_do_not_advance_and_navigation_resets() {
    use SelectionMode::*;
    let mut m = model();
    let c = Camera::default();
    let p = pos2(200., 200.);
    click(&mut m, c, p, Replace);
    let top = m.view.selected.primary().unwrap().clone();
    click(&mut m, c, p, Replace);
    let bottom = m.view.selected.primary().unwrap().clone();
    assert_ne!(top.layer_id, bottom.layer_id);
    click(&mut m, c, p, Remove);
    assert!(m.view.selected.ordered.is_empty());
    click(&mut m, c, p, Add);
    assert_eq!(m.view.selected.primary(), Some(&top));
    click(&mut m, c, p, Replace);
    assert_eq!(m.view.selected.primary(), Some(&top));
    click(&mut m, c, p, Replace);
    assert_eq!(m.view.selected.primary(), Some(&bottom));
    let zoom = Camera { scale: 20., ..c };
    click(&mut m, zoom, p, Replace);
    assert_eq!(m.view.selected.primary(), Some(&top));
    let before = m.view.info.clone();
    click(&mut m, zoom, pos2(202., 200.), Replace);
    assert_eq!(m.view.selected.primary(), Some(&bottom));
    click(&mut m, zoom, pos2(202.1, 200.), Replace);
    assert_eq!(m.view.selected.primary(), Some(&top));
    assert_eq!(m.view.info, before, "selection must never manufacture");
    let mut nav = crate::selection::ClickNavigation::default();
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    let a = nav.observe(c, rect, 1.);
    nav.observe(zoom, rect, 1.);
    let b = nav.observe(c, rect, 1.);
    assert_ne!(a, b, "zoom out-and-back must reset");
}
#[test]
fn i1_covered_selected_object_drag_and_modifiers_are_safe() {
    use SelectionMode::*;
    let mut m = model();
    let c = Camera::default();
    let p = pos2(200., 200.);
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    click(&mut m, c, p, Replace);
    click(&mut m, c, p, Replace);
    let selected = m.view.selected.clone();
    let mut g = Gesture::arm(&m.view, p, c, rect, 1., Replace);
    m.run(Action::ProbeDrag(MmPoint::new(0., 0.), c.tolerance(1.)));
    assert!(m.view.drag_hit);
    g.confirm(&m.view);
    g.update(pos2(220., 200.));
    m.run(g.release().unwrap());
    assert!(m.view.error.is_none());
    assert_eq!(
        m.view.selected.primary().unwrap().layer_id,
        selected.primary().unwrap().layer_id
    );
    m.run(Action::History(false));
    assert_eq!(m.view.selected, selected);
    let after = m.view.info.clone();
    for mode in [Add, Remove] {
        let mut g = Gesture::arm(&m.view, p, c, rect, 1., mode);
        m.run(Action::ProbeDrag(MmPoint::new(0., 0.), c.tolerance(1.)));
        g.confirm(&m.view);
        g.update(pos2(220., 200.));
        assert!(
            g.release().is_none(),
            "modifier-drag over object must neither edit nor cycle"
        );
    }
    assert_eq!(m.view.info, after);
}
#[test]
fn i1_cross_layer_all_basic_commands_are_one_transaction() {
    use crate::state::{MirrorDirection, PivotInput};
    for action in [
        Action::Rotate("90".into(), PivotInput::Custom("2".into(), "3".into())),
        Action::Mirror(MirrorDirection::Horizontal),
        Action::Duplicate,
        Action::Delete,
    ] {
        let mut m = model();
        m.run(Action::SelectRect(
            BoundsMm {
                min_x_mm: -1.,
                min_y_mm: -1.,
                max_x_mm: 1.,
                max_y_mm: 1.,
            },
            editor_core::hit_test::SelectRectMode::Window,
        ));
        let before = m.view.snap_snapshot.clone().unwrap();
        let info = m.view.info.clone().unwrap();
        m.run(action);
        assert!(m.view.error.is_none(), "{:?}", m.view.error);
        assert_eq!(
            m.view.info.as_ref().unwrap().undo_entries,
            info.undo_entries + 1
        );
        let changed = m.view.snap_snapshot.clone().unwrap();
        m.run(Action::History(false));
        assert!(m.view.error.is_none());
        assert_eq!(m.view.snap_snapshot.as_ref().unwrap().layers, before.layers);
        m.run(Action::History(true));
        assert_eq!(
            m.view.snap_snapshot.as_ref().unwrap().layers,
            changed.layers
        );
    }
}

#[test]
fn i1_cycle_cancel_and_task_version_fencing_preserve_selection_and_cycle() {
    use editor_service::task::TaskContext;
    let mut m = model();
    let camera = Camera::default();
    let point = pos2(200., 200.);
    click(&mut m, camera, point, SelectionMode::Replace);
    let before = m.view.selected.clone();
    let index = m.view.click_cycle.as_ref().unwrap().index;
    let context = crate::selection::ClickContext::new(
        point,
        camera,
        Rect::from_min_max(pos2(0., 0.), pos2(400., 400.)),
        1.,
    );
    let task = TaskContext::new(90, m.task_version().unwrap());
    task.cancel_token.cancel();
    m.run_task(
        task,
        Action::CanvasSelect(context.clone(), SelectionMode::Replace),
    );
    assert_eq!(m.view.selected, before);
    assert_eq!(m.view.click_cycle.as_ref().unwrap().index, index);
    assert_eq!(m.view.error.as_ref().unwrap().code, "CANCELLED");
    let task = TaskContext::new(91, m.task_version().unwrap());
    m.view.rule_revision += 1;
    m.run_task(task, Action::CanvasSelect(context, SelectionMode::Replace));
    assert_eq!(m.view.error.as_ref().unwrap().code, "STALE_TASK");
    assert_eq!(m.view.selected, before);
    assert_eq!(m.view.click_cycle.as_ref().unwrap().index, index);
}
#[test]
fn i1_hidden_and_locked_layers_cycle_policy_and_whole_edit_refusal() {
    let mut m = model();
    let camera = Camera::default();
    let point = pos2(200., 200.);
    click(&mut m, camera, point, SelectionMode::Replace);
    let top = m.view.selected.primary().unwrap().layer_id.clone();
    let info = m.view.info.clone().unwrap();
    m.run(Action::Layer(editor_service::LayerUpdateParams {
        layer_id: top.clone(),
        expected_workspace_revision: info.workspace_revision,
        locked: Some(true),
        ..Default::default()
    }));
    click(&mut m, camera, point, SelectionMode::Replace);
    assert_eq!(
        m.view.selected.primary().unwrap().layer_id,
        top,
        "locked layer remains inspectable"
    );
    let before = m.view.info.clone();
    let selected = m.view.selected.clone();
    m.run(Action::Move("1".into(), "0".into()));
    assert_eq!(m.view.error.as_ref().unwrap().code, "LAYER_LOCKED");
    assert_eq!(m.view.info, before);
    assert_eq!(m.view.selected, selected);
    let revision = m.view.info.as_ref().unwrap().workspace_revision.clone();
    m.run(Action::Layer(editor_service::LayerUpdateParams {
        layer_id: top.clone(),
        expected_workspace_revision: revision,
        visible: Some(false),
        ..Default::default()
    }));
    click(&mut m, camera, point, SelectionMode::Replace);
    assert_ne!(m.view.selected.primary().unwrap().layer_id, top);
    click(&mut m, camera, point, SelectionMode::Replace);
    assert_ne!(m.view.selected.primary().unwrap().layer_id, top);
}

#[test]
fn i1_text_is_one_logical_cycle_candidate() {
    use editor_service::*;
    let mut m = model();
    let info = m.view.info.clone().unwrap();
    let layer = m.view.layers[0].layer_id.clone();
    m.run(Action::TextCreate(crate::text_tool::Request {
        generation: 1,
        document: info.document_id,
        revision: info.revision,
        params: TextParams {
            layer_id: layer,
            font: builtin_stroke_font().identity,
            layout: TextLayout {
                text: "A".into(),
                x_mm: 0.,
                y_mm: 0.,
                height_mm: 3.,
                tracking_mm: 0.,
                h_align: HorizontalAlign::Left,
                v_align: VerticalAlign::Bottom,
                rotation_deg: 0.,
                curve_tolerance_mm: 0.00025,
                baseline_spacing_mm: 0.,
                stroke_width_mm: 0.15,
                outline_offset_mm: 0.,
            },
        },
    }));
    assert!(m.view.error.is_none());
    let text_count = m.view.selected.ordered.len();
    assert!(text_count > 1);
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    let camera = Camera {
        center: MmPoint::new(0., 0.),
        scale: 1.,
    };
    // 6mm hit radius intentionally covers every text stroke and both imported circles.
    let context = crate::selection::ClickContext::new(pos2(200., 200.), camera, rect, 1.);
    let mut seen = Vec::new();
    for _ in 0..3 {
        m.run(Action::CanvasSelect(
            context.clone(),
            SelectionMode::Replace,
        ));
        assert!(m.view.error.is_none());
        let cycle = m.view.click_cycle.as_ref().unwrap();
        assert_eq!(
            cycle.candidates.len(),
            3,
            "whole glyph plus one circle from each layer"
        );
        seen.push(m.view.selected.ordered.len());
    }
    seen.sort();
    assert_eq!(seen, vec![1, 1, text_count]);
}
#[test]
#[ignore = "release-only full100k canvas-cycle CPU evidence"]
fn i1_p100k_canvas_cycle_cpu() {
    assert!(!cfg!(debug_assertions));
    let mut m = Model::default();
    m.open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr"),
    )
    .unwrap();
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(1200., 800.));
    let camera = Camera {
        center: MmPoint::new(200., 125.),
        scale: 100.,
    };
    let before = m.view.info.clone();
    let mut times = Vec::new();
    for n in 0..210 {
        let world = MmPoint::new((n % 200 + 1) as f64, 1.);
        let context =
            crate::selection::ClickContext::new(camera.screen(world, rect), camera, rect, 1.);
        let start = std::time::Instant::now();
        m.run(Action::CanvasSelect(context, SelectionMode::Replace));
        let ms = start.elapsed().as_secs_f64() * 1000.;
        assert!(m.view.error.is_none());
        assert_eq!(m.view.selected.ordered.len(), 1);
        if n >= 10 {
            times.push(ms);
        }
    }
    assert_eq!(m.view.info, before);
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let p95 = sorted[189];
    eprintln!(
        "I1_CANVAS_CPU {}",
        serde_json::json!({"raw_ms":times,"p95_ms":p95,"scope":"full Model.run CanvasSelect including metrics; no GPU latency"})
    );
    assert!(p95 <= 20., "existing P100K point CPU p95 budget");
}

#[test]
fn i1_copied_ordinary_objects_remain_individual_cycle_targets() {
    let mut m = model();
    let layer = m.view.layers[0].layer_id.clone();
    m.run(Action::Select(
        MmPoint::new(0., 0.),
        0.,
        SelectionMode::Replace,
    ));
    m.run(Action::Duplicate);
    m.run(Action::Move("0.2".into(), "0".into()));
    // Select both ordinary objects from this one layer then copy them together.
    let snapshot = m.view.snap_snapshot.clone().unwrap();
    m.view.selected.ordered = snapshot
        .layers
        .iter()
        .find(|l| l.id == layer)
        .unwrap()
        .objects
        .iter()
        .map(|o| editor_service::ObjectInfo {
            layer_id: layer.clone(),
            object: o.clone(),
        })
        .collect();
    assert_eq!(m.view.selected.ordered.len(), 2);
    m.run(Action::Duplicate);
    assert!(m.view.error.is_none());
    let context = crate::selection::ClickContext::new(
        pos2(200., 200.),
        Camera {
            center: MmPoint::new(0., 0.),
            scale: 10.,
        },
        Rect::from_min_max(pos2(0., 0.), pos2(400., 400.)),
        1.,
    );
    let mut ids = std::collections::HashSet::new();
    for _ in 0..5 {
        m.run(Action::CanvasSelect(
            context.clone(),
            SelectionMode::Replace,
        ));
        assert!(m.view.error.is_none());
        assert_eq!(
            m.view.selected.ordered.len(),
            1,
            "ordinary copy provenance must not create a logical group"
        );
        ids.insert((
            m.view.selected.primary().unwrap().layer_id.clone(),
            m.view.selected.primary().unwrap().object.object_id.clone(),
        ));
    }
    assert_eq!(ids.len(), 5);
}

#[test]
fn i1_cycle_uses_true_holes_concavity_and_tolerance_across_layers() {
    let dir = std::env::temp_dir().join(format!("rcam-i1-truth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // At x=10 the L-shaped region has a 1..4 x 1..4 notch; AABB must not hit it.
    let text = "%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,4X2*%\nD10*\nX0Y0D03*\nG36*\nX10000000Y0D02*\nX14000000Y0D01*\nX14000000Y1000000D01*\nX11000000Y1000000D01*\nX11000000Y4000000D01*\nX10000000Y4000000D01*\nX10000000Y0D01*\nG37*\nM02*\n";
    let paths = ["a.gbr", "b.gbr"].map(|n| {
        let p = dir.join(n);
        std::fs::write(&p, text).unwrap();
        p
    });
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    m.run(Action::ImportGerbers(paths.to_vec()));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(800., 600.));
    let c = Camera {
        center: MmPoint::new(0., 0.),
        scale: 100.,
    };
    for (point, expected_count) in [
        (MmPoint::new(0., 0.), 0),
        (MmPoint::new(1.5, 0.), 2),
        (MmPoint::new(0.95, 0.), 2),
        (MmPoint::new(0.93, 0.), 0),
        (MmPoint::new(12., 2.), 0),
        (MmPoint::new(10.5, 2.), 2),
    ] {
        m.run(Action::Select(
            MmPoint::new(-50., -50.),
            0.,
            SelectionMode::Replace,
        ));
        let context = crate::selection::ClickContext::new(c.screen(point, rect), c, rect, 1.);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..3 {
            m.run(Action::CanvasSelect(
                context.clone(),
                SelectionMode::Replace,
            ));
            assert!(m.view.error.is_none(), "{:?}", m.view.error);
            if let Some(o) = m.view.selected.primary() {
                seen.insert((o.layer_id.clone(), o.object.object_id.clone()));
            }
        }
        assert_eq!(seen.len(), expected_count, "truth at {point:?}");
    }
}

#[test]
fn i1_cycle_context_identity_all_dimensions_and_candidate_change() {
    use crate::selection::ClickContext;
    let mut m = model();
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    let c = Camera::default();
    let context = ClickContext::new(pos2(200., 200.), c, rect, 2.);
    m.run(Action::CanvasSelect(
        context.clone(),
        SelectionMode::Replace,
    ));
    let top = m.view.selected.clone();
    let mut boundary = context.clone();
    boundary.point.x += 1.;
    assert!(context.same_place(&boundary));
    boundary.point.x += 0.01;
    assert!(!context.same_place(&boundary));
    for n in 0..5 {
        m.run(Action::CanvasSelect(
            context.clone(),
            SelectionMode::Replace,
        ));
        let mut changed = context.clone();
        match n {
            0 => changed.ppp = 1.,
            1 => changed.rect.max.x += 1.,
            2 => changed.camera[0] += 1.,
            3 => changed.navigation_epoch += 1,
            _ => changed.camera[2] *= 2.,
        };
        assert!(!context.same_place(&changed));
        m.run(Action::CanvasSelect(changed, SelectionMode::Replace));
        assert_eq!(m.view.selected, top);
        m.run(Action::CanvasSelect(
            context.clone(),
            SelectionMode::Replace,
        ));
        assert_eq!(m.view.selected, top);
    }
    m.run(Action::CanvasSelect(
        context.clone(),
        SelectionMode::Replace,
    ));
    assert_ne!(m.view.selected, top);
    m.run(Action::Delete);
    assert!(m.view.error.is_none());
    m.run(Action::CanvasSelect(
        context.clone(),
        SelectionMode::Replace,
    ));
    assert_eq!(m.view.selected, top);
    m.run(Action::History(false));
    m.run(Action::CanvasSelect(context, SelectionMode::Replace));
    assert_eq!(m.view.selected, top);
}

#[test]
#[ignore = "release-only measured two-layer cache invalidation"]
fn i1_multilayer_move_cache_measure() {
    assert!(!cfg!(debug_assertions));
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s2b3_2/P10K_CIRCLES.gbr");
    let mut m = Model::default();
    m.run(Action::NewWorkspace);
    m.run(Action::ImportGerbers(vec![p.clone(), p]));
    assert!(m.view.error.is_none());
    let snapshot = m.view.snap_snapshot.clone().unwrap();
    m.view.selected.ordered = snapshot
        .layers
        .iter()
        .flat_map(|l| {
            l.objects
                .iter()
                .take(100)
                .map(|o| editor_service::ObjectInfo {
                    layer_id: l.id.clone(),
                    object: o.clone(),
                })
        })
        .collect();
    assert_eq!(m.view.selected.ordered.len(), 200);
    let selected = m.view.selected.clone();
    let mut timings = Vec::new();
    for _ in 0..12 {
        let t = std::time::Instant::now();
        m.run(Action::Move("1".into(), "2".into()));
        let move_ms = t.elapsed().as_secs_f64() * 1000.;
        assert!(m.view.error.is_none());
        let t = std::time::Instant::now();
        m.run(Action::History(false));
        let undo_ms = t.elapsed().as_secs_f64() * 1000.;
        assert_eq!(m.view.selected, selected);
        timings.push(serde_json::json!({"move_ms":move_ms,"undo_ms":undo_ms}));
    }
    eprintln!(
        "I1_MULTILAYER_CACHE {}",
        serde_json::json!({"layer_count":2,"document_objects":20000,"selection_count":200,"raw":timings,"scope":"full Model.run CPU including conservative display/cache rebuild; no GPU/PMIX latency claim"})
    );
}

#[test]
fn i1_intermediate_motion_reproduces_noop_shift_and_preserves_cycle() {
    let mut m = model();
    let c = Camera::default();
    let p = pos2(200., 200.);
    let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
    for _ in 0..3 {
        click(&mut m, c, p, SelectionMode::Replace);
    }
    let before = m.view.selected.clone();
    let cycle = m.view.click_cycle.as_ref().unwrap().index;
    let mut g = Gesture::arm(&m.view, p, c, rect, 2., SelectionMode::Remove);
    m.run(Action::ProbeDrag(MmPoint::new(0., 0.), c.tolerance(2.)));
    // External pointer movement while the worker reply is pending, then a release
    // back at the requested point. The movement latch must not be cleared.
    g.update(pos2(204., 200.));
    g.confirm(&m.view);
    g.update(p);
    assert!(g.release().is_none());
    assert_eq!(m.view.selected, before);
    assert_eq!(m.view.click_cycle.as_ref().unwrap().index, cycle);
    click(&mut m, c, p, SelectionMode::Add);
    assert_eq!(m.view.selected, before);
    click(&mut m, c, p, SelectionMode::Replace);
    assert_eq!(m.view.selected, before);
}

#[test]
fn i1_release_before_probe_reply_preserves_modifier_and_selection_action() {
    for mode in [SelectionMode::Remove, SelectionMode::Add] {
        for delay in 0..8 {
            let mut m = model();
            let c = Camera::default();
            let p = pos2(200., 200.);
            let rect = Rect::from_min_max(pos2(0., 0.), pos2(400., 400.));
            click(&mut m, c, p, SelectionMode::Replace);
            let mut g = Gesture::arm(&m.view, p, c, rect, 1., mode);
            g.released = true;
            for _ in 0..delay {
                // Production UI retains a released gesture until confirmation.
                assert!(!g.confirmed);
                g.update(p);
            }
            m.run(Action::ProbeDrag(MmPoint::new(0., 0.), c.tolerance(1.)));
            g.confirm(&m.view);
            let Action::CanvasSelect(context, actual) = g.release().unwrap() else {
                panic!("released modifier gesture lost its selection action");
            };
            assert_eq!(actual, mode);
            m.run(Action::CanvasSelect(context, actual));
            assert!(m.view.error.is_none());
            assert_eq!(
                m.view.selected.ordered.is_empty(),
                mode == SelectionMode::Remove
            );
        }
    }
}

#[test]
fn i1_due_recovery_defers_to_pending_and_held_canvas_gesture() {
    let mut app = crate::modal::tests::app();
    app.view = model().view;
    let (tx, requests) = std::sync::mpsc::sync_channel(4);
    app.tx = tx;
    let now = std::time::Instant::now();
    let info = app.view.info.as_ref().unwrap();
    app.last_dirty_identity = format!(
        "{}:{}:{}",
        info.project_id, info.revision, info.workspace_revision
    );
    app.dirty_since = now - std::time::Duration::from_secs(31);
    app.last_recovery_at = now - std::time::Duration::from_secs(61);
    let p = pos2(200., 200.);
    app.drag = Some(Gesture::arm(
        &app.view,
        p,
        Camera::default(),
        Rect::from_min_max(pos2(0., 0.), pos2(400., 400.)),
        1.,
        SelectionMode::Replace,
    ));
    app.tick_recovery(now);
    assert!(requests.try_recv().is_err());
    assert!(!app.busy);
    assert!(app.pending_recovery_identity.is_none());
    // At the end of the UI frame the accepted press owns busy before recovery.
    app.send(Action::ProbeDrag(MmPoint::new(0., 0.), 0.1));
    assert!(app.busy);
    app.tick_recovery(now);
    assert!(matches!(
        requests.try_recv().unwrap().2,
        Action::ProbeDrag(..)
    ));
    assert!(requests.try_recv().is_err());
    assert!(app.pending_recovery_identity.is_none());
}

#[test]
fn i1_review_idle_recovery_preserves_same_place_cycle() {
    let mut m = model();
    let camera = Camera::default();
    let point = pos2(200., 200.);
    click(&mut m, camera, point, SelectionMode::Replace);
    let first = m.view.selected.clone();
    let info = m.view.info.clone();
    let snapshot = m.view.snap_snapshot.clone().unwrap();
    assert!(info.as_ref().unwrap().project_dirty);
    assert!(m.view.click_cycle.is_some());
    let dir = std::env::temp_dir().join(format!("i1-review-recovery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let task = editor_service::task::TaskContext::new(1234, m.task_version().unwrap());
    m.run_task(task, Action::RecoveryWrite(dir));
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(
        m.view.info, info,
        "recovery must not mutate manufacturing revision/history/dirty"
    );
    assert_eq!(m.view.selected, first);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().as_ref(),
        snapshot.as_ref()
    );
    eprintln!(
        "I1_REVIEW_RECOVERY cycle_present_after_write={} first={:?}",
        m.view.click_cycle.is_some(),
        first.primary().unwrap().layer_id
    );
    click(&mut m, camera, point, SelectionMode::Replace);
    eprintln!(
        "I1_REVIEW_RECOVERY after_click={:?}",
        m.view.selected.primary().unwrap().layer_id
    );
    assert_ne!(
        m.view.selected.primary().unwrap().layer_id,
        first.primary().unwrap().layer_id,
        "same-place click after background recovery must reach the next covered layer, without a geometry/selection/navigation reset"
    );
}

#[test]
fn i1_review_idle_recovery_preserves_lower_layer_shift_remove() {
    let mut m = model();
    let camera = Camera::default();
    let point = pos2(200., 200.);
    click(&mut m, camera, point, SelectionMode::Replace);
    let top = m.view.selected.primary().unwrap().layer_id.clone();
    click(&mut m, camera, point, SelectionMode::Replace);
    let bottom = m.view.selected.primary().unwrap().layer_id.clone();
    assert_ne!(top, bottom);
    let info = m.view.info.clone();
    let selected = m.view.selected.clone();
    let snapshot = m.view.snap_snapshot.clone().unwrap();
    let dir = std::env::temp_dir().join(format!("i1-review-shift-recovery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    m.run_task(
        editor_service::task::TaskContext::new(1235, m.task_version().unwrap()),
        Action::RecoveryWrite(dir),
    );
    assert!(m.view.error.is_none(), "{:?}", m.view.error);
    assert_eq!(m.view.info, info);
    assert_eq!(m.view.selected, selected);
    assert_eq!(
        m.view.snap_snapshot.as_ref().unwrap().as_ref(),
        snapshot.as_ref()
    );
    click(&mut m, camera, point, SelectionMode::Remove);
    assert!(
        m.view.selected.ordered.is_empty(),
        "a background recovery write must not retarget same-place Shift from the current lower-layer candidate to an unselected top-layer object"
    );
}
