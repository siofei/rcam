//! View-only grid and two-point ruler. Never owns manufacturing state.
use crate::camera::Camera;
use editor_core::{MmPoint, grid::snap_scalar};
use eframe::egui::{self, Color32, Rect, Stroke};

pub use editor_core::units::DisplayUnit;

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
    /// Display levels only; snapping always reads spacing_mm directly.
    pub fn visual_levels(self, camera: Camera, ppp: f32) -> Vec<(f64, f32)> {
        let physical = camera.scale * f64::from(ppp);
        if !self.spacing_mm.is_finite()
            || self.spacing_mm <= 0.
            || !physical.is_finite()
            || physical <= 0.
        {
            return vec![];
        }
        let density = |px: f64| {
            let t = ((px - 6.) / 12.).clamp(0., 1.);
            t * t * (3. - 2. * t)
        };
        let mut levels = Vec::new();
        let mut previous = [0.; 2];
        for decade in 0..=30 {
            for multiplier in [1., 2., 5.] {
                let step = self.spacing_mm * multiplier * 10f64.powi(decade);
                let d = density(step * physical);
                let weight = d * (1. - previous[0]);
                previous = [previous[1], d];
                if weight > 0. {
                    levels.push((step, weight as f32));
                }
                if previous == [1., 1.] {
                    return levels;
                }
            }
        }
        levels
    }
    pub fn paint(
        self,
        painter: &egui::Painter,
        camera: Camera,
        rect: Rect,
        ppp: f32,
        opacity: f32,
    ) {
        if opacity <= 0. {
            return;
        }
        let a = camera.world(rect.left_bottom(), rect);
        let b = camera.world(rect.right_top(), rect);
        for (step, density) in self.visual_levels(camera, ppp) {
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
                // Every rendered spacing is a 1/2/5 integer multiple of the base.
                let count = ((hi - lo) / step).ceil().max(0.) as usize + 2;
                for n in 0..count.min(8192) {
                    let index = first + n as f64;
                    let v = index * step;
                    if !v.is_finite() || v > hi {
                        break;
                    }
                    let style = if index.rem_euclid(5.) == 0. { 85. } else { 42. };
                    let alpha = (opacity * density * style).round() as u8;
                    let stroke = Stroke::new(
                        1. / ppp,
                        Color32::from_rgba_unmultiplied(130, 160, 180, alpha),
                    );
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
}

/// Transient overlay state, never part of a document or undo transaction.
#[derive(Default)]
pub struct GridVisualState {
    pub opacity: f32,
}
impl GridVisualState {
    pub fn advance(&mut self, visible: bool, dt: f32) -> bool {
        let target = if visible { 1. } else { 0. };
        let delta = if dt.is_finite() {
            dt.max(0.) / 0.180
        } else {
            0.
        };
        self.opacity += (target - self.opacity).clamp(-delta, delta);
        self.opacity != target
    }
}
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum ActiveTool {
    #[default]
    Select,
    Measure,
    Text,
    Block,
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
    pub a_kind: Option<editor_core::snap::SnapKind>,
    pub b_kind: Option<editor_core::snap::SnapKind>,
    pub fixed: bool,
    pub completed: Vec<Measurement>,
}
impl MeasureState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    #[cfg(test)]
    pub fn click(&mut self, p: MmPoint) {
        self.click_snapped(p, None);
    }
    pub fn click_snapped(&mut self, p: MmPoint, kind: Option<editor_core::snap::SnapKind>) {
        if self.a.is_none() || self.fixed {
            self.a = Some(p);
            self.b = None;
            self.a_kind = kind;
            self.b_kind = None;
            self.fixed = false;
        } else {
            self.b = Some(p);
            self.b_kind = kind;
            self.fixed = true;
            self.completed.push(Measurement {
                a: self.a.expect("measurement start exists"),
                b: p,
            });
        }
    }
    pub fn hover(&mut self, p: Option<MmPoint>) {
        self.hover_snapped(p, None);
    }
    pub fn hover_snapped(&mut self, p: Option<MmPoint>, kind: Option<editor_core::snap::SnapKind>) {
        if self.a.is_some() && !self.fixed {
            self.b = p;
            self.b_kind = kind;
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
        self.label_in(DisplayUnit::Millimeter)
    }
    pub fn label_in(&self, unit: DisplayUnit) -> String {
        self.label_with_resolution(unit, 0.0001)
    }
    pub fn label_with_resolution(&self, unit: DisplayUnit, resolution: f64) -> String {
        let retained = self.completed.len();
        let Some(a) = self.a else {
            return format!(
                "两点直线测距 · 已保留 {retained} 条 · 点击 A，然后点击 B · Esc 清除全部"
            );
        };
        let length = |v| unit.format_length(v, resolution);
        let mut label = format!(
            "两点直线测距 · 已保留 {retained} 条 · +X 轴逆时针角度\nA{} ({}, {})",
            self.a_kind.map_or(String::new(), |kind| format!(
                "：{}",
                crate::object_snap::kind_label(kind)
            )),
            length(a.x_mm),
            length(a.y_mm)
        );
        if let (Some(b), Some((dx, dy, d)), Some(angle)) = (self.b, self.values(), self.angle_deg())
        {
            label.push_str(&format!(
                "\nB{} ({}, {})\nΔX {}  ΔY {}\nDistance {}  Angle {angle:.6}° · {}",
                self.b_kind.map_or(String::new(), |kind| format!(
                    "：{}",
                    crate::object_snap::kind_label(kind)
                )),
                length(b.x_mm),
                length(b.y_mm),
                length(dx),
                length(dy),
                length(d),
                if self.fixed { "已固定" } else { "动态" }
            ));
        }
        label
    }
    pub fn paint_in(
        &self,
        painter: &egui::Painter,
        camera: Camera,
        rect: Rect,
        unit: DisplayUnit,
        resolution: f64,
    ) {
        for measurement in &self.completed {
            paint_measurement(painter, camera, rect, *measurement, unit, resolution);
        }
        if !self.fixed
            && let Some(a) = self.a
        {
            if let Some(b) = self.b {
                paint_measurement(
                    painter,
                    camera,
                    rect,
                    Measurement { a, b },
                    unit,
                    resolution,
                );
            } else {
                painter.circle_filled(camera.screen(a, rect), 3., Color32::YELLOW);
            }
        }
        overlay_label(
            painter,
            rect.left_top() + egui::vec2(12., 36.),
            egui::Align2::LEFT_TOP,
            self.label_with_resolution(unit, resolution),
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
    resolution: f64,
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
            "{}  ∠ {angle_deg:.6}°",
            unit.format_length(distance, resolution)
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
    fn grid_fade_has_monotone_exact_endpoints_and_stops() {
        let mut visual = GridVisualState::default();
        for visible in [true, false] {
            for _ in 0..20 {
                let old = visual.opacity;
                visual.advance(visible, 0.01);
                assert!(if visible {
                    visual.opacity >= old
                } else {
                    visual.opacity <= old
                });
            }
            assert_eq!(visual.opacity, if visible { 1. } else { 0. });
            assert!(!visual.advance(visible, 0.01));
        }
    }
    #[test]
    fn density_weights_are_continuous_across_nice_steps() {
        let grid = GridSettings::default();
        for scale in [12., 30., 60., 90., 180.] {
            let levels = |scale| {
                grid.visual_levels(
                    Camera {
                        scale,
                        ..Default::default()
                    },
                    2.,
                )
            };
            let a = levels(scale * (1. - 1e-7));
            let b = levels(scale * (1. + 1e-7));
            for (step, alpha) in a.iter().chain(&b) {
                let weight =
                    |v: &Vec<(f64, f32)>| v.iter().find(|x| x.0 == *step).map_or(0., |x| x.1);
                assert!((weight(&a) - weight(&b)).abs() < 1e-5, "{step} {alpha}");
            }
        }
    }
    #[test]
    fn visual_density_does_not_change_snap_step() {
        let grid = GridSettings {
            visible: true,
            spacing_mm: 0.01,
            snap_enabled: true,
        };
        for scale in [1e-6, 1., 100., 1e8] {
            let c = Camera {
                scale,
                ..Default::default()
            };
            for (step, alpha) in grid.visual_levels(c, 2.) {
                assert!(step >= grid.spacing_mm && step * scale * 2. > 6.);
                assert!((step / grid.spacing_mm - (step / grid.spacing_mm).round()).abs() < 1e-4);
                assert!((0. ..=1.).contains(&alpha));
            }
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
    fn inch_display_does_not_change_manufacturing_values() {
        let mut measure = MeasureState::default();
        measure.click(MmPoint::new(0., 0.));
        measure.click(MmPoint::new(25.4, 0.));
        assert_eq!(measure.values(), Some((25.4, 0., 25.4)));
        assert!(
            measure
                .label_in(DisplayUnit::Inch)
                .contains("Distance 1.000000 inch")
        );
    }
}
