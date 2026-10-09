//! Real App/raw-input/serial-worker CPU regressions; native input remains external.
use crate::{
    EditorApp,
    move_place::Phase,
    point_input::Context,
    project_ui::Transition,
    state::{Action, Model, View},
};
use editor_core::{
    MmPoint,
    command::{CommandDispatcher, ids},
};
use editor_service::{
    SelectionEdit,
    task::{CancelOutcome, TaskContext},
};
use eframe::{App, egui};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
};

#[cfg(test)]
mod hud_layout {
    use super::*;

    const GENERAL: &str = "几何多选  ·  中键 / 双指平移  ·  捏合缩放";

    fn captions(output: egui::FullOutput) -> Vec<(String, egui::Rect, egui::Rect, bool)> {
        output
            .shapes
            .into_iter()
            .filter_map(|shape| {
                let egui::Shape::Text(text) = shape.shape else {
                    return None;
                };
                let label = text.galley.text();
                (label == GENERAL || label.starts_with("移动 · ")).then(|| {
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

    fn capture(
        run: &mut Run,
        size: egui::Vec2,
        ppp: f32,
    ) -> Vec<(String, egui::Rect, egui::Rect, bool)> {
        capture_events(run, size, ppp, vec![])
    }

    fn capture_events(
        run: &mut Run,
        size: egui::Vec2,
        ppp: f32,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect, egui::Rect, bool)> {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        run.app.raw_input_hook(&run.ctx, &mut input);
        captions(
            run.ctx
                .run(input, |ctx| run.app.update(ctx, &mut run.frame)),
        )
    }

    fn settle_viewport(run: &mut Run, size: egui::Vec2, ppp: f32) {
        let _ = capture(run, size, ppp);
        // Increasing native DPI legitimately requests a higher-LOD viewport.
        // Complete its real read-only worker task before admitting Move.
        for _ in 0..16 {
            let Ok((id, _, action, task)) = run.requests.try_recv() else {
                assert!(run.app.command_enabled(ids::OBJECT_MOVE_PLACE));
                return;
            };
            assert!(matches!(
                action,
                Action::Viewport(..) | Action::SelectionCenters(..)
            ));
            run.model.run_task(task, action);
            run.replies.send((id, run.model.view.clone())).unwrap();
            let _ = capture(run, size, ppp);
        }
        panic!("viewport did not settle");
    }

    #[test]
    fn actual_app_move_owns_fixed_hud_and_cancel_restores_navigation() {
        for size in [egui::vec2(980., 600.), egui::vec2(1280., 832.)] {
            for ppp in [1., 2., 3.] {
                for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
                    for zoom in [1., 1.25] {
                        let mut run = Run::new();
                        run.ctx.set_visuals(visuals.clone());
                        run.ctx.set_zoom_factor(zoom);
                        // Native DPI is RawInput, not Context's UI zoom override.
                        // Settle the viewport before measuring phase-only layout.
                        settle_viewport(&mut run, size, ppp);
                        run.start();
                        let before = run.model.view.info.clone();
                        let preparing = capture(&mut run, size, ppp);
                        assert_eq!(preparing.len(), 1, "{preparing:?}");
                        assert!(preparing[0].0.contains("正在检查制造边界与容量 · Esc 取消"));
                        let fixed_canvas = run.app.canvas_rect;
                        let (id, view, _) = run.work(false);
                        run.replies.send((id, view)).unwrap();
                        let following = capture(&mut run, size, ppp);
                        assert_eq!(following.len(), 1, "{following:?}");
                        assert!(
                            following[0]
                                .0
                                .contains("点击目标提交 · Esc / 右键取消 · Alt 暂停吸附")
                        );
                        assert_eq!(run.app.canvas_rect, fixed_canvas);
                        for caption in [&preparing[0], &following[0]] {
                            assert_eq!(caption.1.min, crate::canvas_hud::slot(fixed_canvas).min);
                            assert!(crate::canvas_hud::slot(fixed_canvas).contains_rect(caption.2));
                            assert!(!caption.3, "actual phase instructions elided: {caption:?}");
                        }
                        assert_eq!(run.model.view.info, before);
                        run.app.cancel_move_place();
                        let idle = capture(&mut run, size, ppp);
                        assert_eq!(idle.len(), 1);
                        assert_eq!(idle[0].0, GENERAL);
                        assert_eq!(run.app.canvas_rect, fixed_canvas);
                    }
                }
            }
        }
    }

    #[test]
    fn all_phase_paint_preserves_text_and_stale_context_does_not_own_hud() {
        let mut run = Run::new();
        run.ready();
        let session = run.app.point_transform.as_ref().unwrap();
        let request = session.requested.clone().unwrap();
        let phases = [
            Phase::Preparing,
            Phase::Following,
            Phase::Frozen,
            Phase::FinalPreview(request.clone()),
            Phase::Ready(request),
            Phase::Applying,
        ];
        let labels = [
            "移动 · 正在检查制造边界与容量 · Esc 取消",
            "移动 · 基点 B 为制造边界中心 · 点击目标提交 · Esc / 右键取消 · Alt 暂停吸附",
            "移动 · 目标已冻结，正在验证 · Esc 取消",
            "移动 · 目标已冻结，正在验证 · Esc 取消",
            "移动 · 正在提交一次事务 · 取消以服务终态为准",
            "移动 · 正在提交一次事务 · 取消以服务终态为准",
        ];
        let canvas = egui::Rect::from_min_size(egui::pos2(30., 40.), egui::vec2(240., 180.));
        let before = run.model.view.info.clone();
        for (phase, label) in phases.into_iter().zip(labels) {
            run.app
                .point_transform
                .as_mut()
                .unwrap()
                .placement
                .as_mut()
                .unwrap()
                .phase = phase;
            let output = run.ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx
                    .layer_painter(egui::LayerId::background())
                    .with_clip_rect(canvas);
                assert!(run.app.paint_move_place(&painter, canvas, 1.));
            });
            let captions = captions(output);
            assert_eq!(captions.len(), 1);
            assert_eq!(captions[0].0, label);
            assert!(!captions[0].3, "{captions:?}");
            assert!(crate::canvas_hud::slot(canvas).contains_rect(captions[0].1));
        }
        assert_eq!(run.model.view.info, before);
        run.app.view.selection_epoch += 1;
        let output = run.ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(canvas);
            assert!(!run.app.paint_move_place(&painter, canvas, 1.));
        });
        assert!(captions(output).is_empty());
        let idle = capture(&mut run, egui::vec2(1280., 832.), 1.);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].0, GENERAL);
    }

    #[test]
    fn paused_hud_keeps_canvas_slot_and_explains_fresh_click_at_small_high_dpi_sizes() {
        for size in [egui::vec2(980., 600.), egui::vec2(1280., 832.)] {
            for ppp in [1., 2., 3.] {
                let mut run = Run::new();
                settle_viewport(&mut run, size, ppp);
                run.ready();
                let following = capture(&mut run, size, ppp);
                let canvas = run.app.canvas_rect;
                let paused = capture_events(&mut run, size, ppp, vec![egui::Event::PointerGone]);
                assert_eq!(paused.len(), 1);
                assert!(paused[0].0.contains("跟踪暂停"));
                assert!(paused[0].0.contains("重新点击"));
                assert_eq!(run.app.canvas_rect, canvas);
                assert_eq!(paused[0].1.min, following[0].1.min);
                assert!(crate::canvas_hud::slot(canvas).contains_rect(paused[0].1));
                assert!(paused[0].2.contains_rect(paused[0].1));
                run.no_edit_requests();
            }
        }
    }
}
type Work = (u64, rcam_diagnostics::Source, Action, TaskContext);
struct Run {
    app: EditorApp,
    model: Model,
    requests: Receiver<Work>,
    replies: SyncSender<(u64, View)>,
    ctx: egui::Context,
    frame: eframe::Frame,
    dir: PathBuf,
}
impl Run {
    fn new() -> Self {
        Self::with_count(2)
    }
    fn with_count(count: usize) -> Self {
        use std::fmt::Write;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rcam-move-place-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("synthetic.gbr");
        let mut source = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\n");
        for n in 0..count {
            writeln!(source, "X{}Y0D02*X{}Y1000000D01*", n * 100, n * 100).unwrap();
        }
        source.push_str("M02*\n");
        std::fs::write(&path, source).unwrap();
        let mut model = Model::default();
        model.run(Action::Open(path));
        model.run(Action::SelectAll);
        assert!(model.view.error.is_none(), "{:?}", model.view.error);
        assert_eq!(model.view.selected.ordered.len(), count);
        // Exercise placement while the optional material-centroid result is unavailable.
        model.view.selection_geometry_identity =
            crate::state::selection_geometry_identity(&model.view);
        model.view.selection_geometry_error = Some("等待面积统计".into());
        let mut app = crate::modal::tests::app();
        let (tx, requests) = sync_channel(16);
        let (replies, rx) = sync_channel(16);
        app.tx = tx;
        app.rx = rx;
        app.view = model.view.clone();
        app.selected_flags = Arc::new(crate::gpu::selection_flags(
            app.view.scene.as_ref().unwrap(),
            &app.view.selected.ids(),
        ));
        app.last_structure_serial = app.view.structure_serial;
        app.camera.scale = 10.;
        app.fit = false;
        app.prefs.interaction.grip_edit = true;
        app.grid.snap_enabled = false;
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        let mut run = Self {
            app,
            model,
            requests,
            replies,
            ctx,
            frame: eframe::Frame::_new_kittest(),
            dir,
        };
        run.update(vec![], egui::Modifiers::NONE, true);
        run.no_edit_requests();
        run
    }
    fn update(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers, focused: bool) {
        let mut raw = egui::RawInput {
            focused,
            modifiers,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 900.),
            )),
            events,
            ..Default::default()
        };
        self.app.raw_input_hook(&self.ctx, &mut raw);
        let _ = self
            .ctx
            .run(raw, |ctx| self.app.update(ctx, &mut self.frame));
    }
    fn hover(&mut self, world: MmPoint, alt: bool) {
        let pos = self.app.camera.screen(world, self.app.canvas_rect);
        self.update(
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers {
                alt,
                ..Default::default()
            },
            true,
        );
    }
    fn pointer(&mut self, world: MmPoint, pressed: bool, alt: bool) {
        let pos = self.app.camera.screen(world, self.app.canvas_rect);
        let modifiers = egui::Modifiers {
            alt,
            ..Default::default()
        };
        self.update(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
            modifiers,
            true,
        );
    }
    fn work(&mut self, apply: bool) -> (u64, View, TaskContext) {
        let (id, _, action, task) = self.requests.try_recv().unwrap();
        assert!(if apply {
            matches!(action, Action::PointApply(..))
        } else {
            matches!(action, Action::PointPreview(..))
        });
        self.model.run_task(task.clone(), action);
        (id, self.model.view.clone(), task)
    }
    fn reply(&mut self, id: u64, view: View) {
        self.replies.send((id, view)).unwrap();
        self.update(vec![], egui::Modifiers::NONE, true);
    }
    fn no_edit_requests(&self) {
        while let Ok((_, _, action, _)) = self.requests.try_recv() {
            assert!(
                matches!(action, Action::SelectionCenters(..) | Action::Viewport(..)),
                "unexpected request: {}",
                match action {
                    Action::Close(_) => "close",
                    Action::ProbeDrag(..) => "probe_drag",
                    Action::CanvasSelect(..) => "canvas_select",
                    Action::CanvasSelectRect(..) => "canvas_select_rect",
                    Action::PointPreview(_) => "point_preview",
                    Action::PointApply(_) => "point_apply",
                    _ => "other",
                }
            );
        }
    }
    fn save_current(&mut self) {
        self.model
            .save_project(Some(&self.dir.join("current.rcam")), false, None)
            .unwrap();
        self.app.view = self.model.view.clone();
        self.app.selected_flags = Arc::new(crate::gpu::selection_flags(
            self.app.view.scene.as_ref().unwrap(),
            &self.app.view.selected.ids(),
        ));
        assert!(!self.app.view.info.as_ref().unwrap().project_dirty);
    }
    fn start(&mut self) {
        assert!(self.app.dispatch(ids::OBJECT_MOVE_PLACE));
        assert!(self.app.move_placing());
        assert!(self.app.modal.is_none());
    }
    fn ready(&mut self) {
        self.start();
        let (id, view, _) = self.work(false);
        assert!(view.error.is_none(), "{:?}", view.error);
        self.reply(id, view);
        assert!(matches!(
            self.app
                .point_transform
                .as_ref()
                .unwrap()
                .placement
                .as_ref()
                .unwrap()
                .phase,
            Phase::Following
        ));
    }
    fn final_preview(&mut self, point: MmPoint) -> crate::point_transform::Request {
        self.pointer(point, true, true);
        self.pointer(point, false, true);
        let phase = &self
            .app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase;
        let Phase::FinalPreview(request) = phase else {
            panic!("no final preview")
        };
        request.clone()
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn esc() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }
}
#[test]
fn exit_classification_keeps_actual_input_and_cleanup_behavior_with_observer_on_or_off() {
    let reasons = [
        "escape",
        "focus_lost",
        "window_focus_lost",
        "pointer_paused",
        "secondary_click",
        "context_changed",
        "modal_replaced",
        "modal_cancelled",
        "point_pick_cancelled",
        "transition",
        "worker_disconnected",
        "tool_changed",
    ];
    for (case, expected) in reasons.into_iter().enumerate() {
        let mut outcomes = Vec::new();
        for enabled in [false, true] {
            let mut run = Run::new();
            run.ready();
            if enabled {
                run.app.frame_trace = Some(crate::frame_trace::Recorder::for_test());
            }
            let before = run.app.view.info.clone();
            match case {
                0 => run.update(vec![esc()], egui::Modifiers::NONE, true),
                1 => run.update(vec![], egui::Modifiers::NONE, false),
                2 => run.update(
                    vec![egui::Event::WindowFocused(false)],
                    egui::Modifiers::NONE,
                    true,
                ),
                3 => run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true),
                4 => {
                    let pos = run.app.canvas_rect.center();
                    for pressed in [true, false] {
                        run.update(
                            vec![egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Secondary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            }],
                            egui::Modifiers::NONE,
                            true,
                        );
                    }
                }
                5 => {
                    run.app.view.selection_epoch += 1;
                    run.update(vec![], egui::Modifiers::NONE, true);
                }
                6 => run.app.open_modal(crate::modal::ActiveModal::Grid),
                7 => run.app.cancel_modal(),
                8 => run.app.finish_point_pick(None),
                9 => {
                    run.app.defer_move_place_transition(Transition::Close);
                }
                10 => run.app.move_place_disconnected(),
                _ => {
                    // Exercise the existing command clear branch with its gate open;
                    // ordinary Following retains a point_pick and blocks this command.
                    run.app.point_pick = None;
                    assert!(run.app.dispatch(ids::TOOL_SELECT));
                }
            }
            assert_eq!(
                run.app.move_placing(),
                case == 3,
                "case {case} observer {enabled}"
            );
            assert_eq!(run.app.view.info, before);
            run.no_edit_requests();
            outcomes.push((
                run.app.view.info == before,
                run.app.view.error.as_ref().map(|e| e.code.clone()),
                run.app.point_input_cancelled,
                run.app.point_commit_blocked,
                run.app.busy,
                run.app.modal,
                run.app.view.point_preview.is_some(),
            ));
            if enabled {
                let records = run.app.frame_trace.as_ref().unwrap().take_test_records();
                if case == 3 {
                    assert!(records.iter().all(|r| r["kind"] != "move_exit"));
                    assert_eq!(
                        records
                            .iter()
                            .find(|r| r["kind"] == "input_counts")
                            .unwrap()["detail"]["pointer_gone"],
                        1
                    );
                } else {
                    let exit = records.iter().find(|r| r["kind"] == "move_exit").unwrap();
                    assert_eq!(exit["reason"], expected, "case {case}");
                    assert_eq!(exit["phase"], "following");
                    assert!(exit["source_ns"].as_str().unwrap().parse::<u64>().is_ok());
                    assert!(exit["binding"]["version_id"].as_u64().unwrap() > 0);
                    if case <= 5 {
                        assert!(exit["binding"]["update_id"].as_u64().unwrap() > 0);
                        assert!(exit["binding"]["input_batch_id"].as_u64().unwrap() > 0);
                    } else {
                        assert_eq!(exit["binding"]["egui_identity_known"], false);
                    }
                    if case == 2 || case == 3 {
                        let detail = &records
                            .iter()
                            .find(|r| r["kind"] == "input_counts")
                            .unwrap()["detail"];
                        assert_eq!(
                            detail[if case == 2 {
                                "window_focus_lost"
                            } else {
                                "pointer_gone"
                            }],
                            1
                        );
                    }
                }
            }
        }
        assert_eq!(outcomes[0], outcomes[1], "case {case}");
    }
}
#[test]
fn task_and_request_exit_classification_keeps_terminal_behavior_with_observer_on_or_off() {
    let reasons = [
        "task_error",
        "preview_missing",
        "invalid_base",
        "preview_phase_mismatch",
        "task_identity_mismatch",
        "modal_task_completed",
        "request_invalid",
        "admission_rejected",
        "initial_admission_rejected",
        "task_error",
    ];
    for (fault, expected) in reasons.into_iter().enumerate() {
        let mut outcomes = Vec::new();
        for enabled in [false, true] {
            let mut run = Run::new();
            if enabled {
                run.app.frame_trace = Some(crate::frame_trace::Recorder::for_test());
            }
            if fault < 5 {
                run.start();
                let (id, mut view, _) = run.work(false);
                match fault {
                    0 => {
                        view.error = Some(editor_service::ServiceError {
                            code: "RESOURCE_LIMIT".into(),
                            message: "synthetic refusal".into(),
                            details: serde_json::json!({}),
                        })
                    }
                    1 => view.point_preview = None,
                    2 => {
                        Arc::make_mut(view.point_preview.as_mut().unwrap())
                            .bounds
                            .min_x_mm = f64::NAN
                    }
                    3 => {
                        run.app
                            .point_transform
                            .as_mut()
                            .unwrap()
                            .placement
                            .as_mut()
                            .unwrap()
                            .phase = Phase::Following
                    }
                    _ => view.info.as_mut().unwrap().project_id.push('x'),
                }
                run.reply(id, view);
            } else if fault == 5 || fault == 9 {
                run.ready();
                run.final_preview(MmPoint::new(3., 2.));
                let (id, view, _) = run.work(false);
                run.reply(id, view);
                let (id, mut view, _) = run.work(true);
                if fault == 9 {
                    // A synthetic terminal error exercises classification only;
                    // it does not make the reply proof of a successful commit.
                    view.error = Some(editor_service::ServiceError {
                        code: "RESOURCE_LIMIT".into(),
                        message: "synthetic terminal failure".into(),
                        details: serde_json::json!({}),
                    });
                }
                run.reply(id, view);
                assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
            } else {
                if fault != 8 {
                    run.ready();
                }
                if fault == 6 {
                    let session = run.app.point_transform.as_mut().unwrap();
                    session.target.x = "invalid-number".into();
                    session.placement.as_mut().unwrap().phase = Phase::Frozen;
                    run.app.tick_move_place();
                } else {
                    // Retire the real worker receiver, so normal send admission fails.
                    let (_, replacement) = sync_channel(1);
                    drop(std::mem::replace(&mut run.requests, replacement));
                    if fault == 8 {
                        run.app.start_move_place();
                    } else {
                        run.app
                            .point_transform
                            .as_mut()
                            .unwrap()
                            .placement
                            .as_mut()
                            .unwrap()
                            .phase = Phase::Frozen;
                        run.app.tick_move_place();
                    }
                }
            }
            assert!(!run.app.move_placing(), "fault {fault} observer {enabled}");
            run.no_edit_requests();
            outcomes.push((
                run.app.view.info.as_ref().unwrap().undo_entries,
                run.app.view.error.as_ref().map(|e| e.code.clone()),
                run.app.busy,
                run.app.canvas_selection_unconfirmed,
                run.app.point_input_cancelled,
                run.app.point_commit_blocked,
                run.app.view.point_preview.is_some(),
            ));
            if enabled {
                let rows = run.app.frame_trace.as_ref().unwrap().take_test_records();
                let exit = rows.iter().find(|r| r["kind"] == "move_exit").unwrap();
                assert_eq!(exit["reason"], expected, "fault {fault}");
                if fault == 9 {
                    assert_eq!(exit["phase"], "applying");
                }
            }
        }
        assert_eq!(outcomes[0], outcomes[1], "fault {fault}");
    }
}

