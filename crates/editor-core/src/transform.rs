//! Rigid manufacturing transforms; never uses display tessellation.
use super::*;
use crate::edit::{EditError, MirrorAxis};

type Matrix = [[f64; 2]; 2];

fn angle(value: f64) -> f64 {
    let normalized = value.rem_euclid(360.0);
    if normalized == 0.0 || normalized == 360.0 {
        0.0
    } else {
        normalized
    }
}

fn rotation(degrees: f64) -> Matrix {
    let (s, c) = match angle(degrees) {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        a => a.to_radians().sin_cos(),
    };
    [[c, -s], [s, c]]
}

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j]))
}
fn local_matrix(t: LocalTransform) -> Matrix {
    let (x, y) = match t.mirror {
        Mirror::None => (1., 1.),
        Mirror::X => (-1., 1.),
        Mirror::Y => (1., -1.),
        Mirror::Xy => (-1., -1.),
    };
    multiply(rotation(t.rotation_deg), [[x, 0.], [0., y]])
}

pub(crate) struct WorldTransform {
    matrix: Matrix,
    pivot: MmPoint,
    degrees: f64,
    mirror: Option<MirrorAxis>,
}

impl WorldTransform {
    pub(crate) fn rotation(degrees: f64, pivot: MmPoint) -> Result<Self, EditError> {
        if !degrees.is_finite() || !pivot.is_valid_geometry() || angle(degrees) == 0.0 {
            return Err(EditError::InvalidArgument);
        }
        let degrees = angle(degrees);
        Ok(Self {
            matrix: rotation(degrees),
            pivot,
            degrees,
            mirror: None,
        })
    }
    pub(crate) fn reflection(axis: MirrorAxis) -> Result<Self, EditError> {
        let (matrix, pivot) = match axis {
            MirrorAxis::Horizontal { coordinate_mm } => {
                ([[1., 0.], [0., -1.]], MmPoint::new(0., coordinate_mm))
            }
            MirrorAxis::Vertical { coordinate_mm } => {
                ([[-1., 0.], [0., 1.]], MmPoint::new(coordinate_mm, 0.))
            }
        };
        if !pivot.is_valid_geometry() {
            return Err(EditError::InvalidArgument);
        }
        Ok(Self {
            matrix,
            pivot,
            degrees: 0.,
            mirror: Some(axis),
        })
    }
    fn point(&self, point: &mut MmPoint) -> Result<(), EditError> {
        let x = point.x_mm - self.pivot.x_mm;
        let y = point.y_mm - self.pivot.y_mm;
        let next = MmPoint::new(
            self.pivot.x_mm + self.matrix[0][0] * x + self.matrix[0][1] * y,
            self.pivot.y_mm + self.matrix[1][0] * x + self.matrix[1][1] * y,
        );
        let magnitude = [
            point.x_mm,
            point.y_mm,
            self.pivot.x_mm,
            self.pivot.y_mm,
            x,
            y,
        ]
        .into_iter()
        .map(f64::abs)
        .fold(1.0, f64::max);
        if !next.is_valid_geometry() || 16.0 * f64::EPSILON * magnitude > EPSILON_MM {
            return Err(EditError::InvalidArgument);
        }
        *point = next;
        Ok(())
    }
    fn local(&self, transform: &mut LocalTransform) -> Result<(), EditError> {
        // Forward Gerber order is R * M * scale (inverse: unscale, R^-1, M).
        let composed = multiply(self.matrix, local_matrix(*transform));
        let mut next = *transform;
        next.rotation_deg = angle(if self.mirror.is_some() {
            -transform.rotation_deg
        } else {
            angle(transform.rotation_deg) + self.degrees
        });
        // Deterministic decomposition retaining both reflection bits. M R(a) = R(-a) M.
        next.mirror = match (self.mirror, transform.mirror) {
            (None, m) => m,
            (Some(MirrorAxis::Horizontal { .. }), Mirror::None) => Mirror::Y,
            (Some(MirrorAxis::Horizontal { .. }), Mirror::X) => Mirror::Xy,
            (Some(MirrorAxis::Horizontal { .. }), Mirror::Y) => Mirror::None,
            (Some(MirrorAxis::Horizontal { .. }), Mirror::Xy) => Mirror::X,
            (Some(MirrorAxis::Vertical { .. }), Mirror::None) => Mirror::X,
            (Some(MirrorAxis::Vertical { .. }), Mirror::X) => Mirror::None,
            (Some(MirrorAxis::Vertical { .. }), Mirror::Y) => Mirror::Xy,
            (Some(MirrorAxis::Vertical { .. }), Mirror::Xy) => Mirror::Y,
        };
        let decomposed = local_matrix(next);
        if (0..2)
            .any(|i| (0..2).any(|j| (composed[i][j] - decomposed[i][j]).abs() > 64. * f64::EPSILON))
        {
            return Err(EditError::InvalidArgument);
        }
        *transform = next;
        Ok(())
    }
    fn arc(&self, path: &mut ArcGeometry) -> Result<(), EditError> {
        let before = *path;
        self.point(&mut path.start)?;
        self.point(&mut path.end)?;
        self.point(&mut path.center)?;
        if self.mirror.is_some() {
            path.direction = match path.direction {
                ArcDirection::Clockwise => ArcDirection::CounterClockwise,
                ArcDirection::CounterClockwise => ArcDirection::Clockwise,
            };
        }
        if (before.start == before.end) != (path.start == path.end)
            || before.zero_sweep() != path.zero_sweep()
            || (before.radius() - path.radius()).abs() > EPSILON_MM
            || (before.end_radius() - path.end_radius()).abs() > EPSILON_MM
            || (before.arc_deviation() - path.arc_deviation()).abs() > EPSILON_MM
        {
            return Err(EditError::InvalidArgument);
        }
        Ok(())
    }
    pub(crate) fn apply(&self, geometry: &mut SemanticGeometry) -> Result<(), EditError> {
        match geometry {
            SemanticGeometry::Flash {
                center, transform, ..
            } => {
                self.point(center)?;
                self.local(transform)?;
            }
            SemanticGeometry::Line { start, end, .. } => {
                self.point(start)?;
                self.point(end)?;
            }
            SemanticGeometry::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            } => {
                if self.degrees % 90.0 != 0.0 {
                    return Err(EditError::UnsupportedTransform);
                }
                self.point(start)?;
                self.point(end)?;
                if self.degrees == 90.0 || self.degrees == 270.0 {
                    std::mem::swap(width_mm, height_mm);
                }
            }
            SemanticGeometry::Arc { path, .. } => self.arc(path)?,
            SemanticGeometry::Region { contours } => {
                for edge in contours.iter_mut().flat_map(|c| &mut c.edges) {
                    match edge {
                        RegionEdge::Line { start, end } => {
                            self.point(start)?;
                            self.point(end)?;
                        }
                        RegionEdge::Arc(path) => self.arc(path)?,
                    }
                }
            }
        }
        Ok(())
    }
}
