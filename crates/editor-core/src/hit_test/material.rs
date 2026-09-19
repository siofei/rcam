//! Ordered macro material boundaries, split at analytic intersections.
//! No tessellation, raster sampling, or inflated primitive boolean operations.
use super::*;
use std::f64::consts::TAU;

#[derive(Clone, Copy)]
pub(super) enum Curve {
    Line(MmPoint, MmPoint),
    Circle(MmPoint, f64),
}
impl Curve {
    fn at(self, t: f64) -> MmPoint {
        match self {
            Self::Line(a, b) => MmPoint::new(
                a.x_mm + (b.x_mm - a.x_mm) * t,
                a.y_mm + (b.y_mm - a.y_mm) * t,
            ),
            Self::Circle(c, r) => {
                MmPoint::new(c.x_mm + r * (t * TAU).cos(), c.y_mm + r * (t * TAU).sin())
            }
        }
    }
    fn parameter(self, p: MmPoint) -> f64 {
        match self {
            Self::Line(a, b) => {
                let v = b.sub(a);
                let q = p.sub(a);
                if v.x_mm.abs() >= v.y_mm.abs() {
                    q.x_mm / v.x_mm
                } else {
                    q.y_mm / v.y_mm
                }
            }
            Self::Circle(c, _) => (p.y_mm - c.y_mm).atan2(p.x_mm - c.x_mm).rem_euclid(TAU) / TAU,
        }
    }
    fn left(self, p: MmPoint) -> MmPoint {
        match self {
            Self::Line(a, b) => {
                let v = b.sub(a);
                let l = v.x_mm.hypot(v.y_mm);
                MmPoint::new(-v.y_mm / l, v.x_mm / l)
            }
            Self::Circle(c, _) => {
                let v = c.sub(p);
                let l = v.x_mm.hypot(v.y_mm);
                MmPoint::new(v.x_mm / l, v.y_mm / l)
            }
        }
    }
    fn edge(self, a: f64, b: f64) -> RegionEdge {
        match self {
            Self::Line(..) => RegionEdge::Line {
                start: self.at(a),
                end: self.at(b),
            },
            Self::Circle(c, _) => RegionEdge::Arc(ArcGeometry {
                start: self.at(a),
                end: if a == 0. && b == 1. {
                    self.at(a)
                } else {
                    self.at(b)
                },
                center: c,
                direction: ArcDirection::CounterClockwise,
                full_circle: a == 0. && b == 1.,
                source: None,
            }),
        }
    }
    fn distance(self, p: MmPoint) -> f64 {
        match self {
            Self::Line(a, b) => segment_distance(p, a, b),
            Self::Circle(c, r) => (p.distance_mm(c) - r).abs(),
        }
    }
    fn coincident_at(self, other: Self, p: MmPoint) -> bool {
        match (self, other) {
            (Self::Circle(a, r), Self::Circle(b, s)) => a == b && r == s,
            (Self::Line(a, b), Self::Line(c, d)) => {
                cross(a, b, c) == 0.
                    && cross(a, b, d) == 0.
                    && (0.0..=1.0).contains(&other.parameter(p))
            }
            _ => false,
        }
    }
}
struct Boundary {
    curve: Curve,
    inside_left: bool,
    owner: usize,
}
enum Shape {
    Circle(MmPoint, f64),
    Polygon(Vec<MmPoint>),
}
impl Shape {
    fn contains(&self, p: MmPoint) -> bool {
        match self {
            Self::Circle(c, r) => p.distance_mm(*c) <= *r,
            Self::Polygon(points) => polygon_points_covers(points, p),
        }
    }
}
struct Primitive {
    shape: Shape,
    exposure: Exposure,
    edges: std::ops::Range<usize>,
}
pub(super) struct Material {
    primitives: Vec<Primitive>,
    pub(super) boundary: Vec<RegionEdge>,
    source_edges: Vec<Curve>,
    precision: f64,
}
impl Material {
    pub(super) fn prepare(
        input: &[MacroPrimitive],
        budget: &mut Budget,
    ) -> Result<Self, HitTestError> {
        budget.charge(input.len())?;
        let mut primitives = Vec::new();
        let mut edges = Vec::new();
        let mut magnitude = 0_f64;
        for (owner, primitive) in input.iter().enumerate() {
            let start = edges.len();
            let shape = match primitive {
                MacroPrimitive::Circle {
                    center,
                    diameter_mm,
                    rotation_deg,
                    ..
                } => Shape::Circle(rotate(*center, *rotation_deg), diameter_mm / 2.),
                MacroPrimitive::CenterLine {
                    center,
                    width_mm,
                    height_mm,
                    rotation_deg,
                    ..
                } => Shape::Polygon(
                    rectangle_points(*width_mm, *height_mm)
                        .into_iter()
                        .map(|p| {
                            rotate(
                                MmPoint::new(p.x_mm + center.x_mm, p.y_mm + center.y_mm),
                                *rotation_deg,
                            )
                        })
                        .collect(),
                ),
                MacroPrimitive::Outline {
                    points,
                    rotation_deg,
                    ..
                } => {
                    budget.charge(points.len())?;
                    Shape::Polygon(points.iter().map(|p| rotate(*p, *rotation_deg)).collect())
                }
            };
            match &shape {
                Shape::Circle(c, r) => {
                    magnitude = magnitude.max(c.x_mm.abs()).max(c.y_mm.abs()).max(*r);
                    edges.push(Boundary {
                        curve: Curve::Circle(*c, *r),
                        inside_left: true,
                        owner,
                    });
                }
                Shape::Polygon(points) => {
                    let base = points
                        .first()
                        .ok_or(HitTestError::Unsupported("empty macro outline"))?;
                    let area: f64 = points
                        .iter()
                        .zip(points.iter().cycle().skip(1))
                        .take(points.len())
                        .map(|(a, b)| cross(*base, *a, *b))
                        .sum();
                    if !area.is_finite() || area == 0. {
                        return Err(HitTestError::Unsupported("degenerate macro boundary"));
                    }
                    for (a, b) in points
                        .iter()
                        .zip(points.iter().cycle().skip(1))
                        .take(points.len())
                    {
                        magnitude = magnitude.max(a.x_mm.abs()).max(a.y_mm.abs());
                        if a != b {
                            edges.push(Boundary {
                                curve: Curve::Line(*a, *b),
                                inside_left: area > 0.,
                                owner,
                            });
                        }
                    }
                }
            }
            primitives.push(Primitive {
                shape,
                exposure: primitive_exposure(primitive),
                edges: start..edges.len(),
            });
        }
        let precision = roundoff(&[magnitude]);
        if !precision.is_finite() {
            return Err(HitTestError::Unsupported("non-finite macro extent"));
        }
        // ponytail: bounded quadratic arrangement only for one query's unique
        // macros; persistent spatial acceleration requires measured demand.
        budget.charge(edges.len().saturating_mul(edges.len()))?;
        let mut cuts = vec![vec![0., 1.]; edges.len()];
        for i in 0..edges.len() {
            for j in i + 1..edges.len() {
                if edges[i].owner == edges[j].owner {
                    continue;
                }
                for p in intersections(edges[i].curve, edges[j].curve, precision)? {
                    for k in [i, j] {
                        let t = edges[k].curve.parameter(p);
                        if (0.0..=1.0).contains(&t) {
                            cuts[k].push(t);
                        }
                    }
                }
            }
        }
        let mut boundary = Vec::new();
        for (i, edge) in edges.iter().enumerate() {
            cuts[i].sort_by(f64::total_cmp);
            cuts[i].dedup();
            for interval in cuts[i].windows(2) {
                let (a, b) = (interval[0], interval[1]);
                if a == b {
                    continue;
                }
                let middle = (a + b) / 2.;
                let point = edge.curve.at(middle);
                if middle == a || middle == b {
                    return Err(HitTestError::Unsupported(
                        "unresolvable macro intersection interval",
                    ));
                }
                let normal = edge.curve.left(point);
                let mut sides = [false, false];
                for (owner, primitive) in primitives.iter().enumerate() {
                    budget.charge(primitive.edges.len() + 1)?;
                    let on = if owner == edge.owner {
                        Some(edge)
                    } else {
                        edges[primitive.edges.clone()]
                            .iter()
                            .find(|other| edge.curve.coincident_at(other.curve, point))
                    };
                    let inside = if let Some(other) = on {
                        let n = other.curve.left(point);
                        let alignment = normal.x_mm * n.x_mm + normal.y_mm * n.y_mm;
                        if alignment.abs() < 0.5 {
                            return Err(HitTestError::Unsupported(
                                "ambiguous coincident macro boundary",
                            ));
                        }
                        let positive = (alignment > 0.) == other.inside_left;
                        [positive, !positive]
                    } else {
                        if edges[primitive.edges.clone()]
                            .iter()
                            .any(|b| b.curve.distance(point) <= precision)
                        {
                            return Err(HitTestError::Unsupported(
                                "macro boundaries closer than numerical resolution",
                            ));
                        }
                        let inside = primitive.shape.contains(point);
                        [inside, inside]
                    };
                    for k in 0..2 {
                        if inside[k] {
                            sides[k] = primitive.exposure == Exposure::Dark;
                        }
                    }
                }
                // Internal edges with material on both sides are also safe
                // distance witnesses; erased seams with neither side are not.
                if sides[0] || sides[1] {
                    boundary.push(edge.curve.edge(a, b));
                }
            }
        }
        Ok(Self {
            primitives,
            boundary,
            source_edges: edges.iter().map(|e| e.curve).collect(),
            precision,
        })
    }
    pub(super) fn distance(
        &self,
        p: MmPoint,
        budget: &mut Budget,
    ) -> Result<(f64, f64), HitTestError> {
        let mut inside = false;
        for primitive in &self.primitives {
            budget.charge(primitive.edges.len() + 1)?;
            if primitive.shape.contains(p) {
                inside = primitive.exposure == Exposure::Dark;
            }
        }
        budget.charge(self.boundary.len())?;
        budget.charge(self.source_edges.len())?;
        let near_source_boundary = self
            .source_edges
            .iter()
            .any(|e| e.distance(p) <= self.precision);
        let distance = if inside && !near_source_boundary {
            0.
        } else {
            self.boundary
                .iter()
                .map(|e| edge_distance(p, e))
                .fold(f64::INFINITY, f64::min)
        };
        Ok((distance, self.precision + roundoff(&[p.x_mm, p.y_mm])))
    }
}