#[test]
fn hover_reuses_template_and_snapshot_without_worker_or_hold_then_one_exact_commit() {
    let mut run = Run::new();
    let before = run.model.view.snap_snapshot.clone().unwrap();
    let info = run.app.view.info.clone();
    let selected = run.app.view.selected.ordered.clone();
    run.ready();
    let template = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .placement
        .as_ref()
        .unwrap()
        .baseline
        .clone()
        .unwrap();
    let base = template.bounds.center();
    assert_eq!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .base
            .value
            .world_mm,
        base
    );
    for n in 0..64 {
        run.hover(MmPoint::new(2. + n as f64 / 100., 3.), true);
        assert!(run.requests.try_recv().is_err());
        assert!(Arc::ptr_eq(
            &template,
            run.app
                .point_transform
                .as_ref()
                .unwrap()
                .placement
                .as_ref()
                .unwrap()
                .baseline
                .as_ref()
                .unwrap()
        ));
        assert!(selected.shares_storage(&run.app.view.selected.ordered));
    }
    assert_eq!(run.app.view.info, info);
    assert!(run.app.drag.is_none() && run.app.grip.is_none());
    let request = run.final_preview(MmPoint::new(3.123456789, 2.987654321));
    let target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .world_mm;
    assert_eq!(
        request.operation,
        SelectionEdit::Move {
            dx_mm: target.x_mm - base.x_mm,
            dy_mm: target.y_mm - base.y_mm
        }
    );
    run.hover(MmPoint::new(7., 5.), false);
    assert_eq!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .target
            .value
            .world_mm,
        target
    );
    let (id, view, _) = run.work(false);
    assert!(view.error.is_none());
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    assert!(view.error.is_none());
    run.reply(id, view);
    assert!(!run.app.move_placing() && run.app.point_pick.is_none());
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    let after = run.model.view.snap_snapshot.clone().unwrap();
    let SelectionEdit::Move { dx_mm, dy_mm } = request.operation else {
        panic!()
    };
    for (a, b) in before.layers[0]
        .objects
        .iter()
        .zip(&after.layers[0].objects)
    {
        assert_eq!(
            b.geometry,
            SelectionEdit::Move { dx_mm, dy_mm }
                .preview_geometry(&a.geometry)
                .unwrap()
        );
    }
    run.no_edit_requests();
    run.model.run(Action::History(false));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        before.layers[0].objects
    );
    run.model.run(Action::History(true));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        after.layers[0].objects
    );
}
#[test]
fn target_click_before_admission_freezes_event_point_and_scale_without_committing_zero_preview() {
    let mut run = Run::new();
    let before = run.app.view.info.clone();
    run.start();
    run.pointer(MmPoint::new(3., 2.), true, true);
    run.pointer(MmPoint::new(3., 2.), false, true);
    let target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .clone();
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Frozen
    ));
    run.app.camera.scale = 8.;
    run.hover(MmPoint::new(6., 4.), false);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let request = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .requested
        .clone()
        .unwrap();
    assert_ne!(
        request.operation,
        SelectionEdit::Move {
            dx_mm: 0.,
            dy_mm: 0.
        }
    );
    assert_eq!(request.ppm, 10.);
    assert_eq!(
        run.app.point_transform.as_ref().unwrap().target.value,
        target
    );
    assert_eq!(run.app.view.info, before);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    run.reply(id, view);
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
}
#[test]
fn escape_focus_pointergone_ime_and_secondary_click_cannot_share_target_commit() {
    for conflict in 0..5 {
        let mut run = Run::new();
        run.ready();
        let before = run.model.view.info.clone();
        let point = MmPoint::new(3., 2.);
        let pos = run.app.camera.screen(point, run.app.canvas_rect);
        run.pointer(point, true, true);
        if conflict == 4 {
            run.update(
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
                egui::Modifiers::ALT,
                true,
            );
        }
        let mut events = vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::ALT,
        }];
        match conflict {
            0 => events.push(esc()),
            1 => events.push(egui::Event::WindowFocused(false)),
            2 => events.push(egui::Event::PointerGone),
            3 => events.push(egui::Event::Ime(egui::ImeEvent::Preedit("x".into()))),
            _ => events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: Default::default(),
            }),
        }
        run.update(events, egui::Modifiers::ALT, conflict != 1);
        assert!(run.requests.try_recv().is_err(), "conflict{conflict}");
        assert_eq!(run.model.view.info, before);
        if conflict != 3 && conflict != 2 {
            assert!(!run.app.move_placing());
        } else if conflict == 2 {
            assert!(run.app.move_placing());
        }
    }
}

