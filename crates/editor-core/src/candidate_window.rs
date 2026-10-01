//! Bounds candidates only: never final material intersection or ownership.
use crate::{BoundsMm, MmPoint, board::CoordinateTransform2D, pnp::MAX_BOARD_MM};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CandidateWindow {
    pub center: MmPoint,
    pub rotation_deg: f64,
    pub width_mm: f64,
    pub height_mm: f64,
}
impl CandidateWindow {
    pub fn new(
        center: MmPoint,
        rotation_deg: f64,
        width_mm: f64,
        height_mm: f64,
    ) -> Result<Self, &'static str> {
        if !center.is_finite()
            || !rotation_deg.is_finite()
            || [width_mm, height_mm]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0. || *v > MAX_BOARD_MM)
        {
            return Err("window dimensions must be finite, positive and <= MAX_BOARD_MM");
        }
        Ok(Self {
            center,
            rotation_deg,
            width_mm,
            height_mm,
        })
    }
    pub fn corners(self) -> [MmPoint; 4] {
        let t = CoordinateTransform2D {
            translation: self.center,
            rotation_deg: self.rotation_deg,
            reflect_x: false,
        };
        let x = self.width_mm / 2.;
        let y = self.height_mm / 2.;
        [
            MmPoint::new(-x, -y),
            MmPoint::new(x, -y),
            MmPoint::new(x, y),
            MmPoint::new(-x, y),
        ]
        .map(|p| t.apply(p))
    }
    pub fn bounds(self) -> BoundsMm {
        let c = self.corners();
        BoundsMm {
            min_x_mm: c.iter().map(|p| p.x_mm).fold(f64::INFINITY, f64::min),
            min_y_mm: c.iter().map(|p| p.y_mm).fold(f64::INFINITY, f64::min),
            max_x_mm: c.iter().map(|p| p.x_mm).fold(f64::NEG_INFINITY, f64::max),
            max_y_mm: c.iter().map(|p| p.y_mm).fold(f64::NEG_INFINITY, f64::max),
        }
    }
    /// Separating-axis test between a world AABB and the rotated rectangle.
    pub fn classify(self, b: BoundsMm) -> (bool, bool) {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        let p = b.center();
        let dx = p.x_mm - self.center.x_mm;
        let dy = p.y_mm - self.center.y_mm;
        let ex = (b.max_x_mm - b.min_x_mm) / 2.;
        let ey = (b.max_y_mm - b.min_y_mm) / 2.;
        let hx = self.width_mm / 2.;
        let hy = self.height_mm / 2.;
        let lx = (c * dx + s * dy).abs();
        let ly = (-s * dx + c * dy).abs();
        let rx = c.abs() * ex + s.abs() * ey;
        let ry = s.abs() * ex + c.abs() * ey;
        let intersects = dx.abs() <= ex + c.abs() * hx + s.abs() * hy
            && dy.abs() <= ey + s.abs() * hx + c.abs() * hy
            && lx <= hx + rx
            && ly <= hy + ry;
        (intersects, lx + rx <= hx && ly + ry <= hy)
    }
    pub fn distance_to_bounds(self, b: BoundsMm) -> f64 {
        let x = self.center.x_mm.clamp(b.min_x_mm, b.max_x_mm);
        let y = self.center.y_mm.clamp(b.min_y_mm, b.max_y_mm);
        self.center.distance_mm(MmPoint::new(x, y))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_window_sat_rejects_aabb_corner_and_contains_center() {
        for a in [0., 37., 90.] {
            let w = CandidateWindow::new(MmPoint::new(20., 30.), a, 12., 8.).unwrap();
            let b = BoundsMm {
                min_x_mm: 19.9,
                min_y_mm: 29.9,
                max_x_mm: 20.1,
                max_y_mm: 30.1,
            };
            assert_eq!(w.classify(b), (true, true));
            for p in w.corners() {
                assert!(p.distance_mm(w.center) - 6_f64.hypot(4.) < 1e-10);
            }
        }
        let w = CandidateWindow::new(MmPoint::new(0., 0.), 45., 10., 2.).unwrap();
        assert_eq!(
            w.classify(BoundsMm {
                min_x_mm: 3.8,
                max_x_mm: 3.9,
                min_y_mm: -3.9,
                max_y_mm: -3.8
            }),
            (false, false)
        );
        for v in [0., -1., f64::NAN, f64::INFINITY, MAX_BOARD_MM + 1.] {
            assert!(CandidateWindow::new(MmPoint::new(0., 0.), 0., v, 1.).is_err());
        }
    }
}
