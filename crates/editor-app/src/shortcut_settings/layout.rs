//! Fixed shortcut settings geometry; overflow is readable without growing rows.
use eframe::egui;

pub(super) const WINDOW: egui::Vec2 = egui::vec2(720., 540.);
pub(super) const LEFT: f32 = 384.;
pub(super) const RIGHT: f32 = 308.;
pub(super) const GAP: f32 = 12.;
pub(super) const PANE_HEIGHT: f32 = 348.;
pub(super) const ROW_HEIGHT: f32 = 28.;

pub(super) fn record(_ui: &egui::Ui, _name: impl std::hash::Hash, _rect: egui::Rect) {
    #[cfg(test)]
    _ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new(("shortcut-layout", _name)), _rect);
    });
}

pub(super) fn region<R>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    size: egui::Vec2,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let id = egui::Id::new(id);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    record(ui, id, rect);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let output = egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .max_width(size.x)
        .max_height(size.y)
        .min_scrolled_width(0.)
        .min_scrolled_height(0.)
        .show(&mut child, content);
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new(("shortcut-scroll", id)),
            (output.id, output.inner_rect, output.content_size),
        )
    });
    output.inner
}

pub(super) fn button(ui: &mut egui::Ui, id: &str, text: &str, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(text).min_size(egui::vec2(284., ROW_HEIGHT)),
    );
    record(ui, id, response.rect);
    response
}

