//! Analytic Object Snap over manufacturing geometry.
//!
//! The product path is deliberately local: physical screen radius -> object
//! spatial query -> nearby objects only -> lazy features/intersections -> one
//! resolver shared with Grid Snap. No renderer mesh or document-wide snap
//! point database participates.

use crate::block::{BlockDefinition, BlockDefinitionId, BlockTransform};
use crate::{
    ApertureDefinition, ArcDirection, ArcGeometry, BoundsMm, MmPoint, RegionEdge, SemanticGeometry,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapKind {
    Endpoint,
    Vertex,
    Midpoint,
    Center,
    Quadrant,
    ArcCenter,
    Intersection,
    Nearest,
    Perpendicular,
    Tangent,
}

impl SnapKind {
    pub fn priority(self) -> u8 {
        match self {
            Self::Endpoint | Self::Vertex => 0,
            Self::Intersection => 1,
            Self::Midpoint => 2,
            Self::Quadrant => 3,
            Self::Center | Self::ArcCenter => 4,
            Self::Nearest => 5,
            Self::Perpendicular | Self::Tangent => 6,
        }
    }
}

/// Stable identity inside one unchanged object. Block ids scope the local
/// feature by definition-object slot so every instance can reuse the cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapFeatureId {
    Vertex { index: u32 },
    Endpoint { index: u32 },
    EdgeMidpoint { index: u32 },
    ArcMidpoint { index: u32 },
    ArcCenter { index: u32 },
    Quadrant { index: u32 },
    Center,
    Nearest { edge: u64 },
    Intersection { first: u64, second: u64, point: u8 },
    Block { object: u32, feature: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapSource {
    ManufacturingBoundary,
    OriginalPath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapFeature {
    pub id: SnapFeatureId,
    pub kind: SnapKind,
    pub point: MmPoint,
    pub source: SnapSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapQuery {
    pub center: MmPoint,
    /// Radius used by the resolver to acquire a new object candidate.
    pub acquire_radius_mm: f64,
    /// Wider radius used only to generate candidates so an existing candidate
    /// can remain available through the hysteresis release band.
    pub candidate_radius_mm: f64,
    pub kinds: Vec<SnapKind>,
    pub manufacturing_boundary: bool,
    pub original_path: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapRadiiPx {
    pub acquire: f64,
    pub candidate: f64,
}

impl SnapQuery {
    /// `points_per_mm` is the egui camera scale. Multiplication by
    /// `pixels_per_point` makes the configured radius physical-pixel stable.
    pub fn from_screen(
        center: MmPoint,
        radii_physical_px: SnapRadiiPx,
        points_per_mm: f64,
        pixels_per_point: f64,
        kinds: Vec<SnapKind>,
        manufacturing_boundary: bool,
        original_path: bool,
    ) -> Option<Self> {
        let pixels_per_mm = points_per_mm * pixels_per_point;
        (center.is_finite()
            && radii_physical_px.acquire.is_finite()
            && radii_physical_px.acquire > 0.
            && radii_physical_px.candidate.is_finite()
            && radii_physical_px.candidate >= radii_physical_px.acquire
            && pixels_per_mm.is_finite()
            && pixels_per_mm > 0.)
            .then(|| Self {
                center,
                acquire_radius_mm: radii_physical_px.acquire / pixels_per_mm,
                candidate_radius_mm: radii_physical_px.candidate / pixels_per_mm,
                kinds,
                manufacturing_boundary,
                original_path,
            })
    }

    pub fn source_enabled(&self, source: SnapSource) -> bool {
        match source {
            SnapSource::ManufacturingBoundary => self.manufacturing_boundary,
            SnapSource::OriginalPath => self.original_path,
        }
    }

    pub fn bounds(&self) -> BoundsMm {
        let radius = self.candidate_radius_mm;
        BoundsMm {
            min_x_mm: self.center.x_mm - radius,
            min_y_mm: self.center.y_mm - radius,
            max_x_mm: self.center.x_mm + radius,
            max_y_mm: self.center.y_mm + radius,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapCandidateId {
    pub layer_id: String,
    pub object_id: String,
    pub related_object_id: Option<String>,
    pub feature_id: SnapFeatureId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapCandidate {
    pub layer_id: String,
    pub object_id: String,
    pub related_object_id: Option<String>,
    pub feature: SnapFeature,
    pub distance_mm: f64,
}

impl SnapCandidate {
    pub fn id(&self) -> SnapCandidateId {
        SnapCandidateId {
            layer_id: self.layer_id.clone(),
            object_id: self.object_id.clone(),
            related_object_id: self.related_object_id.clone(),
            feature_id: self.feature.id.clone(),
        }
    }
}

pub trait SnapFeatureProvider {
    fn snap_features(&self, query: &SnapQuery) -> Vec<SnapFeature>;
}

#[derive(Debug, Clone, PartialEq)]
struct SnapEdge {
    id: u64,
    edge: RegionEdge,
    source: SnapSource,
    bounds: EdgeBounds,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EdgeBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl EdgeBounds {
    fn intersects(self, query: BoundsMm) -> bool {
        self.min_x <= query.max_x_mm
            && self.max_x >= query.min_x_mm
            && self.min_y <= query.max_y_mm
            && self.max_y >= query.min_y_mm
    }
}

/// Static analytic features/edges for one object. Nearest and intersections
/// remain query-dependent and are never cached as global points.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapGeometry {
    features: Vec<SnapFeature>,
    edges: Vec<SnapEdge>,
}

impl SnapFeatureProvider for SnapGeometry {
    fn snap_features(&self, query: &SnapQuery) -> Vec<SnapFeature> {
        let mut result: Vec<_> = self
            .features
            .iter()
            .filter(|feature| {
                query.kinds.contains(&feature.kind)
                    && query.source_enabled(feature.source)
                    && feature.point.distance_mm(query.center) <= query.candidate_radius_mm
            })
            .cloned()
            .collect();
        if query.kinds.contains(&SnapKind::Nearest) {
            for edge in self.nearby_edges(query) {
                let point = nearest_on_edge(query.center, &edge.edge);
                if point.distance_mm(query.center) <= query.candidate_radius_mm {
                    push_feature(
                        &mut result,
                        SnapFeature {
                            id: SnapFeatureId::Nearest { edge: edge.id },
                            kind: SnapKind::Nearest,
                            point,
                            source: edge.source,
                        },
                    );
                }
            }
        }
        dedup_features(&mut result);
        result
    }
}

impl SnapGeometry {
    fn nearby_edges(&self, query: &SnapQuery) -> Vec<&SnapEdge> {
        let bounds = query.bounds();
        self.edges
            .iter()
            .filter(|edge| query.source_enabled(edge.source) && edge.bounds.intersects(bounds))
            .collect()
    }

    fn append_scoped(&mut self, local: &SnapGeometry, object: u32, transform: BlockTransform) {
        let t = transform.to_coordinate_transform();
        for feature in &local.features {
            self.features.push(SnapFeature {
                id: SnapFeatureId::Block {
                    object,
                    feature: feature_code(&feature.id),
                },
                point: t.apply(feature.point),
                ..feature.clone()
            });
        }
        for edge in &local.edges {
            let edge_id = (u64::from(object) << 32) | (edge.id & u64::from(u32::MAX));
            let transformed = transform_edge(&edge.edge, &t, transform.mirror);
            self.edges.push(SnapEdge {
                id: edge_id,
                bounds: edge_bounds(&transformed),
                edge: transformed,
                source: edge.source,
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapBuildError {
    Geometry(String),
    UnknownBlock(BlockDefinitionId),
    BlockTransform,
}

impl std::fmt::Display for SnapBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SnapBuildError {}

/// Only BlockDefinition-local analytic geometry is cached. Normal objects are
/// generated lazily after the spatial query; changing definition revision
/// invalidates the local entry before every instance transform.
#[derive(Default)]
pub struct SnapGeometryCache {
    blocks: HashMap<(String, u64), Vec<SnapGeometry>>,
}

impl SnapGeometryCache {
    pub fn clear(&mut self) {
        self.blocks.clear();
    }

    pub fn geometry_for(
        &mut self,
        geometry: &SemanticGeometry,
        apertures: &[ApertureDefinition],
        blocks: &[BlockDefinition],
    ) -> Result<SnapGeometry, SnapBuildError> {
        let SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } = geometry
        else {
            return build_geometry(geometry, apertures);
        };
        let definition = blocks
            .iter()
            .find(|definition| definition.id == *definition_id)
            .ok_or_else(|| SnapBuildError::UnknownBlock(definition_id.clone()))?;
        if !transform.is_valid() {
            return Err(SnapBuildError::BlockTransform);
        }
        let key = (definition.id.0.clone(), definition.revision);
        if !self.blocks.contains_key(&key) {
            self.blocks.retain(|(id, _), _| id != &definition.id.0);
            let local: Result<Vec<_>, _> = definition
                .objects
                .iter()
                .map(|object| build_geometry(&object.geometry.clone().into(), apertures))
                .collect();
            self.blocks.insert(key.clone(), local?);
        }
        let mut result = SnapGeometry::default();
        for (index, local) in self.blocks[&key].iter().enumerate() {
            result.append_scoped(local, index as u32, *transform);
        }
        dedup_features(&mut result.features);
        Ok(result)
    }
}

pub fn intersection_features(
    first: &SnapGeometry,
    second: &SnapGeometry,
    query: &SnapQuery,
) -> (Vec<SnapFeature>, usize) {
    if !query.kinds.contains(&SnapKind::Intersection) {
        return (Vec::new(), 0);
    }
    let first_edges = first.nearby_edges(query);
    let second_edges = second.nearby_edges(query);
    let mut result = Vec::new();
    let mut pairs = 0;
    for a in first_edges {
        for b in &second_edges {
            pairs += 1;
            let mut points = crate::edge_intersection_points(&a.edge, &b.edge);
            points.sort_by(|left, right| {
                left.x_mm
                    .total_cmp(&right.x_mm)
                    .then(left.y_mm.total_cmp(&right.y_mm))
            });
            for (point_index, point) in points.into_iter().enumerate() {
                if point.distance_mm(query.center) <= query.candidate_radius_mm {
                    let (first_id, second_id) = if a.id <= b.id {
                        (a.id, b.id)
                    } else {
                        (b.id, a.id)
                    };
                    push_feature(
                        &mut result,
                        SnapFeature {
                            id: SnapFeatureId::Intersection {
                                first: first_id,
                                second: second_id,
                                point: point_index.min(usize::from(u8::MAX)) as u8,
                            },
                            kind: SnapKind::Intersection,
                            point,
                            source: if a.source == SnapSource::ManufacturingBoundary
                                && b.source == SnapSource::ManufacturingBoundary
                            {
                                SnapSource::ManufacturingBoundary
                            } else {
                                SnapSource::OriginalPath
                            },
                        },
                    );
                }
            }
        }
    }
    dedup_features(&mut result);
    (result, pairs)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapResolution {
    pub point: MmPoint,
    pub kind: Option<SnapKind>,
    pub feature: Option<SnapFeatureId>,
    pub source: Option<SnapSource>,
    pub candidate: Option<SnapCandidateId>,
    pub distance_px: Option<f64>,
    pub from_grid: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SnapResolver {
    pub release_extra_px: f64,
    pub kind_penalty_px: f64,
}

impl Default for SnapResolver {
    fn default() -> Self {
        Self {
            release_extra_px: 3.,
            kind_penalty_px: 0.35,
        }
    }
}

impl SnapResolver {
    pub fn resolve(
        &self,
        raw: MmPoint,
        pixels_per_mm: f64,
        radius_physical_px: f64,
        object_candidates: &[SnapCandidate],
        grid: Option<MmPoint>,
        previous: Option<&SnapCandidateId>,
    ) -> SnapResolution {
        let distance_px = |candidate: &SnapCandidate| candidate.distance_mm * pixels_per_mm;
        if let Some(previous) = previous
            && let Some(candidate) = object_candidates.iter().find(|candidate| {
                candidate.id() == *previous
                    && distance_px(candidate) <= radius_physical_px + self.release_extra_px
            })
        {
            return candidate_resolution(candidate, distance_px(candidate));
        }
        let score = |candidate: &SnapCandidate| {
            distance_px(candidate)
                + f64::from(candidate.feature.kind.priority()) * self.kind_penalty_px
        };
        if let Some(candidate) = object_candidates
            .iter()
            .filter(|candidate| distance_px(candidate) <= radius_physical_px)
            .min_by(|left, right| {
                score(left)
                    .total_cmp(&score(right))
                    .then_with(|| left.id().cmp(&right.id()))
            })
        {
            return candidate_resolution(candidate, distance_px(candidate));
        }
        match grid {
            Some(point) => SnapResolution {
                point,
                kind: None,
                feature: None,
                source: None,
                candidate: None,
                distance_px: Some(raw.distance_mm(point) * pixels_per_mm),
                from_grid: true,
            },
            None => SnapResolution {
                point: raw,
                kind: None,
                feature: None,
                source: None,
                candidate: None,
                distance_px: None,
                from_grid: false,
            },
        }
    }
}

fn candidate_resolution(candidate: &SnapCandidate, distance_px: f64) -> SnapResolution {
    SnapResolution {
        point: candidate.feature.point,
        kind: Some(candidate.feature.kind),
        feature: Some(candidate.feature.id.clone()),
        source: Some(candidate.feature.source),
        candidate: Some(candidate.id()),
        distance_px: Some(distance_px),
        from_grid: false,
    }
}

fn build_geometry(
    geometry: &SemanticGeometry,
    apertures: &[ApertureDefinition],
) -> Result<SnapGeometry, SnapBuildError> {
    let boundary = crate::hit_test::manufacturing_boundary_edges(geometry, apertures)
        .map_err(|error| SnapBuildError::Geometry(error.to_string()))?;
    let mut result = SnapGeometry::default();
    let obround = matches!(geometry, SemanticGeometry::Flash { aperture_id, .. } if apertures.iter().any(|a| a.id == *aperture_id && matches!(a.shape, crate::ApertureShape::Obround { .. })));
    append_edges(
        &mut result,
        boundary,
        SnapSource::ManufacturingBoundary,
        obround,
    );
    match geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            ..
        } => {
            push_feature(
                &mut result.features,
                feature(
                    SnapFeatureId::Center,
                    SnapKind::Center,
                    *center,
                    SnapSource::ManufacturingBoundary,
                ),
            );
            let circle = apertures.iter().any(|aperture| {
                aperture.id == *aperture_id
                    && matches!(aperture.shape, crate::ApertureShape::Circle { .. })
            });
            if circle
                && let Some(RegionEdge::Arc(circle)) = result.edges.first().map(|edge| &edge.edge)
            {
                result.features.retain(|candidate| {
                    !matches!(
                        candidate.kind,
                        SnapKind::Vertex | SnapKind::Midpoint | SnapKind::ArcCenter
                    )
                });
                append_quadrants(
                    &mut result.features,
                    *circle,
                    0,
                    SnapSource::ManufacturingBoundary,
                );
            }
        }
        SemanticGeometry::Line { start, end, .. }
        | SemanticGeometry::RectangularSweep { start, end, .. } => {
            append_path_line(&mut result, *start, *end);
        }
        SemanticGeometry::Arc { path, .. } => append_path_arc(&mut result, *path),
        SemanticGeometry::Region { .. } => {}
        SemanticGeometry::BlockInstance { .. } => unreachable!("resolved by SnapGeometryCache"),
    }
    dedup_features(&mut result.features);
    Ok(result)
}

fn append_edges(
    geometry: &mut SnapGeometry,
    edges: Vec<RegionEdge>,
    source: SnapSource,
    obround_quadrants: bool,
) {
    for (index, edge) in edges.into_iter().enumerate() {
        let index = index as u32;
        // Every analytic edge owns a disjoint pair of endpoint ids. Adjacent
        // contour vertices are de-duplicated by position, while disconnected
        // macro/region contours must never alias each other's stable ids.
        let start_index = index.saturating_mul(2);
        let end_index = start_index.saturating_add(1);
        match edge {
            RegionEdge::Line { start, end } => {
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::Vertex { index: start_index },
                        SnapKind::Vertex,
                        start,
                        source,
                    ),
                );
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::Vertex { index: end_index },
                        SnapKind::Vertex,
                        end,
                        source,
                    ),
                );
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::EdgeMidpoint { index },
                        SnapKind::Midpoint,
                        midpoint(start, end),
                        source,
                    ),
                );
            }
            RegionEdge::Arc(arc) => {
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::Vertex { index: start_index },
                        SnapKind::Vertex,
                        arc.start,
                        source,
                    ),
                );
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::Vertex { index: end_index },
                        SnapKind::Vertex,
                        arc.end,
                        source,
                    ),
                );
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::ArcCenter { index },
                        SnapKind::Center,
                        arc.center,
                        source,
                    ),
                );
                push_feature(
                    &mut geometry.features,
                    feature(
                        SnapFeatureId::ArcMidpoint { index },
                        if obround_quadrants {
                            SnapKind::Quadrant
                        } else {
                            SnapKind::Midpoint
                        },
                        arc_point(arc, 0.5),
                        source,
                    ),
                );
                append_quadrants(&mut geometry.features, arc, index.saturating_mul(4), source);
            }
        }
        let id = u64::from(index);
        geometry.edges.push(SnapEdge {
            id,
            bounds: edge_bounds(&edge),
            edge,
            source,
        });
    }
}

fn append_path_line(geometry: &mut SnapGeometry, start: MmPoint, end: MmPoint) {
    let source = SnapSource::OriginalPath;
    for (id, kind, point) in [
        (
            SnapFeatureId::Endpoint { index: 0 },
            SnapKind::Endpoint,
            start,
        ),
        (
            SnapFeatureId::Endpoint { index: 1 },
            SnapKind::Endpoint,
            end,
        ),
        (
            SnapFeatureId::EdgeMidpoint { index: 0 },
            SnapKind::Midpoint,
            midpoint(start, end),
        ),
    ] {
        push_feature(&mut geometry.features, feature(id, kind, point, source));
    }
    let edge = RegionEdge::Line { start, end };
    geometry.edges.push(SnapEdge {
        id: 1_u64 << 63,
        bounds: edge_bounds(&edge),
        edge,
        source,
    });
}

fn append_path_arc(geometry: &mut SnapGeometry, arc: ArcGeometry) {
    let source = SnapSource::OriginalPath;
    for (id, kind, point) in [
        (
            SnapFeatureId::Endpoint { index: 0 },
            SnapKind::Endpoint,
            arc.start,
        ),
        (
            SnapFeatureId::Endpoint { index: 1 },
            SnapKind::Endpoint,
            arc.end,
        ),
        (
            SnapFeatureId::ArcMidpoint { index: 0 },
            SnapKind::Midpoint,
            arc_point(arc, 0.5),
        ),
        (
            SnapFeatureId::ArcCenter { index: 0 },
            SnapKind::Center,
            arc.center,
        ),
    ] {
        push_feature(&mut geometry.features, feature(id, kind, point, source));
    }
    append_quadrants(&mut geometry.features, arc, 0, source);
    let edge = RegionEdge::Arc(arc);
    geometry.edges.push(SnapEdge {
        id: 1_u64 << 63,
        bounds: edge_bounds(&edge),
        edge,
        source,
    });
}

fn append_quadrants(
    features: &mut Vec<SnapFeature>,
    arc: ArcGeometry,
    base: u32,
    source: SnapSource,
) {
    let radius = arc.radius();
    for (quadrant, angle) in [
        0.,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
        3. * std::f64::consts::FRAC_PI_2,
    ]
    .into_iter()
    .enumerate()
    {
        let point = MmPoint::new(
            arc.center.x_mm + radius * angle.cos(),
            arc.center.y_mm + radius * angle.sin(),
        );
        if arc.full_circle || crate::point_on_arc(point, arc) {
            push_feature(
                features,
                feature(
                    SnapFeatureId::Quadrant {
                        index: base.saturating_add(quadrant as u32),
                    },
                    SnapKind::Quadrant,
                    point,
                    source,
                ),
            );
        }
    }
}

fn feature(id: SnapFeatureId, kind: SnapKind, point: MmPoint, source: SnapSource) -> SnapFeature {
    SnapFeature {
        id,
        kind,
        point,
        source,
    }
}

fn push_feature(features: &mut Vec<SnapFeature>, feature: SnapFeature) {
    features.push(feature);
}

/// Analytic contours commonly repeat a shared endpoint. Sort once and remove
/// those duplicates in O(n log n); checking the growing vector for every edge
/// made a single dense Region quadratic before it even reached the query.
fn dedup_features(features: &mut Vec<SnapFeature>) {
    features.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.source.cmp(&right.source))
            .then(left.point.x_mm.total_cmp(&right.point.x_mm))
            .then(left.point.y_mm.total_cmp(&right.point.y_mm))
            .then(left.id.cmp(&right.id))
    });
    let mut unique: Vec<SnapFeature> = Vec::with_capacity(features.len());
    for feature in features.drain(..) {
        if unique.last().is_none_or(|previous| {
            previous.kind != feature.kind
                || previous.source != feature.source
                || previous.point.distance_mm(feature.point) > crate::EPSILON_MM
        }) {
            unique.push(feature);
        }
    }
    *features = unique;
}

