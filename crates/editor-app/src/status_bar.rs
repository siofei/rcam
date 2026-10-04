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
        area: "面积 —".into(),
        perimeter: "周长 —".into(),
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
                out.area = format!("面积 {}", unit.format_area(*area_mm2, resolution));
                out.perimeter = format!("周长 {}", unit.format_length(*perimeter_mm, resolution));
                out.tooltip.push_str(&format!("\n面积 {area_mm2:.12} mm²，误差界 ≤ {area_error_mm2:.12} mm²\n周长 {perimeter_mm:.12} mm，误差界 ≤ {perimeter_error_mm:.12} mm"));
                out.state = "ready";
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
                out.area = "面积 不可用".into();
                out.perimeter = "周长 不可用".into();
                out.tooltip.push_str(&format!("\n{error:?}"));
                out.state = "unavailable";
            }
        },
        Err(error) => {
            // An old result must never reappear during selection/revision changes.
            let failed = view.selection_geometry_identity
                == crate::state::selection_geometry_identity(view)
                && view.selection_geometry_error.is_some();
            out.area = if failed {
                "面积 不可用"
            } else {
                "面积 计算中…"
            }
            .into();
            out.perimeter = if failed {
                "周长 不可用"
            } else {
                "周长 计算中…"
            }
            .into();
            out.tooltip.push_str(&format!("\n{error}"));
            out.state = if failed { "unavailable" } else { "pending" };
        }
    }
    out
}
fn description(view: &View, unit: DisplayUnit, resolution: f64) -> String {
    let n = view.selected.ordered.len();
    if n == 0 {
        return "未选择图形".into();
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
/// Coordinate priority, then description, area, perimeter. These rectangles
/// depend only on available width, never on text or pending/ready state.
pub(crate) fn layout(rect: Rect) -> [Option<Rect>; 4] {
    let width = rect.width().max(0.);
    let coords = Rect::from_min_max(
        Pos2::new(rect.right() - (width.min(260.)), rect.top()),
        rect.max,
    );
    let mut remaining = (coords.left() - rect.left() - 4.).max(0.);
    let mut x = rect.left();
    let mut out = [None, None, None, Some(coords)];
    for (index, w) in [(0, 200.), (1, 155.), (2, 155.)] {
        let use_width = if index == 0 {
            remaining.min(w)
        } else if remaining >= w {
            w
        } else {
            0.
        };
        if use_width >= 60. {
            out[index] = Some(Rect::from_min_size(
                Pos2::new(x, rect.top()),
                Vec2::new(use_width, rect.height()),
            ));
            x += use_width + 4.;
            remaining = (remaining - use_width - 4.).max(0.);
        }
    }
    out
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
pub(crate) fn paint(ui: &mut egui::Ui, fields: &Fields, coordinates: &str) -> [Option<Rect>; 4] {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.), egui::Sense::hover());
    let slots = layout(rect);
    for (index, text) in [
        &fields.selection,
        &fields.area,
        &fields.perimeter,
        &coordinates.to_string(),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(slot) = slots[index] {
            ui.put(
                slot,
                egui::Label::new(egui::RichText::new(display_line(text)).color(
                    if fields.state == "unavailable" && (index == 1 || index == 2) {
                        egui::Color32::YELLOW
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
    slots
}