pub(super) fn row(
    ui: &mut egui::Ui,
    text: (&str, &str, &str, &str),
    selected: bool,
    enabled: bool,
) -> bool {
    let (id, name, keys, details) = text;
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.;
            let name = ui
                .add_enabled_ui(enabled, |ui| {
                    ui.add_sized(
                        egui::vec2(164., ROW_HEIGHT),
                        egui::Button::new(name).truncate().selected(selected),
                    )
                })
                .inner
                .on_hover_text(details);
            record(ui, (id, "name"), name.rect);
            let keys = ui
                .add_enabled_ui(enabled, |ui| {
                    ui.add_sized(
                        egui::vec2(184., ROW_HEIGHT),
                        egui::Button::new(keys).truncate().selected(selected),
                    )
                })
                .inner
                .on_hover_text(details);
            record(ui, (id, "keys"), keys.rect);
            name.clicked() || keys.clicked()
        })
        .inner
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::super::Settings;
    use super::*;
    use editor_core::command::{Key, Modifiers, Platform, Shortcut, ids};

    fn rect(ctx: &egui::Context, name: impl std::hash::Hash) -> egui::Rect {
        ctx.data(|d| d.get_temp(egui::Id::new(("shortcut-layout", name))))
            .unwrap()
    }
    fn region_rect(ctx: &egui::Context, name: &str) -> egui::Rect {
        rect(ctx, egui::Id::new(name))
    }
    fn frame(
        ctx: &egui::Context,
        s: &mut Settings,
        viewport: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                events,
                ..Default::default()
            },
            |ctx| s.window(ctx, false, false),
        )
    }
    fn settle(ctx: &egui::Context, s: &mut Settings, viewport: egui::Vec2) {
        for _ in 0..3 {
            let _ = frame(ctx, s, viewport, vec![]);
        }
    }
    fn settings() -> Settings {
        let mut s = Settings::load(None, Platform::current());
        s.open = true;
        // Synthetic usable configuration for UI-only tests; no disk writes.
        s.protected = false;
        s.warning = None;
        s.editing = Some(ids::EDIT_SELECT_ALL);
        s.keys = s
            .current
            .config
            .entry(ids::EDIT_SELECT_ALL)
            .shortcuts
            .clone();
        s
    }
    fn click(ctx: &egui::Context, s: &mut Settings, viewport: egui::Vec2, point: egui::Pos2) {
        for pressed in [true, false] {
            let _ = frame(
                ctx,
                s,
                viewport,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        settle(ctx, s, viewport);
    }

    #[test]
    fn modal_columns_controls_and_rows_are_stable_across_data_dpi_and_viewports() {
        for viewport in [egui::vec2(980., 760.), egui::vec2(320., 420.)] {
            for ppp in [1., 1.25, 1.5, 2., 3., 4.] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(ppp);
                let mut s = settings();
                let original = s.current.config.bytes().unwrap();
                settle(&ctx, &mut s, viewport);
                let snapshot = |ctx: &egui::Context| {
                    vec![
                        rect(ctx, "modal"),
                        region_rect(ctx, "status"),
                        region_rect(ctx, "command-pane"),
                        region_rect(ctx, "editor-pane"),
                        rect(ctx, "search"),
                        rect(ctx, "record"),
                        region_rect(ctx, "candidate"),
                        rect(ctx, "save"),
                        rect(ctx, "clear"),
                        rect(ctx, "reset"),
                        rect(ctx, "close"),
                    ]
                };
                let baseline = snapshot(&ctx);
                assert!(baseline[0].width() <= viewport.x && baseline[0].height() <= viewport.y);
                assert_eq!(baseline[3].left() - baseline[2].right(), GAP);
                assert_eq!(baseline[2].size(), egui::vec2(LEFT, PANE_HEIGHT));
                assert_eq!(baseline[3].size(), egui::vec2(RIGHT, PANE_HEIGHT));
                for command in crate::shortcut_config::commands() {
                    let name = rect(&ctx, (command.id.0, "name"));
                    let keys = rect(&ctx, (command.id.0, "keys"));
                    assert_eq!(name.size(), egui::vec2(164., ROW_HEIGHT));
                    assert_eq!(keys.size(), egui::vec2(184., ROW_HEIGHT));
                    assert!(name.right() <= keys.left());
                }
                if viewport.x > WINDOW.x {
                    assert!(
                        baseline[3].contains_rect(rect(&ctx, "reset")),
                        "reset={:?}, pane={:?}",
                        rect(&ctx, "reset"),
                        baseline[3]
                    );
                    assert!(baseline[0].contains_rect(rect(&ctx, "close")));
                }
                let first = rect(&ctx, (crate::shortcut_config::commands()[0].id.0, "name"));
                let last = rect(
                    &ctx,
                    (
                        crate::shortcut_config::commands().last().unwrap().id.0,
                        "name",
                    ),
                );
                assert_eq!(first.left(), last.left());
                assert!(last.top() > first.bottom());
                for phase in 0..6 {
                    s.warning = Some("保护原文件；补缺与冲突详情\n".repeat(100));
                    s.message = Some(if phase == 0 {
                        "已自动保存".into()
                    } else {
                        "Conflict: 长命令与四个绑定的完整错误\n".repeat(100)
                    });
                    s.search = if phase % 2 == 0 {
                        "not-a-command".into()
                    } else {
                        "".into()
                    };
                    s.keys = (0..phase.min(4))
                        .map(|i| Shortcut::new(Modifiers::PRIMARY_SHIFT, Key::F(5 + i as u8)))
                        .collect();
                    s.recording = phase == 2;
                    s.candidate = Some(Shortcut::new(Modifiers::NONE, Key::Escape));
                    s.protected = phase == 3;
                    if phase == 4 {
                        s.pending = Some(std::sync::mpsc::channel().1);
                    } else {
                        s.pending = None;
                    }
                    settle(&ctx, &mut s, viewport);
                    assert_eq!(
                        snapshot(&ctx),
                        baseline,
                        "viewport={viewport:?}, ppp={ppp}, phase={phase}"
                    );
                    assert_eq!(s.current.config.bytes().unwrap(), original);
                }
            }
        }
    }

    #[test]
    fn long_row_name_and_four_aliases_do_not_grow_cells_or_next_row() {
        for ppp in [1., 1.5, 2., 3.] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(ppp);
            let mut baseline = None;
            for text in ["Move".to_owned(), "Long command name 名称".repeat(200)] {
                for _ in 0..3 {
                    let _ = ctx.run(egui::RawInput::default(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.set_width(356.);
                            row(ui, ("long", &text, &text, &text), true, true);
                            row(ui, ("next", "Next", "Ctrl+A", "Full details"), false, true);
                        });
                    });
                }
                let measured = [
                    rect(&ctx, ("long", "name")),
                    rect(&ctx, ("long", "keys")),
                    rect(&ctx, ("next", "name")),
                    rect(&ctx, ("next", "keys")),
                ];
                if let Some(baseline) = baseline {
                    assert_eq!(measured, baseline);
                } else {
                    baseline = Some(measured);
                }
            }
        }
    }

    #[test]
    fn actual_select_record_cancel_and_alias_remove_only_edit_the_candidate() {
        let ctx = egui::Context::default();
        let viewport = egui::vec2(980., 760.);
        let mut s = settings();
        let original = s.current.config.bytes().unwrap();
        settle(&ctx, &mut s, viewport);
        let point = rect(&ctx, (ids::OBJECT_MOVE_PLACE.0, "name")).center();
        // The complete dynamic list contains new commands even below its scroll viewport.
        assert!(
            point.y > region_rect(&ctx, "command-pane").bottom(),
            "move={point:?}, pane={:?}",
            region_rect(&ctx, "command-pane")
        );
        let search = rect(&ctx, "search").center();
        click(&ctx, &mut s, viewport, search);
        assert!(ctx.memory(|m| m.focused().is_some()));
        let record = rect(&ctx, "record").center();
        click(&ctx, &mut s, viewport, record);
        assert!(s.recording);
        assert!(ctx.memory(|m| m.focused().is_none()));
        click(&ctx, &mut s, viewport, rect(&ctx, "record").center());
        assert!(!s.recording);
        click(
            &ctx,
            &mut s,
            viewport,
            rect(&ctx, ("remove", 0usize)).center(),
        );
        assert!(s.keys.is_empty());
        let pointer = region_rect(&ctx, "command-pane").center();
        let _ = frame(
            &ctx,
            &mut s,
            viewport,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -480.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..48 {
            let _ = frame(&ctx, &mut s, viewport, vec![]);
        }
        let point = rect(&ctx, (ids::OBJECT_MOVE_PLACE.0, "name")).center();
        assert!(
            region_rect(&ctx, "command-pane").contains(point),
            "move={point:?}"
        );
        click(&ctx, &mut s, viewport, point);
        assert_eq!(s.editing, Some(ids::OBJECT_MOVE_PLACE));
        assert_eq!(s.current.config.bytes().unwrap(), original);
    }

    #[test]
    fn long_status_has_scrollable_full_text_and_small_viewport_actions_are_reachable() {
        let ctx = egui::Context::default();
        let viewport = egui::vec2(320., 420.);
        let mut s = settings();
        s.warning = Some("Warning first\n".repeat(120));
        s.message = Some("Error detail\n".repeat(120) + "END OF FULL ERROR");
        s.pending = Some(std::sync::mpsc::channel().1);
        settle(&ctx, &mut s, viewport);
        let scroll: (egui::Id, egui::Rect, egui::Vec2) = ctx
            .data(|d| d.get_temp(egui::Id::new(("shortcut-scroll", egui::Id::new("status")))))
            .unwrap();
        assert!(scroll.2.y > scroll.1.height());
        let mut state = egui::scroll_area::State::load(&ctx, scroll.0).unwrap();
        state.offset.y = 100000.;
        state.store(&ctx, scroll.0);
        settle(&ctx, &mut s, viewport);
        let output = frame(&ctx, &mut s, viewport, vec![]);
        let text = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text().contains("END OF FULL ERROR")
                {
                    Some((text, shape.clip_rect))
                } else {
                    None
                }
            })
            .expect("full error was painted");
        let last_line = text
            .0
            .galley
            .rows
            .last()
            .unwrap()
            .rect()
            .translate(text.0.pos.to_vec2());
        let actual_scroll: (egui::Id, egui::Rect, egui::Vec2) = ctx
            .data(|d| d.get_temp(egui::Id::new(("shortcut-scroll", egui::Id::new("status")))))
            .unwrap();
        assert!(
            text.1.contains_rect(last_line),
            "last line={last_line:?}, clip={:?}, scroll={actual_scroll:?}, state={:?}",
            text.1,
            egui::scroll_area::State::load(&ctx, scroll.0)
                .unwrap()
                .offset
        );
        s.pending = None;
        settle(&ctx, &mut s, viewport);
        // Wheel input over chrome reaches the outer horizontal/vertical scroll container.
        let pointer = egui::pos2(viewport.x / 2., rect(&ctx, "modal").top() + 16.);
        for _ in 0..8 {
            let _ = frame(
                &ctx,
                &mut s,
                viewport,
                vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(-2000., -2000.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let record = rect(&ctx, "record");
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, viewport).contains(record.center()),
            "record={record:?}"
        );
        click(&ctx, &mut s, viewport, record.center());
        assert!(s.recording);
        click(&ctx, &mut s, viewport, rect(&ctx, "record").center());
        assert!(!s.recording);
        for _ in 0..8 {
            let _ = frame(
                &ctx,
                &mut s,
                viewport,
                vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(2000., -2000.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let close = rect(&ctx, "close");
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, viewport).contains(close.center()),
            "close={close:?}"
        );
        click(&ctx, &mut s, viewport, close.center());
        assert!(!s.open);
    }
}
