//! Stable presentation of the existing selected-material cache. No queries.
use crate::{point_input, state::View, tools::DisplayUnit};
use editor_core::{ApertureShape, SemanticGeometry};
use editor_service::{CompositeMaterial, SelectionMaterialResult};
use eframe::egui::{self, Pos2, Rect, Vec2};

#[derive(Clone, Debug)]
pub(crate) struct Fields {
    pub selection: String,
    pub area: String,
    pub perimeter: String,
    pub tooltip: String,
    pub state: &'static str,
}
pub(crate) fn fields(view: &View, unit: DisplayUnit, resolution: f64) -> Fields {
    let selection = description(view, unit, resolution);
    let mut out = Fields {
        selection,
        area: String::new(),
        perimeter: String::new(),
        tooltip: "仅计算选中对象；同层按原 Dark/Clear 合成，去除重叠内部边；含孔边，跨层独立相加。"
            .into(),
        state: "empty",
    };
    if view.selected.ordered.is_empty() {
        return out;
    }
    match point_input::current_centers(view) {
        Ok(result) => match &result.material {
            SelectionMaterialResult::Computed {
                value:
                    CompositeMaterial::Ready {
                        area_mm2,
                        perimeter_mm,
                        area_error_mm2,
                        perimeter_error_mm,
                        ..
                    },
            } => {
                if area_mm2.is_finite() && *area_mm2 >= 0. {
                    out.area = format!("面积 {}", unit.format_area(*area_mm2, resolution));
                }
                if perimeter_mm.is_finite() && *perimeter_mm >= 0. {
                    out.perimeter =
                        format!("周长 {}", unit.format_length(*perimeter_mm, resolution));
                }
                out.tooltip.push_str(&format!("\n面积 {area_mm2:.12} mm²，误差界 ≤ {area_error_mm2:.12} mm²\n周长 {perimeter_mm:.12} mm，误差界 ≤ {perimeter_error_mm:.12} mm"));
                out.state = if out.area.is_empty() || out.perimeter.is_empty() {
                    "unavailable"
                } else {
                    "ready"
                };
            }
            SelectionMaterialResult::Computed {
                value: CompositeMaterial::ZeroArea,
            } => {
                out.area = format!("面积 {}", unit.format_area(0., resolution));
                out.perimeter = format!("周长 {}", unit.format_length(0., resolution));
                out.tooltip
                    .push_str("\n合成材料面积为零；面积中心不可用。退化中心线不作为材料边界。 ");
                out.state = "zero";
            }
            SelectionMaterialResult::Unavailable { error } => {
                out.tooltip.push_str(&format!("\n{error:?}"));
                out.state = "unavailable";
            }
        },
        Err(error) => {
            // An old result must never reappear during selection/revision changes.
            let failed = view.selection_geometry_identity
                == crate::state::selection_geometry_identity(view)
                && view.selection_geometry_error.is_some();
            out.tooltip.push_str(&format!("\n{error}"));
            out.state = if failed { "unavailable" } else { "pending" };
        }
    }
    out
}
fn description(view: &View, unit: DisplayUnit, resolution: f64) -> String {
    let n = view.selected.ordered.len();
    if n == 0 {
        return String::new();
    }
    if n > 1 {
        return format!("选中 {n} 个图形");
    }
    let length = |v| unit.format_length(v, resolution);
    match &view.selected.ordered[0].object.geometry {
        SemanticGeometry::Flash { aperture_id, .. } => view
            .apertures
            .iter()
            .find(|a| &a.id == aperture_id)
            .map(|a| match &a.shape {
                ApertureShape::Circle { diameter_mm, .. } => {
                    format!("光圈 C Ø{}", length(*diameter_mm))
                }
                ApertureShape::Rectangle {
                    width_mm,
                    height_mm,
                    ..
                } => format!("光圈 R {} × {}", length(*width_mm), length(*height_mm)),
                ApertureShape::Obround {
                    width_mm,
                    height_mm,
                    ..
                } => format!("光圈 O {} × {}", length(*width_mm), length(*height_mm)),
                ApertureShape::Polygon {
                    diameter_mm,
                    vertices,
                    ..
                } => format!("光圈 P {vertices}边 Ø{}", length(*diameter_mm)),
                ApertureShape::Macro { .. } => "光圈 Macro".into(),
            })
            .unwrap_or_else(|| "光圈 不可用".into()),
        SemanticGeometry::BlockInstance { definition_id, .. } => format!(
            "Block {}",
            view.block_definitions
                .iter()
                .find(|b| &b.id == definition_id)
                .map_or("不可用", |b| b.name.as_str())
        ),
        SemanticGeometry::Line { width_mm, .. } => format!("线段 Ø{}", length(*width_mm)),
        SemanticGeometry::RectangularSweep {
            width_mm,
            height_mm,
            ..
        } => format!("矩形线 {} × {}", length(*width_mm), length(*height_mm)),
        SemanticGeometry::Arc { width_mm, .. } => format!("圆弧 Ø{}", length(*width_mm)),
        SemanticGeometry::Region { .. } => "Region".into(),
    }
}
const ROW_HEIGHT: f32 = 20.;
const GAP: f32 = 4.;
const FIELD_WIDTHS: [f32; 3] = [200., 155., 155.];

