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
        geometries_bounds_with_blocks(
            self.layers
                .iter()
                .filter(|layer| layer_id.is_none_or(|id| layer.id == id))
                .flat_map(|layer| layer.objects.iter().map(|object| &object.geometry)),
            &self.apertures,
            &self.block_definitions,
        )
    }
}

/// Union bounds for an explicit manufacturing-geometry selection. No block
/// instance can appear in `geometries` here: callers with document context
/// (block-aware) use [`geometries_bounds_with_blocks`] instead.
///
/// This is intentionally independent of renderer meshes and screen pixels.
pub fn geometries_bounds<'a>(
    geometries: impl IntoIterator<Item = &'a SemanticGeometry>,
    apertures: &[ApertureDefinition],
) -> Result<Option<BoundsMm>, SemanticError> {
    geometries_bounds_with_blocks(geometries, apertures, &[])
}

/// Same as [`geometries_bounds`], additionally resolving
/// [`SemanticGeometry::BlockInstance`] leaves against `blocks`. Block bounds
/// use the actual transformed child geometries, not a transformed local AABB.
/// The resolved manufacturing envelope is cached by definition revision and
/// orientation for the duration of the query.
pub fn geometries_bounds_with_blocks<'a>(
    geometries: impl IntoIterator<Item = &'a SemanticGeometry>,
    apertures: &[ApertureDefinition],
    blocks: &[crate::block::BlockDefinition],
) -> Result<Option<BoundsMm>, SemanticError> {
    let apertures: HashMap<_, _> = apertures
        .iter()
        .map(|aperture| (aperture.id.as_str(), &aperture.shape))
        .collect();
    let blocks: HashMap<_, _> = blocks.iter().map(|def| (def.id.0.as_str(), def)).collect();
    let mut block_cache = HashMap::new();
    let mut result = None;
    for geometry in geometries {
        if let Some(bounds) = geometry_bounds(geometry, &apertures, &blocks, &mut block_cache)? {
            result = Some(result.map_or(bounds, |previous: BoundsMm| previous.union(bounds)));
        }
    }
    Ok(result)
}

/// Return one analytic world manufacturing bound per input geometry, sharing
/// aperture, block-definition and resolved-orientation lookup caches across
/// the whole batch. This keeps ordered object selections linear in their size
/// while using the same per-geometry bound implementation as the document
/// union API.
pub fn individual_geometries_bounds_with_blocks<'a>(
    geometries: impl IntoIterator<Item = &'a SemanticGeometry>,
    apertures: &[ApertureDefinition],
    blocks: &[crate::block::BlockDefinition],
) -> Result<Vec<Option<BoundsMm>>, SemanticError> {
    let apertures: HashMap<_, _> = apertures
        .iter()
        .map(|aperture| (aperture.id.as_str(), &aperture.shape))
        .collect();
    let blocks: HashMap<_, _> = blocks.iter().map(|def| (def.id.0.as_str(), def)).collect();
    let mut block_cache = HashMap::new();
    geometries
        .into_iter()
        .map(|geometry| geometry_bounds(geometry, &apertures, &blocks, &mut block_cache))
        .collect()
}

fn geometry_bounds<'a>(
    geometry: &SemanticGeometry,
    apertures: &HashMap<&str, &ApertureShape>,
    blocks: &HashMap<&str, &'a crate::block::BlockDefinition>,
    block_cache: &mut HashMap<(&'a str, u64, u64, bool), Option<BoundsMm>>,
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
                let canonical = derived_region_contour(contour)?;
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
        SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } => {
            let definition = *blocks.get(definition_id.0.as_str()).ok_or_else(|| {
                SemanticError::Invalid(format!("unknown block definition {}", definition_id.0))
            })?;
            let rotation = transform.rotation_deg.rem_euclid(360.0);
            let rotation_deg = if rotation == 0.0 { 0.0 } else { rotation };
            let key = (
                definition.id.0.as_str(),
                definition.revision,
                rotation_deg.to_bits(),
                transform.mirror,
            );
            if !block_cache.contains_key(&key) {
                let orientation = crate::block::BlockTransform {
                    translation: MmPoint::new(0.0, 0.0),
                    rotation_deg,
                    mirror: transform.mirror,
                };
                let resolved =
                    crate::block::resolve_instance(definition, &orientation).map_err(|error| {
                        SemanticError::Invalid(format!(
                            "block {} cannot be resolved for bounds: {error:?}",
                            definition.id.0
                        ))
                    })?;
                let mut oriented = None;
                for object in resolved {
                    if let Some(bounds) =
                        geometry_bounds(&object.geometry, apertures, blocks, block_cache)?
                    {
                        oriented = Some(oriented.map_or(bounds, |p: BoundsMm| p.union(bounds)));
                    }
                }
                block_cache.insert(key, oriented);
            }
            block_cache[&key].map(|bounds| BoundsMm {
                min_x_mm: bounds.min_x_mm + transform.translation.x_mm,
                min_y_mm: bounds.min_y_mm + transform.translation.y_mm,
                max_x_mm: bounds.max_x_mm + transform.translation.x_mm,
                max_y_mm: bounds.max_y_mm + transform.translation.y_mm,
            })
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