fn midpoint(a: MmPoint, b: MmPoint) -> MmPoint {
    MmPoint::new(
        a.x_mm + (b.x_mm - a.x_mm) / 2.,
        a.y_mm + (b.y_mm - a.y_mm) / 2.,
    )
}

fn arc_point(arc: ArcGeometry, fraction: f64) -> MmPoint {
    if arc.zero_sweep() {
        return arc.start;
    }
    let start = (arc.start.y_mm - arc.center.y_mm).atan2(arc.start.x_mm - arc.center.x_mm);
    let sweep = arc.sweep_radians().unwrap_or(0.);
    let sign = if arc.direction == ArcDirection::CounterClockwise {
        1.
    } else {
        -1.
    };
    let angle = start + sign * sweep * fraction;
    MmPoint::new(
        arc.center.x_mm + arc.radius() * angle.cos(),
        arc.center.y_mm + arc.radius() * angle.sin(),
    )
}

fn nearest_on_edge(point: MmPoint, edge: &RegionEdge) -> MmPoint {
    match edge {
        RegionEdge::Line { start, end } => {
            let dx = end.x_mm - start.x_mm;
            let dy = end.y_mm - start.y_mm;
            let length_squared = dx * dx + dy * dy;
            if length_squared == 0. {
                *start
            } else {
                let t = (((point.x_mm - start.x_mm) * dx + (point.y_mm - start.y_mm) * dy)
                    / length_squared)
                    .clamp(0., 1.);
                MmPoint::new(start.x_mm + t * dx, start.y_mm + t * dy)
            }
        }
        RegionEdge::Arc(arc) => {
            let distance = point.distance_mm(arc.center);
            if distance > 0. {
                let projected = MmPoint::new(
                    arc.center.x_mm + (point.x_mm - arc.center.x_mm) * arc.radius() / distance,
                    arc.center.y_mm + (point.y_mm - arc.center.y_mm) * arc.radius() / distance,
                );
                if arc.full_circle || crate::point_on_arc(projected, *arc) {
                    return projected;
                }
            }
            if point.distance_mm(arc.start) <= point.distance_mm(arc.end) {
                arc.start
            } else {
                arc.end
            }
        }
    }
}