impl Fields {
    pub fn notice(&self) -> Option<&'static str> {
        match self.state {
            "pending" => Some("选中统计计算中…"),
            "unavailable" => Some("选中统计不可用"),
            _ => None,
        }
    }
    fn visible(&self) -> [bool; 3] {
        [
            !self.selection.is_empty(),
            !self.area.is_empty(),
            !self.perimeter.is_empty(),
        ]
    }
}

/// Keep the original status/warning as well as the statistics availability notice.
pub(crate) fn message(base: String, fields: &Fields) -> String {
    match fields.notice() {
        Some(notice) if base.is_empty() => notice.into(),
        Some(notice) => format!("{base} · {notice}"),
        None => base,
    }
}

pub(crate) struct Layout {
    pub fields: [Option<Rect>; 4],
    pub message: Option<Rect>,
    pub controls: Option<Rect>,
    pub scale: Rect,
    pub height: f32,
}

/// One stable row of chrome. Only present data that cannot fit adds a second row.
/// Slot widths depend on presence and viewport width, never on formatted text length.
pub(crate) fn layout(rect: Rect, fields: &Fields, controls_width: f32) -> Layout {
    let width = rect.width().max(0.);
    let gap = GAP.min(width / 3.);
    let coordinates_width = if width >= 494. {
        260.
    } else {
        (width - gap) / 2.
    };
    let scale_width = 230f32.min((width - coordinates_width - gap).max(0.));
    let coordinates = Rect::from_min_size(
        Pos2::new(rect.right() - coordinates_width, rect.top()),
        Vec2::new(coordinates_width, ROW_HEIGHT),
    );
    let scale_right = (coordinates.left() - gap).max(rect.left());
    let scale = Rect::from_min_max(
        Pos2::new((scale_right - scale_width).max(rect.left()), rect.top()),
        Pos2::new(scale_right, rect.top() + ROW_HEIGHT),
    );
    let mut left_width = (scale.left() - gap - rect.left()).max(0.);
    let control_width = controls_width.max(0.).min(left_width);
    let controls = (control_width > 0.).then(|| {
        Rect::from_min_size(
            Pos2::new(rect.left() + left_width - control_width, rect.top()),
            Vec2::new(control_width, ROW_HEIGHT),
        )
    });
    if controls.is_some() {
        left_width = (left_width - control_width - gap).max(0.);
    }
    let visible = fields.visible();
    let count = visible.iter().filter(|present| **present).count();
    let desired = visible
        .iter()
        .zip(FIELD_WIDTHS)
        .filter(|(present, _)| **present)
        .map(|(_, width)| width)
        .sum::<f32>();
    let inline = count == 0 || left_width >= desired + count as f32 * GAP + 60.;
    let height = if inline {
        ROW_HEIGHT
    } else {
        2. * ROW_HEIGHT + GAP
    };
    let mut out = [None, None, None, Some(coordinates)];
    let data_width = if inline { left_width } else { width };
    let data_gap = GAP.min(data_width / (count as f32 + 1.));
    let factor = if desired > 0. {
        ((data_width - count.saturating_sub(1) as f32 * data_gap).max(0.) / desired).min(1.)
    } else {
        1.
    };
    let mut x = rect.left();
    for (index, present) in visible.into_iter().enumerate() {
        if present && data_width > 0. {
            let width = (FIELD_WIDTHS[index] * factor).min((rect.right() - x).max(0.));
            out[index] = Some(Rect::from_min_size(
                Pos2::new(x, rect.top() + if inline { 0. } else { ROW_HEIGHT + GAP }),
                Vec2::new(width, ROW_HEIGHT),
            ));
            x += width + data_gap;
        }
    }
    let message_start = if inline && count > 0 { x } else { rect.left() };
    let message_width = (rect.left() + left_width - message_start).max(0.);
    Layout {
        fields: out,
        message: (message_width > 0.).then(|| {
            Rect::from_min_size(
                Pos2::new(message_start, rect.top()),
                Vec2::new(message_width, ROW_HEIGHT),
            )
        }),
        controls,
        scale,
        height,
    }
}

