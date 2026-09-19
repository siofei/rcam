//! Analytic independent-object material metrics. Never reads display geometry.
use crate::*;
use std::f64::consts::{PI, TAU};

pub const MAX_METRICS_WORK: usize = 2_000_000;
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeometryMetrics {
    pub area_mm2: f64,
    pub perimeter_mm: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricsError {
    Unsupported(&'static str),
    ResourceLimit,
}
use MetricsError::Unsupported;
fn checked(area_mm2: f64, perimeter_mm: f64) -> Result<GeometryMetrics, MetricsError> {
    if !area_mm2.is_finite() || !perimeter_mm.is_finite() || area_mm2 < 0. || perimeter_mm < 0. {
        return Err(Unsupported("non-finite or negative metrics"));
    }
    Ok(GeometryMetrics {
        area_mm2,
        perimeter_mm,
    })
}
fn positive(v: f64) -> Result<(), MetricsError> {
    if !v.is_finite() || v <= 0. {
        Err(Unsupported("invalid dimension or scale"))
    } else {
        Ok(())
    }
}
fn dot(radius: f64) -> Result<GeometryMetrics, MetricsError> {
    checked(PI * radius * radius, TAU * radius)
}

/// Work is shared by a service batch; callers can bound all analytic preparation.
pub fn calculate(
    document: &SemanticDocument,
    geometry: &SemanticGeometry,
    work: &mut usize,
) -> Result<GeometryMetrics, MetricsError> {
    charge(work, 1)?;
    match geometry {
        SemanticGeometry::Flash {
            aperture_id,
            transform,
            ..
        } => {
            positive(transform.scale)?;
            if !transform.rotation_deg.is_finite() {
                return Err(Unsupported("invalid rotation"));
            }
            let shape = &document
                .apertures
                .iter()
                .find(|a| &a.id == aperture_id)
                .ok_or(Unsupported("missing aperture"))?
                .shape;
            if matches!(shape, ApertureShape::Macro { .. }) {
                return Err(Unsupported(
                    "macro material union has no metrics boundary proof",
                ));
            }
            validate_aperture_shape(shape)
                .map_err(|_| Unsupported("invalid aperture or hole containment"))?;
            let (a, p, hole) = match *shape {
                ApertureShape::Circle {
                    diameter_mm: d,
                    hole_diameter_mm: h,
                } => (PI * (d / 2.).powi(2), PI * d, h),
                ApertureShape::Rectangle {
                    width_mm: w,
                    height_mm: h,
                    hole_diameter_mm: hole,
                } => (w * h, 2. * (w + h), hole),
                ApertureShape::Obround {
                    width_mm: w,
                    height_mm: h,
                    hole_diameter_mm: hole,
                } => {
                    let d = w.min(h);
                    let l = (w - h).abs();
                    (l * d + PI * (d / 2.).powi(2), 2. * l + PI * d, hole)
                }
                ApertureShape::Polygon {
                    diameter_mm: d,
                    vertices: n,
                    hole_diameter_mm: hole,
                    ..
                } => {
                    let n = f64::from(n);
                    (
                        n * d * d * (TAU / n).sin() / 8.,
                        n * d * (PI / n).sin(),
                        hole,
                    )
                }
                ApertureShape::Macro { .. } => unreachable!(),
            };
            let h = hole.unwrap_or(0.);
            checked(
                (a - PI * (h / 2.).powi(2)) * transform.scale.powi(2),
                (p + PI * h) * transform.scale,
            )
        }
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => {
            positive(*width_mm)?;
            let l = start.distance_mm(*end);
            let r = width_mm / 2.;
            checked(l * width_mm + PI * r * r, 2. * l + TAU * r)
        }
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => {
            positive(*width_mm)?;
            positive(*height_mm)?;
            if !start.is_valid_geometry() || !end.is_valid_geometry() {
                return Err(Unsupported("invalid sweep points"));
            }
            let points = hit_test::rectangular_sweep_points(*start, *end, *width_mm, *height_mm);
            let base = points[0];
            let (mut a, mut p) = (0., 0.);
            for (v, w) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                a += cross_local(base, *v, *w) / 2.;
                p += v.distance_mm(*w);
            }
            checked(a.abs(), p)
        }
        SemanticGeometry::Arc { path, width_mm } => {
            positive(*width_mm)?;
            if !path.is_valid() {
                return Err(Unsupported("invalid arc"));
            }
            let r = width_mm / 2.;
            if path.zero_sweep() {
                return dot(r);
            }
            // Real radial connectors are not part of the simple circular-tube formula.
            if path.arc_deviation() > path.numeric_tolerance() {
                return Err(Unsupported(
                    "arc deviation connectors require a union boundary",
                ));
            }
            let circle = path.canonical_circle();
            let radius = circle.radius();
            if path.full_circle {
                let outer = radius + r;
                let inner = (radius - r).max(0.);
                return checked(PI * (outer * outer - inner * inner), TAU * (outer + inner));
            }
            let angle = circle.sweep_radians().ok_or(Unsupported("invalid sweep"))?;
            // The tube is embedded only below the curvature radius and with a
            // gap wider than both endpoint disks for major arcs. Tangency fails closed.
            if r >= radius
                || (angle > PI
                    && circle.start.distance_mm(circle.end) <= 2. * r + path.numeric_tolerance())
            {
                return Err(Unsupported("arc offset or endpoint caps may overlap"));
            }
            checked(
                radius * angle * width_mm + PI * r * r,
                2. * radius * angle + TAU * r,
            )
        }
        SemanticGeometry::Region { contours } => region(contours, work),
    }
}
fn charge(work: &mut usize, amount: usize) -> Result<(), MetricsError> {
    *work = work.saturating_add(amount);
    if *work > MAX_METRICS_WORK {
        Err(MetricsError::ResourceLimit)
    } else {
        Ok(())
    }
}
fn cross_local(o: MmPoint, a: MmPoint, b: MmPoint) -> f64 {
    (a.x_mm - o.x_mm) * (b.y_mm - o.y_mm) - (a.y_mm - o.y_mm) * (b.x_mm - o.x_mm)
}
fn integral(edges: &[RegionEdge]) -> Result<(f64, f64), MetricsError> {
    let base = edge_start(&edges[0]);
    let (mut area, mut perimeter) = (0., 0.);
    for e in edges {
        area += cross_local(base, edge_start(e), edge_end(e)) / 2.;
        match e {
            RegionEdge::Line { start, end } => perimeter += start.distance_mm(*end),
            RegionEdge::Arc(a) => {
                let angle = a.sweep_radians().ok_or(Unsupported("invalid Region arc"))?;
                let sign = if a.direction == ArcDirection::CounterClockwise {
                    1.
                } else {
                    -1.
                };
                area += sign * a.radius().powi(2) * (angle - angle.sin()) / 2.;
                perimeter += a.radius() * angle;
            }
        }
    }
    if !area.is_finite() || area == 0. {
        return Err(Unsupported("degenerate Region loop"));
    }
    Ok((area, perimeter))
}
fn region(contours: &[RegionContour], work: &mut usize) -> Result<GeometryMetrics, MetricsError> {
    if contours.is_empty() {
        return Err(Unsupported("empty Region"));
    }
    let count: usize = contours.iter().map(|c| c.edges.len()).sum();
    charge(work, count.saturating_mul(count).saturating_mul(3))?;
    let mut all: Vec<Vec<Vec<RegionEdge>>> = Vec::new();
    for contour in contours {
        validate_contour(contour).map_err(|_| Unsupported("unproven Region topology"))?;
        let canonical = canonical_region_contour(contour)
            .map_err(|_| Unsupported("unproven canonical Region"))?;
        let edges = canonical.edges;
        // Reject approximate endpoint closure: no invisible extra boundary is invented.
        for (a, b) in edges
            .iter()
            .zip(edges.iter().cycle().skip(1))
            .take(edges.len())
        {
            if edge_end(a) != edge_start(b) {
                return Err(Unsupported("Region endpoints are not exactly connected"));
            }
        }
        let mut omitted = vec![false; edges.len()];
        for i in 0..edges.len() {
            for j in i + 1..edges.len() {
                if legal_cutin_pair(&edges[i], &edges[j]) {
                    if edge_start(&edges[i]) != edge_end(&edges[j])
                        || edge_end(&edges[i]) != edge_start(&edges[j])
                        || omitted[i]
                        || omitted[j]
                    {
                        return Err(Unsupported("ambiguous retraced Region connector"));
                    }
                    omitted[i] = true;
                    omitted[j] = true;
                }
            }
        }
        let mut loops = Vec::new();
        for first in 0..edges.len() {
            if omitted[first] {
                continue;
            }
            let mut cycle = Vec::new();
            let mut at = first;
            loop {
                omitted[at] = true;
                cycle.push(edges[at].clone());
                let end = edge_end(&edges[at]);
                if end == edge_start(&edges[first]) {
                    break;
                }
                let mut next =
                    (0..edges.len()).filter(|&k| !omitted[k] && edge_start(&edges[k]) == end);
                at = next.next().ok_or(Unsupported("open material boundary"))?;
                if next.next().is_some() {
                    return Err(Unsupported("branched material boundary"));
                }
            }
            loops.push(cycle);
        }
        // Proven disjoint simple boundaries; retained seams cannot be counted as perimeter.
        for i in 0..loops.len() {
            for j in i + 1..loops.len() {
                for a in &loops[i] {
                    for b in &loops[j] {
                        if !edge_intersection_points(a, b).is_empty() {
                            return Err(Unsupported("touching Region loops"));
                        }
                    }
                }
            }
        }
        all.push(loops);
    }
    // Multiple contours are unioned by existing manufacturing semantics. Only
    // disjoint envelopes are supported here; never blindly sum overlapping solids.
    for i in 0..all.len() {
        for j in i + 1..all.len() {
            for a in all[i].iter().flatten() {
                for b in all[j].iter().flatten() {
                    if bounds_overlap(edge_bounds(a), edge_bounds(b)) {
                        return Err(Unsupported("multiple Region contours need union proof"));
                    }
                }
            }
            let a = &contours[i];
            let b = &contours[j];
            if contour_covers(a, edge_start(&b.edges[0]))
                || contour_covers(b, edge_start(&a.edges[0]))
            {
                return Err(Unsupported("nested Region contours need union proof"));
            }
        }
    }
    let (mut area, mut perimeter) = (0., 0.);
    for loops in all {
        for (i, edges) in loops.iter().enumerate() {
            let (signed, p) = integral(edges)?;
            let point = edge_start(&edges[0]);
            let outside: i32 = loops
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .flat_map(|(_, e)| e)
                .map(|e| edge_ray_winding(e, point))
                .sum();
            let inside = outside + if signed > 0. { 1 } else { -1 };
            if outside == 0 {
                area += signed.abs();
                perimeter += p;
            } else if inside == 0 {
                area -= signed.abs();
                perimeter += p;
            }
        }
    }
    checked(area, perimeter)
}
