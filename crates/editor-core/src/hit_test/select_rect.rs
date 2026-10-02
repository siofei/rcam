//! Analytic closed-material vs closed-rectangle relations. No display sampling.
use super::*;
use material::{Curve, Material};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectRectMode {
    Window,
    Crossing,
}

fn circle(c: MmPoint, radius: f64) -> RegionEdge {
    let start = MmPoint::new(c.x_mm + radius, c.y_mm);
    RegionEdge::Arc(ArcGeometry {
        start,
        end: start,
        center: c,
        direction: ArcDirection::CounterClockwise,
        full_circle: true,
        source: None,
    })
}
fn polygon(points: &[MmPoint]) -> Vec<RegionEdge> {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| RegionEdge::Line { start: *a, end: *b })
        .collect()
}
// Exact capsule perimeter. In contrast to full end circles, these semicircles
// remain valid material witnesses even when an obround contains a local hole.
fn capsule(a: MmPoint, b: MmPoint, r: f64) -> Vec<RegionEdge> {
    if a == b {
        return vec![circle(a, r)];
    }
    let d = b.sub(a);
    let len = d.x_mm.hypot(d.y_mm);
    let n = MmPoint::new(-d.y_mm / len * r, d.x_mm / len * r);
    let plus = |p: MmPoint| MmPoint::new(p.x_mm + n.x_mm, p.y_mm + n.y_mm);
    let minus = |p: MmPoint| MmPoint::new(p.x_mm - n.x_mm, p.y_mm - n.y_mm);
    vec![
        RegionEdge::Line {
            start: plus(a),
            end: plus(b),
        },
        RegionEdge::Arc(ArcGeometry {
            start: plus(b),
            end: minus(b),
            center: b,
            direction: ArcDirection::Clockwise,
            full_circle: false,
            source: None,
        }),
        RegionEdge::Line {
            start: minus(b),
            end: minus(a),
        },
        RegionEdge::Arc(ArcGeometry {
            start: minus(a),
            end: plus(a),
            center: a,
            direction: ArcDirection::Clockwise,
            full_circle: false,
            source: None,
        }),
    ]
}
fn aperture_edges(shape: &ApertureShape, include_hole: bool) -> Vec<RegionEdge> {
    let zero = MmPoint::new(0., 0.);
    let (mut edges, hole) = match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => (vec![circle(zero, diameter_mm / 2.)], hole_diameter_mm),
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => (
            polygon(&rectangle_points(*width_mm, *height_mm)),
            hole_diameter_mm,
        ),
        ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            let offset = (width_mm - height_mm).abs() / 2.;
            let b = if width_mm >= height_mm {
                MmPoint::new(offset, 0.)
            } else {
                MmPoint::new(0., offset)
            };
            (
                capsule(
                    MmPoint::new(-b.x_mm, -b.y_mm),
                    b,
                    width_mm.min(*height_mm) / 2.,
                ),
                hole_diameter_mm,
            )
        }
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            let points: Vec<_> = (0..*vertices)
                .map(|i| {
                    let a = rotation_deg.to_radians()
                        + std::f64::consts::TAU * f64::from(i) / f64::from(*vertices);
                    MmPoint::new(a.cos() * diameter_mm / 2., a.sin() * diameter_mm / 2.)
                })
                .collect();
            (polygon(&points), hole_diameter_mm)
        }
        ApertureShape::Macro { .. } => unreachable!("prepared material"),
    };
    if include_hole && let Some(d) = hole {
        edges.push(circle(zero, d / 2.));
    }
    edges
}
fn transform_edge(e: RegionEdge, c: MmPoint, t: LocalTransform) -> RegionEdge {
    let point = |mut p: MmPoint| {
        if matches!(t.mirror, Mirror::X | Mirror::Xy) {
            p.x_mm = -p.x_mm;
        }
        if matches!(t.mirror, Mirror::Y | Mirror::Xy) {
            p.y_mm = -p.y_mm;
        }
        p = rotate(p, t.rotation_deg);
        MmPoint::new(c.x_mm + p.x_mm * t.scale, c.y_mm + p.y_mm * t.scale)
    };
    match e {
        RegionEdge::Line { start, end } => RegionEdge::Line {
            start: point(start),
            end: point(end),
        },
        RegionEdge::Arc(mut a) => {
            a.start = point(a.start);
            a.end = point(a.end);
            a.center = point(a.center);
            if matches!(t.mirror, Mirror::X | Mirror::Y) {
                a.direction = match a.direction {
                    ArcDirection::Clockwise => ArcDirection::CounterClockwise,
                    ArcDirection::CounterClockwise => ArcDirection::Clockwise,
                };
            }
            RegionEdge::Arc(a)
        }
    }
}
fn geometry_edges(
    g: &SemanticGeometry,
    budget: &mut Budget,
) -> Result<Vec<RegionEdge>, HitTestError> {
    Ok(match g {
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => capsule(*start, *end, width_mm / 2.),
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => polygon(&rectangular_sweep_points(
            *start, *end, *width_mm, *height_mm,
        )),

        SemanticGeometry::Arc { path, width_mm } => {
            if !path.is_valid() {
                return Err(HitTestError::Unsupported("invalid manufacturing arc"));
            }
            let r = width_mm / 2.;
            if path.zero_sweep() {
                vec![circle(path.start, r)]
            } else {
                let a = path.canonical_circle();
                let radius = a.radius();
                let mut edges = Vec::new();
                for offset in [r, -r] {
                    if radius + offset <= 0. {
                        continue;
                    }
                    let project = |p: MmPoint| {
                        MmPoint::new(
                            a.center.x_mm + (p.x_mm - a.center.x_mm) * (radius + offset) / radius,
                            a.center.y_mm + (p.y_mm - a.center.y_mm) * (radius + offset) / radius,
                        )
                    };
                    edges.push(RegionEdge::Arc(ArcGeometry {
                        start: project(a.start),
                        end: project(a.end),
                        ..a
                    }));
                }
                // All supplemental capsule boundaries lie in the swept material;
                // their union with the offset arcs includes its entire boundary.
                edges.extend(capsule(path.start, a.start, r));
                edges.extend(capsule(a.end, path.end, r));
                edges
            }
        }
        SemanticGeometry::Region { contours } => {
            let mut edges = Vec::new();
            for c in contours {
                budget.charge(c.edges.len())?;
                edges.extend(derived_region_contour(c)?.edges);
            }
            edges
        }
        SemanticGeometry::Flash { .. } => unreachable!("aperture lookup"),
        SemanticGeometry::BlockInstance { .. } => {
            unreachable!("resolved into primitives by the caller")
        }
    })
}
/// Edges of one Flash-or-simpler geometry. `BlockInstance` is never passed in
/// here: the caller resolves it into primitives first (see `select_rect`),
/// since only the caller has the document's `block_definitions` table.
pub(super) fn edges_for(
    g: &SemanticGeometry,
    apertures: &HashMap<&str, &ApertureShape>,
    macros: &mut HashMap<String, Material>,
    budget: &mut Budget,
    include_standard_holes: bool,
) -> Result<Vec<RegionEdge>, HitTestError> {
    if let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = g
    {
        let shape = *apertures
            .get(aperture_id.as_str())
            .ok_or_else(|| SemanticError::MissingAperture(aperture_id.clone()))?;
        let local = if let ApertureShape::Macro { primitives } = shape {
            if !macros.contains_key(aperture_id.as_str()) {
                macros.insert(aperture_id.clone(), Material::prepare(primitives, budget)?);
            }
            budget.charge(macros[aperture_id.as_str()].boundary.len())?;
            macros[aperture_id.as_str()].boundary.clone()
        } else {
            aperture_edges(shape, include_standard_holes)
        };
        Ok(local
            .into_iter()
            .map(|e| transform_edge(e, *center, *transform))
            .collect())
    } else {
        geometry_edges(g, budget)
    }
}