pub(crate) fn controls_width(undo: bool, close: bool, details: bool) -> f32 {
    let (width, count) = [(undo, 50.), (close, 70.), (details, 40.)]
        .into_iter()
        .filter(|(visible, _)| *visible)
        .fold((0., 0usize), |(sum, count), (_, width)| {
            (sum + width, count + 1)
        });
    width + count.saturating_sub(1) as f32 * GAP
}

pub(crate) fn height(width: f32, fields: &Fields, controls_width: f32) -> f32 {
    layout(
        Rect::from_min_size(Pos2::ZERO, Vec2::new(width, ROW_HEIGHT)),
        fields,
        controls_width,
    )
    .height
}
pub(crate) fn display_line(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}
pub(crate) fn scale_text(scale: f64) -> String {
    if scale.is_finite() && scale > 0. {
        if (0.0001..100_000.).contains(&scale) {
            format!("缩放 {scale:.4} 逻辑点/mm")
        } else {
            format!("缩放 {scale:.4e} 逻辑点/mm")
        }
    } else {
        "缩放 — 逻辑点/mm".into()
    }
}
pub(crate) fn paint_scale(ui: &mut egui::Ui, scale: f64, ppp: f32) -> Rect {
    let text = scale_text(scale);
    ui.add_sized([ui.available_width().min(230.), 20.], egui::Label::new(text).truncate().halign(egui::Align::Max))
        .on_hover_text(format!(
            "画布比例：{scale:.12} 逻辑屏幕点/mm。基准 1 表示 1 mm 占 1 个逻辑点。\n每逻辑点 {ppp} 个物理像素；屏幕尺寸/DPI 不决定实物比例，不表示实际尺寸倍率。"
    )).rect
}

pub(crate) fn paint_controls<R>(
    ui: &mut egui::Ui,
    slot: Rect,
    paint: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let mut controls = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(slot)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    controls.set_clip_rect(ui.clip_rect().intersect(slot));
    controls.spacing_mut().item_spacing.x = GAP;
    paint(&mut controls)
}
pub(crate) fn paint(
    ui: &mut egui::Ui,
    fields: &Fields,
    coordinates: &str,
    controls_width: f32,
) -> Layout {
    let height = height(ui.available_width(), fields, controls_width);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let layout = layout(rect, fields, controls_width);
    for (index, text) in [
        &fields.selection,
        &fields.area,
        &fields.perimeter,
        coordinates,
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(slot) = layout.fields[index] {
            ui.put(
                slot,
                egui::Label::new(egui::RichText::new(display_line(text)).color(
                    if fields.state == "unavailable" && index == 0 {
                        crate::ui::tokens::warning_text(ui.visuals())
                    } else {
                        ui.visuals().text_color()
                    },
                ))
                .truncate()
                .halign(if index == 3 {
                    egui::Align::Max
                } else {
                    egui::Align::Min
                }),
            )
            .on_hover_text(format!(
                "{}\n{}\n{}\n{}",
                fields.selection, fields.area, fields.perimeter, fields.tooltip
            ));
        }
    }
    layout
}

