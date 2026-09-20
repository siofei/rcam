//! Analytic manufacturing envelopes. Never inferred from display meshes.
use crate::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundsMm {
    pub min_x_mm: f64,
    pub min_y_mm: f64,
    pub max_x_mm: f64,
    pub max_y_mm: f64,
}

impl BoundsMm {
    fn points(a: MmPoint, b: MmPoint) -> Self {
        Self {
            min_x_mm: a.x_mm.min(b.x_mm),
            min_y_mm: a.y_mm.min(b.y_mm),
            max_x_mm: a.x_mm.max(b.x_mm),
            max_y_mm: a.y_mm.max(b.y_mm),
        }
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min_x_mm: self.min_x_mm.min(other.min_x_mm),
            min_y_mm: self.min_y_mm.min(other.min_y_mm),
            max_x_mm: self.max_x_mm.max(other.max_x_mm),
            max_y_mm: self.max_y_mm.max(other.max_y_mm),
        }
    }

    pub fn center(self) -> MmPoint {
        MmPoint::new(
            self.min_x_mm + (self.max_x_mm - self.min_x_mm) / 2.,
            self.min_y_mm + (self.max_y_mm - self.min_y_mm) / 2.,
        )
    }

    fn expand(self, x: f64, y: f64) -> Self {
        Self {
            min_x_mm: self.min_x_mm - x,
            min_y_mm: self.min_y_mm - y,
            max_x_mm: self.max_x_mm + x,
            max_y_mm: self.max_y_mm + y,
        }
    }

    fn checked(self) -> Result<Self, SemanticError> {
        if [self.min_x_mm, self.min_y_mm, self.max_x_mm, self.max_y_mm]
            .into_iter()
            .all(f64::is_finite)
            && self.min_x_mm <= self.max_x_mm
            && self.min_y_mm <= self.max_y_mm
        {
            Ok(self)
        } else {
            Err(SemanticError::Invalid(
                "non-finite or inverted manufacturing bounds".into(),
            ))
        }
    }
}

fn combine(items: impl IntoIterator<Item = BoundsMm>) -> Option<BoundsMm> {
    items.into_iter().reduce(BoundsMm::union)
}

impl SemanticDocument {
    /// On validated semantic documents. Includes both Dark and Clear objects;
    /// macro envelopes conservatively retain all Dark primitive extents.
    pub fn manufacturing_bounds(
        &self,
        layer_id: Option<&str>,
    ) -> Result<Option<BoundsMm>, SemanticError> {
        if layer_id.is_some_and(|id| !self.layers.iter().any(|layer| layer.id == id)) {
            return Err(SemanticError::Invalid("unknown bounds layer".into()));
        }
        geometries_bounds(
            self.layers
                .iter()
                .filter(|layer| layer_id.is_none_or(|id| layer.id == id))
                .flat_map(|layer| layer.objects.iter().map(|object| &object.geometry)),
            &self.apertures,
        )
    }
}

/// Union bounds for an explicit manufacturing-geometry selection.
///
/// This is intentionally independent of renderer meshes and screen pixels.
pub fn geometries_bounds<'a>(
    geometries: impl IntoIterator<Item = &'a SemanticGeometry>,
    apertures: &[ApertureDefinition],
) -> Result<Option<BoundsMm>, SemanticError> {
    let apertures: HashMap<_, _> = apertures
        .iter()
        .map(|aperture| (aperture.id.as_str(), &aperture.shape))
        .collect();
    let mut result = None;
    for geometry in geometries {
        if let Some(bounds) = geometry_bounds(geometry, &apertures)? {
            result = Some(result.map_or(bounds, |previous: BoundsMm| previous.union(bounds)));
        }
    }
    Ok(result)
}