fn primary(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::ALT,
    }
}

fn assert_unfrozen(run: &Run, before: &Option<editor_service::DocumentInfo>) {
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Preparing | Phase::Following
    ));
    assert_eq!(&run.app.view.info, before);
    assert_eq!(&run.model.view.info, before);
    assert!(run.requests.try_recv().is_err());
}

#[test]
fn pointer_gone_batches_reentry_click_and_old_release_cannot_confirm() {
    for gone_first in [false, true] {
        let mut run = Run::new();
        run.ready();
        let before = run.app.view.info.clone();
        let point = MmPoint::new(3., 2.);
        run.pointer(point, true, true);
        let session = run.app.point_transform.as_ref().unwrap();
        let base = session.base.value.clone();
        let target = session.target.value.clone();
        let baseline = session
            .placement
            .as_ref()
            .unwrap()
            .baseline
            .clone()
            .unwrap();
        let pos = run
            .app
            .camera
            .screen(MmPoint::new(7., 5.), run.app.canvas_rect);
        let mut events = vec![
            egui::Event::PointerMoved(pos),
            primary(pos, false),
            primary(pos, true),
            primary(pos, false),
        ];
        events.insert(
            if gone_first { 0 } else { events.len() },
            egui::Event::PointerGone,
        );
        run.update(events, egui::Modifiers::ALT, true);
        assert_unfrozen(&run, &before);
        let session = run.app.point_transform.as_ref().unwrap();
        assert_eq!(session.base.value, base);
        assert_eq!(session.target.value, target);
        assert!(Arc::ptr_eq(
            session
                .placement
                .as_ref()
                .unwrap()
                .baseline
                .as_ref()
                .unwrap(),
            &baseline
        ));
        assert!(run.app.object_snap_runtime.current.is_none());
        // Idle/stale hover and an outside pointer event do not resume tracking.
        run.update(vec![], egui::Modifiers::ALT, true);
        run.update(
            vec![egui::Event::PointerMoved(egui::pos2(-1., -1.))],
            egui::Modifiers::ALT,
            true,
        );
        assert_unfrozen(&run, &before);
        // The re-entry batch's entire click is discarded, in either event order.
        run.update(
            vec![
                egui::Event::PointerMoved(pos),
                primary(pos, true),
                primary(pos, false),
            ],
            egui::Modifiers::ALT,
            true,
        );
        assert_unfrozen(&run, &before);
        run.update(vec![primary(pos, false)], egui::Modifiers::ALT, true);
        assert_unfrozen(&run, &before);
        run.final_preview(point);
        let (id, view, _) = run.work(false);
        run.reply(id, view);
        let (id, view, _) = run.work(true);
        run.reply(id, view);
        assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
        run.no_edit_requests();
    }
}

