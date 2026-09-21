//! Drill / Excellon layer reservation (S4-B1 placeholder).
//!
//! No Excellon parser exists; the service refuses to create a `LayerKind::Drill`
//! layer. These types fix the future identity rules: every import has its own
//! tool namespace (`Import A :: T01` is not `Import B :: T01`), and a Drill layer
//! shares LayerId / colour / visibility / locking / display-mode state with
//! Gerber layers.

use crate::MmPoint;
use serde::{Deserialize, Serialize};

/// Tool identity inside one import: the same `T01` in two files never collides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DrillToolKey {
    pub source_id: String,
    pub tool: String,
}

impl DrillToolKey {
    pub fn new(source_id: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
            tool: tool.into(),
        }
    }

    /// Document-global identity, e.g. `src-2::T01`.
    pub fn global_id(&self) -> String {
        format!("{}::{}", self.source_id, self.tool)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillHit {
    pub tool: DrillToolKey,
    pub center: MmPoint,
    pub diameter_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillSlot {
    pub tool: DrillToolKey,
    pub start: MmPoint,
    pub end: MmPoint,
    pub width_mm: f64,
}

/// Future display semantics (documented, not rendered yet):
/// Filled = true hole/slot size, Outline = true boundary,
/// ZeroWidth = hit centre marker / slot-route centre path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrillObject {
    Hit(DrillHit),
    Slot(DrillSlot),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_namespace_is_per_import() {
        let a = DrillToolKey::new("src-1", "T01");
        let b = DrillToolKey::new("src-2", "T01");
        assert_ne!(a, b);
        assert_ne!(a.global_id(), b.global_id());
    }
}