fn edge_bounds(edge: &RegionEdge) -> EdgeBounds {
    match edge {
        RegionEdge::Line { start, end } => EdgeBounds {
            min_x: start.x_mm.min(end.x_mm),
            min_y: start.y_mm.min(end.y_mm),
            max_x: start.x_mm.max(end.x_mm),
            max_y: start.y_mm.max(end.y_mm),
        },
        RegionEdge::Arc(arc) => {
            let radius = arc.radius();
            EdgeBounds {
                min_x: arc.center.x_mm - radius,
                min_y: arc.center.y_mm - radius,
                max_x: arc.center.x_mm + radius,
                max_y: arc.center.y_mm + radius,
            }
        }
    }
}

fn transform_edge(
    edge: &RegionEdge,
    transform: &crate::board::CoordinateTransform2D,
    reflected: bool,
) -> RegionEdge {
    match edge {
        RegionEdge::Line { start, end } => RegionEdge::Line {
            start: transform.apply(*start),
            end: transform.apply(*end),
        },
        RegionEdge::Arc(arc) => RegionEdge::Arc(ArcGeometry {
            start: transform.apply(arc.start),
            end: transform.apply(arc.end),
            center: transform.apply(arc.center),
            direction: if reflected {
                match arc.direction {
                    ArcDirection::Clockwise => ArcDirection::CounterClockwise,
                    ArcDirection::CounterClockwise => ArcDirection::Clockwise,
                }
            } else {
                arc.direction
            },
            ..*arc
        }),
    }
}

