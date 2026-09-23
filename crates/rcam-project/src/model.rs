//! `.rcam` schema v1 in-memory model (§9-§48 of the S4-B2 brief).
//!
//! `RCamProject` is the project-level truth: manufacturing geometry, layer
//! structure, workspace/view state, manufacturing precision, block
//! definitions, snap/grid settings, board placeholder, and import
//! provenance. It deliberately does not hold Solo (§13), Selection (§14),
//! Undo history (§40), or any `AppPreferences` (§11/§44) — those stay
//! session-only or move to a separate `AppPreferences` store later.

use editor_core::block::BlockDefinition;
use editor_core::snap::SnapKind;
use editor_core::units::ManufacturingPrecision;
use editor_core::workspace::{ImportProvenance, LayerWorkspaceState};
use editor_core::{
    ApertureDefinition, MmPoint, SemanticDocument, SemanticFormat, SemanticLayer, SourceMetadata,
};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "rcam";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectId(pub String);

/// Project/Workspace grid settings (§45). Distinct from `editor-app`'s own
/// GUI-only `tools::GridSettings`, which never crosses this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridSettings {
    pub spacing_mm: f64,
    pub visible: bool,
    pub snap: bool,
}

/// Project/Workspace Object Snap settings (§45/§60). `radius_px` is a UI
/// convenience captured as-is; it is never treated as manufacturing truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapSettingsState {
    pub enabled: bool,
    pub enabled_kinds: Vec<SnapKind>,
    pub radius_px: f64,
}

/// Workspace convenience only (§15); never manufacturing truth. An invalid
/// or out-of-range camera must not block loading — the reader falls back to
/// Fit Visible (handled by the S4-B3 GUI layer, not here).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub center_mm: MmPoint,
    pub scale: f64,
}

impl CameraState {
    pub fn is_valid(&self) -> bool {
        self.center_mm.is_finite() && self.scale.is_finite() && self.scale > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayUnit {
    Millimeters,
    Inches,
    Mils,
    Micrometers,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingProjectSettings {
    pub precision: ManufacturingPrecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceProjectState {
    pub display_unit: DisplayUnit,
    pub grid: GridSettings,
    pub snap: SnapSettingsState,
    pub active_layer_id: Option<String>,
    pub camera: Option<CameraState>,
}

/// One persisted layer: manufacturing geometry (`layer.objects`, which may
/// include `SemanticGeometry::BlockInstance` leaves, §36), its workspace
/// view style (§46), and optional import provenance (§16/§47 — basename +
/// hash + timestamp only, never an absolute source path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerProjectState {
    pub layer: SemanticLayer,
    pub workspace: LayerWorkspaceState,
    #[serde(default)]
    pub provenance: Option<ImportProvenance>,
}

/// Reserved extension point (§42): a real `.rcam` schema position for board
/// coordinate data once RCam actually owns some, without inventing fake
/// component-list semantics today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BoardProjectState {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RCamProject {
    pub format_version: u32,
    pub project_id: ProjectId,
    pub manufacturing: ManufacturingProjectSettings,
    pub workspace: WorkspaceProjectState,
    pub layer_order: Vec<String>,
    pub layers: Vec<LayerProjectState>,
    /// Shared across every layer, exactly like the live
    /// `SemanticDocument.apertures` table (S4-B1 namespacing rules apply).
    pub apertures: Vec<ApertureDefinition>,
    /// Project-level reusable geometry (§18); referenced, never duplicated,
    /// by `SemanticGeometry::BlockInstance` leaves inside `layers`.
    pub block_definitions: Vec<BlockDefinition>,
    pub board: Option<BoardProjectState>,
}

impl RCamProject {
    /// Canonical coordinate format for the synthetic validation document:
    /// `.rcam` does not persist a Gerber coordinate format (source-detached,
    /// §16), so this is a fixed, always-normalized placeholder, never a
    /// roundtripped value.
    fn synthetic_format() -> SemanticFormat {
        SemanticFormat {
            integer: 4,
            decimal: 6,
            leading_zero_omission: true,
            absolute: true,
        }
    }

    /// Reassemble the manufacturing-truth `SemanticDocument` this project
    /// describes, for `SemanticDocument::validate()` and for any core
    /// function (bounds/hit-test/metrics/export) that expects one.
    pub fn to_semantic_document(&self) -> SemanticDocument {
        SemanticDocument {
            id: self.project_id.0.clone(),
            unit: "mm".into(),
            format: Self::synthetic_format(),
            layers: self.layers.iter().map(|l| l.layer.clone()).collect(),
            apertures: self.apertures.clone(),
            source: SourceMetadata::default(),
            block_definitions: self.block_definitions.clone(),
        }
    }

    /// Full semantic validation (§7's "schema parse -> semantic validate"
    /// step): manufacturing geometry/blocks via `SemanticDocument::validate`,
    /// plus the project-only fields that document validation never sees
    /// (`layer_order` is a permutation of `layers`, every f64 project setting
    /// is finite per §48, no `LayerKind::Drill` layer since v1 cannot
    /// roundtrip `DrillObject`, §43).
    pub fn validate(&self) -> Result<(), crate::error::ProjectError> {
        use crate::error::ProjectError;
        if self.format_version != FORMAT_VERSION {
            return Err(ProjectError::UnknownFormatVersion(self.format_version));
        }
        if self.project_id.0.trim().is_empty() {
            return Err(ProjectError::SchemaInvalid("empty project_id".into()));
        }
        let mut order_ids: Vec<&str> = self.layer_order.iter().map(String::as_str).collect();
        let mut layer_ids: Vec<&str> = self.layers.iter().map(|l| l.layer.id.as_str()).collect();
        order_ids.sort_unstable();
        layer_ids.sort_unstable();
        if order_ids != layer_ids || self.layer_order.len() != self.layers.len() {
            return Err(ProjectError::SchemaInvalid(
                "layer_order is not a permutation of layers".into(),
            ));
        }
        for layer in &self.layers {
            if layer.workspace.kind == editor_core::workspace::LayerKind::Drill {
                return Err(ProjectError::UnsupportedFeature(
                    "persisted Drill layer (DrillObject has no .rcam v1 representation)".into(),
                ));
            }
        }
        if !self.manufacturing.precision.resolution_mm.is_finite()
            || self.manufacturing.precision.resolution_mm <= 0.0
        {
            return Err(ProjectError::NonFiniteValue(
                "manufacturing.precision.resolution_mm".into(),
            ));
        }
        if !self.workspace.grid.spacing_mm.is_finite() {
            return Err(ProjectError::NonFiniteValue(
                "workspace.grid.spacing_mm".into(),
            ));
        }
        if !self.workspace.snap.radius_px.is_finite() {
            return Err(ProjectError::NonFiniteValue(
                "workspace.snap.radius_px".into(),
            ));
        }
        // An invalid camera is deliberately *not* checked here (§15): it must
        // never block loading. `codec::decode` sanitizes it to `None` before
        // this method ever sees it; `encode_v1` never writes an invalid one.
        self.to_semantic_document()
            .validate()
            .map_err(|error| ProjectError::SemanticInvalid(error.to_string()))?;
        Ok(())
    }
}
