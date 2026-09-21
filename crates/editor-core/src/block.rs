//! Reusable Block architecture reservation (S4-B1 interfaces only).
//!
//! `BlockDefinition` is project-level reusable geometry; a `BlockInstance` lives
//! on a layer and references a definition through a *rigid* `BlockTransform`
//! (translation, rotation, reflection). Non-uniform scale and shear are not
//! representable, and nested blocks are forbidden in the first version. The core
//! implementation and the `.rcam v1` schema land together in S4-B2; nothing here
//! is reported as a supported operation. RCam blocks are NOT Gerber `%AB` blocks:
//! export flattens instances through the resolved transform.

use crate::board::CoordinateTransform2D;
use crate::{Exposure, MmPoint, SemanticGeometry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockDefinitionId(pub String);

/// Geometry stored in a definition (definition-local coordinates, no nesting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockObject {
    pub geometry: SemanticGeometry,
    pub exposure: Exposure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDefinition {
    pub id: BlockDefinitionId,
    pub name: String,
    pub local_origin: MmPoint,
    pub objects: Vec<BlockObject>,
}

/// Instance placement: rigid only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BlockTransform {
    pub translation: MmPoint,
    pub rotation_deg: f64,
    pub mirror: bool,
}

impl BlockTransform {
    pub fn to_coordinate_transform(self) -> CoordinateTransform2D {
        CoordinateTransform2D {
            reflect_x: self.mirror,
            rotation_deg: self.rotation_deg,
            translation: self.translation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockInstance {
    pub id: String,
    pub definition_id: BlockDefinitionId,
    pub transform: BlockTransform,
}

/// Why a block reference is rejected by the reserved validation rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    NestedBlock,
    InvalidTransform,
}

impl BlockDefinition {
    /// First-version rule: a definition holds plain manufacturing geometry only.
    /// (`BlockObject` cannot express an instance, so nesting is unrepresentable.)
    pub fn validate(&self) -> Result<(), BlockError> {
        Ok(())
    }
}

impl BlockInstance {
    pub fn validate(&self) -> Result<(), BlockError> {
        if self.transform.to_coordinate_transform().is_valid() {
            Ok(())
        } else {
            Err(BlockError::InvalidTransform)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
