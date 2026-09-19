//! Exact object-material queries; no display data or workspace state.
use crate::*;
use std::collections::HashMap;

mod material;
mod select_rect;
pub use select_rect::SelectRectMode;
pub const MAX_HIT_TEST_WORK: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum HitTestError {
    InvalidArgument(&'static str),
    MissingLayer(String),
    Geometry(SemanticError),
    Unsupported(&'static str),
    ResourceLimit { limit: usize, attempted: usize },
}
impl std::fmt::Display for HitTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for HitTestError {}
impl From<SemanticError> for HitTestError {
    fn from(e: SemanticError) -> Self {
        Self::Geometry(e)
    }
}

struct Budget(usize);
impl Budget {
    fn charge(&mut self, n: usize) -> Result<(), HitTestError> {
        self.0 = self.0.checked_sub(n).ok_or(HitTestError::ResourceLimit {
            limit: MAX_HIT_TEST_WORK,
            attempted: (MAX_HIT_TEST_WORK - self.0).saturating_add(n),
        })?;
        Ok(())
    }
}

pub fn validate_hit_point(point: MmPoint, tolerance: f64) -> Result<(), HitTestError> {
    if !point.is_valid_geometry() {
        return Err(HitTestError::InvalidArgument("point"));
    }
    if !tolerance.is_finite() || !(0.0..=MAX_GEOMETRY_MM).contains(&tolerance) {
        return Err(HitTestError::InvalidArgument("tolerance_mm"));
    }
    Ok(())
}

impl SemanticDocument {
    /// The service supplies validated manufacturing geometry. A query either
    /// returns all matching IDs in exposure order, or one error (no partial IDs).
    pub fn hit_test(
        &self,
        layer_id: &str,
        point: MmPoint,
        tolerance: f64,
    ) -> Result<Vec<String>, HitTestError> {
        validate_hit_point(point, tolerance)?;
        if layer_id.trim().is_empty() {
            return Err(HitTestError::InvalidArgument("layer_id"));
        }
        let layer = self
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or_else(|| HitTestError::MissingLayer(layer_id.into()))?;
        let apertures: HashMap<_, _> = self
            .apertures
            .iter()
            .map(|a| (a.id.as_str(), &a.shape))
            .collect();
        let mut macros = HashMap::new();
        let mut budget = Budget(MAX_HIT_TEST_WORK);
        let mut result = Vec::new();
        for object in &layer.objects {
            budget.charge(1)?;
            let (distance, uncertainty) = match &object.geometry {
                SemanticGeometry::Flash {
                    center,
                    aperture_id,
                    transform,
                } => {
                    let shape = apertures
                        .get(aperture_id.as_str())
                        .ok_or_else(|| SemanticError::MissingAperture(aperture_id.clone()))?;
                    let local = apply_inverse_transform(point, *center, *transform);
                    if !local.is_finite() || !transform.scale.is_finite() || transform.scale <= 0. {
                        return Err(HitTestError::Unsupported(
                            "unrepresentable inverse transform",
                        ));
                    }
                    let (d, local_error) = if let ApertureShape::Macro { primitives } = shape {
                        if !macros.contains_key(aperture_id.as_str()) {
                            macros.insert(
                                aperture_id.as_str(),
                                material::Material::prepare(primitives, &mut budget)?,
                            );
                        }
                        macros[aperture_id.as_str()].distance(local, &mut budget)?
                    } else {
                        (
                            aperture_distance(shape, local, &mut budget)?,
                            roundoff(&[local.x_mm, local.y_mm]),
                        )
                    };
                    (
                        d * transform.scale,
                        local_error * transform.scale
                            + if point == *center {
                                0.
                            } else {
                                roundoff(&[point.x_mm, point.y_mm, center.x_mm, center.y_mm])
                            },
                    )
                }
                geometry => (
                    geometry_distance(geometry, point, &mut budget)?,
                    geometry_roundoff(geometry, point),
                ),
            };
            if !uncertainty.is_finite() || uncertainty > EPSILON_MM || distance.is_nan() {
                return Err(HitTestError::Unsupported(
                    "hit-test numerical uncertainty exceeds manufacturing tolerance",
                ));
            }
            if distance <= tolerance + uncertainty {
                result.push(object.object_id.clone());
            }
        }
        Ok(result)
    }
}

fn roundoff(values: &[f64]) -> f64 {
    128. * f64::EPSILON * values.iter().map(|v| v.abs()).fold(0., f64::max)
}
fn geometry_roundoff(g: &SemanticGeometry, p: MmPoint) -> f64 {
    let mut scale = p.x_mm.abs().max(p.y_mm.abs());
    let mut add = |p: MmPoint| {
        scale = scale.max(p.x_mm.abs()).max(p.y_mm.abs());
    };
    match g {
        SemanticGeometry::Line { start, end, .. }
        | SemanticGeometry::RectangularSweep { start, end, .. } => {
            add(*start);
            add(*end);
        }
        SemanticGeometry::Arc { path, .. } => {
            add(path.start);
            add(path.end);
            add(path.center);
        }
        SemanticGeometry::Region { contours } => {
            for e in contours.iter().flat_map(|c| &c.edges) {
                add(edge_start(e));
                add(edge_end(e));
                if let RegionEdge::Arc(a) = e {
                    add(a.center);
                }
            }
        }
        SemanticGeometry::Flash { center, .. } => add(*center),
    }
    roundoff(&[scale])
}

// Normalizing with hypot avoids a fixed squared-length clamp, including for
// sub-micron segments. Only exactly equal endpoints reduce to a dot.
fn segment_distance(p: MmPoint, a: MmPoint, b: MmPoint) -> f64 {
    let v = b.sub(a);
    let length = v.x_mm.hypot(v.y_mm);
    if length == 0. {
        return p.distance_mm(a);
    }
    let u = MmPoint::new(v.x_mm / length, v.y_mm / length);
    let q = p.sub(a);
    let along = (q.x_mm * u.x_mm + q.y_mm * u.y_mm).clamp(0., length);
    (q.x_mm - along * u.x_mm).hypot(q.y_mm - along * u.y_mm)
}
fn circle_arc_distance(p: MmPoint, arc: ArcGeometry) -> f64 {
    if arc.zero_sweep() {
        return p.distance_mm(arc.start);
    }
    let angle = (p.y_mm - arc.center.y_mm).atan2(p.x_mm - arc.center.x_mm);
    if arc.full_circle || arc_parameter(arc, angle).is_some() {
        (p.distance_mm(arc.center) - arc.radius()).abs()
    } else {
        p.distance_mm(arc.start).min(p.distance_mm(arc.end))
    }
}
fn edge_distance(p: MmPoint, edge: &RegionEdge) -> f64 {
    match edge {
        RegionEdge::Line { start, end } => segment_distance(p, *start, *end),
        RegionEdge::Arc(a) => circle_arc_distance(p, *a),
    }
}
fn polygon_distance(points: &[MmPoint], p: MmPoint) -> f64 {
    if polygon_points_covers(points, p) {
        return 0.;
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| segment_distance(p, *a, *b))
        .fold(f64::INFINITY, f64::min)
}
fn rectangle_points(w: f64, h: f64) -> Vec<MmPoint> {
    vec![
        MmPoint::new(-w / 2., -h / 2.),
        MmPoint::new(w / 2., -h / 2.),
        MmPoint::new(w / 2., h / 2.),
        MmPoint::new(-w / 2., h / 2.),
    ]
}
fn aperture_distance(
    shape: &ApertureShape,
    p: MmPoint,
    budget: &mut Budget,
) -> Result<f64, HitTestError> {
    let (outer, hole) = match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => (
            (p.x_mm.hypot(p.y_mm) - diameter_mm / 2.).max(0.),
            *hole_diameter_mm,
        ),
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => (
            (p.x_mm.abs() - width_mm / 2.)
                .max(0.)
                .hypot((p.y_mm.abs() - height_mm / 2.).max(0.)),
            *hole_diameter_mm,
        ),
        ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            let (long, short, along) = if width_mm >= height_mm {
                (*width_mm, *height_mm, p)
            } else {
                (*height_mm, *width_mm, MmPoint::new(p.y_mm, p.x_mm))
            };
            (
                ((along.x_mm.abs() - (long - short) / 2.)
                    .max(0.)
                    .hypot(along.y_mm)
                    - short / 2.)
                    .max(0.),
                *hole_diameter_mm,
            )
        }
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            budget.charge(usize::from(*vertices))?;
            let points: Vec<_> = (0..*vertices)
                .map(|i| {
                    let a = std::f64::consts::TAU * f64::from(i) / f64::from(*vertices)
                        + rotation_deg.to_radians();
                    MmPoint::new(a.cos() * diameter_mm / 2., a.sin() * diameter_mm / 2.)
                })
                .collect();
            (polygon_distance(&points, p), *hole_diameter_mm)
        }
        ApertureShape::Macro { .. } => unreachable!("prepared separately"),
    };
    Ok(outer.max(hole.map_or(0., |d| (d / 2. - p.x_mm.hypot(p.y_mm)).max(0.))))
}