fn feature_code(id: &SnapFeatureId) -> u64 {
    match id {
        SnapFeatureId::Vertex { index } => u64::from(*index),
        SnapFeatureId::Endpoint { index } => (1_u64 << 56) | u64::from(*index),
        SnapFeatureId::EdgeMidpoint { index } => (2_u64 << 56) | u64::from(*index),
        SnapFeatureId::ArcMidpoint { index } => (3_u64 << 56) | u64::from(*index),
        SnapFeatureId::ArcCenter { index } => (4_u64 << 56) | u64::from(*index),
        SnapFeatureId::Quadrant { index } => (5_u64 << 56) | u64::from(*index),
        SnapFeatureId::Center => 6_u64 << 56,
        SnapFeatureId::Nearest { edge } => (7_u64 << 56) | (edge & ((1_u64 << 56) - 1)),
        SnapFeatureId::Intersection { .. } | SnapFeatureId::Block { .. } => 8_u64 << 56,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockObject, BlockObjectGeometry};
    use crate::{ApertureShape, Exposure, LocalTransform};

    fn aperture(id: &str, shape: ApertureShape) -> ApertureDefinition {
        ApertureDefinition {
            id: id.into(),
            source_dcode: 10,
            shape,
        }
    }

    fn query(center: MmPoint, kinds: Vec<SnapKind>) -> SnapQuery {
        SnapQuery::from_screen(
            center,
            SnapRadiiPx {
                acquire: 8.,
                candidate: 8.,
            },
            10.,
            2.,
            kinds,
            true,
            false,
        )
        .unwrap()
    }

    #[test]
    fn retina_radius_converts_physical_pixels_once() {
        let q = query(MmPoint::new(1., 2.), vec![SnapKind::Endpoint]);
        assert!((q.acquire_radius_mm - 0.4).abs() < 1e-12);
        assert!((q.candidate_radius_mm - 0.4).abs() < 1e-12);
        assert!(
            SnapQuery::from_screen(
                MmPoint::new(0., 0.),
                SnapRadiiPx {
                    acquire: 8.,
                    candidate: 11.,
                },
                40.,
                0.,
                vec![],
                true,
                false,
            )
            .is_none()
        );
        assert!(
            SnapQuery::from_screen(
                MmPoint::new(0., 0.),
                SnapRadiiPx {
                    acquire: 11.,
                    candidate: 8.,
                },
                40.,
                1.,
                vec![],
                true,
                false,
            )
            .is_none(),
            "candidate radius cannot be narrower than acquire radius"
        );
    }

    #[test]
    fn rectangle_circle_polygon_and_obround_use_real_boundaries() {
        let cases = [
            (
                "r",
                ApertureShape::Rectangle {
                    width_mm: 4.,
                    height_mm: 2.,
                    hole_diameter_mm: None,
                },
                9,
            ),
            (
                "p",
                ApertureShape::Polygon {
                    diameter_mm: 4.,
                    vertices: 5,
                    rotation_deg: 18.,
                    hole_diameter_mm: None,
                },
                11,
            ),
            (
                "o",
                ApertureShape::Obround {
                    width_mm: 6.,
                    height_mm: 2.,
                    hole_diameter_mm: None,
                },
                9,
            ),
        ];
        for (id, shape, minimum) in cases {
            let apertures = [aperture(id, shape)];
            let geometry = SemanticGeometry::Flash {
                center: MmPoint::new(10., 20.),
                aperture_id: id.into(),
                transform: LocalTransform::default(),
            };
            let built = build_geometry(&geometry, &apertures).unwrap();
            assert!(built.features.len() >= minimum, "{id} {:?}", built.features);
            assert!(
                built
                    .features
                    .iter()
                    .any(|f| { f.kind == SnapKind::Center && f.point == MmPoint::new(10., 20.) })
            );
        }
        let circle = aperture(
            "c",
            ApertureShape::Circle {
                diameter_mm: 4.,
                hole_diameter_mm: Some(1.),
            },
        );
        let geometry = SemanticGeometry::Flash {
            center: MmPoint::new(2., 3.),
            aperture_id: "c".into(),
            transform: LocalTransform::default(),
        };
        let built = build_geometry(&geometry, &[circle]).unwrap();
        assert_eq!(
            built
                .features
                .iter()
                .filter(|f| f.kind == SnapKind::Quadrant)
                .count(),
            4
        );
        assert_eq!(
            built.edges.len(),
            1,
            "standard hole boundary is intentionally excluded in S4-C1"
        );
    }

    #[test]
    fn nearest_is_analytic_and_disabled_by_default() {
        let geometry = SemanticGeometry::Region {
            contours: vec![crate::RegionContour {
                role: crate::RegionRole::Solid,
                edges: vec![RegionEdge::Line {
                    start: MmPoint::new(0., 0.),
                    end: MmPoint::new(10., 0.),
                }],
            }],
        };
        let built = build_geometry(&geometry, &[]).unwrap();
        let disabled = query(MmPoint::new(4., 0.2), vec![SnapKind::Vertex]);
        assert!(built.snap_features(&disabled).is_empty());
        let nearest = query(MmPoint::new(4., 0.2), vec![SnapKind::Nearest]);
        assert_eq!(built.snap_features(&nearest)[0].point, MmPoint::new(4., 0.));
    }

    fn candidate(
        object: &str,
        kind: SnapKind,
        id: SnapFeatureId,
        distance_mm: f64,
    ) -> SnapCandidate {
        SnapCandidate {
            layer_id: "layer".into(),
            object_id: object.into(),
            related_object_id: None,
            feature: feature(
                id,
                kind,
                MmPoint::new(distance_mm, 0.),
                SnapSource::ManufacturingBoundary,
            ),
            distance_mm,
        }
    }

    #[test]
    fn resolver_object_grid_distance_priority_and_hysteresis() {
        let resolver = SnapResolver::default();
        let vertex = candidate(
            "a",
            SnapKind::Vertex,
            SnapFeatureId::Vertex { index: 0 },
            0.08,
        );
        let center = candidate("b", SnapKind::Center, SnapFeatureId::Center, 0.01);
        let hit = resolver.resolve(
            MmPoint::new(0., 0.),
            100.,
            8.,
            &[vertex.clone(), center],
            Some(MmPoint::new(1., 1.)),
            None,
        );
        assert_eq!(
            hit.kind,
            Some(SnapKind::Center),
            "screen distance outweighs type"
        );
        let previous = vertex.id();
        let sticky = resolver.resolve(
            MmPoint::new(0., 0.),
            100.,
            8.,
            &[vertex],
            None,
            Some(&previous),
        );
        assert_eq!(sticky.candidate, Some(previous));
        let grid = resolver.resolve(
            MmPoint::new(0., 0.),
            100.,
            8.,
            &[],
            Some(MmPoint::new(1., 1.)),
            None,
        );
        assert!(grid.from_grid);
    }

    #[test]
    fn analytic_intersections_are_query_bounded() {
        let region = |edge| {
            build_geometry(
                &SemanticGeometry::Region {
                    contours: vec![crate::RegionContour {
                        role: crate::RegionRole::Solid,
                        edges: vec![edge],
                    }],
                },
                &[],
            )
            .unwrap()
        };
        let line_a = region(RegionEdge::Line {
            start: MmPoint::new(-2., 0.),
            end: MmPoint::new(2., 0.),
        });
        let line_b = region(RegionEdge::Line {
            start: MmPoint::new(0., -2.),
            end: MmPoint::new(0., 2.),
        });
        let q = SnapQuery::from_screen(
            MmPoint::new(0., 0.),
            SnapRadiiPx {
                acquire: 30.,
                candidate: 30.,
            },
            10.,
            1.,
            vec![SnapKind::Intersection],
            true,
            false,
        )
        .unwrap();
        assert_eq!(intersection_features(&line_a, &line_b, &q).0.len(), 1);
        let circle = |center: MmPoint| {
            region(RegionEdge::Arc(ArcGeometry {
                start: MmPoint::new(center.x_mm + 1., center.y_mm),
                end: MmPoint::new(center.x_mm + 1., center.y_mm),
                center,
                direction: ArcDirection::CounterClockwise,
                full_circle: true,
                source: None,
            }))
        };
        assert_eq!(
            intersection_features(&line_a, &circle(MmPoint::new(0., 0.)), &q)
                .0
                .len(),
            2
        );
        assert_eq!(
            intersection_features(
                &circle(MmPoint::new(-1., 0.)),
                &circle(MmPoint::new(1., 0.)),
                &q
            )
            .0
            .len(),
            1
        );
        let far = query(MmPoint::new(100., 100.), vec![SnapKind::Intersection]);
        assert_eq!(intersection_features(&line_a, &line_b, &far).1, 0);
    }

    #[test]
    fn region_hole_compatibility_and_arc_keep_analytic_features() {
        let square = |min: f64, max: f64| {
            let points = [
                MmPoint::new(min, min),
                MmPoint::new(max, min),
                MmPoint::new(max, max),
                MmPoint::new(min, max),
            ];
            points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
                .map(|(start, end)| RegionEdge::Line {
                    start: *start,
                    end: *end,
                })
                .collect()
        };
        let geometry = SemanticGeometry::Region {
            contours: vec![
                crate::RegionContour {
                    role: crate::RegionRole::CompatibilitySolid,
                    edges: square(-2., 2.),
                },
                crate::RegionContour {
                    role: crate::RegionRole::Hole,
                    edges: square(-1., 1.),
                },
                crate::RegionContour {
                    role: crate::RegionRole::Solid,
                    edges: vec![RegionEdge::Arc(ArcGeometry {
                        start: MmPoint::new(4., 0.),
                        end: MmPoint::new(3., 1.),
                        center: MmPoint::new(3., 0.),
                        direction: ArcDirection::CounterClockwise,
                        full_circle: false,
                        source: None,
                    })],
                },
            ],
        };
        let built = build_geometry(&geometry, &[]).unwrap();
        assert_eq!(built.edges.len(), 9);
        assert!(built.features.iter().any(|feature| {
            feature.kind == SnapKind::Vertex && feature.point == MmPoint::new(-1., -1.)
        }));
        assert_eq!(
            built
                .features
                .iter()
                .filter(|feature| feature.kind == SnapKind::Quadrant)
                .count(),
            2,
            "only quadrants on the finite quarter arc are candidates"
        );
    }

    #[test]
    fn block_features_reuse_definition_local_geometry_and_apply_rigid_transform() {
        let mut definition = BlockDefinition {
            id: BlockDefinitionId("block-a".into()),
            name: "fixture".into(),
            local_origin: MmPoint::new(0., 0.),
            objects: vec![BlockObject {
                geometry: BlockObjectGeometry::Line {
                    start: MmPoint::new(0., 0.),
                    end: MmPoint::new(2., 0.),
                    width_mm: 0.2,
                },
                exposure: Exposure::Dark,
            }],
            revision: 1,
        };
        let instance = SemanticGeometry::BlockInstance {
            definition_id: definition.id.clone(),
            transform: BlockTransform {
                translation: MmPoint::new(10., 20.),
                rotation_deg: 90.,
                mirror: true,
            },
        };
        let mut cache = SnapGeometryCache::default();
        let built = cache
            .geometry_for(&instance, &[], std::slice::from_ref(&definition))
            .unwrap();
        let query = SnapQuery::from_screen(
            MmPoint::new(10., 18.),
            SnapRadiiPx {
                acquire: 8.,
                candidate: 8.,
            },
            10.,
            1.,
            vec![SnapKind::Endpoint],
            false,
            true,
        )
        .unwrap();
        let features = built.snap_features(&query);
        assert!(features.iter().any(|feature| {
            feature.point.distance_mm(MmPoint::new(10., 18.)) < 1e-9
                && matches!(feature.id, SnapFeatureId::Block { object: 0, .. })
        }));

        definition.revision = 2;
        definition.objects[0].geometry = BlockObjectGeometry::Line {
            start: MmPoint::new(0., 0.),
            end: MmPoint::new(4., 0.),
            width_mm: 0.2,
        };
        let rebuilt = cache.geometry_for(&instance, &[], &[definition]).unwrap();
        let query = SnapQuery::from_screen(
            MmPoint::new(10., 16.),
            SnapRadiiPx {
                acquire: 8.,
                candidate: 8.,
            },
            10.,
            1.,
            vec![SnapKind::Endpoint],
            false,
            true,
        )
        .unwrap();
        assert!(
            rebuilt
                .snap_features(&query)
                .iter()
                .any(|feature| { feature.point.distance_mm(MmPoint::new(10., 16.)) < 1e-9 })
        );
    }
}