#[test]
fn preparing_reply_supplies_base_without_unpausing_or_reusing_old_press() {
    let mut run = Run::new();
    run.start();
    let before = run.app.view.info.clone();
    let point = MmPoint::new(3., 2.);
    run.pointer(point, true, true);
    run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
    let (id, view, task) = run.work(false);
    assert!(view.error.is_none(), "{:?}", view.error);
    assert_eq!(task.cancel_token.cancel(), CancelOutcome::TooLate);
    // The read-only result remains live and installs its exact manufacturing B.
    let expected_base = view.point_preview.as_ref().unwrap().bounds.center();
    run.reply(id, view);
    assert_unfrozen(&run, &before);
    assert_eq!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .base
            .value
            .world_mm,
        expected_base
    );
    let saved_target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .clone();
    let pos = run
        .app
        .camera
        .screen(MmPoint::new(8., 6.), run.app.canvas_rect);
    run.update(vec![], egui::Modifiers::NONE, true);
    assert_eq!(
        run.app.point_transform.as_ref().unwrap().target.value,
        saved_target
    );
    // An old release with a valid position can only re-enter; it cannot freeze.
    run.update(vec![primary(pos, false)], egui::Modifiers::ALT, true);
    assert_unfrozen(&run, &before);
    run.update(vec![primary(pos, false)], egui::Modifiers::ALT, true);
    assert_unfrozen(&run, &before);
    run.final_preview(point);
}