pub(super) fn intersections(
    a: Curve,
    b: Curve,
    precision: f64,
) -> Result<Vec<MmPoint>, HitTestError> {
    let mut points = Vec::new();
    match (a, b) {
        (Curve::Line(p, q), Curve::Line(r, s)) => {
            let v = q.sub(p);
            let w = s.sub(r);
            let z = r.sub(p);
            let denominator = v.x_mm * w.y_mm - v.y_mm * w.x_mm;
            if denominator == 0. {
                if cross(p, q, r) == 0. {
                    for point in [p, q, r, s] {
                        if (0.0..=1.0).contains(&a.parameter(point))
                            && (0.0..=1.0).contains(&b.parameter(point))
                        {
                            points.push(point);
                        }
                    }
                }
            } else {
                let t = (z.x_mm * w.y_mm - z.y_mm * w.x_mm) / denominator;
                let u = (z.x_mm * v.y_mm - z.y_mm * v.x_mm) / denominator;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                    points.push(a.at(t));
                }
            }
        }
        (Curve::Line(p, q), Curve::Circle(c, r)) | (Curve::Circle(c, r), Curve::Line(p, q)) => {
            let v = q.sub(p);
            let length = v.x_mm.hypot(v.y_mm);
            let u = MmPoint::new(v.x_mm / length, v.y_mm / length);
            let z = c.sub(p);
            let along = z.x_mm * u.x_mm + z.y_mm * u.y_mm;
            let perpendicular = z.x_mm * u.y_mm - z.y_mm * u.x_mm;
            let difference = r - perpendicular.abs();
            if difference < 0. {
                if difference.abs() <= precision {
                    return Err(HitTestError::Unsupported("uncertain line/circle tangency"));
                }
            } else {
                let h = (difference * (r + perpendicular.abs())).sqrt();
                for t in [along - h, along + h] {
                    if (0.0..=length).contains(&t) {
                        points.push(MmPoint::new(p.x_mm + u.x_mm * t, p.y_mm + u.y_mm * t));
                    }
                }
            }
        }
        (Curve::Circle(c, r), Curve::Circle(d, s)) => {
            let distance = c.distance_mm(d);
            if c == d {
                return Ok(points);
            }
            let outer = distance - (r + s);
            let inner = (r - s).abs() - distance;
            if outer > 0. || inner > 0. {
                if (outer > 0. && outer <= precision) || (inner > 0. && inner <= precision) {
                    return Err(HitTestError::Unsupported(
                        "uncertain circle/circle tangency",
                    ));
                }
                return Ok(points);
            }
            let along = ((r - s) * (r + s) + distance * distance) / (2. * distance);
            let h2 = (r - along) * (r + along);
            if h2 < 0. {
                return Err(HitTestError::Unsupported(
                    "uncertain macro circle intersection",
                ));
            }
            let h = h2.sqrt();
            let v = d.sub(c);
            let u = MmPoint::new(v.x_mm / distance, v.y_mm / distance);
            for sign in [-1., 1.] {
                points.push(MmPoint::new(
                    c.x_mm + along * u.x_mm - sign * h * u.y_mm,
                    c.y_mm + along * u.y_mm + sign * h * u.x_mm,
                ));
            }
        }
    }
    Ok(points)
}
