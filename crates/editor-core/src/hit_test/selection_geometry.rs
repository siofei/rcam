//! Selection-only, ordered material composition. True line/arc Green integrals.
//! No renderer data, tessellation, model mutation or independent-object sums.
use super::*;
#[cfg(test)]
mod budget_tests;
mod intersection_bounds;
use crate::bounds::{BoundsMm, geometries_bounds};
use crate::edit::SelectionGroup;
use std::f64::consts::TAU;
use std::time::{Duration, Instant};

pub const MAX_SELECTED: usize = 10_000;
pub const MAX_PRIMITIVES: usize = 100_000;
pub const MAX_EDGES: usize = 1_000_000;
pub const MAX_WORK: usize = 2_000_000;
pub const MAX_TEMP_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CompositeMaterial {
    Ready {
        area_mm2: f64,
        perimeter_mm: f64,
        first_moment_x_mm3: f64,
        first_moment_y_mm3: f64,
        centroid_mm: MmPoint,
        centroid_error_mm: f64,
        area_error_mm2: f64,
        perimeter_error_mm: f64,
    },
    ZeroArea,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionGeometry {
    pub bounds_mm: Option<BoundsMm>,
    pub bounding_center_mm: Option<MmPoint>,
    pub material: CompositeMaterial,
    pub selected_count: usize,
    pub expanded_count: usize,
    pub work: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "reason", rename_all = "snake_case")]
pub enum QueryError {
    InvalidArgument(String),
    UnsupportedGeometry(String),
    PrecisionUncertain(String),
    NumericOverflow,
    ResourceLimit,
    Cancelled,
}
fn geometry_error(e: impl std::fmt::Debug) -> QueryError {
    QueryError::UnsupportedGeometry(format!("{e:?}"))
}
fn uncertain(reason: &str) -> QueryError {
    QueryError::PrecisionUncertain(reason.into())
}
/// Conservative source admission BEFORE cloning or expanding large geometry.
/// The estimate reserves geometry, canonical curves, cuts and arrangement data.
pub fn admission_cost(
    document: &SemanticDocument,
    geometry: &SemanticGeometry,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(usize, usize), QueryError> {
    fn region(contours: &[RegionContour]) -> usize {
        contours
            .iter()
            .fold(0usize, |n, c| n.saturating_add(c.edges.len()))
    }
    fn aperture(document: &SemanticDocument, id: &str) -> Result<usize, QueryError> {
        let shape = &document
            .apertures
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| geometry_error("missing aperture"))?
            .shape;
        Ok(match shape {
            ApertureShape::Circle { .. } => 2,
            ApertureShape::Rectangle { .. } => 5,
            ApertureShape::Obround { .. } => 5,
            ApertureShape::Polygon { vertices, .. } => usize::from(*vertices) + 1,
            ApertureShape::Macro { primitives } => primitives.iter().fold(0usize, |n, p| {
                n.saturating_add(match p {
                    MacroPrimitive::Circle { .. } => 1,
                    MacroPrimitive::CenterLine { .. } => 4,
                    MacroPrimitive::Outline { points, .. } => points.len(),
                })
            }),
        })
    }
    if cancelled() {
        return Err(QueryError::Cancelled);
    }
    let edges = match geometry {
        SemanticGeometry::Region { contours } => region(contours),
        SemanticGeometry::Flash { aperture_id, .. } => aperture(document, aperture_id)?,
        SemanticGeometry::Line { .. } => 4,
        SemanticGeometry::RectangularSweep { .. } => 8,
        SemanticGeometry::Arc { .. } => 16,
        SemanticGeometry::BlockInstance { definition_id, .. } => {
            let definition = document
                .block_definition(definition_id)
                .ok_or_else(|| geometry_error("unknown block"))?;
            if definition.objects.len() > MAX_PRIMITIVES {
                return Err(QueryError::ResourceLimit);
            }
            let mut edges = 0usize;
            for child in &definition.objects {
                if cancelled() {
                    return Err(QueryError::Cancelled);
                }
                edges = edges.saturating_add(match &child.geometry {
                    block::BlockObjectGeometry::Region { contours } => region(contours),
                    block::BlockObjectGeometry::Flash { aperture_id, .. } => {
                        aperture(document, aperture_id)?
                    }
                    block::BlockObjectGeometry::Line { .. } => 4,
                    block::BlockObjectGeometry::RectangularSweep { .. } => 8,
                    block::BlockObjectGeometry::Arc { .. } => 16,
                });
                if edges.saturating_mul(1024) > MAX_TEMP_BYTES {
                    return Err(QueryError::ResourceLimit);
                }
            }
            return Ok((definition.objects.len(), edges));
        }
    };
    if edges.saturating_mul(1024) > MAX_TEMP_BYTES {
        return Err(QueryError::ResourceLimit);
    }
    Ok((1, edges))
}

struct Work<'a> {
    count: usize,
    deadline: Instant,
    cancelled: &'a mut dyn FnMut() -> bool,
}
impl Work<'_> {
    fn charge(&mut self, n: usize) -> Result<(), QueryError> {
        if (self.cancelled)() {
            return Err(QueryError::Cancelled);
        }
        self.count = self.count.saturating_add(n);
        if self.count > MAX_WORK || Instant::now() >= self.deadline {
            return Err(QueryError::ResourceLimit);
        }
        Ok(())
    }
}
/// Preserve circle construction before center+radius and world transforms round.
/// Equal generated arcs do not prove equal source material (e.g. a thin annulus
/// translated to 1e6 mm). Different constructions are rejected if they alias.
#[derive(Clone, Copy, PartialEq)]
struct CircleSource {
    center: MmPoint,
    radius: f64,
    rotation_deg: f64,
    world_center: MmPoint,
    transform: LocalTransform,
}
struct Shape {
    geometry: SemanticGeometry,
    exposure: Exposure,
    edges: Vec<RegionEdge>,
    bounds: BoundsMm,
    endpoint_errors: Vec<(f64, f64)>,
    boundary_error_mm: f64,
    circle_sources: Vec<Option<CircleSource>>,
}
fn same_edges(a: &[RegionEdge], b: &[RegionEdge], w: &mut Work<'_>) -> Result<bool, QueryError> {
    w.charge(1)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        w.charge(1)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}