#[test]
fn repeated_pointer_loss_requires_a_new_owned_click_after_every_reentry() {
    let mut run = Run::new();
    run.ready();
    let before = run.app.view.info.clone();
    let point = MmPoint::new(3., 2.);
    let pos = run.app.camera.screen(point, run.app.canvas_rect);
    for _ in 0..4 {
        run.pointer(point, true, true);
        run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
        run.update(
            vec![egui::Event::PointerGone, egui::Event::PointerMoved(pos)],
            egui::Modifiers::ALT,
            true,
        );
        run.update(
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::ALT,
            true,
        );
        run.update(vec![primary(pos, false)], egui::Modifiers::ALT, true);
        assert_unfrozen(&run, &before);
    }
    run.final_preview(point);
}

#[test]
fn pointer_loss_and_reentry_barriers_survive_multiple_egui_passes() {
    let mut run = Run::new();
    run.ready();
    run.ctx
        .options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(2).unwrap());
    let before = run.app.view.info.clone();
    let point = MmPoint::new(3., 2.);
    let pos = run.app.camera.screen(point, run.app.canvas_rect);
    run.pointer(point, true, true);
    for events in [
        vec![
            egui::Event::PointerGone,
            egui::Event::PointerMoved(pos),
            primary(pos, false),
        ],
        vec![
            egui::Event::PointerMoved(pos),
            primary(pos, true),
            primary(pos, false),
        ],
    ] {
        let mut raw = egui::RawInput {
            focused: true,
            modifiers: egui::Modifiers::ALT,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 900.),
            )),
            events,
            ..Default::default()
        };
        run.app.raw_input_hook(&run.ctx, &mut raw);
        let mut passes = 0;
        let _ = run.ctx.run(raw, |ctx| {
            passes += 1;
            run.app.update(ctx, &mut run.frame);
            if ctx.current_pass_index() == 0 {
                ctx.request_discard("input barrier regression");
            }
        });
        assert_eq!(passes, 2);
        assert_unfrozen(&run, &before);
    }
    run.update(vec![primary(pos, false)], egui::Modifiers::ALT, true);
    assert_unfrozen(&run, &before);
    run.final_preview(point);
}

#[test]
fn paused_placement_still_cancels_on_focus_escape_secondary_and_context_change() {
    for cause in 0..7 {
        let mut run = Run::new();
        run.ready();
        let before = run.app.view.info.clone();
        run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
        let pos = run.app.canvas_rect.center();
        match cause {
            0 => run.update(
                vec![egui::Event::PointerGone, esc()],
                egui::Modifiers::NONE,
                true,
            ),
            1 => run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, false),
            2 => run.update(
                vec![
                    egui::Event::WindowFocused(false),
                    egui::Event::PointerMoved(pos),
                ],
                egui::Modifiers::NONE,
                true,
            ),
            3 => {
                for pressed in [true, false] {
                    run.update(
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                        egui::Modifiers::NONE,
                        true,
                    );
                }
            }
            _ => {
                match cause {
                    4 => run.app.view.selection_epoch += 1,
                    5 => run.app.view.rule_revision += 1,
                    _ => run.app.view.task_generation += 1,
                }
                run.update(
                    vec![
                        egui::Event::PointerMoved(pos),
                        primary(pos, true),
                        primary(pos, false),
                    ],
                    egui::Modifiers::ALT,
                    true,
                );
            }
        }
        assert!(!run.app.move_placing(), "cause {cause}");
        assert_eq!(run.app.view.info, before);
        assert_eq!(run.model.view.info, before);
        run.no_edit_requests();
    }
}

