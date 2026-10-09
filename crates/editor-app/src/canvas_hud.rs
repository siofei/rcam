//! Painter-only canvas instructions. The viewport fixes the slot; text never
//! allocates UI space or captures input.
use eframe::egui::{self, Color32, FontId, Pos2, Rect, text::LayoutJob};

const INSET: f32 = 12.;
const ROW_HEIGHT: f32 = 20.;
const MAX_ROWS: usize = 6;

pub(crate) fn slot(canvas: Rect) -> Rect {
    let left = (canvas.left() + INSET).min(canvas.right());
    let top = (canvas.top() + INSET).min(canvas.bottom());
    Rect::from_min_max(
        Pos2::new(left, top),
        Pos2::new(
            (canvas.right() - INSET).max(left),
            (canvas.bottom() - INSET)
                .min(top + ROW_HEIGHT * MAX_ROWS as f32)
                .max(top),
        ),
    )
}

/// Returns whether this instruction owns a visible slot. No new Response,
/// Area, interaction or canvas allocation is introduced.
pub(crate) fn paint(
    painter: &egui::Painter,
    canvas: Rect,
    text: &str,
    font: FontId,
    color: Color32,
) -> bool {
    let slot = slot(canvas).intersect(painter.clip_rect());
    if !slot.is_positive() {
        return false;
    }
    let mut job = LayoutJob::simple(text.into(), font, color, slot.width());
    job.sections[0].format.line_height = Some(ROW_HEIGHT);
    job.wrap.max_rows = MAX_ROWS;
    let galley = painter.layout_job(job);
    painter.with_clip_rect(slot).galley(slot.min, galley, color);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_slot_and_actual_text_stay_inside_canvas_across_content_and_dpi() {
        for size in [
            egui::vec2(0., 0.),
            egui::vec2(18., 18.),
            egui::vec2(240., 180.),
            egui::vec2(800., 500.),
        ] {
            for ppp in [1., 2., 3.] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(ppp);
                let canvas = Rect::from_min_size(egui::pos2(30., 40.), size);
                let expected = slot(canvas);
                assert!(canvas.contains_rect(expected));
                for text in [
                    "移动 · 等待",
                    "移动 · Esc / 右键取消 · Alt 暂停吸附",
                    &"超长提示 ABC123 ".repeat(200),
                ] {
                    let output = ctx.run(egui::RawInput::default(), |ctx| {
                        let painter = ctx
                            .layer_painter(egui::LayerId::background())
                            .with_clip_rect(canvas);
                        assert_eq!(
                            paint(
                                &painter,
                                canvas,
                                text,
                                FontId::proportional(14.),
                                Color32::YELLOW
                            ),
                            expected.is_positive()
                        );
                    });
                    assert_eq!(slot(canvas), expected);
                    for shape in output.shapes {
                        if let egui::Shape::Text(text) = shape.shape {
                            assert_eq!(text.pos, expected.min);
                            assert!(expected.contains_rect(shape.clip_rect));
                            assert!(text.galley.rows.len() <= MAX_ROWS);
                            assert!(text.galley.rect.height() <= ROW_HEIGHT * MAX_ROWS as f32 + 1.);
                            assert!(canvas.contains_rect(
                                text.visual_bounding_rect().intersect(shape.clip_rect)
                            ));
                        }
                    }
                }
            }
        }
    }
}
