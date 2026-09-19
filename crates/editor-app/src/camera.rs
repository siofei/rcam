use editor_core::{BoundsMm, MmPoint};
use eframe::egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub center: MmPoint,
    /// Logical egui points per millimetre, never physical pixels.
    pub scale: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: MmPoint::new(0., 0.),
            scale: 10.,
        }
    }
}
impl Camera {
    pub fn fit(&mut self, bounds: Option<BoundsMm>, rect: Rect) {
        let Some(b) = bounds else {
            *self = Self::default();
            return;
        };
        self.center = MmPoint::new(
            b.min_x_mm + (b.max_x_mm - b.min_x_mm) / 2.,
            b.min_y_mm + (b.max_y_mm - b.min_y_mm) / 2.,
        );
        let width = (b.max_x_mm - b.min_x_mm).max(1e-9);
        let height = (b.max_y_mm - b.min_y_mm).max(1e-9);
        self.scale = (f64::from(rect.width().max(1.)) * 0.9 / width)
            .min(f64::from(rect.height().max(1.)) * 0.9 / height)
            .clamp(1e-6, 1e8);
    }
    pub fn world(&self, pos: Pos2, rect: Rect) -> MmPoint {
        MmPoint::new(
            self.center.x_mm + f64::from(pos.x - rect.center().x) / self.scale,
            self.center.y_mm - f64::from(pos.y - rect.center().y) / self.scale,
        )
    }
    pub fn screen(&self, p: MmPoint, rect: Rect) -> Pos2 {
        Pos2::new(
            rect.center().x + ((p.x_mm - self.center.x_mm) * self.scale) as f32,
            rect.center().y - ((p.y_mm - self.center.y_mm) * self.scale) as f32,
        )
    }
    pub fn pan(&mut self, delta: Vec2) {
        self.center.x_mm -= f64::from(delta.x) / self.scale;
        self.center.y_mm += f64::from(delta.y) / self.scale;
    }
    pub fn zoom(&mut self, factor: f64, cursor: Pos2, rect: Rect) {
        if !factor.is_finite() || factor <= 0. {
            return;
        }
        let before = self.world(cursor, rect);
        self.scale = (self.scale * factor).clamp(1e-6, 1e8);
        let after = self.world(cursor, rect);
        self.center.x_mm += before.x_mm - after.x_mm;
        self.center.y_mm += before.y_mm - after.y_mm;
    }
    pub fn tolerance(&self, pixels_per_point: f32) -> f64 {
        6. / (self.scale * f64::from(pixels_per_point))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(213., 74.), Vec2::new(800., 600.))
    }
    #[test]
    fn camera_fit_uses_f64_bounds() {
        let mut c = Camera::default();
        c.fit(
            Some(BoundsMm {
                min_x_mm: 1e8,
                max_x_mm: 1e8 + 2.,
                min_y_mm: 3.,
                max_y_mm: 7.,
            }),
            rect(),
        );
        assert_eq!(c.center, MmPoint::new(1e8 + 1., 5.));
        assert_eq!(c.scale, 135.);
    }
    #[test]
    fn camera_zoom_keeps_cursor_world_point() {
        let mut c = Camera::default();
        let p = Pos2::new(350., 240.);
        let w = c.world(p, rect());
        for f in [2., 0.1, 37., 1e15, 1e-30] {
            c.zoom(f, p, rect());
            assert!(c.world(p, rect()).distance_mm(w) < 1e-6);
        }
    }
    #[test]
    fn screen_world_roundtrip_retina_scale() {
        for dpi in [1., 2., 3.] {
            let c = Camera::default();
            let p = Pos2::new(421., 310.);
            let w = c.world(p, rect());
            assert!(c.screen(w, rect()).distance(p) < 1e-4);
            assert_eq!(c.tolerance(dpi) * c.scale * f64::from(dpi), 6.);
        }
    }
    #[test]
    fn selection_tolerance_is_pixel_stable() {
        for scale in [0.01, 1., 10., 10000.] {
            let c = Camera {
                scale,
                ..Default::default()
            };
            assert!((c.tolerance(2.) * scale * 2. - 6.).abs() < 1e-12);
        }
    }
}