fn same_macro_material(
    a: &MacroPrimitive,
    b: &MacroPrimitive,
    w: &mut Work<'_>,
) -> Result<bool, QueryError> {
    w.charge(1)?;
    Ok(match (a, b) {
        (
            MacroPrimitive::Circle {
                center: a,
                diameter_mm: da,
                rotation_deg: ra,
                ..
            },
            MacroPrimitive::Circle {
                center: b,
                diameter_mm: db,
                rotation_deg: rb,
                ..
            },
        ) => a == b && da == db && ra == rb,
        (
            MacroPrimitive::CenterLine {
                center: a,
                width_mm: wa,
                height_mm: ha,
                rotation_deg: ra,
                ..
            },
            MacroPrimitive::CenterLine {
                center: b,
                width_mm: wb,
                height_mm: hb,
                rotation_deg: rb,
                ..
            },
        ) => a == b && wa == wb && ha == hb && ra == rb,
        (
            MacroPrimitive::Outline {
                points: a,
                rotation_deg: ra,
                ..
            },
            MacroPrimitive::Outline {
                points: b,
                rotation_deg: rb,
                ..
            },
        ) => {
            if ra != rb || a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                w.charge(1)?;
                if a != b {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}
fn same_aperture(
    a: &ApertureShape,
    b: &ApertureShape,
    w: &mut Work<'_>,
) -> Result<bool, QueryError> {
    w.charge(1)?;
    if let (ApertureShape::Macro { primitives: a }, ApertureShape::Macro { primitives: b }) = (a, b)
    {
        if a.len() != b.len() {
            return Ok(false);
        }
        for (a, b) in a.iter().zip(b) {
            w.charge(1)?;
            let exposure = |p: &MacroPrimitive| match p {
                MacroPrimitive::Circle { exposure, .. }
                | MacroPrimitive::CenterLine { exposure, .. }
                | MacroPrimitive::Outline { exposure, .. } => *exposure,
            };
            if exposure(a) != exposure(b) || !same_macro_material(a, b, w)? {
                return Ok(false);
            }
        }
        Ok(true)
    } else {
        Ok(a == b)
    }
}
/// Macro membership is local ordered exposure, unlike layer-level Block expansion.
fn covers(
    g: &SemanticGeometry,
    p: MmPoint,
    apertures: &HashMap<&str, &ApertureShape>,
    w: &mut Work<'_>,
) -> Result<bool, QueryError> {
    w.charge(1)?;
    if let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = g
    {
        let local = apply_inverse_transform(p, *center, *transform);
        let shape = apertures
            .get(aperture_id.as_str())
            .ok_or_else(|| uncertain("missing aperture"))?;
        if let ApertureShape::Macro { primitives } = shape {
            let mut inside = false;
            for primitive in primitives {
                w.charge(1)?;
                let (hit, exposure) = match primitive {
                    MacroPrimitive::Circle {
                        center,
                        diameter_mm,
                        rotation_deg,
                        exposure,
                    } => (
                        local.distance_mm(rotate(*center, *rotation_deg)) < diameter_mm / 2.,
                        *exposure,
                    ),
                    MacroPrimitive::CenterLine {
                        center,
                        width_mm,
                        height_mm,
                        rotation_deg,
                        exposure,
                    } => {
                        let q = rotate(local, -*rotation_deg).sub(*center);
                        (
                            q.x_mm.abs() < width_mm / 2. && q.y_mm.abs() < height_mm / 2.,
                            *exposure,
                        )
                    }
                    MacroPrimitive::Outline {
                        points,
                        rotation_deg,
                        exposure,
                    } => {
                        w.charge(points.len())?;
                        (
                            polygon_points_covers(points, rotate(local, -*rotation_deg)),
                            *exposure,
                        )
                    }
                };
                if hit {
                    inside = exposure == Exposure::Dark;
                }
            }
            return Ok(inside);
        }
        return aperture_distance(shape, local, &mut Budget(None))
            .map(|d| d == 0.)
            .map_err(geometry_error);
    }
    // Canonical contours are prepared once; no validation/quadratic topology work here.
    if let SemanticGeometry::Region { contours } = g {
        for c in contours {
            let mut winding = 0;
            for edge in &c.edges {
                w.charge(1)?;
                winding += edge_ray_winding(edge, p);
            }
            if winding != 0 {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    geometry_distance(g, p, &mut Budget(None))
        .map(|d| d == 0.)
        .map_err(geometry_error)
}
fn macro_edges(
    primitives: &[MacroPrimitive],
    w: &mut Work<'_>,
) -> Result<Vec<RegionEdge>, QueryError> {
    let mut edges = Vec::new();
    let mut ranges: Vec<(usize, usize, &MacroPrimitive)> = Vec::new();
    for primitive in primitives {
        w.charge(1)?;
        let (local, rotation) = match primitive {
            MacroPrimitive::Circle {
                center,
                diameter_mm,
                rotation_deg,
                ..
            } => {
                let c = rotate(*center, *rotation_deg);
                let p = MmPoint::new(c.x_mm + diameter_mm / 2., c.y_mm);
                (
                    vec![RegionEdge::Arc(ArcGeometry {
                        start: p,
                        end: p,
                        center: c,
                        direction: ArcDirection::CounterClockwise,
                        full_circle: true,
                        source: None,
                    })],
                    0.,
                )
            }
            MacroPrimitive::CenterLine {
                center,
                width_mm,
                height_mm,
                rotation_deg,
                ..
            } => {
                let points: Vec<_> = rectangle_points(*width_mm, *height_mm)
                    .into_iter()
                    .map(|p| MmPoint::new(p.x_mm + center.x_mm, p.y_mm + center.y_mm))
                    .collect();
                if (*width_mm > 0. && points[0] == points[1])
                    || (*height_mm > 0. && points[1] == points[2])
                {
                    return Err(uncertain(
                        "macro source rectangle collapsed in local coordinates",
                    ));
                }
                (polygon_edges(&points), *rotation_deg)
            }
            MacroPrimitive::Outline {
                points,
                rotation_deg,
                ..
            } => {
                w.charge(points.len())?;
                (polygon_edges(points), *rotation_deg)
            }
        };
        let transformed: Vec<_> = local
            .into_iter()
            .map(|e| {
                let transformed = select_rect::transform_edge(
                    e.clone(),
                    MmPoint::new(0., 0.),
                    LocalTransform {
                        rotation_deg: rotation,
                        ..LocalTransform::default()
                    },
                );
                if edge_start(&e) != edge_end(&e)
                    && edge_start(&transformed) == edge_end(&transformed)
                {
                    return Err(uncertain("macro source edge collapsed under rotation"));
                }
                Ok(transformed)
            })
            .collect::<Result<_, QueryError>>()?;
        for &(start, end, previous) in &ranges {
            w.charge(1)?;
            if same_edges(&edges[start..end], &transformed, w)?
                && !same_macro_material(previous, primitive, w)?
            {
                return Err(uncertain(
                    "distinct macro primitives rounded to identical boundaries",
                ));
            }
        }
        ranges.push((edges.len(), edges.len() + transformed.len(), primitive));
        edges.extend(transformed);
    }
    Ok(edges)
}
fn polygon_edges(points: &[MmPoint]) -> Vec<RegionEdge> {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .filter(|(a, b)| a != b)
        .map(|(a, b)| RegionEdge::Line { start: *a, end: *b })
        .collect()
}
fn prepare(
    mut geometry: SemanticGeometry,
    exposure: Exposure,
    document: &SemanticDocument,
    apertures: &HashMap<&str, &ApertureShape>,
    w: &mut Work<'_>,
) -> Result<Shape, QueryError> {
    w.charge(1)?;
    if let SemanticGeometry::Region { contours } = &mut geometry {
        for c in contours {
            w.charge(c.edges.len())?;
            *c = derived_region_contour(c).map_err(geometry_error)?;
        }
    }
    let bounds = geometries_bounds([&geometry], &document.apertures)
        .map_err(geometry_error)?
        .ok_or_else(|| uncertain("empty geometry bounds"))?;
    let mut endpoint_errors = Vec::new();
    let mut circle_sources = Vec::new();
    let mut boundary_error_mm = 0.;
    let edges = if let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = &geometry
    {
        let shape = apertures
            .get(aperture_id.as_str())
            .ok_or_else(|| uncertain("missing aperture"))?;
        let local = match shape {
            ApertureShape::Macro { primitives } => {
                // Compose the aperture in its own coordinates BEFORE rotation.
                // Internal coincident seams must not acquire a spurious gap
                // merely through independently rounded world transformations.
                let local_geometry = SemanticGeometry::Flash {
                    center: MmPoint::new(0., 0.),
                    aperture_id: aperture_id.clone(),
                    transform: LocalTransform::default(),
                };
                let local_bounds = geometries_bounds([&local_geometry], &document.apertures)
                    .map_err(geometry_error)?
                    .ok_or_else(|| uncertain("empty macro bounds"))?;
                let edges = macro_edges(primitives, w)?;
                let mut sources = Vec::new();
                for primitive in primitives {
                    let count = macro_edges(std::slice::from_ref(primitive), w)?.len();
                    let source = if let MacroPrimitive::Circle {
                        center,
                        diameter_mm,
                        rotation_deg,
                        ..
                    } = primitive
                    {
                        Some(CircleSource {
                            center: *center,
                            radius: diameter_mm / 2.,
                            rotation_deg: *rotation_deg,
                            world_center: MmPoint::new(0., 0.),
                            transform: LocalTransform::default(),
                        })
                    } else {
                        None
                    };
                    sources.extend(std::iter::repeat_n(source, count));
                }
                boundary_error_mm = boundary_roundoff(&edges);
                let shape = Shape {
                    geometry: local_geometry,
                    exposure: Exposure::Dark,
                    endpoint_errors: vec![(0., 0.); edges.len()],
                    boundary_error_mm: 0.,
                    circle_sources: sources,
                    edges,
                    bounds: local_bounds,
                };
                let mut boundary = Vec::new();
                component_integral(
                    &[shape],
                    &[0],
                    MmPoint::new(0., 0.),
                    apertures,
                    w,
                    Some(&mut boundary),
                )?;
                endpoint_errors = boundary
                    .iter()
                    .map(|b: &BoundaryEdge| (b.start_error_mm, b.end_error_mm))
                    .collect();
                circle_sources = boundary
                    .iter()
                    .map(|b| {
                        b.circle_source.map(|mut s| {
                            s.world_center = *center;
                            s.transform = *transform;
                            s
                        })
                    })
                    .collect();
                boundary.into_iter().map(|b| b.edge).collect()
            }
            shape => {
                let edges = select_rect::aperture_edges(shape, true);
                if matches!(shape, ApertureShape::Circle { .. }) {
                    circle_sources = edges
                        .iter()
                        .map(|e| {
                            if let RegionEdge::Arc(a) = e {
                                Some(CircleSource {
                                    center: a.center,
                                    radius: a.radius(),
                                    rotation_deg: 0.,
                                    world_center: *center,
                                    transform: LocalTransform {
                                        scale: transform.scale,
                                        ..LocalTransform::default()
                                    },
                                })
                            } else {
                                None
                            }
                        })
                        .collect();
                }
                edges
            }
        };
        let macro_rounding = if boundary_error_mm > 0. {
            let scale = transform.scale.abs();
            let position_error = transform_position_roundoff(&local, *center, *transform, w)?;
            boundary_error_mm = ((boundary_error_mm * scale).next_up() + position_error).next_up();
            Some((scale, position_error))
        } else {
            None
        };
        let mut world = Vec::with_capacity(local.len());
        for (index, e) in local.into_iter().enumerate() {
            w.charge(1)?;
            let transformed = select_rect::transform_edge(e.clone(), *center, *transform);
            if edge_start(&e) != edge_end(&e) && edge_start(&transformed) == edge_end(&transformed)
            {
                return Err(uncertain(
                    "nondegenerate source edge collapsed in world coordinates",
                ));
            }
            if let Some((scale, position_error)) = macro_rounding {
                // Rotation/mirror are isometries; uniform scale changes mm.
                // Add world point rounding converted to arclength as at any cut.
                let extra = if position_error == 0. {
                    0.
                } else {
                    cut_error(&transformed, position_error)?
                };
                let (start, end) = endpoint_errors[index];
                endpoint_errors[index] = (
                    ((start * scale).next_up() + extra).next_up(),
                    ((end * scale).next_up() + extra).next_up(),
                );
            }
            world.push(transformed);
        }
        world
    } else {
        select_rect::geometry_edges(&geometry, &mut Budget(None)).map_err(geometry_error)?
    };
    w.charge(edges.len())?;
    if endpoint_errors.is_empty() {
        endpoint_errors = vec![(0., 0.); edges.len()];
    }
    if circle_sources.is_empty() {
        circle_sources = vec![None; edges.len()];
    }
    Ok(Shape {
        geometry,
        exposure,
        edges,
        bounds,
        endpoint_errors,
        boundary_error_mm,
        circle_sources,
    })
}
fn transform_position_roundoff(
    edges: &[RegionEdge],
    center: MmPoint,
    transform: LocalTransform,
    w: &mut Work<'_>,
) -> Result<f64, QueryError> {
    // Identity translation/rotation/scale, including exact axis reflection,
    // consists only of exact sign operations and does not add point rounding.
    if center == MmPoint::new(0., 0.) && transform.rotation_deg == 0. && transform.scale == 1. {
        return Ok(0.);
    }
    let mut magnitude = 0_f64;
    for e in edges {
        w.charge(1)?;
        let b = edge_bounds(e);
        let x = b.min_x.abs().max(b.max_x.abs());
        let y = b.min_y.abs().max(b.max_y.abs());
        magnitude = magnitude.max((x + y).next_up());
        if let RegionEdge::Arc(a) = e {
            magnitude =
                magnitude.max((a.center.x_mm.abs() + a.center.y_mm.abs() + a.radius()).next_up());
        }
    }
    // Bound operands before rotate/scale/add, including large translation that
    // cancels a scaled local coordinate and leaves a small final world AABB.
    let scaled = (magnitude * transform.scale.abs()).next_up();
    let error = roundoff(&[scaled, center.x_mm, center.y_mm]);
    if !error.is_finite() {
        return Err(QueryError::NumericOverflow);
    }
    Ok(error)
}
fn overlaps(a: BoundsMm, b: BoundsMm) -> bool {
    a.min_x_mm <= b.max_x_mm
        && b.min_x_mm <= a.max_x_mm
        && a.min_y_mm <= b.max_y_mm
        && b.min_y_mm <= a.max_y_mm
}
fn root(parents: &mut [usize], mut i: usize) -> usize {
    while parents[i] != i {
        parents[i] = parents[parents[i]];
        i = parents[i];
    }
    i
}
fn components(shapes: &[Shape], w: &mut Work<'_>) -> Result<Vec<Vec<usize>>, QueryError> {
    let mut order: Vec<_> = (0..shapes.len()).collect();
    order.sort_by(|&a, &b| {
        shapes[a]
            .bounds
            .min_x_mm
            .total_cmp(&shapes[b].bounds.min_x_mm)
    });
    let mut parents: Vec<_> = (0..shapes.len()).collect();
    for (at, &i) in order.iter().enumerate() {
        for &j in &order[at + 1..] {
            w.charge(1)?;
            if shapes[j].bounds.min_x_mm > shapes[i].bounds.max_x_mm {
                break;
            }
            if overlaps(shapes[i].bounds, shapes[j].bounds) {
                let a = root(&mut parents, i);
                let b = root(&mut parents, j);
                parents[b] = a;
            }
        }
    }
    let mut groups = std::collections::BTreeMap::<usize, Vec<usize>>::new();
    // Iterate original exposure order, never sweep order.
    for i in 0..shapes.len() {
        groups.entry(root(&mut parents, i)).or_default().push(i);
    }
    Ok(groups.into_values().collect())
}
fn point_at(e: &RegionEdge, t: f64) -> MmPoint {
    if t == 0. {
        return edge_start(e);
    }
    if t == 1. {
        return edge_end(e);
    }
    match e {
        RegionEdge::Line { start, end } => MmPoint::new(
            start.x_mm + (end.x_mm - start.x_mm) * t,
            start.y_mm + (end.y_mm - start.y_mm) * t,
        ),
        RegionEdge::Arc(a) => {
            let theta = (a.start.y_mm - a.center.y_mm).atan2(a.start.x_mm - a.center.x_mm)
                + signed_sweep(*a) * t;
            MmPoint::new(
                a.center.x_mm + a.radius() * theta.cos(),
                a.center.y_mm + a.radius() * theta.sin(),
            )
        }
    }
}
fn interval_edge(e: &RegionEdge, a: f64, b: f64, sign: f64) -> RegionEdge {
    let start = point_at(e, if sign > 0. { a } else { b });
    let end = point_at(e, if sign > 0. { b } else { a });
    match e {
        RegionEdge::Line { .. } => RegionEdge::Line { start, end },
        RegionEdge::Arc(arc) => RegionEdge::Arc(ArcGeometry {
            start,
            end,
            direction: if sign > 0. {
                arc.direction
            } else if arc.direction == ArcDirection::CounterClockwise {
                ArcDirection::Clockwise
            } else {
                ArcDirection::CounterClockwise
            },
            full_circle: arc.full_circle && a == 0. && b == 1.,
            source: None,
            ..*arc
        }),
    }
}
fn signed_sweep(a: ArcGeometry) -> f64 {
    a.sweep_radians().unwrap_or(f64::NAN)
        * if a.direction == ArcDirection::CounterClockwise {
            1.
        } else {
            -1.
        }
}
fn parameter(e: &RegionEdge, p: MmPoint, error: f64) -> Option<f64> {
    if p.distance_mm(edge_start(e)) <= error {
        return Some(0.);
    }
    if p.distance_mm(edge_end(e)) <= error {
        return Some(1.);
    }
    match e {
        RegionEdge::Line { start, end } => {
            let d = end.sub(*start);
            let q = p.sub(*start);
            let length = d.x_mm.hypot(d.y_mm);
            if length == 0. {
                return None;
            }
            let t = (q.x_mm * d.x_mm + q.y_mm * d.y_mm) / (length * length);
            if segment_distance(p, *start, *end) <= error
                && t >= -error / length
                && t <= 1. + error / length
            {
                Some(t.clamp(0., 1.))
            } else {
                None
            }
        }
        RegionEdge::Arc(a) => {
            if (p.distance_mm(a.center) - a.radius()).abs() > error {
                return None;
            }
            if p.distance_mm(a.end) <= error && !a.full_circle {
                return Some(1.);
            }
            let start = (a.start.y_mm - a.center.y_mm).atan2(a.start.x_mm - a.center.x_mm);
            let angle = (p.y_mm - a.center.y_mm).atan2(p.x_mm - a.center.x_mm);
            let d = if a.direction == ArcDirection::CounterClockwise {
                (angle - start).rem_euclid(TAU)
            } else {
                (start - angle).rem_euclid(TAU)
            };
            let sweep = signed_sweep(*a).abs();
            if d <= sweep + error / a.radius() {
                Some((d / sweep).clamp(0., 1.))
            } else {
                None
            }
        }
    }
}
fn coincident(a: &RegionEdge, b: &RegionEdge, p: MmPoint) -> bool {
    match (a, b) {
        (RegionEdge::Line { start: a, end: b }, RegionEdge::Line { start: c, end: d }) => {
            cross(*a, *b, *c) == 0.
                && cross(*a, *b, *d) == 0.
                && segment_distance(p, *c, *d) <= roundoff(&[p.x_mm, p.y_mm])
        }
        (RegionEdge::Arc(a), RegionEdge::Arc(b)) => {
            a.center == b.center
                && a.radius() == b.radius()
                && parameter(
                    &RegionEdge::Arc(*b),
                    p,
                    roundoff(&[p.x_mm, p.y_mm, a.radius()]),
                )
                .is_some()
        }
        _ => false,
    }
}
fn curve(e: &RegionEdge) -> material::Curve {
    match e {
        RegionEdge::Line { start, end } => material::Curve::Line(*start, *end),
        RegionEdge::Arc(a) => material::Curve::Circle(a.center, a.radius()),
    }
}
fn intersection_points(
    a: &RegionEdge,
    b: &RegionEdge,
    error: f64,
    known_join: bool,
) -> Result<Vec<(MmPoint, bool)>, QueryError> {
    if known_join {
        let points: Vec<_> = [edge_start(a), edge_end(a), edge_start(b), edge_end(b)]
            .into_iter()
            .filter(|p| {
                if parameter(a, *p, error).is_none() || parameter(b, *p, error).is_none() {
                    return false;
                }
                match (a, b) {
                    (RegionEdge::Line { start, end }, RegionEdge::Arc(arc))
                    | (RegionEdge::Arc(arc), RegionEdge::Line { start, end }) => {
                        let v = end.sub(*start);
                        let length = v.x_mm.hypot(v.y_mm);
                        if length <= error * 64. {
                            return false;
                        }
                        let unit = MmPoint::new(v.x_mm / length, v.y_mm / length);
                        let z = arc.center.sub(*start);
                        let along = z.x_mm * unit.x_mm + z.y_mm * unit.y_mm;
                        p.distance_mm(MmPoint::new(
                            start.x_mm + along * unit.x_mm,
                            start.y_mm + along * unit.y_mm,
                        )) <= error
                    }
                    (RegionEdge::Arc(a), RegionEdge::Arc(b)) => {
                        let v = b.center.sub(a.center);
                        let length = v.x_mm.hypot(v.y_mm);
                        if length <= error * 64. {
                            return false;
                        }
                        segment_distance(*p, a.center, b.center).min(
                            // Internal tangent may lie beyond the center segment.
                            cross(a.center, b.center, *p).abs() / length,
                        ) <= error
                    }
                    _ => false,
                }
            })
            .map(|p| (p, true))
            .collect();
        if !points.is_empty() {
            return Ok(points);
        }
    }
    material::intersections(curve(a), curve(b), error)
        .map(|points| points.into_iter().map(|p| (p, false)).collect())
        .map_err(|_| uncertain("unresolved curve intersection"))
}
fn supports_known_join(g: &SemanticGeometry, apertures: &HashMap<&str, &ApertureShape>) -> bool {
    match g {
        SemanticGeometry::Line { .. }
        | SemanticGeometry::Arc { .. }
        | SemanticGeometry::Region { .. } => true,
        SemanticGeometry::Flash { aperture_id, .. } => matches!(
            apertures.get(aperture_id.as_str()),
            Some(ApertureShape::Obround { .. })
        ),
        _ => false,
    }
}
#[derive(Clone, Copy)]
struct Cut {
    t: f64,
    arclength_error_mm: f64,
}
fn cut_error(e: &RegionEdge, position_error: f64) -> Result<f64, QueryError> {
    if let RegionEdge::Arc(arc) = e {
        // A disk of radius rho viewed from distance r subtends asin(rho/r).
        // asin(z)<=2z for 0<=z<=1/2, so radius*angle error <=2rho.
        if position_error >= arc.radius() / 2. {
            return Err(uncertain("cut angle not certifiable"));
        }
    }
    Ok((2. * position_error).next_up())
}
#[derive(Default)]
struct Sum {
    value: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, v: f64) {
        let y = v - self.correction;
        let t = self.value + y;
        self.correction = (t - self.value) - y;
        self.value = t;
    }
}
#[derive(Default)]
struct Integral {
    area: Sum,
    mx: Sum,
    my: Sum,
    perimeter: Sum,
    area_error: f64,
    moment_error: f64,
    perimeter_error: f64,
}
fn integrate(e: &RegionEdge, a: f64, b: f64, origin: MmPoint) -> (f64, f64, f64, f64) {
    match e {
        RegionEdge::Line { .. } => {
            let p = point_at(e, a).sub(origin);
            let q = point_at(e, b).sub(origin);
            let d = q.sub(p);
            (
                (p.x_mm * q.y_mm - p.y_mm * q.x_mm) / 2.,
                d.y_mm * (p.x_mm * p.x_mm + p.x_mm * d.x_mm + d.x_mm * d.x_mm / 3.) / 2.,
                -d.x_mm * (p.y_mm * p.y_mm + p.y_mm * d.y_mm + d.y_mm * d.y_mm / 3.) / 2.,
                p.distance_mm(q),
            )
        }
        RegionEdge::Arc(arc) => {
            let r = arc.radius();
            let c = arc.center.sub(origin);
            let theta = (arc.start.y_mm - arc.center.y_mm).atan2(arc.start.x_mm - arc.center.x_mm);
            let u = theta + signed_sweep(*arc) * a;
            let v = theta + signed_sweep(*arc) * b;
            let d = v - u;
            let cos = v.sin() - u.sin();
            let sin = u.cos() - v.cos();
            let cos2 = d / 2. + ((2. * v).sin() - (2. * u).sin()) / 4.;
            let sin2 = d / 2. - ((2. * v).sin() - (2. * u).sin()) / 4.;
            let cos3 = (v.sin() - v.sin().powi(3) / 3.) - (u.sin() - u.sin().powi(3) / 3.);
            let sin3 = (-v.cos() + v.cos().powi(3) / 3.) - (-u.cos() + u.cos().powi(3) / 3.);
            (
                (r * c.x_mm * cos + r * c.y_mm * sin + r * r * d) / 2.,
                r * (c.x_mm * c.x_mm * cos + 2. * c.x_mm * r * cos2 + r * r * cos3) / 2.,
                r * (c.y_mm * c.y_mm * sin + 2. * c.y_mm * r * sin2 + r * r * sin3) / 2.,
                r * d.abs(),
            )
        }
    }
}
fn integration_roundoff(
    e: &RegionEdge,
    a: f64,
    b: f64,
    origin: MmPoint,
    envelope: f64,
    values: (f64, f64, f64, f64),
) -> (f64, f64, f64) {
    let (area, mx, my, perimeter) = values;
    let unit = 1024. * f64::EPSILON;
    if let RegionEdge::Arc(arc) = e {
        let c = arc.center.sub(origin);
        let r = arc.radius();
        let theta = (arc.start.y_mm - arc.center.y_mm).atan2(arc.start.x_mm - arc.center.x_mm);
        let u = theta + signed_sweep(*arc) * a;
        let v = theta + signed_sweep(*arc) * b;
        let d = (v - u).abs();
        let phase = u.abs() + v.abs() + d + 1.;
        // Bound intermediate trig/polynomial operands, including subtraction
        // of nearly equal sin/cos values. Final small material AABB alone cannot
        // bound cancellation in a shallow arc with a distant circle center.
        let ab = r * (c.x_mm.abs() + c.y_mm.abs()) * 2. + r * r * phase + area.abs();
        let coordinate = c.x_mm.abs().max(c.y_mm.abs());
        let mb = r * (2. * coordinate * coordinate + 2. * coordinate * r * (d + 1.) + 3. * r * r)
            + mx.abs()
            + my.abs();
        (unit * ab, unit * mb, unit * (r * phase + perimeter))
    } else {
        (
            unit * (envelope * envelope + area.abs()),
            unit * (envelope.powi(3) + mx.abs() + my.abs()),
            unit * perimeter,
        )
    }
}
struct BoundaryEdge {
    edge: RegionEdge,
    start_error_mm: f64,
    end_error_mm: f64,
    circle_source: Option<CircleSource>,
}
fn boundary_roundoff(edges: &[RegionEdge]) -> f64 {
    boundary_roundoff_refs(&edges.iter().collect::<Vec<_>>())
}
fn boundary_roundoff_refs(edges: &[&RegionEdge]) -> f64 {
    let magnitude = edges
        .iter()
        .flat_map(|e| {
            let b = edge_bounds(e);
            let (cx, cy, r) = match e {
                RegionEdge::Arc(a) => (a.center.x_mm, a.center.y_mm, a.radius()),
                _ => (0., 0., 0.),
            };
            [b.min_x, b.max_x, b.min_y, b.max_y, cx, cy, r]
        })
        .map(f64::abs)
        .fold(1., f64::max);
    roundoff(&[magnitude])
}
fn check_source_aliases(
    shapes: &[Shape],
    group: &[usize],
    apertures: &HashMap<&str, &ApertureShape>,
    w: &mut Work<'_>,
) -> Result<(), QueryError> {
    for (at, &i) in group.iter().enumerate() {
        for &j in &group[at + 1..] {
            w.charge(1)?;
            if let (
                SemanticGeometry::Flash {
                    center: a,
                    aperture_id: ia,
                    transform: ta,
                },
                SemanticGeometry::Flash {
                    center: b,
                    aperture_id: ib,
                    transform: tb,
                },
            ) = (&shapes[i].geometry, &shapes[j].geometry)
            {
                let (sa, sb) = (apertures[ia.as_str()], apertures[ib.as_str()]);
                if (!matches!(sa, ApertureShape::Circle { .. })
                    || !matches!(sb, ApertureShape::Circle { .. }))
                    && same_edges(&shapes[i].edges, &shapes[j].edges, w)?
                    && (a != b || ta != tb || !same_aperture(sa, sb, w)?)
                {
                    return Err(uncertain(
                        "distinct source apertures rounded to identical boundaries",
                    ));
                }
            }
        }
    }
    Ok(())
}
fn component_integral(
    shapes: &[Shape],
    group: &[usize],
    origin: MmPoint,
    apertures: &HashMap<&str, &ApertureShape>,
    w: &mut Work<'_>,
    mut boundary: Option<&mut Vec<BoundaryEdge>>,
) -> Result<Integral, QueryError> {
    check_source_aliases(shapes, group, apertures, w)?;
    let sourced: Vec<_> = group
        .iter()
        .flat_map(|&i| {
            shapes[i]
                .edges
                .iter()
                .enumerate()
                .map(move |(j, e)| (i, j, e))
        })
        .filter(|(_, _, e)| {
            edge_start(e) != edge_end(e) || matches!(e,RegionEdge::Arc(a) if a.full_circle)
        })
        .collect();
    let edges: Vec<_> = sourced.iter().map(|(_, _, e)| *e).collect();
    if edges.len() > MAX_EDGES || edges.len().saturating_mul(256) > MAX_TEMP_BYTES {
        return Err(QueryError::ResourceLimit);
    }
    let error = boundary_roundoff_refs(&edges).max(
        group
            .iter()
            .map(|&i| shapes[i].boundary_error_mm)
            .fold(0., f64::max),
    );
    let mut cuts: Vec<Vec<Cut>> = sourced
        .iter()
        .map(|&(i, j, _)| {
            let (start, end) = shapes[i].endpoint_errors[j];
            vec![
                Cut {
                    t: 0.,
                    arclength_error_mm: start,
                },
                Cut {
                    t: 1.,
                    arclength_error_mm: end,
                },
            ]
        })
        .collect();
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            w.charge(1)?;
            let si = shapes[sourced[i].0].circle_sources[sourced[i].1];
            let sj = shapes[sourced[j].0].circle_sources[sourced[j].1];
            if let (Some(a), Some(b)) = (si, sj)
                && a != b
                && matches!((edges[i],edges[j]),(RegionEdge::Arc(x),RegionEdge::Arc(y)) if x.center==y.center && x.radius()==y.radius())
            {
                return Err(uncertain(
                    "distinct source circles rounded to coincident boundaries",
                ));
            }
            // Macro boundary topology/cuts were certified in aperture coordinates.
            // Re-solving its own transformed supporting circles would split an
            // already shared cut into two unrelated, rounded near-endpoint cuts.
            // Carry their local endpoint uncertainty instead; only intersections
            // with OTHER selected shapes require a fresh world-space solve.
            if sourced[i].0 == sourced[j].0
                && matches!(&shapes[sourced[i].0].geometry,
                    SemanticGeometry::Flash {aperture_id,..}
                    if matches!(apertures.get(aperture_id.as_str()),Some(ApertureShape::Macro {..})))
                && shapes[sourced[i].0].boundary_error_mm > 0.
            {
                continue;
            }
            let known_join = sourced[i].0 == sourced[j].0
                && supports_known_join(&shapes[sourced[i].0].geometry, apertures);
            let points = intersection_points(edges[i], edges[j], error, known_join)?;
            for (p, known) in points.into_iter().chain([
                (edge_start(edges[i]), true),
                (edge_end(edges[i]), true),
                (edge_start(edges[j]), true),
                (edge_end(edges[j]), true),
            ]) {
                if let (Some(a), Some(b)) =
                    (parameter(edges[i], p, error), parameter(edges[j], p, error))
                {
                    w.charge(2)?;
                    let pe = if known {
                        error
                    } else {
                        intersection_bounds::position_error(edges[i], edges[j], p, error)?
                    };
                    cuts[i].push(Cut {
                        t: a,
                        arclength_error_mm: cut_error(edges[i], pe)?,
                    });
                    cuts[j].push(Cut {
                        t: b,
                        arclength_error_mm: cut_error(edges[j], pe)?,
                    });
                }
            }
        }
    }
    let mut result = Integral::default();
    for (i, e) in edges.iter().enumerate() {
        cuts[i].sort_by(|a, b| a.t.total_cmp(&b.t));
        cuts[i].dedup_by(|later, earlier| {
            if later.t == earlier.t {
                earlier.arclength_error_mm =
                    earlier.arclength_error_mm.max(later.arclength_error_mm);
                true
            } else {
                false
            }
        });
        for interval in cuts[i].windows(2) {
            w.charge(1)?;
            let (a, b) = (interval[0].t, interval[1].t);
            let endpoint_error =
                (interval[0].arclength_error_mm + interval[1].arclength_error_mm).next_up();
            if a == b {
                continue;
            }
            let length = match e {
                RegionEdge::Line { start, end } => start.distance_mm(*end) * (b - a),
                RegionEdge::Arc(arc) => arc.radius() * signed_sweep(*arc).abs() * (b - a),
            };
            if length <= endpoint_error {
                return Err(uncertain("cut order not numerically separable"));
            }
            let mid = (a + b) / 2.;
            let p = point_at(e, mid);
            let mut duplicate = false;
            for other in &edges[..i] {
                w.charge(1)?;
                if coincident(e, other, p) {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            let mut clearance = point_at(e, a)
                .distance_mm(p)
                .min(point_at(e, b).distance_mm(p));
            let normal = match e {
                RegionEdge::Line { start, end } => {
                    let d = end.sub(*start);
                    let len = d.x_mm.hypot(d.y_mm);
                    MmPoint::new(-d.y_mm / len, d.x_mm / len)
                }
                RegionEdge::Arc(arc) => {
                    clearance = clearance.min(arc.radius());
                    let n = arc.center.sub(p);
                    let len = n.x_mm.hypot(n.y_mm);
                    let sign = if arc.direction == ArcDirection::CounterClockwise {
                        1.
                    } else {
                        -1.
                    };
                    MmPoint::new(sign * n.x_mm / len, sign * n.y_mm / len)
                }
            };
            // For full circles the endpoints coincide but the midpoint lies opposite.
            for other in &edges {
                w.charge(1)?;
                if !coincident(e, other, p) {
                    clearance = clearance.min(edge_distance(p, other));
                }
            }
            let delta = clearance / 8.;
            if !delta.is_finite() || delta <= error * 64. {
                return Err(uncertain(
                    "boundary interval has no numerically separated sides",
                ));
            }
            let sides = [
                MmPoint::new(p.x_mm + delta * normal.x_mm, p.y_mm + delta * normal.y_mm),
                MmPoint::new(p.x_mm - delta * normal.x_mm, p.y_mm - delta * normal.y_mm),
            ];
            let mut inside = [false, false];
            // Apply every selected primitive in ORIGINAL layer exposure order.
            for &j in group {
                for k in 0..2 {
                    // Macro/Flash material remains local; the query supplies semantic apertures.
                    if covers(&shapes[j].geometry, sides[k], apertures, w)? {
                        inside[k] = shapes[j].exposure == Exposure::Dark;
                    }
                }
            }
            if inside[0] == inside[1] {
                continue;
            }
            let sign = if inside[0] { 1. } else { -1. };
            if let Some(output) = boundary.as_deref_mut() {
                let (start_error_mm, end_error_mm) = if sign > 0. {
                    (
                        interval[0].arclength_error_mm,
                        interval[1].arclength_error_mm,
                    )
                } else {
                    (
                        interval[1].arclength_error_mm,
                        interval[0].arclength_error_mm,
                    )
                };
                output.push(BoundaryEdge {
                    edge: interval_edge(e, a, b, sign),
                    start_error_mm,
                    end_error_mm,
                    circle_source: shapes[sourced[i].0].circle_sources[sourced[i].1],
                });
            }
            let (area, mx, my, perimeter) = integrate(e, a, b, origin);
            result.area.add(sign * area);
            result.mx.add(sign * mx);
            result.my.add(sign * my);
            result.perimeter.add(perimeter);
            let bounds = edge_bounds(e);
            let x = (bounds.min_x - origin.x_mm)
                .abs()
                .max((bounds.max_x - origin.x_mm).abs());
            let y = (bounds.min_y - origin.y_mm)
                .abs()
                .max((bounds.max_y - origin.y_mm).abs());
            let radius = (x * x + y * y).sqrt() + error + endpoint_error;
            let (arithmetic_area, arithmetic_moment, arithmetic_perimeter) =
                integration_roundoff(e, a, b, origin, radius, (area, mx, my, perimeter));
            // Green integrands, parametrized by arclength, obey |dA/ds|<=R/2
            // and |dMx/ds|,|dMy/ds|<=R²/2 on the enclosed boundary.
            let area_error =
                perimeter * error * 8. + arithmetic_area + radius * endpoint_error / 2.;
            result.area_error += area_error;
            result.moment_error +=
                area_error * radius + arithmetic_moment + radius * radius * endpoint_error / 2.;
            result.perimeter_error += error * 16. + arithmetic_perimeter + endpoint_error;
        }
    }
    Ok(result)
}

/// Selection IDs are explicit; composition order always comes from the model.
/// The closure is checked inside all potentially large loops, without a library Boolean call.
pub fn calculate(
    document: &SemanticDocument,
    groups: &[SelectionGroup],
    resolution_mm: f64,
    cancelled: impl FnMut() -> bool,
) -> Result<SelectionGeometry, QueryError> {
    calculate_with_deadline(
        document,
        groups,
        resolution_mm,
        Instant::now() + Duration::from_secs(2),
        cancelled,
    )
}

/// The service includes its ID resolution/bounds preparation in the SAME
/// two-second admission deadline, not a fresh budget for every sub-operation.
pub fn calculate_with_deadline(
    document: &SemanticDocument,
    groups: &[SelectionGroup],
    resolution_mm: f64,
    deadline: Instant,
    mut cancelled: impl FnMut() -> bool,
) -> Result<SelectionGeometry, QueryError> {
    if !resolution_mm.is_finite() || resolution_mm <= 0. {
        return Err(QueryError::InvalidArgument("resolution_mm".into()));
    }
    let mut w = Work {
        count: 0,
        deadline,
        cancelled: &mut cancelled,
    };
    w.charge(0)?;
    let count = groups
        .iter()
        .try_fold(0usize, |n, g| n.checked_add(g.object_ids.len()))
        .ok_or(QueryError::ResourceLimit)?;
    if count > MAX_SELECTED || groups.len() > MAX_SELECTED {
        return Err(QueryError::ResourceLimit);
    }
    let mut layer_ids = std::collections::HashSet::new();
    let apertures: HashMap<_, _> = document
        .apertures
        .iter()
        .map(|a| (a.id.as_str(), &a.shape))
        .collect();
    let mut layers = Vec::new();
    let mut bounds: Option<BoundsMm> = None;
    let mut expanded = 0usize;
    let mut edge_count = 0usize;
    let mut admitted_edges = 0usize;
    let mut admitted_primitives = 0usize;
    for group in groups {
        w.charge(1)?;
        if !layer_ids.insert(&group.layer_id) || group.object_ids.is_empty() {
            return Err(QueryError::InvalidArgument(
                "empty/duplicate layer group".into(),
            ));
        }
        let wanted: std::collections::HashSet<_> =
            group.object_ids.iter().map(String::as_str).collect();
        if wanted.len() != group.object_ids.len() {
            return Err(QueryError::InvalidArgument("duplicate object ID".into()));
        }
        let layer = document
            .layers
            .iter()
            .find(|l| l.id == group.layer_id)
            .ok_or_else(|| QueryError::InvalidArgument("unknown layer".into()))?;
        let mut shapes = Vec::new();
        let mut found = 0;
        for object in &layer.objects {
            w.charge(1)?;
            if !wanted.contains(object.object_id.as_str()) {
                continue;
            }
            found += 1;
            let (primitives, edges) =
                admission_cost(document, &object.geometry, || (w.cancelled)())?;
            admitted_primitives = admitted_primitives.saturating_add(primitives);
            admitted_edges = admitted_edges.saturating_add(edges);
            if admitted_primitives > MAX_PRIMITIVES
                || admitted_edges > MAX_EDGES
                || admitted_edges
                    .saturating_mul(1024)
                    .saturating_add(admitted_primitives.saturating_mul(512))
                    > MAX_TEMP_BYTES
            {
                return Err(QueryError::ResourceLimit);
            }
            w.charge(primitives)?;
            let geometries = match &object.geometry {
                SemanticGeometry::BlockInstance {
                    definition_id,
                    transform,
                } => {
                    let definition = document
                        .block_definition(definition_id)
                        .ok_or_else(|| geometry_error("unknown block"))?;
                    if definition.objects.len() > MAX_PRIMITIVES.saturating_sub(expanded) {
                        return Err(QueryError::ResourceLimit);
                    }
                    let mut resolved = Vec::with_capacity(definition.objects.len());
                    for child in &definition.objects {
                        w.charge(1)?;
                        resolved.push((
                            block::resolve_geometry(child.geometry.clone().into(), transform)
                                .map_err(geometry_error)?,
                            child.exposure,
                        ));
                    }
                    resolved
                }
                geometry => vec![(geometry.clone(), object.exposure)],
            };
            for (geometry, exposure) in geometries {
                expanded += 1;
                if expanded > MAX_PRIMITIVES {
                    return Err(QueryError::ResourceLimit);
                }
                let shape = prepare(geometry, exposure, document, &apertures, &mut w)?;
                bounds = Some(bounds.map_or(shape.bounds, |b| b.union(shape.bounds)));
                edge_count = edge_count.saturating_add(shape.edges.len());
                if edge_count > MAX_EDGES
                    || edge_count.saturating_mul(256) + expanded.saturating_mul(512)
                        > MAX_TEMP_BYTES
                {
                    return Err(QueryError::ResourceLimit);
                }
                shapes.push(shape);
            }
        }
        if found != wanted.len() {
            return Err(QueryError::InvalidArgument(
                "unknown object in group".into(),
            ));
        }
        layers.push(shapes);
    }
    let origin = bounds.map(BoundsMm::center).unwrap_or(MmPoint::new(0., 0.));
    let mut total = Integral::default();
    let mut boundaries = false;
    for shapes in &layers {
        for group in components(shapes, &mut w)? {
            w.charge(1)?;
            let local_origin = group
                .iter()
                .map(|&i| shapes[i].bounds)
                .reduce(BoundsMm::union)
                .unwrap()
                .center();
            let local = component_integral(shapes, &group, local_origin, &apertures, &mut w, None)?;
            boundaries |= local.perimeter.value > 0.;
            let delta = local_origin.sub(origin);
            total.area.add(local.area.value);
            total.mx.add(local.mx.value + local.area.value * delta.x_mm);
            total.my.add(local.my.value + local.area.value * delta.y_mm);
            total.perimeter.add(local.perimeter.value);
            total.area_error += local.area_error;
            total.moment_error += local.moment_error
                + local.area_error * delta.x_mm.abs().max(delta.y_mm.abs())
                + 1024.
                    * f64::EPSILON
                    * (local.area.value * delta.x_mm)
                        .abs()
                        .max((local.area.value * delta.y_mm).abs());
            total.perimeter_error += local.perimeter_error;
        }
    }
    w.charge(0)?;
    let material = if !boundaries {
        CompositeMaterial::ZeroArea
    } else {
        let area = total.area.value;
        let low = area - total.area_error;
        if !low.is_finite() || low <= 0. {
            return Err(uncertain("composite area not provably positive"));
        }
        let local = MmPoint::new(total.mx.value / area, total.my.value / area);
        let centroid = MmPoint::new(origin.x_mm + local.x_mm, origin.y_mm + local.y_mm);
        let error =
            (total.moment_error + local.x_mm.abs().max(local.y_mm.abs()) * total.area_error) / low
                + roundoff(&[origin.x_mm, origin.y_mm, centroid.x_mm, centroid.y_mm]);
        let mx = total.mx.value + area * origin.x_mm;
        let my = total.my.value + area * origin.y_mm;
        if ![
            area,
            total.perimeter.value,
            mx,
            my,
            centroid.x_mm,
            centroid.y_mm,
            error,
        ]
        .into_iter()
        .all(f64::is_finite)
        {
            return Err(QueryError::NumericOverflow);
        }
        if error > 1e-5_f64.min(resolution_mm / 10.) {
            return Err(uncertain(
                "centroid error exceeds manufacturing query tolerance",
            ));
        }
        CompositeMaterial::Ready {
            area_mm2: area,
            perimeter_mm: total.perimeter.value,
            first_moment_x_mm3: mx,
            first_moment_y_mm3: my,
            centroid_mm: centroid,
            centroid_error_mm: error,
            area_error_mm2: total.area_error,
            perimeter_error_mm: total.perimeter_error,
        }
    };
    Ok(SelectionGeometry {
        bounds_mm: bounds,
        bounding_center_mm: bounds.map(BoundsMm::center),
        material,
        selected_count: count,
        expanded_count: expanded,
        work: w.count,
    })
}
