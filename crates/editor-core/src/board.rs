//! Independent PCB/PnP Board coordinates and rigid registration (S4-D1).
//!
//! The canonical manufacturing coordinate stays f64 millimetres ("Manufacturing
//! World"). Future PnP/centroid data lives in a *Board* space and reaches the
//! world through one rigid `CoordinateTransform2D`. Arbitrary scale and shear are
//! intentionally not expressible.

use crate::MmPoint;
use serde::{Deserialize, Serialize};

/// The three coordinate spaces the architecture distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    /// Coordinates as written by the importing file.
    Source,
    /// PCB board frame (origin/orientation of the board design).
    Board,
    /// Manufacturing World, f64 mm; the only space geometry is validated in.
    ManufacturingWorld,
}

/// Rigid transform: optional reflection about the local Y axis (x -> -x), then
/// rotation (degrees, counter-clockwise), then translation. `scale` is absent
/// by design.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinateTransform2D {
    pub reflect_x: bool,
    pub rotation_deg: f64,
    pub translation: MmPoint,
}

impl Default for CoordinateTransform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl CoordinateTransform2D {
    pub const IDENTITY: Self = Self {
        reflect_x: false,
        rotation_deg: 0.,
        translation: MmPoint::new(0., 0.),
    };

    pub fn translation(dx_mm: f64, dy_mm: f64) -> Self {
        Self {
            translation: MmPoint::new(dx_mm, dy_mm),
            ..Self::IDENTITY
        }
    }

    pub fn is_valid(&self) -> bool {
        self.rotation_deg.is_finite() && self.translation.is_finite()
    }

    pub fn apply(&self, point: MmPoint) -> MmPoint {
        let x = if self.reflect_x {
            -point.x_mm
        } else {
            point.x_mm
        };
        let (sin, cos) = self.rotation_deg.to_radians().sin_cos();
        MmPoint::new(
            cos * x - sin * point.y_mm + self.translation.x_mm,
            sin * x + cos * point.y_mm + self.translation.y_mm,
        )
    }

    /// Inverse transform (always exists: the transform is rigid).
    pub fn inverse(&self) -> Self {
        // T(p) = R(a) M^s p + t  =>  T^-1(q) = M^s R(-a) (q - t).
        // M R(-a) = R(a) M, so the linear part is again "rotate after reflect".
        let linear = Self {
            reflect_x: self.reflect_x,
            rotation_deg: if self.reflect_x {
                self.rotation_deg
            } else {
                -self.rotation_deg
            },
            translation: MmPoint::new(0., 0.),
        };
        let moved = linear.apply(self.translation);
        Self {
            translation: MmPoint::new(-moved.x_mm, -moved.y_mm),
            ..linear
        }
    }

    /// `self` applied after `first` (`self(first(p))`).
    pub fn after(&self, first: &Self) -> Self {
        // R2 M2^s2 R1 M1^s1: a reflection between rotations negates the inner angle.
        let (reflect_x, rotation_deg) = if self.reflect_x {
            (!first.reflect_x, self.rotation_deg - first.rotation_deg)
        } else {
            (first.reflect_x, self.rotation_deg + first.rotation_deg)
        };
        Self {
            reflect_x,
            rotation_deg: normalize_degrees(rotation_deg),
            translation: self.apply(first.translation),
        }
    }
}

fn normalize_degrees(value: f64) -> f64 {
    let v = value.rem_euclid(360.);
    if v > 180. { v - 360. } else { v }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardPoint {
    pub x_mm: f64,
    pub y_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardSide {
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentId(pub String);

/// Independent model for PnP / centroid data. It is deliberately *not* attached to
/// `SemanticObject`: a RefDes belongs to a component, not to Gerber geometry.
/// Queries live in ApplicationService; focus is a session-only GUI operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentPlacement {
    pub id: ComponentId,
    pub refdes: String,
    pub position: BoardPoint,
    pub rotation_deg: f64,
    pub side: BoardSide,
    pub footprint: Option<String>,
    pub value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: MmPoint, b: MmPoint) -> bool {
        a.distance_mm(b) < 1e-9
    }

    fn samples() -> Vec<CoordinateTransform2D> {
        let mut out = vec![CoordinateTransform2D::IDENTITY];
        for reflect_x in [false, true] {
            for rotation_deg in [0., 30., 90., -135., 180., 271.5] {
                out.push(CoordinateTransform2D {
                    reflect_x,
                    rotation_deg,
                    translation: MmPoint::new(12.25, -3.5),
                });
            }
        }
        out
    }

    #[test]
    fn rigid_transform_preserves_distance_and_inverts() {
        let (a, b) = (MmPoint::new(1., 2.), MmPoint::new(-4., 7.5));
        for t in samples() {
            assert!(t.is_valid());
            let (ta, tb) = (t.apply(a), t.apply(b));
            assert!(
                (ta.distance_mm(tb) - a.distance_mm(b)).abs() < 1e-9,
                "{t:?}"
            );
            let inv = t.inverse();
            assert!(close(inv.apply(ta), a), "{t:?}");
            assert!(close(t.apply(inv.apply(b)), b), "{t:?}");
        }
    }

    #[test]
    fn composition_matches_sequential_application() {
        let p = MmPoint::new(3.5, -2.25);
        for first in samples() {
            for second in samples() {
                let composed = second.after(&first);
                assert!(
                    close(composed.apply(p), second.apply(first.apply(p))),
                    "{first:?} then {second:?}"
                );
            }
        }
    }

    #[test]
    fn reflection_and_rotation_are_expressible_scale_and_shear_are_not() {
        let mirror = CoordinateTransform2D {
            reflect_x: true,
            ..CoordinateTransform2D::IDENTITY
        };
        assert!(close(
            mirror.apply(MmPoint::new(2., 3.)),
            MmPoint::new(-2., 3.)
        ));
    }
}
