//! Object-local manufacturing handles. No document clones or display geometry.
use crate::edit::EditError;
use crate::*;

pub const MAX_GRIP_FEATURES_PER_OBJECT: usize = 50_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GripFeatureId {
    Start,
    End,
    Radius,
    Left,
    Right,
    Top,
    Bottom,
    Corner { index: u8 },
    Vertex { contour: usize, vertex: usize },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GripFeature {
    pub id: GripFeatureId,
    pub position_mm: MmPoint,
}
#[derive(Debug, Clone, PartialEq)]
pub struct GripEditPreview {
    pub geometry: SemanticGeometry,
    pub aperture_shape: Option<ApertureShape>,
}
fn world(p: MmPoint, center: MmPoint, t: LocalTransform) -> MmPoint {
    let x = p.x_mm
        * if matches!(t.mirror, Mirror::X | Mirror::Xy) {
            -1.
        } else {
            1.
        };
    let y = p.y_mm
        * if matches!(t.mirror, Mirror::Y | Mirror::Xy) {
            -1.
        } else {
            1.
        };
    let (s, c) = t.rotation_deg.to_radians().sin_cos();
    MmPoint::new(
        center.x_mm + t.scale * (x * c - y * s),
        center.y_mm + t.scale * (x * s + y * c),
    )
}
fn region_supported(contours: &[RegionContour]) -> bool {
    !contours.is_empty()
        && contours.iter().all(|c| {
            c.role == RegionRole::Solid
                && c.edges.iter().all(|e| matches!(e, RegionEdge::Line { .. }))
        })
}
/// The caller supplies only the selected object and its resolved aperture.
pub fn grip_features(
    object: &SemanticObject,
    aperture: Option<&ApertureShape>,
) -> Result<Vec<GripFeature>, EditError> {
    use GripFeatureId::*;
    if matches!(object.origin, ObjectOrigin::GeneratedText { .. }) {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    let mut add = |id, position_mm| result.push(GripFeature { id, position_mm });
    match &object.geometry {
        SemanticGeometry::Flash {
            center, transform, ..
        } => match aperture {
            Some(
                ApertureShape::Circle { diameter_mm, .. }
                | ApertureShape::Polygon { diameter_mm, .. },
            ) => add(
                Radius,
                world(MmPoint::new(diameter_mm / 2., 0.), *center, *transform),
            ),
            Some(
                ApertureShape::Rectangle {
                    width_mm,
                    height_mm,
                    ..
                }
                | ApertureShape::Obround {
                    width_mm,
                    height_mm,
                    ..
                },
            ) => {
                let (w, h) = (width_mm / 2., height_mm / 2.);
                for (id, x, y) in [
                    (Left, -w, 0.),
                    (Right, w, 0.),
                    (Bottom, 0., -h),
                    (Top, 0., h),
                    (Corner { index: 0 }, -w, -h),
                    (Corner { index: 1 }, w, -h),
                    (Corner { index: 2 }, w, h),
                    (Corner { index: 3 }, -w, h),
                ] {
                    add(id, world(MmPoint::new(x, y), *center, *transform));
                }
            }
            _ => {}
        },
        SemanticGeometry::Line { start, end, .. }
        | SemanticGeometry::RectangularSweep { start, end, .. } => {
            add(Start, *start);
            add(End, *end);
        }
        SemanticGeometry::Arc { path, .. } => {
            if !path.full_circle {
                add(Start, path.start);
                add(End, path.end);
            }
            let sign = if path.direction == ArcDirection::Clockwise {
                -1.
            } else {
                1.
            };
            let angle = (path.start.y_mm - path.center.y_mm)
                .atan2(path.start.x_mm - path.center.x_mm)
                + sign * path.sweep_radians().unwrap_or(0.) / 2.;
            add(
                Radius,
                MmPoint::new(
                    path.center.x_mm + path.radius() * angle.cos(),
                    path.center.y_mm + path.radius() * angle.sin(),
                ),
            );
        }
        SemanticGeometry::Region { contours } if region_supported(contours) => {
            if contours.iter().map(|c| c.edges.len()).sum::<usize>() > MAX_GRIP_FEATURES_PER_OBJECT
            {
                return Err(EditError::ResourceLimit);
            }
            for (contour, c) in contours.iter().enumerate() {
                for (vertex, e) in c.edges.iter().enumerate() {
                    add(Vertex { contour, vertex }, edge_start(e));
                }
            }
        }
        _ => {}
    }
    Ok(result)
}
/// Pure transient preview. Region topology is checked once at transaction time.
pub fn preview_grip_edit(
    object: &SemanticObject,
    aperture: Option<&ApertureShape>,
    id: GripFeatureId,
    target: MmPoint,
) -> Result<GripEditPreview, EditError> {
    use GripFeatureId::*;
    if !target.is_valid_geometry() {
        return Err(EditError::InvalidArgument);
    }
    let feature = grip_features(object, aperture)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or(EditError::UnsupportedTransform)?;
    if feature.position_mm == target {
        return Ok(GripEditPreview {
            geometry: object.geometry.clone(),
            aperture_shape: aperture.cloned(),
        });
    }
    let mut geometry = object.geometry.clone();
    let mut aperture_shape = None;
    match &mut geometry {
        SemanticGeometry::Flash {
            center, transform, ..
        } => {
            let old = aperture.ok_or(EditError::InvalidArgument)?;
            let p = apply_inverse_transform(target, *center, *transform);
            let (width, height) = match old {
                ApertureShape::Circle { .. } | ApertureShape::Polygon { .. } => {
                    (2. * p.distance_mm(MmPoint::new(0., 0.)), None)
                }
                ApertureShape::Rectangle {
                    width_mm,
                    height_mm,
                    ..
                }
                | ApertureShape::Obround {
                    width_mm,
                    height_mm,
                    ..
                } => {
                    let (sx, sy) = match id {
                        Left => (-1., 0.),
                        Right => (1., 0.),
                        Bottom => (0., -1.),
                        Top => (0., 1.),
                        Corner { index: 0 } => (-1., -1.),
                        Corner { index: 1 } => (1., -1.),
                        Corner { index: 2 } => (1., 1.),
                        Corner { index: 3 } => (-1., 1.),
                        _ => return Err(EditError::InvalidArgument),
                    };
                    let w = if sx == 0. {
                        *width_mm
                    } else {
                        sx * p.x_mm + width_mm / 2.
                    };
                    let h = if sy == 0. {
                        *height_mm
                    } else {
                        sy * p.y_mm + height_mm / 2.
                    };
                    let offset = MmPoint::new(sx * (w - width_mm) / 2., sy * (h - height_mm) / 2.);
                    *center = world(offset, *center, *transform);
                    (w, Some(h))
                }
                _ => return Err(EditError::UnsupportedTransform),
            };
            let shape = crate::edit::resized_shape(old, width, height)?;
            validate_aperture_shape(&shape).map_err(EditError::InvalidGeometry)?;
            aperture_shape = Some(shape);
        }
        SemanticGeometry::Line { start, end, .. }
        | SemanticGeometry::RectangularSweep { start, end, .. } => match id {
            Start => *start = target,
            End => *end = target,
            _ => return Err(EditError::InvalidArgument),
        },
        SemanticGeometry::Arc { path, .. } => {
            let original = *path;
            let distance = path.center.distance_mm(target);
            if distance <= EPSILON_MM || path.radius() <= EPSILON_MM {
                return Err(EditError::InvalidArgument);
            }
            let project = |p: MmPoint, r: f64| {
                let d = path.center.distance_mm(p);
                MmPoint::new(
                    path.center.x_mm + (p.x_mm - path.center.x_mm) * r / d,
                    path.center.y_mm + (p.y_mm - path.center.y_mm) * r / d,
                )
            };
            let coincident = path.full_circle || path.zero_sweep();
            match id {
                Start => path.start = project(target, path.radius()),
                End => path.end = project(target, path.radius()),
                Radius => {
                    let end = project(path.end, distance);
                    path.start = project(path.start, distance);
                    path.end = if coincident { path.start } else { end };
                }
                _ => return Err(EditError::InvalidArgument),
            }
            if *path != original {
                path.source = None;
            }
        }
        SemanticGeometry::Region { contours } => {
            let Vertex { contour, vertex } = id else {
                return Err(EditError::InvalidArgument);
            };
            let edges = &mut contours[contour].edges;
            let previous = (vertex + edges.len() - 1) % edges.len();
            if let RegionEdge::Line { start, .. } = &mut edges[vertex] {
                *start = target;
            }
            if let RegionEdge::Line { end, .. } = &mut edges[previous] {
                *end = target;
            }
            for c in contours {
                for e in &c.edges {
                    if edge_start(e).distance_mm(edge_end(e)) <= EPSILON_MM {
                        return Err(EditError::InvalidArgument);
                    }
                }
            }
        }
        _ => return Err(EditError::UnsupportedTransform),
    }
    if !matches!(geometry, SemanticGeometry::Region { .. }) {
        let ids = match &geometry {
            SemanticGeometry::Flash { aperture_id, .. } => {
                [aperture_id.clone()].into_iter().collect()
            }
            _ => Default::default(),
        };
        validate_geometry(&geometry, &ids, &Default::default())
            .map_err(EditError::InvalidGeometry)?;
    }
    Ok(GripEditPreview {
        geometry,
        aperture_shape,
    })
}