#[test]
fn paused_clean_and_dirty_project_transitions_retire_session_and_wait_for_task() {
    for dirty in [false, true] {
        for preparing in [false, true] {
            let mut run = Run::new();
            if !dirty {
                run.save_current();
            } else {
                run.model.run(Action::Move("0.1".into(), "0.2".into()));
                assert!(run.model.view.error.is_none());
                run.app.view = run.model.view.clone();
            }
            assert_eq!(run.app.view.info.as_ref().unwrap().project_dirty, dirty);
            if preparing {
                run.start();
            } else {
                run.ready();
            }
            let before = run.app.view.info.clone();
            run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
            assert!(run.app.move_placing());
            run.app.begin_transition(Transition::Close);
            assert!(!run.app.move_placing());
            assert_eq!(run.app.view.info, before);
            if preparing {
                let (id, view, _) = run.work(false);
                run.reply(id, view);
                assert!(!run.app.move_placing());
            }
            assert_eq!(run.app.close_prompt, dirty);
            if !dirty {
                let (id, _, action, task) = run.requests.try_recv().unwrap();
                assert!(matches!(action, Action::Close(false)));
                run.model.run_task(task, action);
                run.reply(id, run.model.view.clone());
                assert!(run.app.view.info.is_none());
                assert!(!run.app.move_placing());
            }
            run.no_edit_requests();
        }
    }
}

#[test]
fn invalid_context_cannot_pause_or_reenter_another_project() {
    for invalid_before_gone in [false, true] {
        let mut run = Run::new();
        run.ready();
        if !invalid_before_gone {
            run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
        }
        run.app
            .view
            .info
            .as_mut()
            .unwrap()
            .project_id
            .push_str("-different");
        let pos = run.app.canvas_rect.center();
        run.update(
            vec![
                egui::Event::PointerGone,
                egui::Event::PointerMoved(pos),
                primary(pos, true),
                primary(pos, false),
            ],
            egui::Modifiers::ALT,
            true,
        );
        assert!(!run.app.move_placing());
        assert_eq!(run.model.view.info.as_ref().unwrap().undo_entries, 0);
        run.no_edit_requests();
    }
}

#[test]
fn stale_paused_move_cannot_transfer_same_batch_to_a_real_grip_or_selection() {
    let mut run = Run::with_count(1);
    run.ready();
    let before = run.app.view.info.clone();
    let geometry = run.app.view.selected.ordered[0].object.geometry.clone();
    let features = crate::grip::features(&run.app.view).unwrap();
    assert!(!features.is_empty());
    let pos = run
        .app
        .camera
        .screen(features[0].position_mm, run.app.canvas_rect);
    assert!(run.app.canvas_rect.contains(pos));
    assert!(crate::grip::hit(&features, pos, run.app.camera, run.app.canvas_rect, 1.).is_some());
    run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
    run.app.view.selection_epoch += 1;
    let end = pos + egui::vec2(20., 20.);
    run.update(
        vec![
            egui::Event::PointerMoved(pos),
            primary(pos, true),
            egui::Event::PointerMoved(end),
            primary(end, false),
        ],
        egui::Modifiers::ALT,
        true,
    );
    assert!(!run.app.move_placing());
    assert!(run.app.point_input_cancelled && run.app.point_commit_blocked);
    assert!(run.app.grip.is_none() && run.app.drag.is_none());
    assert_eq!(run.app.view.info, before);
    assert_eq!(run.model.view.info, before);
    assert_eq!(run.app.view.selected.ordered[0].object.geometry, geometry);
    run.no_edit_requests();
}

