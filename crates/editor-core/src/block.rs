//! Reusable Block Core (S4-B2).
//!
//! `BlockDefinition` is project-level reusable geometry, stored on
//! [`crate::SemanticDocument::block_definitions`] rather than on any one layer.
//! A `BlockInstance` lives inside a layer's object list as a
//! [`crate::SemanticGeometry::BlockInstance`] leaf that references a definition
//! through a *rigid* [`BlockTransform`] (translation, rotation, reflection).
//! Non-uniform scale and shear are not representable, and nested blocks are
//! forbidden: [`BlockObjectGeometry`] is a deliberately smaller enum than
//! [`crate::SemanticGeometry`] with no `BlockInstance`-shaped variant, so a
//! `BlockDefinition` cannot reference another block at the type level. RCam
//! blocks are NOT Gerber `%AB` blocks: export flattens every instance through
//! its resolved transform.

use crate::board::CoordinateTransform2D;
use crate::edit::{MirrorAxis, translate};
use crate::transform::WorldTransform;
use crate::{ArcGeometry, Exposure, LocalTransform, MmPoint, RegionContour, SemanticGeometry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockDefinitionId(pub String);

/// Geometry a [`BlockDefinition`] can hold, in definition-local coordinates.
///
/// This is a *subset* of [`crate::SemanticGeometry`]: it mirrors every
/// primitive variant but has no `BlockInstance` case, so a `BlockObject` is
/// structurally incapable of holding another block instance. Nesting is
/// therefore rejected by the type system, not just by a runtime check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockObjectGeometry {
    Flash {
        center: MmPoint,
        aperture_id: String,
        transform: LocalTransform,
    },
    Line {
        start: MmPoint,
        end: MmPoint,
        width_mm: f64,
    },
    RectangularSweep {
        start: MmPoint,
        end: MmPoint,
        width_mm: f64,
        height_mm: f64,
    },
    Arc {
        path: ArcGeometry,
        width_mm: f64,
    },
    Region {
        contours: Vec<RegionContour>,
    },
}

impl From<BlockObjectGeometry> for SemanticGeometry {
    fn from(value: BlockObjectGeometry) -> Self {
        match value {
            BlockObjectGeometry::Flash {
                center,
                aperture_id,
                transform,
            } => SemanticGeometry::Flash {
                center,
                aperture_id,
                transform,
            },
            BlockObjectGeometry::Line {
                start,
                end,
                width_mm,
            } => SemanticGeometry::Line {
                start,
                end,
                width_mm,
            },
            BlockObjectGeometry::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            } => SemanticGeometry::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            },
            BlockObjectGeometry::Arc { path, width_mm } => SemanticGeometry::Arc { path, width_mm },
            BlockObjectGeometry::Region { contours } => SemanticGeometry::Region { contours },
        }
    }
}

/// Converting a resolved/world primitive back into definition-local storage.
/// Fails (rather than silently dropping data) when the source is itself a
/// block instance: this is the one runtime backstop against nesting, used
/// only by `blocks.create_definition_from_objects` when it captures selected
/// layer objects into a new definition.
impl TryFrom<SemanticGeometry> for BlockObjectGeometry {
    type Error = BlockError;

    fn try_from(value: SemanticGeometry) -> Result<Self, Self::Error> {
        Ok(match value {
            SemanticGeometry::Flash {
                center,
                aperture_id,
                transform,
            } => Self::Flash {
                center,
                aperture_id,
                transform,
            },
            SemanticGeometry::Line {
                start,
                end,
                width_mm,
            } => Self::Line {
                start,
                end,
                width_mm,
            },
            SemanticGeometry::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            } => Self::RectangularSweep {
                start,
                end,
                width_mm,
                height_mm,
            },
            SemanticGeometry::Arc { path, width_mm } => Self::Arc { path, width_mm },
            SemanticGeometry::Region { contours } => Self::Region { contours },
            SemanticGeometry::BlockInstance { .. } => return Err(BlockError::NestedBlock),
        })
    }
}

/// Geometry stored in a definition (definition-local coordinates, no nesting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockObject {
    pub geometry: BlockObjectGeometry,
    pub exposure: Exposure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDefinition {
    pub id: BlockDefinitionId,
    pub name: String,
    pub local_origin: MmPoint,
    pub objects: Vec<BlockObject>,
    /// Bumped on every geometry-changing edit; instance bounds/metrics caches
    /// (owned by the service layer) key off `(id, revision)`.
    pub revision: u64,
}

