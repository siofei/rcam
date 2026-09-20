//! View-only grid and two-point ruler. Never owns manufacturing state.
use crate::camera::Camera;
use editor_core::{MmPoint, grid::snap_scalar};
use eframe::egui::{self, Color32, Rect, Stroke};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayUnit {
    #[default]
    Mm,
    Inch,
}
impl DisplayUnit {
    fn value(self, millimetres: f64) -> f64 {
        match self {
            Self::Mm => millimetres,
            Self::Inch => millimetres / 25.4,
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Inch => "in",
        }
    }
    pub fn point_label(self, point: MmPoint) -> String {
        format!(
            "X {:.6}  Y {:.6} {}",
            self.value(point.x_mm),
            self.value(point.y_mm),
            self.suffix()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    Endpoint,
    Center,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapPoint {
    pub point: MmPoint,
    pub object_id: String,
    pub kind: SnapKind,
}

pub fn snap_point(
    raw: MmPoint,
    grid: GridSettings,
    candidates: &[SnapPoint],
    camera: Camera,
    excluded_object_ids: Option<&HashSet<String>>,
    temporarily_disabled: bool,
) -> Result<MmPoint, String> {
    if !raw.is_finite() {
        return Err("坐标不是有限值".into());
    }
    if !grid.snap_enabled || temporarily_disabled {
        return Ok(raw);
    }
    let radius_mm = 8. / camera.scale;
    if !radius_mm.is_finite() || radius_mm <= 0. {
        return Err("当前缩放无法计算吸附半径".into());
    }
    let best = candidates
        .iter()
        .filter(|candidate| {
            excluded_object_ids.is_none_or(|excluded| !excluded.contains(&candidate.object_id))
        })
        .filter_map(|candidate| {
            let distance = raw.distance_mm(candidate.point);
            (distance <= radius_mm).then_some((candidate, distance))
        })
        .min_by(|(left, left_distance), (right, right_distance)| {
            left_distance
                .total_cmp(right_distance)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.object_id.cmp(&right.object_id))
        });
    if let Some((candidate, _)) = best {
        Ok(candidate.point)
    } else {
        grid.point(raw)
    }
}

#[derive(Clone, Copy)]
pub struct GridSettings {
    pub visible: bool,
    pub spacing_mm: f64,
    pub snap_enabled: bool,
}
impl Default for GridSettings {
    fn default() -> Self {
        Self {
            visible: false,
            spacing_mm: 0.1,
            snap_enabled: false,
        }
    }
}
impl GridSettings {
    pub fn point(self, p: MmPoint) -> Result<MmPoint, String> {
        if !p.is_finite() {
            return Err("坐标不是有限值".into());
        }
        if !self.snap_enabled {
            return Ok(p);
        }
        let scalar = |v| {
            snap_scalar(v, self.spacing_mm, 0.)
                .map_err(|_| "网格步长无效或当前坐标精度不足".to_string())
        };
        Ok(MmPoint::new(scalar(p.x_mm)?, scalar(p.y_mm)?))
    }
    pub fn visual_step(self, camera: Camera, rect: Rect) -> Option<f64> {
        if !self.visible
            || !self.spacing_mm.is_finite()
            || self.spacing_mm <= 0.
            || !camera.scale.is_finite()
            || camera.scale <= 0.
        {
            return None;
        }
        let min_points = 12f64.max(f64::from(rect.width().max(rect.height())) / 500.);
        let factor = (min_points / camera.scale / self.spacing_mm)
            .log10()
            .ceil()
            .max(0.);
        let step = self.spacing_mm * 10f64.powf(factor);
        (step.is_finite() && step > 0.).then_some(step)
    }
    pub fn paint(self, painter: &egui::Painter, camera: Camera, rect: Rect, ppp: f32) {
        let Some(step) = self.visual_step(camera, rect) else {
            return;
        };
        let a = camera.world(rect.left_bottom(), rect);
        let b = camera.world(rect.right_top(), rect);
        let stroke = Stroke::new(1. / ppp, Color32::from_rgba_unmultiplied(130, 160, 180, 65));
        for axis in 0..2 {
            let (lo, hi) = if axis == 0 {
                (a.x_mm, b.x_mm)
            } else {
                (a.y_mm, b.y_mm)
            };
            let first = (lo / step).ceil();
            if !first.is_finite() || first.abs() >= 2f64.powi(52) {
                continue;
            }
            for n in 0..512 {
                let v = (first + f64::from(n)) * step;
                if !v.is_finite() || v > hi {
                    break;
                }
                let (p, q) = if axis == 0 {
                    (MmPoint::new(v, a.y_mm), MmPoint::new(v, b.y_mm))
                } else {
                    (MmPoint::new(a.x_mm, v), MmPoint::new(b.x_mm, v))
                };
                painter.line_segment([camera.screen(p, rect), camera.screen(q, rect)], stroke);
            }
        }
    }
}
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum ActiveTool {
    #[default]
    Select,
    Measure,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub a: MmPoint,
    pub b: MmPoint,
}
impl Measurement {
    pub fn values(self) -> Option<(f64, f64, f64, f64)> {
        let (dx, dy) = (self.b.x_mm - self.a.x_mm, self.b.y_mm - self.a.y_mm);
        let distance = dx.hypot(dy);
        let angle_deg = dy.atan2(dx).to_degrees().rem_euclid(360.);
        (dx.is_finite() && dy.is_finite() && distance.is_finite() && angle_deg.is_finite())
            .then_some((dx, dy, distance, angle_deg))
    }
}

#[derive(Default)]
pub struct MeasureState {
    pub a: Option<MmPoint>,
    pub b: Option<MmPoint>,
    pub fixed: bool,
    pub completed: Vec<Measurement>,
}
impl MeasureState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn click(&mut self, p: MmPoint) {
        if self.a.is_none() || self.fixed {
            self.a = Some(p);
            self.b = None;
            self.fixed = false;
        } else {
            self.b = Some(p);
            self.fixed = true;
            self.completed.push(Measurement {
                a: self.a.expect("measurement start exists"),
                b: p,
            });
        }
    }
    pub fn hover(&mut self, p: Option<MmPoint>) {
        if self.a.is_some() && !self.fixed {
            self.b = p;
        }
    }
    pub fn values(&self) -> Option<(f64, f64, f64)> {
        let (dx, dy, distance, _) = Measurement {
            a: self.a?,
            b: self.b?,
        }
        .values()?;
        Some((dx, dy, distance))
    }
    pub fn angle_deg(&self) -> Option<f64> {
        Measurement {
            a: self.a?,
            b: self.b?,
        }
        .values()
        .map(|(_, _, _, angle)| angle)
    }
    pub fn label(&self) -> String {
        self.label_in(DisplayUnit::Mm)
    }
    pub fn label_in(&self, unit: DisplayUnit) -> String {
        let retained = self.completed.len();
        let Some(a) = self.a else {
            return format!(
                "两点直线测距 · 已保留 {retained} 条 · 点击 A，然后点击 B · Esc 清除全部"
            );
        };
        let mut label = format!(
            "两点直线测距 · 已保留 {retained} 条 · +X 轴逆时针角度\nA ({:.6}, {:.6}) {}",
            unit.value(a.x_mm),
            unit.value(a.y_mm),
            unit.suffix()
        );
        if let (Some(b), Some((dx, dy, d)), Some(angle)) = (self.b, self.values(), self.angle_deg())
        {
            label.push_str(&format!(
                "\nB ({:.6}, {:.6}) {}\nΔX {:.6}  ΔY {:.6} {}\nDistance {:.6} {}  Angle {angle:.6}° · {}",
                unit.value(b.x_mm),
                unit.value(b.y_mm),
                unit.suffix(),
                unit.value(dx),
                unit.value(dy),
                unit.suffix(),
                unit.value(d),
                unit.suffix(),
                if self.fixed { "已固定" } else { "动态" }
            ));
        }
        label
    }
    pub fn paint_in(&self, painter: &egui::Painter, camera: Camera, rect: Rect, unit: DisplayUnit) {
        for measurement in &self.completed {
            paint_measurement(painter, camera, rect, *measurement, unit);
        }
        if !self.fixed
            && let Some(a) = self.a
        {
            if let Some(b) = self.b {
                paint_measurement(painter, camera, rect, Measurement { a, b }, unit);
            } else {
                painter.circle_filled(camera.screen(a, rect), 3., Color32::YELLOW);
            }
        }
        overlay_label(
            painter,
            rect.left_top() + egui::vec2(12., 36.),
            egui::Align2::LEFT_TOP,
            self.label_in(unit),
            Color32::YELLOW,
        );
    }
}

fn paint_measurement(
    painter: &egui::Painter,
    camera: Camera,
    rect: Rect,
    measurement: Measurement,
    unit: DisplayUnit,
) {
    let (p, q) = (
        camera.screen(measurement.a, rect),
        camera.screen(measurement.b, rect),
    );
    painter.line_segment([p, q], Stroke::new(1., Color32::YELLOW));
    painter.circle_filled(p, 3., Color32::YELLOW);
    painter.circle_filled(q, 3., Color32::YELLOW);
    let Some((_, _, distance, angle_deg)) = measurement.values() else {
        return;
    };
    let delta = q - p;
    let length = delta.length();
    let offset = if length > 0. {
        egui::vec2(-delta.y, delta.x) * (14. / length)
    } else {
        egui::vec2(0., -14.)
    };
    overlay_label(
        painter,
        p + delta * 0.5 + offset,
        egui::Align2::CENTER_CENTER,
        format!(
            "{:.6} {}  ∠ {angle_deg:.6}°",
            unit.value(distance),
            unit.suffix()
        ),
        Color32::YELLOW,
    );
}

/// Opaque backing keeps readouts legible over dense selected manufacturing geometry.
pub fn overlay_label(
    painter: &egui::Painter,
    pos: egui::Pos2,
    align: egui::Align2,
    text: String,
    color: Color32,
) {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.), color);
    let bounds = align.anchor_size(pos, galley.size());
    painter.rect_filled(bounds.expand(4.), 3., Color32::from_rgb(14, 18, 22));
    painter.galley(bounds.min, galley, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_density_does_not_change_snap_step() {
        let grid = GridSettings {
            visible: true,
            spacing_mm: 0.01,
            snap_enabled: true,
        };
        let rect = Rect::from_min_size(egui::pos2(0., 0.), egui::vec2(800., 600.));
        for scale in [1e-6, 1., 100., 1e8] {
            let c = Camera {
                scale,
                ..Default::default()
            };
            let step = grid.visual_step(c, rect).unwrap();
            assert!(step >= grid.spacing_mm && step * scale >= 12. - 1e-9);
            assert!(rect.width() as f64 / (step * scale) < 512.);
            assert_eq!(
                grid.point(MmPoint::new(0.123, -0.123)).unwrap(),
                MmPoint::new(0.12, -0.12)
            );
        }
    }
    #[test]
    fn measure_angle_multiple_retention_and_escape() {
        let mut m = MeasureState::default();
        m.click(MmPoint::new(0., 0.));
        m.hover(Some(MmPoint::new(3., 4.)));
        assert_eq!(m.values(), Some((3., 4., 5.)));
        assert!((m.angle_deg().unwrap() - 53.130_102_354_155_98).abs() < 1e-12);
        m.click(MmPoint::new(3., 4.));
        m.hover(Some(MmPoint::new(8., 8.)));
        assert_eq!(m.values(), Some((3., 4., 5.)));
        assert_eq!(m.completed.len(), 1);
        m.click(MmPoint::new(10., 10.));
        m.hover(Some(MmPoint::new(9., 9.)));
        m.click(MmPoint::new(9., 9.));
        assert_eq!(m.completed.len(), 2);
        assert!((m.angle_deg().unwrap() - 225.).abs() < 1e-12);
        assert!(m.label().contains("已保留 2 条"));
        assert!(m.label().contains("Angle 225.000000°"));
        m.clear();
        assert!(m.a.is_none() && m.values().is_none() && m.completed.is_empty());
    }
    #[test]
    fn measurement_angle_is_world_ccw_and_normalized() {
        for (b, expected) in [
            (MmPoint::new(1., 0.), 0.),
            (MmPoint::new(0., 1.), 90.),
            (MmPoint::new(-1., 0.), 180.),
            (MmPoint::new(0., -1.), 270.),
        ] {
            let measurement = Measurement {
                a: MmPoint::new(0., 0.),
                b,
            };
            assert_eq!(measurement.values().unwrap().3, expected);
        }
    }
    #[test]
    fn object_snap_precedes_grid_and_uses_logical_point_radius() {
        let grid = GridSettings {
            visible: true,
            spacing_mm: 1.,
            snap_enabled: true,
        };
        let candidate = SnapPoint {
            point: MmPoint::new(1.4, 2.4),
            object_id: "object-1".into(),
            kind: SnapKind::Endpoint,
        };
        let camera = Camera {
            scale: 10.,
            ..Default::default()
        };
        assert_eq!(
            snap_point(
                MmPoint::new(1.45, 2.45),
                grid,
                std::slice::from_ref(&candidate),
                camera,
                None,
                false
            )
            .unwrap(),
            candidate.point
        );
        assert_eq!(
            snap_point(
                MmPoint::new(1.45, 2.45),
                grid,
                &[candidate],
                camera,
                None,
                true
            )
            .unwrap(),
            MmPoint::new(1.45, 2.45)
        );
    }
    #[test]
    fn inch_display_does_not_change_manufacturing_values() {
        let mut measure = MeasureState::default();
        measure.click(MmPoint::new(0., 0.));
        measure.click(MmPoint::new(25.4, 0.));
        assert_eq!(measure.values(), Some((25.4, 0., 25.4)));
        assert!(
            measure
                .label_in(DisplayUnit::Inch)
                .contains("Distance 1.000000 in")
        );
    }
}