#[test]
fn pointer_loss_after_freeze_retires_preview_and_preserves_late_apply_terminal() {
    for applying in [false, true] {
        let mut run = Run::new();
        run.ready();
        run.final_preview(MmPoint::new(3., 2.));
        let (id, view, _) = run.work(false);
        let (id, view) = if applying {
            run.reply(id, view);
            let (id, view, task) = run.work(true);
            assert_eq!(task.cancel_token.cancel(), CancelOutcome::TooLate);
            (id, view)
        } else {
            (id, view)
        };
        run.update(vec![egui::Event::PointerGone], egui::Modifiers::NONE, true);
        assert!(!run.app.move_placing());
        run.reply(id, view);
        assert!(!run.app.move_placing());
        assert_eq!(
            run.app.view.info.as_ref().unwrap().undo_entries,
            usize::from(applying)
        );
        run.no_edit_requests();
    }
}
#[test]
fn cancel_retired_baseline_and_final_previews_never_resurrect_or_commit() {
    for final_stage in [false, true] {
        let mut run = Run::new();
        if final_stage {
            run.ready();
            run.final_preview(MmPoint::new(3., 2.));
        } else {
            run.start();
        }
        let before = run.app.view.info.clone();
        run.update(vec![esc()], egui::Modifiers::NONE, true);
        let (id, view, _) = run.work(false);
        run.reply(id, view.clone());
        run.reply(id, view);
        assert!(!run.app.move_placing() && run.app.view.point_preview.is_none());
        assert_eq!(run.app.view.info, before);
        assert!(!run.app.busy);
        run.no_edit_requests();
    }
}
#[test]
fn late_cancel_installs_real_commit_and_one_undo_without_restarting_placement() {
    let mut run = Run::new();
    run.ready();
    run.final_preview(MmPoint::new(3., 2.));
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, view, task) = run.work(true);
    assert_eq!(task.cancel_token.cancel(), CancelOutcome::TooLate);
    run.update(vec![esc()], egui::Modifiers::NONE, true);
    run.reply(id, view.clone());
    run.reply(id, view);
    assert!(!run.app.move_placing());
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    assert_eq!(run.app.view.info, run.model.view.info);
    run.no_edit_requests();
}
#[test]
fn exact_context_rejects_project_selection_revision_permission_and_precision_changes() {
    for change in 0..7 {
        let mut run = Run::new();
        run.start();
        let (id, view, _) = run.work(false);
        let old = Context::capture(&run.app.view);
        match change {
            0 => run.app.view.info.as_mut().unwrap().project_id.push('x'),
            1 => {
                run.app.view.selected.ordered =
                    run.app.view.selected.ordered.iter().cloned().collect()
            }
            2 => run.app.view.selection_epoch += 1,
            3 => run.app.view.info.as_mut().unwrap().revision.push('x'),
            4 => run
                .app
                .view
                .info
                .as_mut()
                .unwrap()
                .workspace_revision
                .push('x'),
            5 => run.app.view.rule_revision += 1,
            _ => {
                run.app
                    .view
                    .info
                    .as_mut()
                    .unwrap()
                    .manufacturing_precision
                    .resolution_mm *= 2.
            }
        }
        assert!(!old.valid(&run.app.view));
        run.reply(id, view);
        assert!(!run.app.move_placing());
        assert_eq!(run.app.view.error.as_ref().unwrap().code, "STALE_TASK");
        run.no_edit_requests();
    }
}
#[test]
fn preview_error_missing_result_and_apply_unknown_or_disconnect_fail_closed() {
    for fault in 0..4 {
        let mut run = Run::new();
        if fault < 2 {
            run.start();
        } else {
            run.ready();
            run.final_preview(MmPoint::new(3., 2.));
            let (id, view, _) = run.work(false);
            run.reply(id, view);
        }
        if fault == 3 {
            run.app.move_place_disconnected();
            assert!(run.app.canvas_selection_unconfirmed);
            continue;
        }
        let (id, mut view, _) = run.work(fault == 2);
        if fault == 0 {
            view.error = Some(editor_service::ServiceError {
                code: "RESOURCE_LIMIT".into(),
                message: "synthetic budget refusal".into(),
                details: serde_json::json!({}),
            });
        } else if fault == 1 {
            view.point_preview = None;
        } else {
            view.info.as_mut().unwrap().project_id.push('x');
        }
        run.reply(id, view);
        assert!(!run.app.move_placing());
        run.no_edit_requests();
        if fault == 2 {
            assert!(run.app.canvas_selection_unconfirmed);
        } else {
            assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 0);
        }
    }
}
#[test]
fn first_open_transition_waits_for_terminal_preview_and_never_uses_retired_selection() {
    let mut run = Run::new();
    run.save_current();
    run.start();
    let path = run.dir.join("synthetic.gbr");
    // Use an existing saved project, not a Gerber passed to OpenProject.
    let project = run.dir.join("target.rcam");
    let mut target = Model::default();
    target.run(Action::NewWorkspace);
    target.save_project(Some(&project), false, None).unwrap();
    let target_id = target.view.info.as_ref().unwrap().project_id.clone();
    run.app.begin_transition(Transition::Open(project.clone()));
    run.app.begin_transition(Transition::Close);
    assert!(!run.app.move_placing() && run.app.busy);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, _, action, task) = run.requests.try_recv().unwrap_or_else(|e|panic!("request {e:?}; busy={} transition={} close={} projecterr={:?} ui={:?} error={:?} pointtask={} unconfirmed={}",run.app.busy,run.app.transition.is_some(),run.app.close_prompt,run.app.project_error,run.app.ui_error,run.app.view.error,run.app.move_place_task.is_some(),run.app.canvas_selection_unconfirmed));
    assert!(matches!(&action,Action::OpenProject(p,false)if p==&project));
    run.model.run_task(task, action);
    run.reply(id, run.model.view.clone());
    assert_eq!(run.app.view.info.as_ref().unwrap().project_id, target_id);
    assert!(run.app.view.selected.ordered.is_empty());
    assert!(path.exists());
}
#[test]
fn eighty_thousand_click_move_keeps_complete_selection_and_exact_one_undo_redo() {
    let mut run = Run::with_count(80_000);
    let before = run.model.view.snap_snapshot.clone().unwrap();
    run.ready();
    assert!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .baseline
            .as_ref()
            .unwrap()
            .simplified
    );
    let request = run.final_preview(MmPoint::new(6., 3.));
    let (id, view, _) = run.work(false);
    assert!(view.error.is_none(), "{:?}", view.error);
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    assert!(view.error.is_none(), "{:?}", view.error);
    run.reply(id, view);
    assert_eq!(run.app.view.selected.ordered.len(), 80_000);
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    let after = run.model.view.snap_snapshot.clone().unwrap();
    for (a, b) in before.layers[0]
        .objects
        .iter()
        .zip(&after.layers[0].objects)
    {
        assert_eq!(
            b.geometry,
            request.operation.preview_geometry(&a.geometry).unwrap()
        );
        assert_eq!(a.object_id, b.object_id);
    }
    run.model.run(Action::History(false));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        before.layers[0].objects
    );
    run.model.run(Action::History(true));
    assert_eq!(
        run.model.view.snap_snapshot.as_ref().unwrap().layers[0].objects,
        after.layers[0].objects
    );
}

#[test]
fn idle_following_disconnect_releases_input_and_template_before_same_frame_click() {
    let mut run = Run::new();
    run.ready();
    assert!(run.app.move_place_task.is_none());
    let (dead, dead_rx) = sync_channel(1);
    drop(dead_rx);
    drop(std::mem::replace(&mut run.replies, dead));
    run.pointer(MmPoint::new(3., 2.), true, true);
    assert!(
        !run.app.move_placing()
            && run.app.point_pick.is_none()
            && run.app.view.point_preview.is_none()
    );
    assert!(run.app.drag.is_none() && run.app.grip.is_none());
    assert_eq!(
        run.app.view.error.as_ref().unwrap().code,
        "WORKER_DISCONNECTED"
    );
    assert!(run.requests.try_recv().is_err());
}
#[test]
fn click_release_alt_owns_raw_target_even_when_final_frame_modifiers_differ() {
    let mut run = Run::new();
    run.ready();
    run.app.grid.snap_enabled = true;
    run.app.grid.spacing_mm = 1.;
    let point = MmPoint::new(3.333333, 2.666667);
    let pos = run.app.camera.screen(point, run.app.canvas_rect);
    run.pointer(point, true, false);
    run.update(
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::ALT,
        }],
        egui::Modifiers::NONE,
        true,
    );
    let request = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .requested
        .clone()
        .unwrap();
    let target = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .target
        .value
        .clone();
    assert_eq!(target.source, crate::point_input::Source::Raw);
    assert_eq!(
        target.world_mm,
        run.app.camera.world(pos, run.app.canvas_rect)
    );
    let base = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .base
        .value
        .world_mm;
    assert_eq!(
        request.operation,
        SelectionEdit::Move {
            dx_mm: target.world_mm.x_mm - base.x_mm,
            dy_mm: target.world_mm.y_mm - base.y_mm
        }
    );
}
#[test]
fn deferred_close_waits_and_does_not_discard_unknown_apply_or_late_success() {
    for outcome in 0..3 {
        let mut run = Run::new();
        run.save_current();
        run.ready();
        run.final_preview(MmPoint::new(3., 2.));
        let (id, view, _) = run.work(false);
        run.reply(id, view);
        let result = if outcome == 0 {
            None
        } else {
            Some(run.work(true))
        };
        run.app.begin_transition(Transition::Close);
        assert!(run.app.busy && !run.app.move_placing());
        let (id, mut view, _) = result.unwrap_or_else(|| run.work(true));
        if outcome == 2 {
            view.task_receipt.as_mut().unwrap().task_id += 1;
        }
        run.reply(id, view);
        match outcome {
            0 => {
                let (_, _, action, _) = run.requests.try_recv().unwrap_or_else(|e|panic!("request {e:?}; busy={} transition={} close={} projecterr={:?} ui={:?} error={:?} pointtask={} unconfirmed={}",run.app.busy,run.app.transition.is_some(),run.app.close_prompt,run.app.project_error,run.app.ui_error,run.app.view.error,run.app.move_place_task.is_some(),run.app.canvas_selection_unconfirmed));
                assert!(matches!(action, Action::Close(false)));
                assert!(!run.app.canvas_selection_unconfirmed);
            }
            1 => {
                assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
                assert!(run.app.close_prompt);
                run.no_edit_requests();
            }
            _ => {
                assert!(run.app.canvas_selection_unconfirmed && !run.app.close_prompt);
                assert!(run.requests.try_recv().is_err());
            }
        }
    }
}