/// Instance placement: rigid only. `mirror` reflects about the definition's
/// local Y axis (`x -> -x`) before rotation, matching
/// [`CoordinateTransform2D`] exactly; combining `mirror` with a 180 degree
/// rotation already expresses "mirror the other axis", so no second mirror
/// field is needed to reach every rigid placement (see
/// `docs/adr/0032-block-core.md`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BlockTransform {
    pub translation: MmPoint,
    pub rotation_deg: f64,
    pub mirror: bool,
}

impl BlockTransform {
    pub const IDENTITY: Self = Self {
        translation: MmPoint::new(0., 0.),
        rotation_deg: 0.,
        mirror: false,
    };

    pub fn to_coordinate_transform(self) -> CoordinateTransform2D {
        CoordinateTransform2D {
            reflect_x: self.mirror,
            rotation_deg: self.rotation_deg,
            translation: self.translation,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.to_coordinate_transform().is_valid()
    }
}

/// Service-facing view of one placed instance: the containing
/// [`crate::SemanticObject::object_id`] plus the
/// [`crate::SemanticGeometry::BlockInstance`] payload it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockInstance {
    pub id: String,
    pub definition_id: BlockDefinitionId,
    pub transform: BlockTransform,
}

impl BlockInstance {
    pub fn validate(&self) -> Result<(), BlockError> {
        if self.transform.is_valid() {
            Ok(())
        } else {
            Err(BlockError::InvalidTransform)
        }
    }
}

/// Why a block reference is rejected by the reserved validation rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    NestedBlock,
    InvalidTransform,
    UnknownDefinition,
    ResourceLimit,
}

impl BlockDefinition {
    /// Structural checks only (no aperture/document context): finite origin,
    /// a non-empty id/name, and at least one object. Full semantic validation
    /// (aperture references, per-object geometry limits) happens alongside
    /// every other object in `SemanticDocument::validate`.
    pub fn validate(&self) -> Result<(), BlockError> {
        if self.id.0.trim().is_empty()
            || self.name.trim().is_empty()
            || !self.local_origin.is_finite()
            || self.objects.is_empty()
        {
            return Err(BlockError::InvalidTransform);
        }
        Ok(())
    }
}

/// Apply a rigid [`BlockTransform`] to one resolved (already-local) geometry,
/// producing its world-space representation. Reflection and rotation reuse
/// the same edit-time transform logic as `objects.rotate`/`objects.mirror`
/// (including the existing arc-direction flip and the "no non-90-degree
/// rotation of a RectangularSweep" guard, see `docs/adr/0032-block-core.md`
/// section on RectangularSweep), so a block instance can never silently
/// produce a geometrically wrong rectangular sweep.
pub fn resolve_geometry(
    mut geometry: SemanticGeometry,
    transform: &BlockTransform,
) -> Result<SemanticGeometry, BlockError> {
    if !transform.is_valid() {
        return Err(BlockError::InvalidTransform);
    }
    if transform.mirror {
        WorldTransform::reflection(MirrorAxis::Vertical { coordinate_mm: 0.0 })
            .and_then(|world| world.apply(&mut geometry))
            .map_err(|_| BlockError::InvalidTransform)?;
    }
    let rotation = transform.rotation_deg.rem_euclid(360.0);
    if rotation != 0.0 {
        WorldTransform::rotation(rotation, MmPoint::new(0., 0.))
            .and_then(|world| world.apply(&mut geometry))
            .map_err(|_| BlockError::InvalidTransform)?;
    }
    if transform.translation.x_mm != 0.0 || transform.translation.y_mm != 0.0 {
        translate(
            &mut geometry,
            transform.translation.x_mm,
            transform.translation.y_mm,
        )
        .map_err(|_| BlockError::InvalidTransform)?;
    }
    Ok(geometry)
}

/// One flattened primitive produced by resolving an instance: definition-local
/// `BlockObject` geometry transformed into world space, keeping the exposure
/// it had inside the definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBlockObject {
    pub geometry: SemanticGeometry,
    pub exposure: Exposure,
}

/// Flatten every object of `definition` through `transform`. Used by Explode,
/// Gerber export, and bounds/metrics/hit-test/snap resolution. Cost is
/// `O(definition.objects.len())`: callers with many instances of the same
/// definition should cache per-`(definition.id, definition.revision)` results
/// rather than re-resolving on every query (the service layer owns that
/// cache; this function stays a pure, uncached primitive).
pub fn resolve_instance(
    definition: &BlockDefinition,
    transform: &BlockTransform,
) -> Result<Vec<ResolvedBlockObject>, BlockError> {
    definition
        .objects
        .iter()
        .map(|object| {
            let world = resolve_geometry(object.geometry.clone().into(), transform)?;
            Ok(ResolvedBlockObject {
                geometry: world,
                exposure: object.exposure,
            })
        })
        .collect()
}

