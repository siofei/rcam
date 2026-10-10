//! Manufacturing, host-publication and real widget geometry regressions.
use crate::{
    modal::ActiveModal,
    session::{self, WorkerHost},
    state::{Action, Model},
    unified_editor_ui as ui, unified_editor_worker as worker,
};
use editor_core::{MmPoint, SemanticGeometry};
use editor_service::{
    DraftStep, SelectionEdit, UnifiedEditorHistory,
    task::{TaskContext, TaskState, TaskVersion},
};
use eframe::egui;
use std::{
    sync::{
        Arc,
        mpsc::{Receiver, sync_channel},
    },
    time::{Duration, Instant},
};
fn rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::new(200., 40.), egui::vec2(800., 600.))
}

mod picker_hud {
    use super::*;
    use eframe::App;

    const PICK: &str = "拾取工作图形上的点；Alt 暂停吸附；Esc 返回";
    const NAV: &str = "几何多选  ·  中键 / 双指平移  ·  捏合缩放";

    #[test]
    fn surviving_modal_paints_on_pointer_exit_and_blocks_same_frame_edits() {
        let size = egui::vec2(1280., 832.);
        let (mut run, ctx) = settled(size, 2., 1.);
        ctx.style_mut(|s| s.animation_time = 0.);
        run.begin();
        for _ in 0..3 {
            capture(&mut run, &ctx, size, 2., vec![]);
        }
        let entry = run.snapshot();
        let info = run.app.view.info.clone();
        ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(2).unwrap());
        for picking in [None, Some(ui::Pick::Base), Some(ui::Pick::Target)] {
            for (focused, loss) in [
                (true, Some(egui::Event::PointerGone)),
                (true, Some(egui::Event::WindowFocused(false))),
                (false, None),
            ] {
                run.app.unified_editor.as_mut().unwrap().picking = picking;
                run.app.modal = picking.is_none().then_some(ActiveModal::UnifiedEditor);
                let before = run.app.unified_editor.as_ref().unwrap();
                let draft_fields = (
                    before.base.x.clone(),
                    before.base.y.clone(),
                    before.target.x.clone(),
                    before.target.y.clone(),
                    before.dirty,
                    before.confirm_cancel,
                    before.targets.iter().map(|t| t.enabled).collect::<Vec<_>>(),
                );
                let mut raw = egui::RawInput {
                    focused,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events: vec![
                        egui::Event::PointerButton {
                            pos: egui::pos2(700., 400.),
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Text("123".into()),
                    ],
                    ..Default::default()
                };
                if let Some(loss) = loss {
                    raw.events.insert(0, loss);
                }
                raw.viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .native_pixels_per_point = Some(2.);
                run.app.raw_input_hook(&ctx, &mut raw);
                let mut passes = 0;
                let output = ctx.run(raw, |ctx| {
                    run.app.update(ctx, &mut eframe::Frame::_new_kittest());
                    passes += 1;
                    if passes == 1 {
                        ctx.request_discard("synthetic modal reentry");
                    }
                });
                assert_eq!(passes, 2);
                assert!(run.app.point_commit_blocked);
                assert_eq!(ctx.pixels_per_point(), 2.);
                let after = run.app.unified_editor.as_ref().unwrap();
                assert_eq!(
                    (
                        after.base.x.clone(),
                        after.base.y.clone(),
                        after.target.x.clone(),
                        after.target.y.clone(),
                        after.dirty,
                        after.confirm_cancel,
                        after.targets.iter().map(|t| t.enabled).collect::<Vec<_>>()
                    ),
                    draft_fields
                );
                assert_eq!(run.app.modal, Some(ActiveModal::UnifiedEditor));
                assert_eq!(run.app.unified_editor.as_ref().unwrap().picking, None);
                assert!(
                    output.shapes.iter().any(|s| matches!(
                        &s.shape, egui::Shape::Text(t) if t.galley.text() == "选区编辑会话"
                    )),
                    "surviving modal heading was omitted on PointerGone"
                );
                assert!(
                    output.shapes.iter().any(|s| matches!(
                        &s.shape, egui::Shape::Rect(r)
                            if r.rect == ctx.content_rect()
                                && r.fill == egui::Color32::from_black_alpha(100)
                    )),
                    "modal backdrop was omitted on PointerGone"
                );
                assert!(run.requests.try_recv().is_err());
                assert_eq!(run.snapshot(), entry);
                assert_eq!(run.app.view.info, info);
            }
        }
    }

    #[test]
    fn changed_apply_publishes_epoch_before_automatic_geometry_queries() {
        let size = egui::vec2(1280., 832.);
        let (mut run, ctx) = settled(size, 2., 1.);
        let entry = run.snapshot();
        let epoch = run.app.view.selection_epoch;
        run.begin();
        let reply = run.command(worker::Command::Apply(Some(run.step(1.))));
        assert_eq!(
            reply.1.unified_editor.as_ref().unwrap().terminal,
            worker::Terminal::Changed
        );
        let published_epoch = run.app.view.selection_epoch;
        let mut queries = 0;
        let mut results = Vec::new();
        for _ in 0..8 {
            capture(&mut run, &ctx, size, 2., vec![]);
            if let Ok(request) = run.requests.try_recv() {
                assert!(matches!(
                    request.2,
                    Action::SelectionCenters(..) | Action::Viewport(..)
                ));
                let reply = run.deliver_request(request);
                results.push((
                    reply.0,
                    reply.1.selection_epoch,
                    reply.1.error.as_ref().map(|e| e.code.clone()),
                    run.app.view.selection_geometry.is_some(),
                ));
                queries += 1;
            } else {
                break;
            }
        }
        assert!(
            results
                .iter()
                .all(|(_, epoch, error, _)| *epoch == published_epoch && error.is_none()),
            "post-Apply task/epoch/error/installed: {results:?}"
        );
        assert_ne!(published_epoch, epoch);
        assert!(
            queries > 0 && queries <= 2,
            "automatic queries did not settle: {queries}"
        );
        assert!(run.app.view.selection_geometry.is_some());
        assert_eq!(
            run.app.view.selection_geometry_identity,
            crate::state::selection_geometry_identity(&run.app.view)
        );
        assert!(!run.app.busy && run.app.geometry_task.is_none());
        run.app.send(Action::History(false));
        run.deliver();
        assert_eq!(run.snapshot().layers, entry.layers);
        assert_eq!(run.snapshot().apertures, entry.apertures);
    }