#[cfg(test)]
mod scale_tests {
    use super::*;
    #[test]
    fn scale_is_logical_points_and_fixed_width_across_numbers_and_dpi() {
        assert_eq!(scale_text(11.8313), "缩放 11.8313 逻辑点/mm");
        assert!(scale_text(1e-30).contains("e-30"));
        assert!(scale_text(f64::NAN).contains('—'));
        let ctx = egui::Context::default();
        let mut rects = Vec::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for (scale, ppp) in [(1., 1.), (11.8313, 2.), (1e-30, 3.), (1e12, 1.)] {
                    rects.push(paint_scale(ui, scale, ppp));
                }
            });
        });
        assert!(rects.iter().all(|r| r.width() == 230. && r.height() == 20.));
        assert!(rects.iter().all(|r| r.left() == rects[0].left()));
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;
    fn fields(visible: [bool; 3], state: &'static str) -> Fields {
        Fields {
            selection: if visible[0] {
                "选中 5 个图形".into()
            } else {
                String::new()
            },
            area: if visible[1] {
                "面积 0 mm²".into()
            } else {
                String::new()
            },
            perimeter: if visible[2] {
                "周长 0 mm".into()
            } else {
                String::new()
            },
            tooltip: "statistics details".into(),
            state,
        }
    }
    #[test]
    fn only_visible_data_can_add_a_second_row() {
        for width in [0., 1., 80., 280., 600., 980., 1280.] {
            for state in ["empty", "pending", "unavailable"] {
                assert_eq!(height(width, &fields([false; 3], state), 168.), 20.);
            }
        }
        let selected = fields([true, false, false], "pending");
        assert_eq!(height(980., &selected, 0.), 20.);
        let ready = fields([true; 3], "ready");
        assert_eq!(height(1280., &ready, 168.), 20.);
        assert_eq!(height(980., &ready, 0.), 44.);
        assert_eq!(height(980., &fields([true; 3], "zero"), 0.), 44.);
    }
    #[test]
    fn statistics_notices_preserve_existing_warning_and_status_text() {
        for state in ["pending", "unavailable"] {
            let fields = fields([true, false, false], state);
            let text = message("⚠ 保留兼容导入告警".into(), &fields);
            assert!(text.starts_with("⚠ 保留兼容导入告警"));
            assert!(text.contains(fields.notice().unwrap()));
            assert_eq!(message(String::new(), &fields), fields.notice().unwrap());
        }
        assert_eq!(
            message("原状态".into(), &fields([true; 3], "ready")),
            "原状态"
        );
    }
    #[test]
    fn all_presence_combinations_stay_inside_the_bar_without_overlap() {
        for width in [0., 1., 8., 40., 80., 280., 420., 600., 980., 1280.] {
            for bits in 0..8 {
                let fields = fields([bits & 1 != 0, bits & 2 != 0, bits & 4 != 0], "ready");
                for controls in [0., 40., 124., 168.] {
                    let layout = layout(
                        Rect::from_min_size(Pos2::new(5., 9.), Vec2::new(width, 20.)),
                        &fields,
                        controls,
                    );
                    let bounds =
                        Rect::from_min_size(Pos2::new(5., 9.), Vec2::new(width, layout.height));
                    assert_eq!(layout.fields[3].unwrap().right(), bounds.right());
                    for index in 0..3 {
                        assert_eq!(
                            layout.fields[index].is_some(),
                            fields.visible()[index] && width > 0.
                        );
                    }
                    let slots: Vec<_> = layout
                        .fields
                        .into_iter()
                        .flatten()
                        .chain(layout.message)
                        .chain(layout.controls)
                        .chain([layout.scale])
                        .filter(|rect| rect.width() > 0.)
                        .collect();
                    for (index, slot) in slots.iter().enumerate() {
                        assert!(
                            bounds.contains_rect(*slot),
                            "width={width}, bits={bits}, {slot:?} vs {bounds:?}"
                        );
                        for other in &slots[index + 1..] {
                            assert!(!slot.intersects(*other), "{slot:?} vs {other:?}");
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn actual_panel_shrinks_back_to_one_row_for_both_themes() {
        for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
            for width in [280., 980., 1280.] {
                let ctx = egui::Context::default();
                ctx.set_visuals(visuals.clone());
                let mut panel_heights = Vec::new();
                for present in [false, true, false, true, false] {
                    let fields = fields([present; 3], if present { "zero" } else { "empty" });
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 200.),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            let frame = egui::Frame::side_top_panel(&ctx.style());
                            let margins = frame.total_margin().sum();
                            let controls_width = controls_width(true, true, true);
                            let expected =
                                height((width - margins.x).max(0.), &fields, controls_width)
                                    + margins.y;
                            let response = egui::TopBottomPanel::bottom("compact-test")
                                .frame(frame)
                                .exact_height(expected)
                                .show(ctx, |ui| {
                                    let layout =
                                        paint(ui, &fields, &"X 9 Y 0".repeat(100), controls_width);
                                    if let Some(slot) = layout.message {
                                        let response = ui.put(
                                            slot,
                                            egui::Label::new(display_line(
                                                &"警告\n详细提示".repeat(100),
                                            ))
                                            .truncate(),
                                        );
                                        assert!(slot.contains_rect(response.rect));
                                    }
                                    if let Some(slot) = layout.controls {
                                        paint_controls(ui, slot, |controls| {
                                            let buttons = [
                                                controls.button("撤销"),
                                                controls.small_button("关闭提示"),
                                                controls.small_button("详情"),
                                            ];
                                            if width >= 980. {
                                                for response in buttons {
                                                    assert!(slot.contains_rect(response.rect));
                                                }
                                            }
                                            assert!(slot.contains_rect(controls.clip_rect()));
                                        });
                                    }
                                    let mut scale = ui.new_child(
                                        egui::UiBuilder::new().max_rect(layout.scale).layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                        ),
                                    );
                                    scale.set_clip_rect(ui.clip_rect().intersect(layout.scale));
                                    let scale_rect = paint_scale(&mut scale, 11.8313, 2.);
                                    assert!(layout.scale.contains_rect(scale_rect));
                                    assert!(ui.min_rect().width() <= width - margins.x);
                                });
                            assert_eq!(response.response.rect.height(), expected);
                            panel_heights.push(expected);
                        },
                    );
                }
                assert_eq!(panel_heights[0], 24.);
                assert_eq!(panel_heights[0], panel_heights[2]);
                assert_eq!(panel_heights[0], panel_heights[4]);
                assert_eq!(panel_heights[1], panel_heights[3]);
            }
        }
    }
}
