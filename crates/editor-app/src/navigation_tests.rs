use super::canvas_pan_delta;
use eframe::egui::{self, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

fn frame(ctx: &Context, events: Vec<Event>, pan: &mut Vec2) {
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (response, _) = ui.allocate_painter(ui.available_size(), egui::Sense::drag());
                *pan += canvas_pan_delta(&response);
            });
        },
    );
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}

#[test]
fn navigation_drag_keeps_release_frame_motion_without_double_counting() {
    for split in [false, true] {
        for delta in [Vec2::new(100.0, 50.0), Vec2::new(-100.0, -50.0)] {
            let ctx = Context::default();
            let mut pan = Vec2::new(7.0, 9.0);
            let baseline = pan;
            let start = Pos2::new(300.0, 300.0);
            let middle = start + delta / 2.0;
            let end = start + delta;
            frame(&ctx, vec![], &mut pan);
            frame(&ctx, vec![Event::PointerMoved(start)], &mut pan);
            frame(&ctx, vec![button(start, true)], &mut pan);
            if split {
                frame(&ctx, vec![Event::PointerMoved(middle)], &mut pan);
            }
            frame(
                &ctx,
                vec![Event::PointerMoved(end), button(end, false)],
                &mut pan,
            );
            println!(
                "split={split} delta={delta:?} expected={:?} actual={pan:?}",
                baseline + delta
            );
            assert_eq!(pan, baseline + delta);
            frame(&ctx, vec![], &mut pan);
            frame(&ctx, vec![Event::PointerMoved(start)], &mut pan);
            assert_eq!(
                pan,
                baseline + delta,
                "idle/hover must not add the released delta again"
            );
        }
    }
}

#[test]
fn navigation_hover_and_unowned_release_do_not_pan() {
    let ctx = Context::default();
    let mut pan = Vec2::ZERO;
    let outside = Pos2::new(-10.0, -10.0);
    let inside = Pos2::new(300.0, 300.0);
    frame(&ctx, vec![], &mut pan);
    frame(
        &ctx,
        vec![Event::PointerMoved(outside), button(outside, true)],
        &mut pan,
    );
    frame(
        &ctx,
        vec![Event::PointerMoved(inside), button(inside, false)],
        &mut pan,
    );
    assert_eq!(pan, Vec2::ZERO);
    frame(&ctx, vec![button(inside, true)], &mut pan);
    frame(&ctx, vec![button(inside, false)], &mut pan);
    assert_eq!(pan, Vec2::ZERO, "stationary press/release must not pan");
}