#[test]
fn launch_frame_and_existing_held_press_cannot_become_the_target_click() {
    let mut run = Run::new();
    let point = MmPoint::new(3., 2.);
    let pos = run.app.camera.screen(point, run.app.canvas_rect);
    // A pre-existing press belongs to the old input owner; start the command
    // before egui consumes it, as a menu/shortcut may do in the same frame.
    let raw = egui::RawInput {
        focused: true,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200., 900.),
        )),
        events: vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::ALT,
            },
        ],
        ..Default::default()
    };
    let _ = run.ctx.run(raw, |ctx| {
        run.app.arbitrate_point_input_frame(ctx);
        run.app.start_move_place();
        run.app.update(ctx, &mut run.frame);
    });
    run.pointer(point, false, true);
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Preparing
    ));
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    assert!(matches!(
        run.app
            .point_transform
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .phase,
        Phase::Following
    ));
    assert!(run.requests.try_recv().is_err());
    run.final_preview(point);
}

#[test]
fn old_release_before_new_press_and_release_uses_only_owned_event_position_and_alt() {
    let mut run = Run::new();
    run.app.grid.snap_enabled = true;
    run.app.grid.spacing_mm = 1.;
    let old = run
        .app
        .camera
        .screen(MmPoint::new(3.33333, 2.66667), run.app.canvas_rect);
    let new = run
        .app
        .camera
        .screen(MmPoint::new(6.33333, 4.66667), run.app.canvas_rect);
    let raw = egui::RawInput {
        focused: true,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200., 900.),
        )),
        events: vec![
            egui::Event::PointerMoved(old),
            egui::Event::PointerButton {
                pos: old,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        ..Default::default()
    };
    let _ = run.ctx.run(raw, |ctx| {
        run.app.arbitrate_point_input_frame(ctx);
        run.app.start_move_place();
        run.app.update(ctx, &mut run.frame);
    });
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    run.update(
        vec![
            egui::Event::PointerButton {
                pos: old,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
            egui::Event::PointerMoved(new),
            egui::Event::PointerButton {
                pos: new,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: new,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::ALT,
            },
        ],
        egui::Modifiers::NONE,
        true,
    );
    let session = run.app.point_transform.as_ref().unwrap();
    assert!(matches!(
        session.placement.as_ref().unwrap().phase,
        Phase::FinalPreview(_)
    ));
    assert_eq!(
        session.target.value.world_mm,
        run.app.camera.world(new, run.app.canvas_rect)
    );
    assert_eq!(session.target.value.source, crate::point_input::Source::Raw);
    let (id, view, _) = run.work(false);
    run.reply(id, view);
    let (id, view, _) = run.work(true);
    run.reply(id, view);
    assert_eq!(run.app.view.info.as_ref().unwrap().undo_entries, 1);
    run.no_edit_requests();
}

#[test]
fn frame_trace_observes_all_six_phases_and_real_update_without_mutation() {
    let mut run = Run::new();
    run.ready();
    let request = run
        .app
        .point_transform
        .as_ref()
        .unwrap()
        .requested
        .clone()
        .unwrap();
    let before = run.app.view.info.clone();
    let phases = [
        Phase::Preparing,
        Phase::Following,
        Phase::Frozen,
        Phase::FinalPreview(request.clone()),
        Phase::Ready(request),
        Phase::Applying,
    ];
    let expected = [
        "preparing",
        "following",
        "frozen",
        "final_preview",
        "ready",
        "applying",
    ];
    run.ctx.set_zoom_factor(1.2);
    let _ = run.ctx.run(egui::RawInput::default(), |_| {});
    for (phase, label) in phases.into_iter().zip(expected) {
        run.app
            .point_transform
            .as_mut()
            .unwrap()
            .placement
            .as_mut()
            .unwrap()
            .phase = phase;
        let snapshot = run.app.trace_snapshot(&run.ctx);
        assert_eq!(snapshot.move_phase, label);
        assert_eq!(snapshot.ui_zoom, 1.2);
    }
    assert_eq!(run.app.view.info, before);
    run.app.point_transform = None;
    assert_eq!(run.app.trace_snapshot(&run.ctx).move_phase, "inactive");
    run.app.frame_trace = Some(crate::frame_trace::Recorder::for_test());
    run.update(vec![], egui::Modifiers::NONE, true);
    let records = run.app.frame_trace.as_ref().unwrap().take_test_records();
    let start = records
        .iter()
        .find(|record| record["kind"] == "update_start")
        .unwrap();
    let end = records
        .iter()
        .find(|record| record["kind"] == "update_end")
        .unwrap();
    assert_eq!(start["binding"]["input_batch_id"], 1);
    assert_eq!(start["binding"]["update_id"], end["binding"]["update_id"]);
    assert!(end["snapshot"]["canvas_physical"].as_array().is_some());
    assert_eq!(end["snapshot"]["move_phase"], "inactive");
    assert!(records.iter().any(|record| record["stage"] == "validation"));
    assert!(
        records
            .iter()
            .any(|record| record["stage"] == "canvas_panel")
    );
    assert_eq!(run.app.view.info, before);
}