fn geometry_bounds(
    geometry: &SemanticGeometry,
    apertures: &HashMap<&str, &ApertureShape>,
) -> Result<Option<BoundsMm>, SemanticError> {
    let bounds = match geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            transform,
        } => {
            let shape = apertures
                .get(aperture_id.as_str())
                .ok_or_else(|| SemanticError::MissingAperture(aperture_id.clone()))?;
            // Axis support is computed in local space, then translated. This is
            // tighter than rotating a precomputed local AABB (notably for O/P).
            let axis = |world: MmPoint| {
                apply_inverse_transform(
                    world,
                    MmPoint::new(0., 0.),
                    LocalTransform {
                        scale: 1.,
                        ..*transform
                    },
                )
            };
            let support = |world: MmPoint| {
                aperture_support(shape, axis(world)).map(|value| value * transform.scale)
            };
            match (
                support(MmPoint::new(-1., 0.)),
                support(MmPoint::new(0., -1.)),
                support(MmPoint::new(1., 0.)),
                support(MmPoint::new(0., 1.)),
            ) {
                (Some(left), Some(bottom), Some(right), Some(top)) => Some(BoundsMm {
                    min_x_mm: center.x_mm - left,
                    min_y_mm: center.y_mm - bottom,
                    max_x_mm: center.x_mm + right,
                    max_y_mm: center.y_mm + top,
                }),
                _ => None, // An all-Clear macro has no object-local material.
            }
        }
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => Some(BoundsMm::points(*start, *end).expand(width_mm / 2., width_mm / 2.)),
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => Some(BoundsMm::points(*start, *end).expand(width_mm / 2., height_mm / 2.)),
        SemanticGeometry::Arc { path, width_mm } => {
            Some(arc_bounds(*path).expand(width_mm / 2., width_mm / 2.))
        }
        SemanticGeometry::Region { contours } => {
            let mut result = None;
            for contour in contours {
                let canonical = canonical_region_contour(contour)?;
                if let Some(bounds) = combine(canonical.edges.iter().map(|edge| match edge {
                    RegionEdge::Line { start, end } => BoundsMm::points(*start, *end),
                    RegionEdge::Arc(arc) => arc_bounds(*arc),
                })) {
                    result =
                        Some(result.map_or(bounds, |previous: BoundsMm| previous.union(bounds)));
                }
            }
            result
        }
    };
    bounds.map(BoundsMm::checked).transpose()
}

fn arc_bounds(arc: ArcGeometry) -> BoundsMm {
    if arc.zero_sweep() {
        return BoundsMm::points(arc.start, arc.start);
    }
    let edge = edge_bounds(&RegionEdge::Arc(arc.canonical_circle()));
    BoundsMm {
        min_x_mm: edge.min_x,
        min_y_mm: edge.min_y,
        max_x_mm: edge.max_x,
        max_y_mm: edge.max_y,
    }
    .union(BoundsMm::points(arc.start, arc.end))
}

fn dot(a: MmPoint, b: MmPoint) -> f64 {
    a.x_mm * b.x_mm + a.y_mm * b.y_mm
}

fn rectangle_support(width: f64, height: f64, direction: MmPoint) -> f64 {
    (width * direction.x_mm.abs() + height * direction.y_mm.abs()) / 2.
}

fn aperture_support(shape: &ApertureShape, direction: MmPoint) -> Option<f64> {
    Some(match shape {
        ApertureShape::Circle { diameter_mm, .. } => {
            diameter_mm / 2. * direction.x_mm.hypot(direction.y_mm)
        }
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            ..
        } => rectangle_support(*width_mm, *height_mm, direction),
        ApertureShape::Obround {
            width_mm,
            height_mm,
            ..
        } => {
            let along = if width_mm >= height_mm {
                direction.x_mm
            } else {
                direction.y_mm
            };
            (width_mm - height_mm).abs() / 2. * along.abs()
                + width_mm.min(*height_mm) / 2. * direction.x_mm.hypot(direction.y_mm)
        }
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            ..
        } => (0..*vertices)
            .map(|index| {
                let angle = rotation_deg.to_radians()
                    + std::f64::consts::TAU * f64::from(index) / f64::from(*vertices);
                dot(MmPoint::new(angle.cos(), angle.sin()), direction) * diameter_mm / 2.
            })
            .reduce(f64::max)?,
        ApertureShape::Macro { primitives } => primitives
            .iter()
            .filter(|primitive| primitive_exposure(primitive) == Exposure::Dark)
            .filter_map(|primitive| match primitive {
                MacroPrimitive::Circle {
                    diameter_mm,
                    center,
                    rotation_deg,
                    ..
                } => Some(
                    dot(rotate(*center, *rotation_deg), direction)
                        + diameter_mm / 2. * direction.x_mm.hypot(direction.y_mm),
                ),
                MacroPrimitive::CenterLine {
                    width_mm,
                    height_mm,
                    center,
                    rotation_deg,
                    ..
                } => Some(
                    dot(rotate(*center, *rotation_deg), direction)
                        + rectangle_support(
                            *width_mm,
                            *height_mm,
                            rotate(direction, -*rotation_deg),
                        ),
                ),
                MacroPrimitive::Outline {
                    points,
                    rotation_deg,
                    ..
                } => points
                    .iter()
                    .map(|point| dot(rotate(*point, *rotation_deg), direction))
                    .reduce(f64::max),
            })
            .reduce(f64::max)?,
    })
}
