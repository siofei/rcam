//! View-only grid and two-point ruler. Never owns manufacturing state.
use crate::camera::Camera;
use editor_core::{MmPoint, grid::snap_scalar};
use eframe::egui::{self, Color32, Rect, Stroke};

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
#[derive(Default)]
pub struct MeasureState {
    pub a: Option<MmPoint>,
    pub b: Option<MmPoint>,
    pub fixed: bool,
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
        }
    }
    pub fn hover(&mut self, p: Option<MmPoint>) {
        if self.a.is_some() && !self.fixed {
            self.b = p;
        }
    }
    pub fn values(&self) -> Option<(f64, f64, f64)> {
        let (a, b) = (self.a?, self.b?);
        let (dx, dy) = (b.x_mm - a.x_mm, b.y_mm - a.y_mm);
        let distance = dx.hypot(dy);
        (dx.is_finite() && dy.is_finite() && distance.is_finite()).then_some((dx, dy, distance))
    }
    pub fn label(&self) -> String {
        let Some(a) = self.a else {
            return "两点直线测距 · 点击 A，然后点击 B · Esc 清除".into();
        };
        let mut label = format!("A ({:.6}, {:.6}) mm", a.x_mm, a.y_mm);
        if let (Some(b), Some((dx, dy, d))) = (self.b, self.values()) {
            label.push_str(&format!(
                "\nB ({:.6}, {:.6}) mm\nΔX {dx:.6}  ΔY {dy:.6} mm\nDistance {d:.6} mm · {}",
                b.x_mm,
                b.y_mm,
                if self.fixed { "已固定" } else { "动态" }
            ));
        }
        label
    }
    pub fn paint(&self, painter: &egui::Painter, camera: Camera, rect: Rect) {
        if let Some(a) = self.a {
            let p = camera.screen(a, rect);
            painter.circle_filled(p, 3., Color32::YELLOW);
            if let Some(b) = self.b {
                let q = camera.screen(b, rect);
                painter.line_segment([p, q], Stroke::new(1., Color32::YELLOW));
                painter.circle_filled(q, 3., Color32::YELLOW);
            }
        }
        overlay_label(
            painter,
            rect.left_top() + egui::vec2(12., 36.),
            egui::Align2::LEFT_TOP,
            self.label(),
            Color32::YELLOW,
        );
    }
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
    fn measure_dynamic_fixed_and_escape() {
        let mut m = MeasureState::default();
        m.click(MmPoint::new(0., 0.));
        m.hover(Some(MmPoint::new(3., 4.)));
        assert_eq!(m.values(), Some((3., 4., 5.)));
        m.click(MmPoint::new(3., 4.));
        m.hover(Some(MmPoint::new(8., 8.)));
        assert_eq!(m.values(), Some((3., 4., 5.)));
        m.clear();
        assert!(m.a.is_none() && m.values().is_none());
    }
}