    #[test]
    fn cancelled_geometry_read_does_not_invalidate_the_published_epoch() {
        let size = egui::vec2(1280., 832.);
        let (mut run, ctx) = settled(size, 2., 1.);
        let info = run.app.view.info.clone();
        let epoch = run.app.view.selection_epoch;
        run.app.view.selection_geometry = None;
        run.app.view.selection_geometry_identity.clear();
        capture(&mut run, &ctx, size, 2., vec![]);
        let request = run.requests.try_recv().unwrap();
        assert!(matches!(request.2, Action::SelectionCenters(..)));
        request.3.cancel_token.cancel();
        run.deliver_request(request);
        let mut results = Vec::new();
        for _ in 0..4 {
            capture(&mut run, &ctx, size, 2., vec![]);
            if let Ok(request) = run.requests.try_recv() {
                assert!(matches!(request.2, Action::SelectionCenters(..)));
                let reply = run.deliver_request(request);
                results.push((
                    reply.1.selection_epoch,
                    reply.1.error.as_ref().map(|e| e.code.clone()),
                ));
            }
        }
        assert!(
            results
                .iter()
                .all(|(worker_epoch, error)| *worker_epoch == epoch && error.is_none()),
            "cancel/retry worker epochs and errors: {results:?}"
        );
        assert_eq!(run.app.view.info, info);
        assert!(run.app.view.selection_geometry.is_some());
        assert!(run.app.geometry_task.is_none() && !run.app.busy);
    }