/// Distance from `p` to one Flash-or-simpler geometry (see [`edges_for`]).
fn distance_for(
    g: &SemanticGeometry,
    p: MmPoint,
    apertures: &HashMap<&str, &ApertureShape>,
    macros: &mut HashMap<String, Material>,
    budget: &mut Budget,
) -> Result<f64, HitTestError> {
    if let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = g
    {
        let local = apply_inverse_transform(p, *center, *transform);
        let shape = *apertures
            .get(aperture_id.as_str())
            .ok_or_else(|| SemanticError::MissingAperture(aperture_id.clone()))?;
        let distance = if let ApertureShape::Macro { primitives } = shape {
            if !macros.contains_key(aperture_id.as_str()) {
                macros.insert(aperture_id.clone(), Material::prepare(primitives, budget)?);
            }
            macros[aperture_id.as_str()].distance(local, budget)?.0
        } else {
            aperture_distance(shape, local, budget)?
        };
        Ok(distance * transform.scale)
    } else {
        geometry_distance(g, p, budget)
    }
}

fn contains(r: BoundsMm, p: MmPoint, e: f64) -> bool {
    p.x_mm >= r.min_x_mm - e
        && p.x_mm <= r.max_x_mm + e
        && p.y_mm >= r.min_y_mm - e
        && p.y_mm <= r.max_y_mm + e
}
fn crosses(edge: &RegionEdge, rect: BoundsMm, precision: f64) -> Result<bool, HitTestError> {
    if contains(rect, edge_start(edge), precision) || contains(rect, edge_end(edge), precision) {
        return Ok(true);
    }
    let corners = [
        MmPoint::new(rect.min_x_mm, rect.min_y_mm),
        MmPoint::new(rect.max_x_mm, rect.min_y_mm),
        MmPoint::new(rect.max_x_mm, rect.max_y_mm),
        MmPoint::new(rect.min_x_mm, rect.max_y_mm),
    ];
    let curve = match edge {
        RegionEdge::Line { start, end } => Curve::Line(*start, *end),
        RegionEdge::Arc(a) => Curve::Circle(a.center, a.radius()),
    };
    for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)).take(4) {
        if a == b {
            if edge_distance(*a, edge) <= precision {
                return Ok(true);
            }
            continue;
        }
        for p in material::intersections(curve, Curve::Line(*a, *b), precision)? {
            if match edge {
                RegionEdge::Line { .. } => true,
                RegionEdge::Arc(a) => {
                    a.full_circle
                        || arc_parameter(*a, (p.y_mm - a.center.y_mm).atan2(p.x_mm - a.center.x_mm))
                            .is_some()
                }
            } {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
impl SemanticDocument {
    /// Validated manufacturing geometry, stable exposure order, atomic failure.
    pub fn select_rect(
        &self,
        layer_id: &str,
        rect: BoundsMm,
        mode: SelectRectMode,
    ) -> Result<Vec<String>, HitTestError> {
        self.select_rect_cancellable(layer_id, rect, mode, || false)
    }
    /// The host supplies a cooperative signal. No partial ID set is returned.
    pub fn select_rect_cancellable(
        &self,
        layer_id: &str,
        rect: BoundsMm,
        mode: SelectRectMode,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Vec<String>, HitTestError> {
        if cancelled() {
            return Err(HitTestError::Cancelled);
        }
        validate_hit_point(MmPoint::new(rect.min_x_mm, rect.min_y_mm), 0.)?;
        validate_hit_point(MmPoint::new(rect.max_x_mm, rect.max_y_mm), 0.)?;
        if rect.min_x_mm > rect.max_x_mm || rect.min_y_mm > rect.max_y_mm {
            return Err(HitTestError::InvalidArgument("rect_mm"));
        }
        if layer_id.trim().is_empty() {
            return Err(HitTestError::InvalidArgument("layer_id"));
        }
        let layer = self
            .layers
            .iter()
            .find(|l| l.id == layer_id)
            .ok_or_else(|| HitTestError::MissingLayer(layer_id.into()))?;
        let apertures: HashMap<_, _> = self
            .apertures
            .iter()
            .map(|a| (a.id.as_str(), &a.shape))
            .collect();
        let mut macros: HashMap<String, Material> = HashMap::new();
        // Large layers retain exact all-or-error selection without work admission.
        let mut budget = Budget(None);
        let mut result = Vec::new();
        for object in &layer.objects {
            if cancelled() {
                return Err(HitTestError::Cancelled);
            }
            budget.charge(1)?;
            let g = &object.geometry;
            let edges = if let SemanticGeometry::BlockInstance {
                definition_id,
                transform,
            } = g
            {
                let definition = self.block_definition(definition_id).ok_or_else(|| {
                    SemanticError::Invalid(format!("unknown block definition {}", definition_id.0))
                })?;
                let resolved = block::resolve_instance(definition, transform)
                    .map_err(|_| HitTestError::Unsupported("block instance transform"))?;
                let mut all = Vec::new();
                for r in &resolved {
                    budget.charge(1)?;
                    all.extend(edges_for(
                        &r.geometry,
                        &apertures,
                        &mut macros,
                        &mut budget,
                        true,
                    )?);
                }
                all
            } else {
                edges_for(g, &apertures, &mut macros, &mut budget, true)?
            };
            budget.charge(edges.len().saturating_mul(8))?;
            if edges.is_empty() {
                continue;
            }
            let mut precision =
                roundoff(&[rect.min_x_mm, rect.min_y_mm, rect.max_x_mm, rect.max_y_mm]);
            for e in &edges {
                let b = edge_bounds(e);
                precision = precision.max(roundoff(&[b.min_x, b.max_x, b.min_y, b.max_y]));
                if ![b.min_x, b.max_x, b.min_y, b.max_y]
                    .into_iter()
                    .all(f64::is_finite)
                {
                    return Err(HitTestError::Unsupported("non-finite selection boundary"));
                }
            }
            if precision > EPSILON_MM {
                return Err(HitTestError::Unsupported(
                    "selection numerical uncertainty exceeds manufacturing tolerance",
                ));
            }
            let hit = match mode {
                SelectRectMode::Window => edges.iter().all(|e| {
                    // Extrema of the actual material curves, never conservative object bounds.
                    let b = edge_bounds(e);
                    contains(rect, MmPoint::new(b.min_x, b.min_y), precision)
                        && contains(rect, MmPoint::new(b.max_x, b.max_y), precision)
                }),
                SelectRectMode::Crossing => {
                    let mut hit = false;
                    for e in &edges {
                        if crosses(e, rect, precision)? {
                            hit = true;
                            break;
                        }
                    }
                    if !hit {
                        let p = MmPoint::new(rect.min_x_mm, rect.min_y_mm);
                        let distance = if let SemanticGeometry::BlockInstance {
                            definition_id,
                            transform,
                        } = g
                        {
                            let definition =
                                self.block_definition(definition_id).ok_or_else(|| {
                                    SemanticError::Invalid(format!(
                                        "unknown block definition {}",
                                        definition_id.0
                                    ))
                                })?;
                            let resolved =
                                block::resolve_instance(definition, transform).map_err(|_| {
                                    HitTestError::Unsupported("block instance transform")
                                })?;
                            let mut nearest = f64::INFINITY;
                            for r in &resolved {
                                budget.charge(1)?;
                                nearest = nearest.min(distance_for(
                                    &r.geometry,
                                    p,
                                    &apertures,
                                    &mut macros,
                                    &mut budget,
                                )?);
                            }
                            nearest
                        } else {
                            distance_for(g, p, &apertures, &mut macros, &mut budget)?
                        };
                        hit = distance <= precision;
                    }
                    hit
                }
            };
            if hit {
                result.push(object.object_id.clone());
            }
        }
        Ok(result)
    }
}