/// Local (definition-space) bounds/metrics use the same analytic functions as
/// any other object list, applied to `definition.objects` directly; a caller
/// resolving many instances of one shared definition therefore pays for the
/// definition's geometry once per lookup, never once per instance times the
/// definition's object count times the flattened world document.
pub fn local_geometries(definition: &BlockDefinition) -> Vec<SemanticGeometry> {
    definition
        .objects
        .iter()
        .map(|object| object.geometry.clone().into())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Exposure;

    fn sample_definition() -> BlockDefinition {
        BlockDefinition {
            id: BlockDefinitionId("d".into()),
            name: "d".into(),
            local_origin: MmPoint::new(0., 0.),
            objects: vec![BlockObject {
                geometry: BlockObjectGeometry::Line {
                    start: MmPoint::new(0., 0.),
                    end: MmPoint::new(1., 0.),
                    width_mm: 0.1,
                },
                exposure: Exposure::Dark,
            }],
            revision: 0,
        }
    }

    #[test]
    fn instance_transform_is_rigid_and_validated() {
        let instance = BlockInstance {
            id: "b1".into(),
            definition_id: BlockDefinitionId("d".into()),
            transform: BlockTransform {
                translation: MmPoint::new(1., 2.),
                rotation_deg: 90.,
                mirror: true,
            },
        };
        assert_eq!(instance.validate(), Ok(()));
        let t = instance.transform.to_coordinate_transform();
        assert!(t.reflect_x && t.rotation_deg == 90.);
        let bad = BlockInstance {
            transform: BlockTransform {
                rotation_deg: f64::NAN,
                ..instance.transform
            },
            ..instance
        };
        assert_eq!(bad.validate(), Err(BlockError::InvalidTransform));
    }

    #[test]
    fn nested_block_is_type_level_impossible_and_rejected_on_capture() {
        let nested = SemanticGeometry::BlockInstance {
            definition_id: BlockDefinitionId("other".into()),
            transform: BlockTransform::IDENTITY,
        };
        assert_eq!(
            BlockObjectGeometry::try_from(nested),
            Err(BlockError::NestedBlock)
        );
    }

    #[test]
    fn mirror_x_then_mirror_y_canonicalizes_to_rotate_180() {
        let mirror_x = BlockTransform {
            translation: MmPoint::new(0., 0.),
            rotation_deg: 0.,
            mirror: true,
        }
        .to_coordinate_transform();
        let mirror_y = BlockTransform {
            translation: MmPoint::new(0., 0.),
            rotation_deg: 180.,
            mirror: true,
        }
        .to_coordinate_transform();
        let both = mirror_y.after(&mirror_x);
        let rotate_180 = CoordinateTransform2D {
            reflect_x: false,
            rotation_deg: 180.,
            translation: MmPoint::new(0., 0.),
        };
        for p in [MmPoint::new(3., 5.), MmPoint::new(-2., 7.)] {
            let a = both.apply(p);
            let b = rotate_180.apply(p);
            assert!(a.distance_mm(b) < 1e-9);
        }
    }

    #[test]
    fn resolve_instance_flattens_and_shares_definition() {
        let definition = sample_definition();
        let transform = BlockTransform {
            translation: MmPoint::new(10., 0.),
            rotation_deg: 90.,
            mirror: false,
        };
        let resolved = resolve_instance(&definition, &transform).unwrap();
        assert_eq!(resolved.len(), 1);
        let SemanticGeometry::Line { start, end, .. } = &resolved[0].geometry else {
            panic!("expected line");
        };
        assert!(start.distance_mm(MmPoint::new(10., 0.)) < 1e-9);
        assert!(end.distance_mm(MmPoint::new(10., 1.)) < 1e-9);
    }

    #[test]
    fn rectangular_sweep_rejects_non_right_angle_rotation() {
        let definition = BlockDefinition {
            objects: vec![BlockObject {
                geometry: BlockObjectGeometry::RectangularSweep {
                    start: MmPoint::new(0., 0.),
                    end: MmPoint::new(1., 0.),
                    width_mm: 0.2,
                    height_mm: 0.2,
                },
                exposure: Exposure::Dark,
            }],
            ..sample_definition()
        };
        let transform = BlockTransform {
            translation: MmPoint::new(0., 0.),
            rotation_deg: 37.,
            mirror: false,
        };
        assert_eq!(
            resolve_instance(&definition, &transform).unwrap_err(),
            BlockError::InvalidTransform
        );
    }
}