    fn capture(
        run: &mut Run,
        ctx: &egui::Context,
        size: egui::Vec2,
        ppp: f32,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect, egui::Rect, bool)> {
        let mut raw = egui::RawInput {
            focused: true,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        run.app.raw_input_hook(ctx, &mut raw);
        let output = ctx.run(raw, |ctx| {
            run.app.update(ctx, &mut eframe::Frame::_new_kittest());
        });
        output
            .shapes
            .into_iter()
            .filter_map(|shape| {
                let egui::Shape::Text(text) = shape.shape else {
                    return None;
                };
                let label = text.galley.text();
                (label == PICK || label == NAV || label.starts_with("移动 · ")).then(|| {
                    (
                        label.into(),
                        text.galley.rect.translate(text.pos.to_vec2()),
                        shape.clip_rect,
                        text.galley.elided,
                    )
                })
            })
            .collect()
    }

    fn settled(size: egui::Vec2, ppp: f32, zoom: f32) -> (Run, egui::Context) {
        let mut model = fixture();
        let info = model.view.info.clone();
        // fixture reverses the selection after SelectAll. A real read-only
        // refresh establishes its metrics/selection epoch before App queries it.
        model.run(Action::Viewport(
            MmPoint::new(0., 0.),
            model.view.bounds.unwrap(),
            20.,
        ));
        assert!(model.view.error.is_none());
        assert_eq!(model.view.info, info);
        let mut run = Run::with_model(model);
        run.app.selected_flags = Arc::new(crate::gpu::selection_flags(
            run.app.view.scene.as_ref().unwrap(),
            &run.app.view.selected.ids(),
        ));
        run.app.last_structure_serial = run.app.view.structure_serial;
        run.app.camera.scale = 10.;
        run.app.fit = false;
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        ctx.set_zoom_factor(zoom);
        for _ in 0..16 {
            capture(&mut run, &ctx, size, ppp, vec![]);
            match run.requests.try_recv() {
                Ok(request) => {
                    assert!(matches!(
                        request.2,
                        Action::Viewport(..) | Action::SelectionCenters(..)
                    ));
                    if let Action::SelectionCenters(identity, params) = &request.2 {
                        assert_eq!(
                            identity,
                            &crate::state::selection_geometry_identity(&run.host.model.view),
                            "App/worker geometry identity"
                        );
                        assert_eq!(
                            params.groups,
                            run.host.model.view.selected.groups(),
                            "App/worker selection groups"
                        );
                    }
                    run.deliver_request(request);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return (run, ctx),
                Err(error) => panic!("worker disconnected: {error}"),
            }
        }
        panic!("read-only viewport did not settle");
    }

    fn assert_caption(
        run: &Run,
        captions: &[(String, egui::Rect, egui::Rect, bool)],
        expected: &str,
    ) {
        assert_eq!(captions.len(), 1, "{captions:?}");
        assert_eq!(captions[0].0, expected);
        let slot = crate::canvas_hud::slot(run.app.canvas_rect);
        for rectangle in [run.app.canvas_rect, slot, captions[0].1, captions[0].2] {
            assert!(rectangle.min.x.is_finite() && rectangle.min.y.is_finite());
            assert!(rectangle.max.x.is_finite() && rectangle.max.y.is_finite());
        }
        assert_eq!(captions[0].1.min, slot.min);
        assert!(slot.contains_rect(captions[0].2));
        assert!(
            run.app
                .canvas_rect
                .contains_rect(captions[0].1.intersect(captions[0].2))
        );
        if slot.width() >= 160. && slot.height() >= 100. {
            assert!(!captions[0].3, "instructions elided: {captions:?}");
        }
    }

    #[test]
    fn actual_app_picker_owns_hud_across_native_dpi_zoom_snap_and_fallback_states() {
        for size in [egui::vec2(640., 480.), egui::vec2(1280., 832.)] {
            for ppp in [1., 1.5, 2., 3.] {
                for zoom in [1., 1.25] {
                    let (mut run, ctx) = settled(size, ppp, zoom);
                    run.begin();
                    let main = run.snapshot();
                    let info = run.app.view.info.clone();
                    let fixed_canvas = run.app.canvas_rect;
                    let resources = run
                        .app
                        .unified_editor
                        .as_ref()
                        .unwrap()
                        .reply
                        .as_ref()
                        .unwrap()
                        .resources;
                    for snap in [true, false] {
                        run.app.object_snap.enabled = snap;
                        for pick in [ui::Pick::Base, ui::Pick::Target] {
                            run.app.unified_editor.as_mut().unwrap().picking = Some(pick);
                            run.app.modal = None;
                            assert!(run.app.show_unified_work_for_pick(ctx.pixels_per_point()));
                            let captions = capture(&mut run, &ctx, size, ppp, vec![]);
                            assert_caption(&run, &captions, PICK);
                            assert_eq!(run.app.canvas_rect, fixed_canvas);
                        }
                    }
                    run.app.unified_editor.as_mut().unwrap().picking = None;
                    run.app.modal = Some(ActiveModal::UnifiedEditor);
                    let captions = capture(&mut run, &ctx, size, ppp, vec![]);
                    assert_caption(&run, &captions, NAV);
                    let reply = run.app.unified_editor.as_mut().unwrap().reply.take();
                    run.app.unified_editor.as_mut().unwrap().picking = Some(ui::Pick::Target);
                    run.app.modal = None;
                    let captions = capture(&mut run, &ctx, size, ppp, vec![]);
                    assert_caption(&run, &captions, NAV);
                    run.app.unified_editor.as_mut().unwrap().reply = reply;
                    let draft = run.app.unified_editor.take();
                    let captions = capture(&mut run, &ctx, size, ppp, vec![]);
                    assert_caption(&run, &captions, NAV);
                    run.app.unified_editor = draft;
                    assert_eq!(run.snapshot(), main);
                    assert_eq!(run.app.view.info, info);
                    assert_eq!(
                        run.app
                            .unified_editor
                            .as_ref()
                            .unwrap()
                            .reply
                            .as_ref()
                            .unwrap()
                            .resources,
                        resources
                    );
                }
            }
        }
    }

    #[test]
    fn actual_app_escape_and_confirm_restore_navigation_without_committing() {
        let size = egui::vec2(1280., 832.);
        let (mut run, ctx) = settled(size, 2., 1.);
        run.begin();
        let main = run.snapshot();
        let info = run.app.view.info.clone();
        for escape in [true, false] {
            run.app.unified_editor.as_mut().unwrap().dirty = false;
            run.app.unified_editor.as_mut().unwrap().picking = Some(ui::Pick::Target);
            run.app.modal = None;
            let captions = capture(&mut run, &ctx, size, 2., vec![]);
            assert_caption(&run, &captions, PICK);
            let events = if escape {
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            } else {
                let position = run.app.canvas_rect.center();
                capture(
                    &mut run,
                    &ctx,
                    size,
                    2.,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            };
            let captions = capture(&mut run, &ctx, size, 2., events);
            assert_caption(&run, &captions, NAV);
            assert_eq!(run.app.modal, Some(ActiveModal::UnifiedEditor));
            assert!(run.app.unified_editor.as_ref().unwrap().picking.is_none());
            assert_eq!(run.app.unified_editor.as_ref().unwrap().dirty, !escape);
            assert!(
                run.requests.try_recv().is_err(),
                "picker dispatched a worker edit"
            );
            assert_eq!(run.snapshot(), main);
            assert_eq!(run.app.view.info, info);
        }
    }

    #[test]
    fn existing_move_hud_keeps_priority_in_a_synthetic_coexisting_session() {
        let size = egui::vec2(1280., 832.);
        let (mut run, ctx) = settled(size, 2., 1.);
        run.begin();
        let main = run.snapshot();
        let mut draft = run.app.unified_editor.take().unwrap();
        run.app.modal = None;
        run.app.start_move_place();
        assert!(run.app.move_placing());
        draft.picking = Some(ui::Pick::Target);
        run.app.unified_editor = Some(draft);
        let captions = capture(&mut run, &ctx, size, 2., vec![]);
        assert_eq!(captions.len(), 1, "{captions:?}");
        assert!(captions[0].0.starts_with("移动 · "));
        assert!(run.app.move_placing());
        assert!(run.app.unified_editor.is_some());
        assert_eq!(run.snapshot(), main);
    }
}
fn fixture() -> Model {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rcam-unified-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let paths:Vec<_>=(0..2).map(|i|{let p=dir.join(format!("layer{i}.gbr"));std::fs::write(&p,b"%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*X0Y0D02*X2000000Y0D01*X4000000Y1000000D03*%LPC*%X5000000Y1000000D03*M02*").unwrap();p}).collect();
    let mut m = Model::default();
    m.import_gerbers(&paths).unwrap();
    m.run(Action::SelectAll);
    assert!(m.view.error.is_none());
    assert_eq!(m.view.selected.groups().len(), 2);
    m.view.selected.ordered.reverse();
    std::fs::remove_dir_all(dir).unwrap();
    m
}
struct Run {
    app: crate::EditorApp,
    host: WorkerHost,
    requests: Receiver<session::Request>,
}
impl Run {
    fn new() -> Self {
        Self::with_model(fixture())
    }
    fn with_model(model: Model) -> Self {
        let mut app = crate::modal::tests::app();
        app.view = model.view.clone();
        app.routing.bind_fixture(&app.view);
        app.canvas_rect = rect();
        app.reported_ppp = 1.;
        let host = WorkerHost::new(app.routing.owner(), model);
        let (tx, requests) = sync_channel(1);
        app.tx = tx;
        Self {
            app,
            host,
            requests,
        }
    }
    fn deliver(&mut self) -> session::Reply {
        self.deliver_request(self.requests.try_recv().unwrap())
    }
    fn deliver_request(&mut self, request: session::Request) -> session::Reply {
        let (id, source, action, task, route) = request;
        self.host.route_action(&route, &action).unwrap();
        rcam_diagnostics::with_source(source, || self.host.model.run_task(task, action));
        let reply = (
            id,
            self.host.model.view.clone(),
            self.host.finish(&route).unwrap(),
        );
        self.app
            .receive_session_reply(reply.clone(), Instant::now());
        reply
    }
    fn begin(&mut self) {
        self.app.open_unified_editor();
        self.deliver();
        assert!(self.app.unified_editor.as_ref().unwrap().ready);
        assert!(self.app.view.error.is_none());
    }
    fn step(&self, dx: f64) -> DraftStep {
        DraftStep {
            groups: self.app.view.selected.groups(),
            operation: SelectionEdit::Move {
                dx_mm: dx,
                dy_mm: 0.,
            },
        }
    }
    fn command(&mut self, c: worker::Command) -> session::Reply {
        self.app.send_unified_editor(c);
        self.deliver()
    }
    fn snapshot(&self) -> editor_service::RenderSnapshot {
        let id = &self.host.model.view.info.as_ref().unwrap().document_id;
        self.host.model.service.render_snapshot(id).unwrap()
    }
}
#[test]
fn changed_apply_keeps_optional_metrics_resource_failure_out_of_the_commit_result() {
    let dir = std::env::temp_dir().join(format!("rcam-unified-metrics-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("synthetic.gbr");
    // One valid Region fits the editor's object/display admission, while its
    // 900 edges exceed the unchanged quadratic optional metrics work budget.
    let mut source = String::from("%FSLAX26Y26*%%MOMM*%%ADD10C,0.2*%D10*G36*");
    for i in 0..900 {
        let angle = std::f64::consts::TAU * f64::from(i) / 900.;
        let x = (1_000_000. + 1_000_000. * angle.cos()).round() as i64;
        let y = (1_000_000. + 1_000_000. * angle.sin()).round() as i64;
        source.push_str(&format!("X{x}Y{y}D{:02}*", if i == 0 { 2 } else { 1 }));
    }
    source.push_str("X2000000Y1000000D01*G37*M02*");
    std::fs::write(&path, source).unwrap();
    let mut model = Model::default();
    model.import_gerbers(&[path]).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    model.run(Action::SelectAll);
    assert!(model.view.error.is_none());
    assert_eq!(model.view.selected.ordered.len(), 1);
    let SemanticGeometry::Region { contours } = &model.view.selected.ordered[0].object.geometry
    else {
        panic!("synthetic Region selection");
    };
    assert_eq!(contours[0].edges.len(), 900);
    assert!(
        model
            .view
            .metrics_error
            .as_ref()
            .unwrap()
            .starts_with("RESOURCE_LIMIT")
    );
    let mut run = Run::with_model(model);
    let entry = run.snapshot();
    let before = run.app.view.info.clone().unwrap();
    let epoch = run.app.view.selection_epoch;
    run.begin();
    let reply = run.command(worker::Command::Apply(Some(run.step(1.))));
    assert_eq!(
        reply.1.unified_editor.as_ref().unwrap().terminal,
        worker::Terminal::Changed
    );
    assert!(reply.1.error.is_none());
    assert_eq!(
        reply.1.task_receipt.as_ref().unwrap().state,
        TaskState::Completed
    );
    assert!(
        reply
            .1
            .metrics_error
            .as_ref()
            .unwrap()
            .starts_with("RESOURCE_LIMIT")
    );
    assert_eq!(reply.1.selection_epoch, epoch.wrapping_add(1));
    assert_eq!(
        reply.1.info.as_ref().unwrap().undo_entries,
        before.undo_entries + 1
    );
    assert!(run.app.unified_editor.is_none() && !run.app.busy);
    assert_ne!(run.snapshot().layers, entry.layers);
    run.app.send(Action::History(false));
    run.deliver();
    assert_eq!(run.snapshot().layers, entry.layers);
    assert_eq!(run.snapshot().apertures, entry.apertures);
}
#[test]
fn accumulating_cross_layer_steps_latest_apply_one_undo_and_primary_order() {
    let mut r = Run::new();
    let entry = r.snapshot();
    let info = r.app.view.info.clone().unwrap();
    let primary = r
        .app
        .view
        .selected
        .primary()
        .unwrap()
        .object
        .object_id
        .clone();
    r.begin();
    r.command(worker::Command::Execute(r.step(2.)));
    assert_eq!(r.snapshot(), entry);
    assert_eq!(r.app.view.info.as_ref().unwrap(), &info);
    let mut step = r.step(0.);
    step.operation = SelectionEdit::Rotate {
        angle_deg: 90.,
        pivot_mm: MmPoint::new(0., 0.),
    };
    step.groups.truncate(1);
    r.command(worker::Command::Execute(step));
    let mirror_layer = r.app.view.selected.groups()[1].layer_id.clone();
    let d = r.app.unified_editor.as_mut().unwrap();
    d.mode = ui::Mode::VerticalMirror;
    d.base.x = "1".into();
    for target in &mut d.targets {
        target.enabled = target.group.layer_id == mirror_layer;
    }
    let mirror = d.step(r.app.display_unit).unwrap();
    r.command(worker::Command::Execute(mirror));
    let moved = r
        .app
        .unified_editor
        .as_ref()
        .unwrap()
        .reply
        .as_ref()
        .unwrap()
        .work
        .as_ref()
        .unwrap()
        .snapshot
        .clone();
    assert_ne!(*moved, entry);
    assert_eq!(
        moved.layers.iter().map(|l| l.objects.len()).sum::<usize>(),
        6
    );
    r.command(worker::Command::History(UnifiedEditorHistory::Undo));
    r.command(worker::Command::History(UnifiedEditorHistory::Redo));
    // Final payload was never previewed or executed.
    let final_reply = r.command(worker::Command::Apply(Some(r.step(1.))));
    assert_eq!(
        final_reply.1.unified_editor.as_ref().unwrap().terminal,
        worker::Terminal::Changed
    );
    assert!(r.app.unified_editor.is_none());
    let after = r.app.view.info.as_ref().unwrap();
    assert_eq!(after.undo_entries, info.undo_entries + 1);
    assert_eq!(
        r.app.view.selected.primary().unwrap().object.object_id,
        primary
    );
    assert!(
        r.host
            .model
            .view
            .unified_editor
            .as_ref()
            .unwrap()
            .work
            .is_none()
    );
    assert!(
        r.host
            .model
            .view
            .unified_editor
            .as_ref()
            .unwrap()
            .reference
            .is_empty()
    );
    r.host.model.run(Action::History(false));
    let undone = r.snapshot();
    assert_eq!(undone.layers, entry.layers);
    assert_eq!(undone.apertures, entry.apertures);
}
#[test]
fn reset_nochange_and_cancel_preserve_main_revision_history_and_metadata() {
    let mut r = Run::new();
    let entry = r.snapshot();
    let info = r.app.view.info.clone();
    r.begin();
    let ghost = r
        .app
        .unified_editor
        .as_ref()
        .unwrap()
        .reply
        .as_ref()
        .unwrap()
        .reference
        .clone();
    r.command(worker::Command::Execute(r.step(3.)));
    assert!(Arc::ptr_eq(
        &ghost,
        &r.app
            .unified_editor
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .reference
    ));
    r.command(worker::Command::History(UnifiedEditorHistory::Reset));
    let d = r.app.unified_editor.as_ref().unwrap();
    assert!(!d.dirty);
    let resources = d.reply.as_ref().unwrap().resources.unwrap();
    assert_eq!((resources.undo_entries, resources.redo_entries), (0, 0));
    let nochange = r.command(worker::Command::Apply(None));
    assert_eq!(
        nochange.1.unified_editor.as_ref().unwrap().terminal,
        worker::Terminal::NoChange
    );
    assert_eq!(r.snapshot(), entry);
    assert_eq!(r.app.view.info, info);
    r.begin();
    r.command(worker::Command::Execute(r.step(4.)));
    r.app.request_unified_cancel();
    assert!(r.app.unified_editor.as_ref().unwrap().confirm_cancel);
    assert!(r.requests.try_recv().is_err());
    r.command(worker::Command::Cancel);
    assert_eq!(r.snapshot(), entry);
    assert_eq!(r.app.view.info, info);
    assert!(
        r.host
            .model
            .view
            .unified_editor
            .as_ref()
            .unwrap()
            .work
            .is_none()
    );
}
#[test]
fn invalid_latest_input_and_display_prepare_refusal_do_not_publish_or_commit() {
    let mut r = Run::new();
    let entry = r.snapshot();
    r.begin();
    r.command(worker::Command::Preview(r.step(7.)));
    let work = r
        .app
        .unified_editor
        .as_ref()
        .unwrap()
        .reply
        .as_ref()
        .unwrap()
        .work
        .clone()
        .unwrap();
    let mut invalid = r.step(1.);
    invalid.operation = SelectionEdit::Rotate {
        angle_deg: f64::NAN,
        pivot_mm: MmPoint::new(0., 0.),
    };
    r.command(worker::Command::Apply(Some(invalid)));
    assert!(r.app.view.error.is_some());
    assert_eq!(r.snapshot(), entry);
    assert!(r.app.unified_editor.is_some());
    assert!(Arc::ptr_eq(
        &work,
        r.app
            .unified_editor
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
    ));
    r.app.camera.scale = f64::MAX;
    r.command(worker::Command::Execute(r.step(2.)));
    assert!(r.app.view.error.is_some());
    assert_eq!(r.snapshot(), entry);
    assert!(Arc::ptr_eq(
        &work,
        r.app
            .unified_editor
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
    ));
    r.app.camera.scale = 10.;
    r.command(worker::Command::Apply(Some(r.step(1.))));
    assert!(r.app.unified_editor.is_none());
    assert!(r.app.view.error.is_none());
}
#[test]
fn point_picker_restores_work_metadata_and_refuses_insufficient_display_scale() {
    let mut r = Run::new();
    let main = r.snapshot();
    r.begin();
    let work = r
        .app
        .unified_editor
        .as_ref()
        .unwrap()
        .reply
        .as_ref()
        .unwrap()
        .work
        .clone()
        .unwrap();
    r.app.reported_ppp = 2.;
    r.command(worker::Command::Preview(r.step(20.)));
    assert!(r.app.view.render_ppm > work.scene.ppm);
    r.app.unified_editor.as_mut().unwrap().picking = Some(ui::Pick::Base);
    assert!(r.app.show_unified_work_for_pick(1.));
    assert!(Arc::ptr_eq(r.app.view.scene.as_ref().unwrap(), &work.scene));
    assert_eq!(r.app.view.bounds, work.bounds);
    assert_eq!(r.app.view.render_ppm, work.scene.ppm);
    assert!(r.app.view.render_viewport.is_none());
    assert!(r.app.view.render_coverage_complete);
    assert!(r.app.view.display_attempt.is_none());
    assert!(Arc::ptr_eq(
        r.app.view.snap_snapshot.as_ref().unwrap(),
        &work.snapshot
    ));
    assert!(!r.app.show_unified_work_for_pick(2.));
    assert!(r.app.unified_editor.as_ref().unwrap().picking.is_none());
    assert_eq!(r.app.modal, Some(ActiveModal::UnifiedEditor));
    assert_eq!(r.snapshot(), main);
    r.command(worker::Command::History(UnifiedEditorHistory::Reset));
    r.app.unified_editor.as_mut().unwrap().picking = Some(ui::Pick::Base);
    assert!(r.app.show_unified_work_for_pick(2.));
    assert_eq!(r.app.view.render_ppm, work.scene.ppm * 2.);
    assert_eq!(r.snapshot(), main);
}
#[test]
fn preview_uses_full_exposure_scene_while_snap_uses_executed_manufacturing_work() {
    let mut r = Run::new();
    let source = r.snapshot();
    r.begin();
    r.command(worker::Command::Preview(r.step(20.)));
    let d = r.app.unified_editor.as_ref().unwrap();
    let work = &d.reply.as_ref().unwrap().work.as_ref().unwrap().snapshot;
    assert_eq!(work.layers, source.layers);
    assert!(Arc::ptr_eq(
        work,
        r.app.view.snap_snapshot.as_ref().unwrap()
    ));
    r.app.unified_editor.as_mut().unwrap().picking = Some(ui::Pick::Base);
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            focused: true,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| r.app.unified_editor_controls(ui));
        },
    );
    assert!(Arc::ptr_eq(
        r.app.view.scene.as_ref().unwrap(),
        &r.app
            .unified_editor
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .scene
    ));
    r.app.request_unified_cancel();
    r.command(worker::Command::Execute(r.step(20.)));
    let work = r.app.view.snap_snapshot.as_ref().unwrap();
    assert_eq!(work.apertures, source.apertures);
    for (a, b) in work.layers.iter().zip(&source.layers) {
        assert_eq!(a.objects.len(), b.objects.len());
        for (a, b) in a.objects.iter().zip(&b.objects) {
            assert_eq!(a.object_id, b.object_id);
            assert_eq!(a.exposure, b.exposure);
            assert_eq!(a.origin, b.origin);
        }
    }
    let settings = r.app.object_snap.contour();
    let mut native = crate::object_snap::Runtime::default();
    let mut admitted = crate::object_snap::Runtime::default();
    for alt in [false, true] {
        for point in [
            MmPoint::new(20., 0.),
            MmPoint::new(0., 0.),
            MmPoint::new(22., 0.),
        ] {
            let a = native
                .resolve(
                    point,
                    &settings,
                    r.app.grid,
                    r.app.camera,
                    1.,
                    Some(work),
                    &r.app.view.snap_index,
                    &r.app.view.layers,
                    None,
                    alt,
                )
                .unwrap();
            let b = admitted
                .resolve_draft(
                    point,
                    &settings,
                    r.app.grid,
                    r.app.camera,
                    1.,
                    Some(work),
                    &r.app.view.snap_index,
                    &r.app.view.layers,
                    alt,
                    0,
                )
                .unwrap();
            assert_eq!(a, b);
            if alt {
                assert_eq!(b.point, point);
            }
        }
    }
    // Entry ghost has no object/index, so its old endpoint is not a candidate.
    let old = admitted
        .resolve_draft(
            MmPoint::new(0., 0.),
            &settings,
            r.app.grid,
            r.app.camera,
            1.,
            Some(work),
            &r.app.view.snap_index,
            &r.app.view.layers,
            false,
            0,
        )
        .unwrap();
    assert!(old.candidate.is_none());
}
#[test]
fn identity_generation_gate_and_confirmed_cancel_wait_for_authentic_terminal() {
    let mut r = Run::new();
    r.begin();
    let request = worker::Request {
        draft: Arc::new(()),
        request: Arc::new(()),
        generation: r.app.unified_editor.as_ref().unwrap().generation,
        context: r.app.unified_editor.as_ref().unwrap().context.clone(),
        command: worker::Command::Execute(r.step(99.)),
        render: worker::RenderInput {
            camera: r.app.camera,
            rect: rect(),
            ppp: 1.,
            ppm: 10.,
            retained_ui_bytes: 0,
        },
    };
    r.host.model.run_task(
        TaskContext::new(
            500,
            TaskVersion::capture(
                r.host.model.view.info.as_ref(),
                r.host.model.view.task_generation,
                r.host.model.view.rule_revision,
            ),
        ),
        Action::UnifiedEditor(Box::new(request)),
    );
    assert_eq!(r.host.model.view.error.as_ref().unwrap().code, "STALE_TASK");
    r.app
        .send_unified_editor(worker::Command::Apply(Some(r.step(1.))));
    let (id, source, action, task, route) = r.requests.try_recv().unwrap();
    r.host.route_action(&route, &action).unwrap();
    rcam_diagnostics::with_source(source, || r.host.model.run_task(task.clone(), action));
    let reply = (
        id,
        r.host.model.view.clone(),
        r.host.finish(&route).unwrap(),
    );
    assert_eq!(
        reply.1.task_receipt.as_ref().unwrap().state,
        TaskState::Completed
    );
    r.app.request_unified_cancel();
    r.app.confirm_unified_cancel();
    assert_eq!(
        task.cancel_token.cancel(),
        editor_service::task::CancelOutcome::TooLate
    );
    assert!(r.app.unified_editor.is_some());
    r.app.receive_session_reply(reply, Instant::now());
    assert!(r.app.unified_editor.is_none());
    assert!(r.requests.try_recv().is_err());
}
#[test]
fn queued_cancel_real_thread_uses_existing_host_and_keeps_draft_open() {
    let mut r = Run::new();
    r.begin();
    let entry = r.snapshot();
    r.app
        .send_unified_editor(worker::Command::Execute(r.step(7.)));
    r.app.pending_task.as_ref().unwrap().cancel_token.cancel();
    let (tx, rx) = sync_channel(1);
    let ctx = egui::Context::default();
    let thread = std::thread::spawn(move || session::run_worker(r.host, r.requests, tx, ctx));
    let reply = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        reply.1.task_receipt.as_ref().unwrap().state,
        TaskState::Cancelled
    );
    r.app.receive_session_reply(reply, Instant::now());
    assert!(r.app.unified_editor.as_ref().unwrap().ready);
    assert_eq!(
        r.app.view.snap_snapshot.as_ref().unwrap().layers,
        entry.layers
    );
    drop(r.app.tx);
    thread.join().unwrap();
}
#[test]
fn ui_fences_lifecycle_picker_escape_and_unknown_reply() {
    let mut r = Run::new();
    r.begin();
    r.app.unified_editor.as_mut().unwrap().picking = Some(ui::Pick::Target);
    r.app.modal = None;
    assert!(r.app.command_context_blocked());
    r.app.send(Action::History(false));
    assert!(r.requests.try_recv().is_err());
    r.app.close(true);
    assert!(r.app.unified_editor.is_some());
    assert!(!r.app.close_prompt);
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        focused: true,
        events: vec![
            egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        assert!(!r.app.arbitrate_point_input_frame(ctx));
        assert!(r.app.point_commit_blocked);
    });
    assert_eq!(r.app.modal, Some(ActiveModal::UnifiedEditor));
    assert!(r.app.unified_editor.as_ref().unwrap().picking.is_none());
    r.app.point_commit_blocked = false;
    r.app
        .send_unified_editor(worker::Command::Apply(Some(r.step(1.))));
    let id = r.app.pending_task.as_ref().unwrap().task_id;
    r.app.reject_owned_reply(
        id,
        editor_service::ServiceError {
            code: "STALE_TASK".into(),
            message: "synthetic malformed terminal".into(),
            details: serde_json::json!({}),
        },
    );
    assert!(r.app.unified_editor.as_ref().unwrap().unknown);
    r.app
        .send_unified_editor(worker::Command::Apply(Some(r.step(2.))));
    assert_eq!(r.requests.try_iter().count(), 1);
}
#[test]
fn fixed_modal_real_input_button_rectangles_survive_values_status_and_small_viewports() {
    let view = fixture().view;
    let mut d = ui::Draft::new(&view);
    d.ready = true;
    let ctx = egui::Context::default();
    ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
    for viewport in [egui::vec2(1200., 900.), egui::vec2(360., 480.)] {
        let mut baseline = None;
        for state in 0..7 {
            d.base.x = match state {
                0 => "1".into(),
                1 => "1234567890123456789012345678901234567890".into(),
                2 => String::new(),
                3 => "invalid".into(),
                _ => "-0.125".into(),
            };
            d.target.y = d.base.x.clone();
            d.dirty = true;
            d.ready = state != 4;
            d.unknown = state == 6;
            ui::geometry_probe::RECTS.with(|r| r.borrow_mut().clear());
            let raw = egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                ..Default::default()
            };
            let mut outer = egui::Rect::NOTHING;
            for _warm in 0..if state == 0 { 3 } else { 1 } {
                ui::geometry_probe::RECTS.with(|r| r.borrow_mut().clear());
                let _ = ctx.run(raw.clone(), |ctx| {
                    outer = crate::ui::modal_widgets::fixed_modal(
                        ctx,
                        egui::Id::new("unified-geometry"),
                        egui::vec2(440., 520.),
                        |ui| {
                            let _ = ui::controls(ui, &mut d, Default::default(), state == 5);
                        },
                    )
                    .response
                    .rect;
                });
            }
            let rectangles = ui::geometry_probe::RECTS.with(|r| r.borrow().clone());
            assert_eq!(
                rectangles
                    .iter()
                    .filter(|(tag, _)| tag.starts_with("unified-"))
                    .count(),
                4
            );
            assert_eq!(rectangles.len(), 12);
            assert!(outer.width() <= viewport.x);
            if let Some((expected, window)) = &baseline {
                assert_eq!(&rectangles, expected, "state {state}");
                assert_eq!(&outer, window);
            } else {
                baseline = Some((rectangles, outer));
            }
        }
    }
}
#[test]
fn snap_admission_refuses_before_features_and_alt_bypasses_without_geometry_change() {
    let m = fixture();
    let settings = crate::object_snap::Settings::default().contour();
    let mut runtime = crate::object_snap::Runtime::default();
    let point = MmPoint::new(0., 0.);
    assert!(
        runtime
            .resolve_draft(
                point,
                &settings,
                Default::default(),
                Default::default(),
                1.,
                m.view.snap_snapshot.as_deref(),
                &m.view.snap_index,
                &m.view.layers,
                false,
                crate::unified_editor_resources::HOST_BYTES
            )
            .is_err()
    );
    assert_eq!(runtime.stats.features_generated, 0);
    assert_eq!(
        runtime
            .resolve_draft(
                point,
                &settings,
                Default::default(),
                Default::default(),
                1.,
                m.view.snap_snapshot.as_deref(),
                &m.view.snap_index,
                &m.view.layers,
                true,
                crate::unified_editor_resources::HOST_BYTES
            )
            .unwrap()
            .point,
        point
    );
    let mut oversized = m.view.snap_snapshot.as_ref().unwrap().as_ref().clone();
    oversized.layers[0].objects[0].geometry = SemanticGeometry::Region {
        contours: vec![editor_core::RegionContour {
            role: editor_core::RegionRole::Solid,
            edges: vec![
                editor_core::RegionEdge::Line {
                    start: point,
                    end: MmPoint::new(1., 0.)
                };
                1_000_000
            ],
        }],
    };
    assert!(
        crate::unified_editor_resources::candidate_reserve(&oversized, 10., &mut || Ok(()))
            .is_err()
    );
}
#[test]
fn complete_text_target_is_typed_and_cannot_absorb_colliding_ordinary_id() {
    let mut view = fixture().view;
    let layer = view.selected.ordered[0].layer_id.clone();
    let mut ordinary = view.selected.ordered[0].clone();
    ordinary.object.object_id = "collision".into();
    ordinary.object.origin = editor_core::ObjectOrigin::Imported { command_index: 0 };
    let mut first = ordinary.clone();
    first.object.object_id = "glyph-a".into();
    first.object.origin = editor_core::ObjectOrigin::GeneratedText {
        operation_id: "collision".into(),
    };
    let mut second = first.clone();
    second.object.object_id = "glyph-b".into();
    view.selected.ordered = vec![ordinary, first, second].into();
    let d = ui::Draft::new(&view);
    assert_eq!(d.targets.len(), 2);
    assert_eq!(d.targets[0].group.object_ids, vec!["collision"]);
    assert_eq!(d.targets[1].group.layer_id, layer);
    assert_eq!(d.targets[1].group.object_ids, vec!["glyph-a", "glyph-b"]);
}
#[test]
fn confirmed_cancel_survives_ordinary_recovery_busy_and_runs_after_owned_terminal() {
    let mut r = Run::new();
    r.begin();
    r.command(worker::Command::Execute(r.step(2.)));
    let entry = r.snapshot();
    let dir = std::env::temp_dir().join(format!("rcam-unified-recovery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    r.app.send(Action::RecoveryWrite(dir.clone()));
    assert!(r.app.busy);
    assert!(r.app.unified_editor.as_ref().unwrap().pending.is_none());
    r.app.request_unified_cancel();
    r.app.confirm_unified_cancel();
    assert!(r.app.unified_editor.as_ref().unwrap().cancel_after_reply);
    let reply = r.deliver();
    assert_eq!(
        reply.1.task_receipt.as_ref().unwrap().state,
        TaskState::Completed
    );
    assert!(
        r.app
            .unified_editor
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .is_some_and(|p| matches!(p.command, worker::Command::Cancel))
    );
    r.deliver();
    assert!(r.app.unified_editor.is_none());
    assert_eq!(r.snapshot(), entry);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn queued_and_published_begin_cancel_never_leave_an_orphan_worker_session() {
    for publish in [false, true] {
        let mut r = Run::new();
        let entry = r.snapshot();
        r.app.open_unified_editor();
        let mut reply = None;
        if publish {
            let (id, source, action, task, route) = r.requests.try_recv().unwrap();
            r.host.route_action(&route, &action).unwrap();
            rcam_diagnostics::with_source(source, || r.host.model.run_task(task, action));
            reply = Some((
                id,
                r.host.model.view.clone(),
                r.host.finish(&route).unwrap(),
            ));
        }
        r.app.request_unified_cancel();
        r.app.confirm_unified_cancel();
        if let Some(reply) = reply {
            r.app.receive_session_reply(reply, Instant::now());
            r.deliver();
        } else {
            r.deliver();
        }
        assert!(r.app.unified_editor.is_none());
        assert!(r.host.model.unified_editor.is_none());
        assert_eq!(r.snapshot(), entry);
    }
}
#[test]
fn snap_query_charges_repeated_layer_identity_before_candidate_allocation() {
    let m = fixture();
    let mut s = m.view.snap_snapshot.as_ref().unwrap().as_ref().clone();
    let old = s.layers[0].id.clone();
    s.layers[0].id = "L".repeat(1024 * 1024);
    let mut policies = m.view.layers.clone();
    for l in &mut policies {
        if l.layer_id == old {
            l.layer_id = s.layers[0].id.clone();
        }
    }
    let mut runtime = crate::object_snap::Runtime::default();
    assert!(
        runtime
            .resolve_draft(
                MmPoint::new(0., 0.),
                &crate::object_snap::Settings::default().contour(),
                Default::default(),
                Default::default(),
                1.,
                Some(&s),
                &m.view.snap_index,
                &policies,
                false,
                0
            )
            .is_err()
    );
    assert_eq!(runtime.stats.features_generated, 0);
}
#[test]
fn cancel_confirmation_owns_enter_and_cannot_submit_apply() {
    let mut r = Run::new();
    r.begin();
    r.command(worker::Command::Execute(r.step(1.)));
    r.app.request_unified_cancel();
    assert!(r.app.unified_editor.as_ref().unwrap().confirm_cancel);
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        focused: true,
        events: vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| r.app.parameter_modal(ctx));
    assert!(r.requests.try_recv().is_err());
    assert!(r.app.unified_editor.is_some());
}
#[test]
fn actual_host_4096_regions_refuses_entry_before_publication_and_keeps_main() {
    let mut m = fixture();
    let info = m.view.info.as_ref().unwrap();
    let mut project = m.service.project_snapshot(&info.document_id).unwrap();
    let target_layer = project.layers[0].layer.id.clone();
    let prototype = project.layers[0].layer.objects[0].clone();
    project.layers[0].layer.objects = (0..4096)
        .map(|i| {
            let mut o = prototype.clone();
            o.object_id = format!("region-{i}");
            let x = (i % 64) as f64;
            let y = (i / 64) as f64;
            let points = [
                MmPoint::new(x, y),
                MmPoint::new(x + 0.25, y),
                MmPoint::new(x + 0.25, y + 0.25),
                MmPoint::new(x, y + 0.25),
            ];
            o.geometry = SemanticGeometry::Region {
                contours: vec![editor_core::RegionContour {
                    role: editor_core::RegionRole::Solid,
                    edges: (0..4)
                        .map(|j| editor_core::RegionEdge::Line {
                            start: points[j],
                            end: points[(j + 1) % 4],
                        })
                        .collect(),
                }],
            };
            o
        })
        .collect();
    m.restore_project(&rcam_project::encode_v1(&project).unwrap())
        .unwrap();
    let snapshot = m.snapshot.clone().unwrap();
    let target_layer = snapshot
        .layers
        .iter()
        .find(|l| l.id == target_layer)
        .unwrap();
    m.view.selected.ordered = target_layer
        .objects
        .iter()
        .map(|o| editor_service::ObjectInfo {
            layer_id: target_layer.id.clone(),
            object: o.clone(),
        })
        .collect();
    assert_eq!(m.view.selected.ordered.len(), 4096);
    let entry = m
        .service
        .render_snapshot(&m.view.info.as_ref().unwrap().document_id)
        .unwrap();
    let mut r = Run::with_model(m);
    r.app.open_unified_editor();
    r.deliver();
    assert_eq!(r.app.view.error.as_ref().unwrap().code, "RESOURCE_LIMIT");
    assert!(r.host.model.unified_editor.is_none());
    assert_eq!(r.snapshot(), entry);
    r.app.request_unified_cancel();
    assert!(r.app.unified_editor.is_none());
}
#[test]
fn actual_host_large_region_4096_edges_64_steps_has_bounded_history_and_context() {
    let mut m = fixture();
    let info = m.view.info.as_ref().unwrap();
    let mut project = m.service.project_snapshot(&info.document_id).unwrap();
    let target_layer = project.layers[0].layer.id.clone();
    let context_layer = project.layers[1].layer.id.clone();
    let prototype = project.layers[0].layer.objects[0].clone();
    let vertices: Vec<_> = (0..4096)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 4096.;
            MmPoint::new(a.cos(), a.sin())
        })
        .collect();
    let mut object = prototype;
    object.object_id = "large-region".into();
    object.geometry = SemanticGeometry::Region {
        contours: vec![editor_core::RegionContour {
            role: editor_core::RegionRole::Solid,
            edges: (0..4096)
                .map(|i| editor_core::RegionEdge::Line {
                    start: vertices[i],
                    end: vertices[(i + 1) % 4096],
                })
                .collect(),
        }],
    };
    project.layers[0].layer.objects = vec![object];
    let context = project.layers[1].layer.objects.clone();
    m.restore_project(&rcam_project::encode_v1(&project).unwrap())
        .unwrap();
    let snapshot = m.snapshot.clone().unwrap();
    let target_layer = snapshot
        .layers
        .iter()
        .find(|l| l.id == target_layer)
        .unwrap();
    m.view.selected.ordered = target_layer
        .objects
        .iter()
        .map(|o| editor_service::ObjectInfo {
            layer_id: target_layer.id.clone(),
            object: o.clone(),
        })
        .collect();
    assert_eq!(m.view.selected.ordered.len(), 1);
    let mut r = Run::with_model(m);
    r.begin();
    let mut max_peak = 0;
    for _ in 0..64 {
        r.command(worker::Command::Execute(r.step(0.01)));
        assert!(r.app.view.error.is_none(), "{:?}", r.app.view.error);
        max_peak = max_peak.max(
            r.app
                .unified_editor
                .as_ref()
                .unwrap()
                .reply
                .as_ref()
                .unwrap()
                .host_peak_bytes,
        );
    }
    let reply = r
        .app
        .unified_editor
        .as_ref()
        .unwrap()
        .reply
        .as_ref()
        .unwrap();
    let resources = reply.resources.unwrap();
    assert_eq!(resources.undo_entries, 64);
    assert!(max_peak <= crate::unified_editor_resources::HOST_BYTES);
    assert_eq!(
        reply
            .work
            .as_ref()
            .unwrap()
            .snapshot
            .layers
            .iter()
            .find(|l| l.id == context_layer)
            .unwrap()
            .objects,
        context
    );
    assert_eq!(r.snapshot().layers, snapshot.layers);
    eprintln!(
        "UNIFIED_HOST_SAMPLE selected=1 region_edges=4096 full_objects={} steps=64 backend_resident={} backend_reserved={} host_reserved_peak={} snapshot_owned={} scene_owned={} index_owned={}",
        snapshot
            .layers
            .iter()
            .map(|l| l.objects.len())
            .sum::<usize>(),
        resources.resident_bytes,
        resources.reserved_peak_bytes,
        max_peak,
        crate::unified_editor_resources::snapshot_cost(&reply.work.as_ref().unwrap().snapshot)
            .unwrap(),
        reply.work.as_ref().unwrap().scene.owned_bytes(),
        reply.work.as_ref().unwrap().index.owned_bytes()
    );
    r.command(worker::Command::Cancel);
    assert_eq!(r.snapshot().layers, snapshot.layers);
}