pub(crate) fn rectangular_sweep_points(
    start: MmPoint,
    end: MmPoint,
    width_mm: f64,
    height_mm: f64,
) -> Vec<MmPoint> {
    let mut points: Vec<_> = [start, end]
        .into_iter()
        .flat_map(|center| {
            rectangle_points(width_mm, height_mm)
                .into_iter()
                .map(move |p| MmPoint::new(p.x_mm + center.x_mm, p.y_mm + center.y_mm))
        })
        .collect();
    points.sort_by(|a, b| a.x_mm.total_cmp(&b.x_mm).then(a.y_mm.total_cmp(&b.y_mm)));
    points.dedup();
    let mut hull: Vec<MmPoint> = Vec::new();
    for point in &points {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], *point) <= 0. {
            hull.pop();
        }
        hull.push(*point);
    }
    let lower = hull.len();
    for point in points.iter().rev().skip(1) {
        while hull.len() > lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], *point) <= 0.
        {
            hull.pop();
        }
        hull.push(*point);
    }
    hull.pop();
    hull
}

fn geometry_distance(
    g: &SemanticGeometry,
    p: MmPoint,
    budget: &mut Budget,
) -> Result<f64, HitTestError> {
    Ok(match g {
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => (segment_distance(p, *start, *end) - width_mm / 2.).max(0.),
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => polygon_distance(
            &rectangular_sweep_points(*start, *end, *width_mm, *height_mm),
            p,
        ),
        SemanticGeometry::Arc { path, width_mm } => {
            if !path.is_valid() {
                return Err(HitTestError::Unsupported("invalid manufacturing arc"));
            }
            let d = if path.zero_sweep() {
                p.distance_mm(path.start)
            } else {
                let c = path.canonical_circle();
                circle_arc_distance(p, c)
                    .min(segment_distance(p, path.start, c.start))
                    .min(segment_distance(p, c.end, path.end))
            };
            (d - width_mm / 2.).max(0.)
        }
        SemanticGeometry::Region { contours } => {
            let mut distance = f64::INFINITY;
            for contour in contours {
                budget.charge(contour.edges.len())?;
                let c = canonical_region_contour(contour)?;
                // Frozen Gerber contours are independently filled; holes are
                // cut-ins within a contour, not sibling SVG subtraction.
                if c.edges
                    .iter()
                    .map(|edge| edge_ray_winding(edge, p))
                    .sum::<i32>()
                    != 0
                {
                    return Ok(0.);
                }
                for edge in &c.edges {
                    distance = distance.min(edge_distance(p, edge));
                }
            }
            distance
        }
        SemanticGeometry::Flash { .. } => unreachable!("handled with aperture lookup"),
    })
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn reports_attempted_work_and_preserves_remaining_budget_on_failure() {
        let mut budget = Budget(MAX_HIT_TEST_WORK);
        budget.charge(123).unwrap();
        assert_eq!(
            budget.charge(MAX_HIT_TEST_WORK),
            Err(HitTestError::ResourceLimit {
                limit: MAX_HIT_TEST_WORK,
                attempted: MAX_HIT_TEST_WORK + 123,
            })
        );
        assert_eq!(budget.0, MAX_HIT_TEST_WORK - 123);
    }
}
