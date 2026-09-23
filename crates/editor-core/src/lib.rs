//! UI-free f64 manufacturing geometry, validation and bounded atomic editing.
//! The original S0 display model remains separate from the S1 semantic model.

mod bounds;
pub mod grid;
pub mod hash;
pub mod hit_test;
pub mod metrics;
pub mod units;
pub use bounds::{BoundsMm, geometries_bounds, geometries_bounds_with_blocks};
pub mod block;
pub mod board;
pub mod command;
pub mod drill;
pub mod edit;
pub mod snap;
mod transform;
pub mod workspace;

use serde::{Deserialize, Serialize};

pub const EPSILON_MM: f64 = 1e-6;
const MAX_GEOMETRY_MM: f64 = 1e9;
const MAX_INTERSECTION_EDGES: usize = 2_000_000;
const MAX_INTERSECTION_CANDIDATES: usize = 2_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MmPoint {
    pub x_mm: f64,
    pub y_mm: f64,
}

impl MmPoint {
    pub const fn new(x_mm: f64, y_mm: f64) -> Self {
        Self { x_mm, y_mm }
    }

    pub fn is_finite(self) -> bool {
        self.x_mm.is_finite() && self.y_mm.is_finite()
    }

    pub fn distance_mm(self, other: Self) -> f64 {
        (self.x_mm - other.x_mm).hypot(self.y_mm - other.y_mm)
    }

    fn is_valid_geometry(self) -> bool {
        self.is_finite() && self.x_mm.abs() <= MAX_GEOMETRY_MM && self.y_mm.abs() <= MAX_GEOMETRY_MM
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure {
    Dark,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CircleAperture {
    pub diameter_mm: f64,
    pub hole_diameter_mm: Option<f64>,
}

impl CircleAperture {
    pub fn new(diameter_mm: f64, hole_diameter_mm: Option<f64>) -> Result<Self, CoreError> {
        if !diameter_mm.is_finite() || diameter_mm / 2.0 <= 0.0 {
            return Err(CoreError::InvalidGeometry(
                "circle diameter must be positive",
            ));
        }
        if hole_diameter_mm
            .is_some_and(|hole| !hole.is_finite() || hole / 2.0 <= 0.0 || hole >= diameter_mm)
        {
            return Err(CoreError::InvalidGeometry(
                "circle hole must be positive and smaller than the outer diameter",
            ));
        }
        Ok(Self {
            diameter_mm,
            hole_diameter_mm,
        })
    }

    pub fn covers(self, point: MmPoint, center: MmPoint) -> bool {
        let radius = point.distance_mm(center);
        let outer = radius <= self.diameter_mm / 2.0 + EPSILON_MM;
        let inner = self
            .hole_diameter_mm
            .is_some_and(|hole| radius < hole / 2.0);
        outer && !inner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Geometry {
    CircleFlash {
        center: MmPoint,
        aperture: CircleAperture,
    },
    Line {
        start: MmPoint,
        end: MmPoint,
        width_mm: f64,
    },
}

impl Geometry {
    pub fn covers(self, point: MmPoint) -> bool {
        match self {
            Self::CircleFlash { center, aperture } => aperture.covers(point, center),
            Self::Line {
                start,
                end,
                width_mm,
            } => {
                if !width_mm.is_finite() || width_mm <= 0.0 {
                    return false;
                }
                let dx = end.x_mm - start.x_mm;
                let dy = end.y_mm - start.y_mm;
                let length_squared = dx * dx + dy * dy;
                let t = if length_squared <= EPSILON_MM * EPSILON_MM {
                    0.0
                } else {
                    ((point.x_mm - start.x_mm) * dx + (point.y_mm - start.y_mm) * dy)
                        / length_squared
                }
                .clamp(0.0, 1.0);
                let nearest = MmPoint::new(start.x_mm + t * dx, start.y_mm + t * dy);
                nearest.distance_mm(point) <= width_mm / 2.0 + EPSILON_MM
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawObject {
    /// Stable for this document session; never use a vector index as identity.
    pub object_id: String,
    pub geometry: Geometry,
    pub exposure: Exposure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub objects: Vec<DrawObject>,
}

impl Layer {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            objects: Vec::new(),
        }
    }

    /// Applies flashes in source order. A clear flash only changes this layer.
    pub fn coverage_at(&self, point: MmPoint) -> bool {
        self.objects.iter().fold(false, |coverage, object| {
            if object.geometry.covers(point) {
                match object.exposure {
                    Exposure::Dark => true,
                    Exposure::Clear => false,
                }
            } else {
                coverage
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    pub document_id: String,
    pub unit: String,
    pub layers: Vec<Layer>,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub unit: String,
    pub layers: Vec<Layer>,
}

impl Document {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            unit: "mm".to_string(),
            layers: Vec::new(),
        }
    }

    pub fn snapshot(&self, revision: u64) -> DocumentSnapshot {
        DocumentSnapshot {
            document_id: self.id.clone(),
            unit: self.unit.clone(),
            layers: self.layers.clone(),
            revision: revision.to_string(),
        }
    }

    pub fn layer_coverage_at(&self, layer_id: &str, point: MmPoint) -> Option<bool> {
        self.layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .map(|layer| layer.coverage_at(point))
    }

    pub fn composite_coverage_at(&self, point: MmPoint) -> Vec<(String, bool)> {
        self.layers
            .iter()
            .map(|layer| (layer.id.clone(), layer.coverage_at(point)))
            .collect()
    }
}

/// The validated S1 geometry model.  It deliberately lives beside the S0
/// model so the existing renderer ABI can keep its small exhaustive enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArcDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mirror {
    None,
    X,
    Y,
    Xy,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalTransform {
    pub mirror: Mirror,
    pub rotation_deg: f64,
    pub scale: f64,
}

impl Default for LocalTransform {
    fn default() -> Self {
        Self {
            mirror: Mirror::None,
            rotation_deg: 0.0,
            scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ApertureShape {
    Circle {
        diameter_mm: f64,
        hole_diameter_mm: Option<f64>,
    },
    Rectangle {
        width_mm: f64,
        height_mm: f64,
        hole_diameter_mm: Option<f64>,
    },
    Obround {
        width_mm: f64,
        height_mm: f64,
        hole_diameter_mm: Option<f64>,
    },
    Polygon {
        diameter_mm: f64,
        vertices: u8,
        rotation_deg: f64,
        hole_diameter_mm: Option<f64>,
    },
    Macro {
        primitives: Vec<MacroPrimitive>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MacroPrimitive {
    Circle {
        exposure: Exposure,
        diameter_mm: f64,
        center: MmPoint,
        rotation_deg: f64,
    },
    CenterLine {
        exposure: Exposure,
        width_mm: f64,
        height_mm: f64,
        center: MmPoint,
        rotation_deg: f64,
    },
    Outline {
        exposure: Exposure,
        points: Vec<MmPoint>,
        rotation_deg: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArcSource {
    pub resolution_mm: f64,
    pub single_quadrant: bool,
}

/// Endpoints and center are the declared manufacturing input, never mesh data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArcGeometry {
    pub start: MmPoint,
    pub end: MmPoint,
    pub center: MmPoint,
    pub direction: ArcDirection,
    pub full_circle: bool,
    pub source: Option<ArcSource>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArcDeviationSummary {
    pub count: usize,
    pub single_quadrant_count: usize,
    pub zero_sweep_count: usize,
    pub above_roundoff_count: usize,
    pub max_deviation_mm: f64,
}

impl ArcDeviationSummary {
    pub fn include(&mut self, arc: ArcGeometry) {
        self.count += 1;
        self.single_quadrant_count += usize::from(arc.source.is_some_and(|s| s.single_quadrant));
        self.zero_sweep_count += usize::from(arc.zero_sweep());
        self.above_roundoff_count += usize::from(arc.arc_deviation() > arc.numeric_tolerance());
        self.max_deviation_mm = self.max_deviation_mm.max(arc.arc_deviation());
    }
}

impl ArcGeometry {
    pub fn radius(self) -> f64 {
        self.start.distance_mm(self.center)
    }

    pub fn numeric_tolerance(self) -> f64 {
        64.0 * f64::EPSILON
            * [
                self.start.x_mm.abs(),
                self.start.y_mm.abs(),
                self.end.x_mm.abs(),
                self.end.y_mm.abs(),
                self.center.x_mm.abs(),
                self.center.y_mm.abs(),
                self.radius(),
                self.end_radius(),
                1.0,
            ]
            .into_iter()
            .fold(0.0, f64::max)
    }

    pub fn angular_uncertainty(self) -> f64 {
        self.numeric_tolerance() / self.radius().min(self.end_radius())
    }

    pub fn end_radius(self) -> f64 {
        self.end.distance_mm(self.center)
    }

    pub fn arc_deviation(self) -> f64 {
        (self.radius() - self.end_radius()).abs()
    }

    pub fn zero_sweep(self) -> bool {
        !self.full_circle && self.start == self.end
    }

    /// A continuous, angularly and radially monotone curve inside the declared
    /// annular band: radial join, mean-radius circular arc, radial join.
    /// The joins and this circle are coverage/render data, not writer input.
    pub fn canonical_circle(self) -> Self {
        if self.zero_sweep() {
            return self;
        }
        let radius = (self.radius() + self.end_radius()) / 2.0;
        let project = |point: MmPoint| {
            let scale = radius / point.distance_mm(self.center);
            MmPoint::new(
                self.center.x_mm + (point.x_mm - self.center.x_mm) * scale,
                self.center.y_mm + (point.y_mm - self.center.y_mm) * scale,
            )
        };
        Self {
            start: project(self.start),
            end: project(self.end),
            source: None,
            ..self
        }
    }

    pub fn has_nonsensical_center(self) -> bool {
        if self.start == self.end {
            return false;
        }
        let dx = self.end.x_mm - self.start.x_mm;
        let dy = self.end.y_mm - self.start.y_mm;
        let chord = dx.hypot(dy);
        let cx = self.center.x_mm - self.start.x_mm;
        let cy = self.center.y_mm - self.start.y_mm;
        let projection = (cx * dx + cy * dy) / (chord * chord);
        let distance = (cx * dy - cy * dx).abs() / chord;
        // The spec gives no numeric definition of "close". Freeze one input
        // quantum plus f64 arithmetic uncertainty, not an arc-deviation cap.
        let close = self.source.map_or(0.0, |s| s.resolution_mm) + self.numeric_tolerance();
        distance <= close && !(projection > 0.0 && projection < 1.0)
    }

    pub fn sweep_radians(self) -> Option<f64> {
        if self.zero_sweep() {
            return Some(0.0);
        }
        let radius = self.radius();
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        if self.full_circle {
            return Some(std::f64::consts::TAU);
        }
        let start = (self.start.y_mm - self.center.y_mm).atan2(self.start.x_mm - self.center.x_mm);
        let end = (self.end.y_mm - self.center.y_mm).atan2(self.end.x_mm - self.center.x_mm);
        let mut sweep = end - start;
        match self.direction {
            ArcDirection::CounterClockwise => {
                if sweep < 0.0 {
                    sweep += std::f64::consts::TAU;
                }
            }
            ArcDirection::Clockwise => {
                if sweep > 0.0 {
                    sweep -= std::f64::consts::TAU;
                }
                sweep = -sweep;
            }
        }
        Some(sweep)
    }

    pub fn is_valid(self) -> bool {
        if !self.start.is_valid_geometry()
            || !self.end.is_valid_geometry()
            || !self.center.is_valid_geometry()
        {
            return false;
        }
        let radius = self.radius();
        let end_radius = self.end.distance_mm(self.center);
        radius.is_finite()
            && end_radius.is_finite()
            && self.numeric_tolerance() <= EPSILON_MM
            && (self.zero_sweep() || (radius > 0.0 && end_radius > 0.0))
            && self.source.is_none_or(|s| {
                s.resolution_mm.is_finite()
                    && s.resolution_mm > 0.0
                    && if s.single_quadrant {
                        !self.full_circle
                            && (self.zero_sweep()
                                || self.sweep_radians().is_some_and(|a| {
                                    a <= std::f64::consts::FRAC_PI_2 + self.angular_uncertainty()
                                }))
                    } else {
                        !self.zero_sweep()
                    }
            })
            && (!self.full_circle || self.start == self.end)
            && !self.has_nonsensical_center()
            && self.sweep_radians().is_some_and(|sweep| {
                if self.zero_sweep() {
                    true
                } else {
                    sweep > 0.0 && sweep <= std::f64::consts::TAU
                }
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegionEdge {
    Line { start: MmPoint, end: MmPoint },
    Arc(ArcGeometry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionRole {
    Solid,
    Hole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionContour {
    pub edges: Vec<RegionEdge>,
    pub role: RegionRole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticGeometry {
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
    /// A placed reference to a project-level `SemanticDocument::block_definitions`
    /// entry (S4-B2). Resolution (bounds/hit-test/metrics/export) happens via
    /// `block::resolve_instance`, not by matching this variant's fields directly:
    /// most existing per-geometry-kind code treats it as an opaque leaf that a
    /// document-level caller must have already resolved before reaching here.
    BlockInstance {
        definition_id: block::BlockDefinitionId,
        transform: block::BlockTransform,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectOrigin {
    Imported {
        command_index: usize,
    },
    Generated {
        operation_id: String,
    },
    /// Generated by the text tool (and copies of such objects); kept distinct so
    /// the display classification does not depend on geometry heuristics.
    GeneratedText {
        operation_id: String,
    },
}

impl ObjectOrigin {
    /// Operation that produced a generated object (`None` for imported ones).
    pub fn operation_id(&self) -> Option<&str> {
        match self {
            Self::Imported { .. } => None,
            Self::Generated { operation_id } | Self::GeneratedText { operation_id } => {
                Some(operation_id)
            }
        }
    }

    pub fn is_generated(&self) -> bool {
        self.operation_id().is_some()
    }
}

/// Source namespace of an aperture identity. Imported apertures of a
/// multi-source document are named `<source_id>::<local id>`; ids without the
/// separator belong to the global (generated) namespace.
pub fn aperture_namespace(aperture_id: &str) -> &str {
    aperture_id
        .split_once("::")
        .map_or("", |(namespace, _)| namespace)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticObject {
    pub object_id: String,
    pub geometry: SemanticGeometry,
    pub exposure: Exposure,
    pub origin: ObjectOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticLayer {
    pub id: String,
    pub objects: Vec<SemanticObject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApertureDefinition {
    pub id: String,
    pub source_dcode: i32,
    pub shape: ApertureShape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticFormat {
    pub integer: u8,
    pub decimal: u8,
    pub leading_zero_omission: bool,
    pub absolute: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SourceMetadata {
    pub image_name: Option<String>,
    pub layer_name: Option<String>,
    /// Repeated LN labels identify source sections rather than one whole layer.
    #[serde(default)]
    pub section_names: Vec<String>,
    pub encoding: Option<String>,
    pub file_attributes: Vec<String>,
    pub dropped_categories: Vec<String>,
    /// Original FS declaration, retained verbatim for safe round trips.
    pub coordinate_format: Option<String>,
    /// Original MO/G70/G71 declarations, in source order.
    pub unit_declarations: Vec<String>,
    /// Original G90/G91 mode declarations, in source order.
    pub coordinate_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticDocument {
    pub id: String,
    pub unit: String,
    pub format: SemanticFormat,
    pub layers: Vec<SemanticLayer>,
    pub apertures: Vec<ApertureDefinition>,
    pub source: SourceMetadata,
    /// Project-level reusable geometry (S4-B2). Never produced by Gerber
    /// import; `#[serde(default)]` keeps every pre-existing document/fixture
    /// (and the JSON automation contract) unaffected.
    #[serde(default)]
    pub block_definitions: Vec<block::BlockDefinition>,
}

impl SemanticDocument {
    pub fn block_definition(
        &self,
        id: &block::BlockDefinitionId,
    ) -> Option<&block::BlockDefinition> {
        self.block_definitions.iter().find(|d| &d.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub object_count: usize,
    pub aperture_count: usize,
    pub region_edge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticError {
    Invalid(String),
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    MissingAperture(String),
    DuplicateId(String),
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::ResourceLimit {
                resource,
                limit,
                actual,
            } => write!(f, "resource limit {resource}: {actual} > {limit}"),
            Self::MissingAperture(id) => write!(f, "missing aperture {id}"),
            Self::DuplicateId(id) => write!(f, "duplicate object or layer id {id}"),
        }
    }
}

impl std::error::Error for SemanticError {}

impl SemanticDocument {
    pub fn arc_deviation_summary(&self) -> ArcDeviationSummary {
        let mut summary = ArcDeviationSummary::default();
        for object in self.layers.iter().flat_map(|layer| &layer.objects) {
            match &object.geometry {
                SemanticGeometry::Arc { path, .. } => summary.include(*path),
                SemanticGeometry::Region { contours } => {
                    for edge in contours.iter().flat_map(|contour| &contour.edges) {
                        if let RegionEdge::Arc(arc) = edge {
                            summary.include(*arc);
                        }
                    }
                }
                _ => {}
            }
        }
        summary
    }

    pub fn validate(&self) -> Result<ValidationReport, SemanticError> {
        if !valid_id(&self.id) || self.unit != "mm" {
            return Err(SemanticError::Invalid("document id/unit is invalid".into()));
        }
        if self.format.integer == 0
            || self.format.integer > 6
            || !(1..=6).contains(&self.format.decimal)
            || !self.format.absolute
        {
            return Err(SemanticError::Invalid(
                "coordinate format is not normalized".into(),
            ));
        }
        let mut aperture_ids = std::collections::HashSet::new();
        // A source DCode is only unique inside its source namespace: two imported
        // files may both define D10 without any relation between them.
        let mut aperture_dcodes = std::collections::HashSet::new();
        for aperture in &self.apertures {
            if !valid_id(&aperture.id)
                || aperture.source_dcode < 10
                || !aperture_dcodes.insert((
                    aperture_namespace(&aperture.id).to_string(),
                    aperture.source_dcode,
                ))
                || !aperture_ids.insert(aperture.id.clone())
            {
                return Err(SemanticError::Invalid(format!(
                    "invalid or duplicate aperture {}",
                    aperture.id
                )));
            }
            validate_aperture_shape(&aperture.shape)?;
        }
        let mut block_definition_ids = std::collections::HashSet::new();
        for definition in &self.block_definitions {
            if !block_definition_ids.insert(definition.id.0.clone()) {
                return Err(SemanticError::DuplicateId(definition.id.0.clone()));
            }
            definition.validate().map_err(|_| {
                SemanticError::Invalid(format!("invalid block definition {}", definition.id.0))
            })?;
            for object in &definition.objects {
                let geometry: SemanticGeometry = object.geometry.clone().into();
                validate_geometry(&geometry, &aperture_ids, &block_definition_ids)?;
            }
        }
        let mut layer_ids = std::collections::HashSet::new();
        let mut object_ids = std::collections::HashSet::new();
        let mut objects = 0;
        let mut region_edges = 0;
        for layer in &self.layers {
            if !valid_id(&layer.id) || !layer_ids.insert(layer.id.clone()) {
                return Err(SemanticError::DuplicateId(layer.id.clone()));
            }
            for object in &layer.objects {
                if !valid_id(&object.object_id) || !object_ids.insert(object.object_id.clone()) {
                    return Err(SemanticError::DuplicateId(object.object_id.clone()));
                }
                validate_geometry(&object.geometry, &aperture_ids, &block_definition_ids)?;
                if let SemanticGeometry::BlockInstance {
                    definition_id,
                    transform,
                } = &object.geometry
                {
                    let definition = self.block_definition(definition_id).ok_or_else(|| {
                        SemanticError::Invalid("block instance definition vanished".into())
                    })?;
                    block::resolve_instance(definition, transform).map_err(|_| {
                        SemanticError::Invalid(format!(
                            "block instance {} cannot be resolved under its transform",
                            object.object_id
                        ))
                    })?;
                }
                if let Some(operation_id) = object.origin.operation_id()
                    && !valid_id(operation_id)
                {
                    return Err(SemanticError::Invalid("invalid generated origin".into()));
                }
                objects += 1;
                if let SemanticGeometry::Region { contours } = &object.geometry {
                    for contour in contours {
                        region_edges += contour.edges.len();
                    }
                }
            }
        }
        Ok(ValidationReport {
            object_count: objects,
            aperture_count: self.apertures.len(),
            region_edge_count: region_edges,
        })
    }

    pub fn layer_coverage_at(&self, layer_id: &str, point: MmPoint) -> Option<bool> {
        let layer = self.layers.iter().find(|layer| layer.id == layer_id)?;
        let apertures: std::collections::HashMap<_, _> = self
            .apertures
            .iter()
            .map(|aperture| (aperture.id.as_str(), &aperture.shape))
            .collect();
        Some(layer.objects.iter().fold(false, |covered, object| {
            if geometry_covers(&object.geometry, point, &apertures) {
                match object.exposure {
                    Exposure::Dark => true,
                    Exposure::Clear => false,
                }
            } else {
                covered
            }
        }))
    }

    pub fn object_count(&self) -> usize {
        self.layers.iter().map(|layer| layer.objects.len()).sum()
    }
}

pub fn validate_aperture_shape(shape: &ApertureShape) -> Result<(), SemanticError> {
    let valid = match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => valid_diameter(*diameter_mm, *hole_diameter_mm),
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        }
        | ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => {
            width_mm.is_finite()
                && height_mm.is_finite()
                && *width_mm > EPSILON_MM
                && *height_mm > EPSILON_MM
                && *width_mm <= MAX_GEOMETRY_MM
                && *height_mm <= MAX_GEOMETRY_MM
                && hole_diameter_mm.is_none_or(|hole| {
                    valid_positive_radius(hole) && hole < width_mm.min(*height_mm)
                })
        }
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => {
            valid_diameter(*diameter_mm, *hole_diameter_mm)
                && (3..=12).contains(vertices)
                && rotation_deg.is_finite()
                && rotation_deg.abs() <= 1e9
                && hole_diameter_mm.is_none_or(|hole| {
                    let apothem_diameter =
                        *diameter_mm * (std::f64::consts::PI / f64::from(*vertices)).cos();
                    valid_positive_radius(hole) && hole < apothem_diameter
                })
        }
        ApertureShape::Macro { primitives } => {
            if primitives.is_empty() {
                return Err(SemanticError::Invalid("macro has no primitives".into()));
            }
            for primitive in primitives {
                validate_macro_primitive(primitive)?;
            }
            true
        }
    };
    if valid {
        Ok(())
    } else {
        Err(SemanticError::Invalid("invalid aperture shape".into()))
    }
}

fn valid_diameter(diameter: f64, hole: Option<f64>) -> bool {
    diameter.is_finite()
        && diameter > EPSILON_MM
        && diameter <= MAX_GEOMETRY_MM
        && hole.is_none_or(|hole| valid_positive_radius(hole) && hole < diameter)
}

fn valid_positive_radius(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value / 2.0 > 0.0
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().all(|character| !character.is_control())
}

fn validate_macro_primitive(primitive: &MacroPrimitive) -> Result<(), SemanticError> {
    let valid = match primitive {
        MacroPrimitive::Circle {
            exposure: _,
            diameter_mm,
            center,
            rotation_deg,
        } => {
            valid_diameter(*diameter_mm, None)
                && center.is_valid_geometry()
                && rotation_deg.is_finite()
                && rotation_deg.abs() <= 1e9
        }
        MacroPrimitive::CenterLine {
            width_mm,
            height_mm,
            center,
            rotation_deg,
            ..
        } => {
            width_mm.is_finite()
                && height_mm.is_finite()
                && *width_mm > EPSILON_MM
                && *height_mm > EPSILON_MM
                && *width_mm <= MAX_GEOMETRY_MM
                && *height_mm <= MAX_GEOMETRY_MM
                && center.is_valid_geometry()
                && rotation_deg.is_finite()
                && rotation_deg.abs() <= 1e9
        }
        MacroPrimitive::Outline {
            points,
            rotation_deg,
            ..
        } => {
            if points.len() < 4
                || points
                    .first()
                    .zip(points.last())
                    .is_none_or(|(first, last)| first.distance_mm(*last) > EPSILON_MM)
                || !points.iter().all(|point| point.is_valid_geometry())
                || !rotation_deg.is_finite()
                || rotation_deg.abs() > 1e9
            {
                return Err(SemanticError::Invalid("invalid macro outline".into()));
            }
            return validate_outline_points(points);
        }
    };
    if valid {
        Ok(())
    } else {
        Err(SemanticError::Invalid("invalid macro primitive".into()))
    }
}

pub(crate) fn validate_geometry(
    geometry: &SemanticGeometry,
    aperture_ids: &std::collections::HashSet<String>,
    block_definition_ids: &std::collections::HashSet<String>,
) -> Result<(), SemanticError> {
    let finite_point = |point: MmPoint| {
        if point.is_valid_geometry() {
            Ok(())
        } else {
            Err(SemanticError::Invalid(
                "coordinate is non-finite or out of range".into(),
            ))
        }
    };
    match geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            transform,
        } => {
            finite_point(*center)?;
            if !aperture_ids.contains(aperture_id) {
                return Err(SemanticError::MissingAperture(aperture_id.clone()));
            }
            if !transform.rotation_deg.is_finite()
                || transform.rotation_deg.abs() > 1e9
                || !transform.scale.is_finite()
                || transform.scale <= 0.0
                || transform.scale > MAX_GEOMETRY_MM
            {
                return Err(SemanticError::Invalid("invalid aperture transform".into()));
            }
        }
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => {
            finite_point(*start)?;
            finite_point(*end)?;
            if !width_mm.is_finite() || *width_mm <= 0.0 || *width_mm > MAX_GEOMETRY_MM {
                return Err(SemanticError::Invalid("invalid line width".into()));
            }
        }
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => {
            finite_point(*start)?;
            finite_point(*end)?;
            if !width_mm.is_finite()
                || !height_mm.is_finite()
                || *width_mm <= 0.0
                || *height_mm <= 0.0
                || *width_mm > MAX_GEOMETRY_MM
                || *height_mm > MAX_GEOMETRY_MM
                || start.distance_mm(*end) <= EPSILON_MM
                || (start.x_mm != end.x_mm && start.y_mm != end.y_mm)
            {
                return Err(SemanticError::Invalid("invalid rectangular sweep".into()));
            }
        }
        SemanticGeometry::Arc { path, width_mm } => {
            if !path.is_valid()
                || !width_mm.is_finite()
                || *width_mm <= 0.0
                || *width_mm > MAX_GEOMETRY_MM
            {
                return Err(SemanticError::Invalid("invalid arc".into()));
            }
        }
        SemanticGeometry::Region { contours } => {
            if contours.is_empty() {
                return Err(SemanticError::Invalid("region has no contours".into()));
            }
            for contour in contours {
                validate_contour(contour)?;
            }
        }
        SemanticGeometry::BlockInstance {
            definition_id,
            transform,
        } => {
            if !transform.is_valid() {
                return Err(SemanticError::Invalid(
                    "invalid block instance transform".into(),
                ));
            }
            if !block_definition_ids.contains(&definition_id.0) {
                return Err(SemanticError::Invalid(format!(
                    "block instance references unknown definition {}",
                    definition_id.0
                )));
            }
        }
    }
    Ok(())
}

/// Check one joined pair while fitting a boundary, using the same analytic
/// intersection rule as final contour validation. This is not whole-ring proof.
pub fn region_join_is_simple(first: &RegionEdge, second: &RegionEdge) -> bool {
    if edge_end(first).distance_mm(edge_start(second)) > EPSILON_MM {
        return false;
    }
    let contour = RegionContour {
        role: RegionRole::Solid,
        edges: vec![first.clone(), second.clone()],
    };
    let Ok(canonical) = canonical_region_contour(&contour) else {
        return false;
    };
    !edges_intersect_beyond(&canonical.edges[0], &canonical.edges[1], edge_end(first))
}

/// Validate a generated contour against the same topology rules used on import.
pub fn validate_region_contour(contour: &RegionContour) -> Result<(), SemanticError> {
    validate_contour(contour)
}

fn validate_contour(contour: &RegionContour) -> Result<(), SemanticError> {
    if contour.role != RegionRole::Solid {
        return Err(SemanticError::Invalid(
            "independent hole contours are unsupported; encode holes as cut-ins".into(),
        ));
    }
    if contour.edges.is_empty()
        || (contour.edges.len() == 1
            && !matches!(&contour.edges[0], RegionEdge::Arc(arc) if arc.full_circle))
    {
        return Err(SemanticError::Invalid(
            "region contour has too few edges".into(),
        ));
    }
    let mut previous_end: Option<MmPoint> = None;
    let first_start = edge_start(&contour.edges[0]);
    for edge in &contour.edges {
        let (start, end) = (edge_start(edge), edge_end(edge));
        if !start.is_valid_geometry() || !end.is_valid_geometry() {
            return Err(SemanticError::Invalid("region has non-finite edge".into()));
        }
        if let Some(previous) = previous_end
            && previous.distance_mm(start) > EPSILON_MM
        {
            return Err(SemanticError::Invalid(
                "region contour is not connected".into(),
            ));
        }
        if matches!(edge, RegionEdge::Line { .. }) && start.distance_mm(end) <= EPSILON_MM {
            return Err(SemanticError::Invalid("region has zero-length edge".into()));
        }
        if let RegionEdge::Arc(arc) = edge
            && !arc.is_valid()
        {
            return Err(SemanticError::Invalid("region has invalid arc".into()));
        }
        previous_end = Some(end);
    }
    if previous_end.is_none_or(|end| end.distance_mm(first_start) > EPSILON_MM) {
        return Err(SemanticError::Invalid(
            "region contour is not closed".into(),
        ));
    }
    if contour
        .edges
        .iter()
        .any(|edge| matches!(edge, RegionEdge::Arc(arc) if arc.full_circle))
        && contour.edges.len() != 1
    {
        return Err(SemanticError::Invalid(
            "full circle is only valid as a standalone contour".into(),
        ));
    }
    let canonical = canonical_region_contour(contour)?;
    validate_contour_intersections(&canonical.edges)?;
    validate_region_envelopes(contour, &canonical)?;
    Ok(())
}

/// A Region may use a circular interpretation only when its entire directed
/// curve stays in the declared annulus. Major fuzzy arcs needing another curve
/// remain unsupported here; stroke coverage handles them with radial joins.
pub fn canonical_region_contour(contour: &RegionContour) -> Result<RegionContour, SemanticError> {
    let mut result = contour.clone();
    for edge in &mut result.edges {
        let RegionEdge::Arc(input) = edge else {
            continue;
        };
        if input.zero_sweep() {
            return Err(SemanticError::Invalid(
                "zero-sweep Region boundary is degenerate".into(),
            ));
        }
        if input.full_circle {
            continue;
        }
        let dx = input.end.x_mm - input.start.x_mm;
        let dy = input.end.y_mm - input.start.y_mm;
        let chord2 = dx * dx + dy * dy;
        let r0 = input.radius();
        let r1 = input.end_radius();
        let correction = (r1 - r0) * (r1 + r0) / (2.0 * chord2);
        let mut circle = *input;
        circle.center = MmPoint::new(
            input.center.x_mm + correction * dx,
            input.center.y_mm + correction * dy,
        );
        circle.source = None;
        if !circle.is_valid() {
            return Err(SemanticError::Invalid(
                "Region canonical circle is invalid".into(),
            ));
        }
        let angle =
            (circle.center.y_mm - input.center.y_mm).atan2(circle.center.x_mm - input.center.x_mm);
        let tolerance = input.numeric_tolerance();
        for point in [circle.start, circle.end] {
            let tangent_rate = circle.radius()
                + ((circle.center.x_mm - input.center.x_mm) * (point.x_mm - circle.center.x_mm)
                    + (circle.center.y_mm - input.center.y_mm) * (point.y_mm - circle.center.y_mm))
                    / circle.radius();
            if tangent_rate <= tolerance {
                return Err(SemanticError::Invalid(
                    "Region interpretation is not angularly monotone".into(),
                ));
            }
        }
        for angle in [angle, angle + std::f64::consts::PI] {
            if arc_parameter(circle, angle).is_some() {
                let point = MmPoint::new(
                    circle.center.x_mm + circle.radius() * angle.cos(),
                    circle.center.y_mm + circle.radius() * angle.sin(),
                );
                let radius = point.distance_mm(input.center);
                let tangent_rate = circle.radius()
                    + (circle.center.x_mm - input.center.x_mm) * angle.cos()
                    + (circle.center.y_mm - input.center.y_mm) * angle.sin();
                if tangent_rate <= tolerance {
                    return Err(SemanticError::Invalid(
                        "Region interpretation is not angularly monotone".into(),
                    ));
                }

                if radius < r0.min(r1) - tolerance || radius > r0.max(r1) + tolerance {
                    return Err(SemanticError::Invalid(
                        "Region fuzzy arc has no supported circular interpretation inside its annulus".into()));
                }
            }
        }
        *input = circle;
    }
    Ok(result)
}

/// The annular sector contains every permitted interpretation, not just our
/// chosen circle. Refuse a Region if another boundary can enter that sector.
/// Endpoint contact is allowed only within the existing core comparison bound.
fn validate_region_envelopes(
    original: &RegionContour,
    canonical: &RegionContour,
) -> Result<(), SemanticError> {
    // A retraced cut-in attaches at a boundary endpoint but is not adjacent
    // in contour order on its return trip. Apply the SAME endpoint contact
    // bound used for adjacent edges after the intersection validator has proven
    // the connector pair. Interior envelope intersections remain rejected.
    let (cutin_points, _, _) = contour_cutins(&original.edges);
    let mut pairs = 0usize;
    for (index, edge) in original.edges.iter().enumerate() {
        let RegionEdge::Arc(arc) = edge else {
            continue;
        };
        if arc.arc_deviation() <= arc.numeric_tolerance() {
            continue;
        }
        for (other, candidate) in original.edges.iter().enumerate() {
            if index == other {
                continue;
            }
            pairs += 1;
            if pairs > MAX_INTERSECTION_CANDIDATES {
                return Err(SemanticError::ResourceLimit {
                    resource: "region_envelope_pairs",
                    limit: MAX_INTERSECTION_CANDIDATES,
                    actual: pairs,
                });
            }
            let adjacent =
                index.abs_diff(other) == 1 || index.abs_diff(other) + 1 == original.edges.len();
            let allowed = if adjacent && original.edges.len() == 2 {
                // A two-edge closed contour shares BOTH endpoints. Match the
                // intersection guard; keep the same EPSILON_MM neighborhood.
                [Some(edge_start(edge)), Some(edge_end(edge))]
            } else if adjacent {
                [
                    Some(adjacent_connection(index, other, &original.edges)),
                    None,
                ]
            } else {
                [None, None]
            };
            let mut allowed = allowed;
            for endpoint in [edge_start(edge), edge_end(edge)] {
                if cutin_points
                    .iter()
                    .any(|p| p.distance_mm(endpoint) <= EPSILON_MM)
                    && [edge_start(candidate), edge_end(candidate)]
                        .iter()
                        .any(|p| p.distance_mm(endpoint) <= EPSILON_MM)
                    && !allowed
                        .iter()
                        .flatten()
                        .any(|p| p.distance_mm(endpoint) <= EPSILON_MM)
                    && let Some(slot) = allowed.iter_mut().find(|p| p.is_none())
                {
                    *slot = Some(endpoint);
                }
            }
            let mut boundaries = vec![canonical.edges[other].clone()];
            if let RegionEdge::Arc(other_arc) = candidate
                && other_arc.arc_deviation() > other_arc.numeric_tolerance()
            {
                boundaries.extend(arc_envelope_boundaries(*other_arc));
                // Test both directions, including complete containment.
                if arc_envelope_boundaries(*arc)
                    .iter()
                    .any(|part| edge_enters_envelope(part, *other_arc, allowed))
                {
                    return Err(SemanticError::Invalid(format!(
                        "Region arc uncertainty envelopes overlap ({index},{other})"
                    )));
                }
            }
            if boundaries
                .iter()
                .any(|part| edge_enters_envelope(part, *arc, allowed))
            {
                return Err(SemanticError::Invalid(format!(
                    "Region boundary enters an arc uncertainty envelope ({index},{other}) arc={arc:?} other={candidate:?}"
                )));
            }
        }
    }
    Ok(())
}

fn arc_envelope_boundaries(arc: ArcGeometry) -> [RegionEdge; 4] {
    let project = |point: MmPoint, radius: f64| {
        let scale = radius / point.distance_mm(arc.center);
        MmPoint::new(
            arc.center.x_mm + (point.x_mm - arc.center.x_mm) * scale,
            arc.center.y_mm + (point.y_mm - arc.center.y_mm) * scale,
        )
    };
    let low = arc.radius().min(arc.end_radius());
    let high = arc.radius().max(arc.end_radius());
    let inner_start = project(arc.start, low);
    let inner_end = project(arc.end, low);
    let outer_start = project(arc.start, high);
    let outer_end = project(arc.end, high);
    let reverse = match arc.direction {
        ArcDirection::Clockwise => ArcDirection::CounterClockwise,
        ArcDirection::CounterClockwise => ArcDirection::Clockwise,
    };
    [
        RegionEdge::Arc(ArcGeometry {
            start: outer_start,
            end: outer_end,
            source: None,
            ..arc
        }),
        RegionEdge::Line {
            start: outer_end,
            end: inner_end,
        },
        RegionEdge::Arc(ArcGeometry {
            start: inner_end,
            end: inner_start,
            direction: reverse,
            source: None,
            ..arc
        }),
        RegionEdge::Line {
            start: inner_start,
            end: outer_start,
        },
    ]
}

fn edge_enters_envelope(
    edge: &RegionEdge,
    arc: ArcGeometry,
    allowed: [Option<MmPoint>; 2],
) -> bool {
    let contains = |point: MmPoint| {
        if allowed
            .iter()
            .flatten()
            .any(|end| end.distance_mm(point) <= EPSILON_MM)
        {
            return false;
        }
        let radius = point.distance_mm(arc.center);
        radius >= arc.radius().min(arc.end_radius()) - arc.numeric_tolerance()
            && radius <= arc.radius().max(arc.end_radius()) + arc.numeric_tolerance()
            && arc_parameter(
                arc,
                (point.y_mm - arc.center.y_mm).atan2(point.x_mm - arc.center.x_mm),
            )
            .is_some()
    };
    let at = |parameter: f64| match edge {
        RegionEdge::Line { start, end } => MmPoint::new(
            start.x_mm + (end.x_mm - start.x_mm) * parameter,
            start.y_mm + (end.y_mm - start.y_mm) * parameter,
        ),
        RegionEdge::Arc(circle) => arc_point_at(*circle, parameter),
    };
    let mut parameters = vec![0.0, 1.0];
    for boundary in arc_envelope_boundaries(arc) {
        for point in edge_intersection_points(edge, &boundary) {
            // Intersection helpers also return epsilon-near endpoint contacts.
            // Such a point can be off the candidate edge. Evaluate membership
            // only on its candidate parameter below, never on that off-edge point.

            let parameter = match edge {
                RegionEdge::Line { start, end } => {
                    let dx = end.x_mm - start.x_mm;
                    let dy = end.y_mm - start.y_mm;
                    let length2 = dx * dx + dy * dy;
                    (length2 > 0.0).then(|| {
                        ((point.x_mm - start.x_mm) * dx + (point.y_mm - start.y_mm) * dy) / length2
                    })
                }
                RegionEdge::Arc(circle) => arc_parameter(
                    *circle,
                    (point.y_mm - circle.center.y_mm).atan2(point.x_mm - circle.center.x_mm),
                ),
            };
            if let Some(t) = parameter {
                parameters.push(t.clamp(0.0, 1.0));
            }
        }
    }
    parameters.sort_by(f64::total_cmp);
    // Each interval stays in one connected component of the sector complement;
    // analytic boundary intersections delimit it, no display tessellation is used.
    parameters.iter().any(|&t| contains(at(t)))
        || parameters
            .windows(2)
            .any(|pair| contains(at((pair[0] + pair[1]) / 2.0)))
}

fn validate_outline_points(points: &[MmPoint]) -> Result<(), SemanticError> {
    if points.len() < 4
        || points
            .first()
            .zip(points.last())
            .is_none_or(|(first, last)| first.distance_mm(*last) > EPSILON_MM)
    {
        return Err(SemanticError::Invalid("invalid macro outline".into()));
    }
    let edges = points
        .windows(2)
        .map(|pair| RegionEdge::Line {
            start: pair[0],
            end: pair[1],
        })
        .collect::<Vec<_>>();
    if edges.iter().any(|edge| {
        let (start, end) = (edge_start(edge), edge_end(edge));
        start.distance_mm(end) <= EPSILON_MM
    }) {
        return Err(SemanticError::Invalid("invalid macro outline".into()));
    }
    validate_contour_intersections(&edges)
}

#[derive(Clone, Copy)]
struct EdgeBounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

fn edge_bounds(edge: &RegionEdge) -> EdgeBounds {
    match edge {
        RegionEdge::Line { start, end } => EdgeBounds {
            min_x: start.x_mm.min(end.x_mm),
            max_x: start.x_mm.max(end.x_mm),
            min_y: start.y_mm.min(end.y_mm),
            max_y: start.y_mm.max(end.y_mm),
        },
        RegionEdge::Arc(arc) => {
            let radius = arc.radius();
            if arc.full_circle {
                return EdgeBounds {
                    min_x: arc.center.x_mm - radius,
                    max_x: arc.center.x_mm + radius,
                    min_y: arc.center.y_mm - radius,
                    max_y: arc.center.y_mm + radius,
                };
            }
            let mut bounds = EdgeBounds {
                min_x: arc.start.x_mm.min(arc.end.x_mm),
                max_x: arc.start.x_mm.max(arc.end.x_mm),
                min_y: arc.start.y_mm.min(arc.end.y_mm),
                max_y: arc.start.y_mm.max(arc.end.y_mm),
            };
            for angle in [
                0.0,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
                3.0 * std::f64::consts::FRAC_PI_2,
            ] {
                if arc_parameter(*arc, angle).is_some() {
                    let candidate = MmPoint::new(
                        arc.center.x_mm + radius * angle.cos(),
                        arc.center.y_mm + radius * angle.sin(),
                    );
                    bounds.min_x = bounds.min_x.min(candidate.x_mm);
                    bounds.max_x = bounds.max_x.max(candidate.x_mm);
                    bounds.min_y = bounds.min_y.min(candidate.y_mm);
                    bounds.max_y = bounds.max_y.max(candidate.y_mm);
                }
            }
            bounds
        }
    }
}

fn bounds_overlap(a: EdgeBounds, b: EdgeBounds) -> bool {
    a.min_x <= b.max_x + EPSILON_MM
        && b.min_x <= a.max_x + EPSILON_MM
        && a.min_y <= b.max_y + EPSILON_MM
        && b.min_y <= a.max_y + EPSILON_MM
}

fn validate_contour_intersections(edges: &[RegionEdge]) -> Result<(), SemanticError> {
    if edges.len() > MAX_INTERSECTION_EDGES {
        return Err(SemanticError::ResourceLimit {
            resource: "region_intersection_edges",
            limit: MAX_INTERSECTION_EDGES,
            actual: edges.len(),
        });
    }
    let bounds = edges.iter().map(edge_bounds).collect::<Vec<_>>();
    let mut order = (0..edges.len()).collect::<Vec<_>>();
    order.sort_by(|&a, &b| bounds[a].min_x.total_cmp(&bounds[b].min_x));
    let mut active: Vec<usize> = Vec::new();
    let mut candidates = 0usize;
    let (cutin_points, _cutin_axis, mixed_cutin_axes) = contour_cutins(edges);
    if mixed_cutin_axes {
        return Err(SemanticError::Invalid(
            "region cut-ins must use one axis".into(),
        ));
    }
    // ponytail: a sweep over x-bounds keeps the normal large contour near O(n log n);
    // a bounded candidate count gives a clear failure for adversarial dense input.
    for &index in &order {
        let min_x = bounds[index].min_x;
        active.retain(|&other| bounds[other].max_x + EPSILON_MM >= min_x);
        for &other in &active {
            candidates += 1;
            if candidates > MAX_INTERSECTION_CANDIDATES {
                return Err(SemanticError::ResourceLimit {
                    resource: "region_intersection_candidates",
                    limit: MAX_INTERSECTION_CANDIDATES,
                    actual: candidates,
                });
            }
            if !bounds_overlap(bounds[index], bounds[other]) {
                continue;
            }
            let adjacent = index.abs_diff(other) == 1
                || (index == 0 && other + 1 == edges.len())
                || (other == 0 && index + 1 == edges.len());
            if legal_cutin_pair(&edges[index], &edges[other]) && !adjacent {
                continue;
            }
            if !adjacent {
                let points = edge_intersection_points(&edges[index], &edges[other]);
                let overlap = edges_overlap_interior(
                    &edges[index],
                    &edges[other],
                    MmPoint::new(f64::NAN, f64::NAN),
                );
                let points_are_allowed = points.iter().all(|point| {
                    cutin_points
                        .iter()
                        .any(|allowed| allowed.distance_mm(*point) <= EPSILON_MM)
                });
                if overlap || (!points.is_empty() && !points_are_allowed) {
                    return Err(SemanticError::Invalid(format!(
                        "region contour self-intersects ({index},{other}) points={points:?} cutins={cutin_points:?}"
                    )));
                }
            }
            if adjacent
                && if edges.len() == 2 {
                    edge_intersection_points(&edges[index], &edges[other])
                        .into_iter()
                        .any(|point| {
                            ![edge_start(&edges[index]), edge_end(&edges[index])]
                                .into_iter()
                                .any(|endpoint| endpoint.distance_mm(point) <= EPSILON_MM)
                        })
                        || edges_overlap_interior(
                            &edges[index],
                            &edges[other],
                            MmPoint::new(f64::NAN, f64::NAN),
                        )
                } else {
                    edges_intersect_beyond(
                        &edges[index],
                        &edges[other],
                        adjacent_connection(index, other, edges),
                    )
                }
            {
                return Err(SemanticError::Invalid(format!(
                    "region contour has overlapping adjacent edges ({index},{other}): {:?} / {:?}",
                    edges[index], edges[other]
                )));
            }
        }
        active.push(index);
    }
    if edges.len() == 2 {
        // Two straight return edges enclose no area.  A semicircle-plus-line
        // contour remains valid because its edges are not coincident.
        if legal_cutin_pair(&edges[0], &edges[1]) {
            return Err(SemanticError::Invalid(
                "region contour is a zero-area cut-in".into(),
            ));
        }
    }
    Ok(())
}

fn point_key(point: MmPoint) -> (i64, i64) {
    (
        (point.x_mm / EPSILON_MM).round() as i64,
        (point.y_mm / EPSILON_MM).round() as i64,
    )
}

fn contour_cutins(edges: &[RegionEdge]) -> (Vec<MmPoint>, Option<MmPoint>, bool) {
    let mut by_start = std::collections::HashMap::<(i64, i64), Vec<usize>>::new();
    for (index, edge) in edges.iter().enumerate() {
        if let RegionEdge::Line { start, .. } = edge {
            by_start.entry(point_key(*start)).or_default().push(index);
        }
    }
    let mut points = Vec::new();
    let mut axis = None;
    let mut mixed_axes = false;
    for (index, edge) in edges.iter().enumerate() {
        let RegionEdge::Line { start, end } = edge else {
            continue;
        };
        let Some(candidates) = by_start.get(&point_key(*end)) else {
            continue;
        };
        for &other in candidates {
            if other <= index {
                continue;
            }
            let RegionEdge::Line { end: other_end, .. } = &edges[other] else {
                continue;
            };
            if other_end.distance_mm(*start) <= EPSILON_MM && legal_cutin_pair(edge, &edges[other])
            {
                let this_axis = MmPoint::new(end.x_mm - start.x_mm, end.y_mm - start.y_mm);
                if axis.is_some_and(|previous: MmPoint| !parallel_cutins(previous, this_axis)) {
                    mixed_axes = true;
                    continue;
                }
                axis = Some(this_axis);
                push_unique(&mut points, *start);
                push_unique(&mut points, *end);
            }
        }
    }
    (points, axis, mixed_axes)
}

fn adjacent_connection(first: usize, second: usize, edges: &[RegionEdge]) -> MmPoint {
    if (first + 1) % edges.len() == second {
        edge_end(&edges[first])
    } else {
        edge_end(&edges[second])
    }
}

fn edges_intersect_beyond(first: &RegionEdge, second: &RegionEdge, connection: MmPoint) -> bool {
    edge_intersection_points(first, second)
        .into_iter()
        .any(|point| point.distance_mm(connection) > EPSILON_MM)
        || edges_overlap_interior(first, second, connection)
}

fn edges_overlap_interior(first: &RegionEdge, second: &RegionEdge, connection: MmPoint) -> bool {
    match (first, second) {
        (RegionEdge::Line { start: a, end: b }, RegionEdge::Line { start: c, end: d }) => {
            let points = line_line_intersections(*a, *b, *c, *d);
            points.len() > 1
                && points.iter().any(|point| {
                    !connection.is_finite() || point.distance_mm(connection) > EPSILON_MM
                })
        }
        (RegionEdge::Arc(a), RegionEdge::Arc(b))
            if a.center.distance_mm(b.center) <= EPSILON_MM
                && (a.radius() - b.radius()).abs() <= EPSILON_MM =>
        {
            coincident_arcs_overlap_interior(*a, *b, connection)
        }
        _ => false,
    }
}

fn coincident_arcs_overlap_interior(
    first: ArcGeometry,
    second: ArcGeometry,
    connection: MmPoint,
) -> bool {
    let candidates = [
        arc_point_at(first, 0.5),
        arc_point_at(second, 0.5),
        first.start,
        first.end,
        second.start,
        second.end,
    ];
    candidates.into_iter().any(|point| {
        let first_interior = point_is_arc_interior(point, first);
        let second_interior = point_is_arc_interior(point, second);
        (first_interior && point_on_arc(point, second)
            || second_interior && point_on_arc(point, first))
            && (!connection.is_finite() || point.distance_mm(connection) > EPSILON_MM)
    })
}

fn point_is_arc_interior(point: MmPoint, arc: ArcGeometry) -> bool {
    let angle = (point.y_mm - arc.center.y_mm).atan2(point.x_mm - arc.center.x_mm);
    arc_parameter(arc, angle).is_some_and(|parameter| parameter > 1e-12 && parameter < 1.0 - 1e-12)
}

fn parallel_cutins(a: MmPoint, b: MmPoint) -> bool {
    let length = a.x_mm.hypot(a.y_mm).min(b.x_mm.hypot(b.y_mm));
    length > EPSILON_MM && (a.x_mm * b.y_mm - a.y_mm * b.x_mm).abs() / length <= EPSILON_MM
}

fn legal_cutin_pair(first: &RegionEdge, second: &RegionEdge) -> bool {
    let (RegionEdge::Line { start: a, end: b }, RegionEdge::Line { start: c, end: d }) =
        (first, second)
    else {
        return false;
    };
    a.distance_mm(*d) <= EPSILON_MM
        && b.distance_mm(*c) <= EPSILON_MM
        && parallel_cutins(
            MmPoint::new(b.x_mm - a.x_mm, b.y_mm - a.y_mm),
            MmPoint::new(d.x_mm - c.x_mm, d.y_mm - c.y_mm),
        )
}

fn edge_intersection_points(first: &RegionEdge, second: &RegionEdge) -> Vec<MmPoint> {
    match (first, second) {
        (RegionEdge::Line { start: a, end: b }, RegionEdge::Line { start: c, end: d }) => {
            line_line_intersections(*a, *b, *c, *d)
        }
        (RegionEdge::Line { start, end }, RegionEdge::Arc(arc))
        | (RegionEdge::Arc(arc), RegionEdge::Line { start, end }) => {
            line_arc_intersections(*start, *end, *arc)
        }
        (RegionEdge::Arc(first), RegionEdge::Arc(second)) => arc_arc_intersections(*first, *second),
    }
}

fn push_unique(points: &mut Vec<MmPoint>, point: MmPoint) {
    if !points
        .iter()
        .any(|existing| existing.distance_mm(point) <= EPSILON_MM)
    {
        points.push(point);
    }
}

fn line_line_intersections(a: MmPoint, b: MmPoint, c: MmPoint, d: MmPoint) -> Vec<MmPoint> {
    let mut points = Vec::new();
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    let denominator = (b.x_mm - a.x_mm) * (d.y_mm - c.y_mm) - (b.y_mm - a.y_mm) * (d.x_mm - c.x_mm);
    if denominator != 0.0
        && ((ab_c > 0.0 && ab_d < 0.0) || (ab_c < 0.0 && ab_d > 0.0))
        && ((cd_a > 0.0 && cd_b < 0.0) || (cd_a < 0.0 && cd_b > 0.0))
    {
        let t = ((c.x_mm - a.x_mm) * (d.y_mm - c.y_mm) - (c.y_mm - a.y_mm) * (d.x_mm - c.x_mm))
            / denominator;
        push_unique(
            &mut points,
            MmPoint::new(
                a.x_mm + t * (b.x_mm - a.x_mm),
                a.y_mm + t * (b.y_mm - a.y_mm),
            ),
        );
    }
    for point in [a, b, c, d] {
        if point_on_segment(point, a, b) && point_on_segment(point, c, d) {
            push_unique(&mut points, point);
        }
    }
    points
}

fn cross(a: MmPoint, b: MmPoint, c: MmPoint) -> f64 {
    (b.x_mm - a.x_mm) * (c.y_mm - a.y_mm) - (b.y_mm - a.y_mm) * (c.x_mm - a.x_mm)
}

fn point_on_segment(point: MmPoint, start: MmPoint, end: MmPoint) -> bool {
    // Cross products have mm^2 units; normalize by edge length so the
    // frozen epsilon remains a physical distance for tiny font Regions too.
    cross(start, end, point).abs() <= EPSILON_MM * start.distance_mm(end)
        && point.x_mm >= start.x_mm.min(end.x_mm) - EPSILON_MM
        && point.x_mm <= start.x_mm.max(end.x_mm) + EPSILON_MM
        && point.y_mm >= start.y_mm.min(end.y_mm) - EPSILON_MM
        && point.y_mm <= start.y_mm.max(end.y_mm) + EPSILON_MM
}

fn line_arc_intersections(start: MmPoint, end: MmPoint, arc: ArcGeometry) -> Vec<MmPoint> {
    let mut points = Vec::new();
    let dx = end.x_mm - start.x_mm;
    let dy = end.y_mm - start.y_mm;
    let fx = start.x_mm - arc.center.x_mm;
    let fy = start.y_mm - arc.center.y_mm;
    let a = dx * dx + dy * dy;
    if a <= EPSILON_MM * EPSILON_MM {
        return points;
    }
    let b = 2.0 * (fx * dx + fy * dy);
    let c = fx * fx + fy * fy - arc.radius() * arc.radius();
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return points;
    }
    let root = discriminant.sqrt();
    for t in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
        if (-EPSILON_MM..=1.0 + EPSILON_MM).contains(&t) {
            let point = MmPoint::new(start.x_mm + t * dx, start.y_mm + t * dy);
            if point_on_arc(point, arc) {
                push_unique(&mut points, point);
            }
        }
    }
    points
}

fn arc_arc_intersections(first: ArcGeometry, second: ArcGeometry) -> Vec<MmPoint> {
    let first_radius = first.radius();
    let second_radius = second.radius();
    let centers = first.center.distance_mm(second.center);
    let mut points = Vec::new();
    if centers <= EPSILON_MM && (first_radius - second_radius).abs() <= EPSILON_MM {
        for point in [first.start, first.end, second.start, second.end] {
            if point_on_arc(point, first) && point_on_arc(point, second) {
                push_unique(&mut points, point);
            }
        }
        return points;
    }
    if centers > first_radius + second_radius + EPSILON_MM
        || centers + first_radius + EPSILON_MM < second_radius
        || centers + second_radius + EPSILON_MM < first_radius
        || centers <= EPSILON_MM
    {
        return points;
    }
    let along = (first_radius * first_radius - second_radius * second_radius + centers * centers)
        / (2.0 * centers);
    let height_squared = first_radius * first_radius - along * along;
    if height_squared < 0.0 {
        return points;
    }
    let height = height_squared.sqrt();
    let axis = MmPoint::new(
        (second.center.x_mm - first.center.x_mm) / centers,
        (second.center.y_mm - first.center.y_mm) / centers,
    );
    let base = MmPoint::new(
        first.center.x_mm + along * axis.x_mm,
        first.center.y_mm + along * axis.y_mm,
    );
    let perpendicular = MmPoint::new(-axis.y_mm * height, axis.x_mm * height);
    for point in [
        MmPoint::new(
            base.x_mm + perpendicular.x_mm,
            base.y_mm + perpendicular.y_mm,
        ),
        MmPoint::new(
            base.x_mm - perpendicular.x_mm,
            base.y_mm - perpendicular.y_mm,
        ),
    ] {
        if point_on_arc(point, first) && point_on_arc(point, second) {
            push_unique(&mut points, point);
        }
    }
    points
}

fn edge_start(edge: &RegionEdge) -> MmPoint {
    match edge {
        RegionEdge::Line { start, .. } | RegionEdge::Arc(ArcGeometry { start, .. }) => *start,
    }
}

fn edge_end(edge: &RegionEdge) -> MmPoint {
    match edge {
        RegionEdge::Line { end, .. } | RegionEdge::Arc(ArcGeometry { end, .. }) => *end,
    }
}

fn apply_inverse_transform(
    mut point: MmPoint,
    center: MmPoint,
    transform: LocalTransform,
) -> MmPoint {
    point.x_mm = (point.x_mm - center.x_mm) / transform.scale;
    point.y_mm = (point.y_mm - center.y_mm) / transform.scale;
    let angle = -transform.rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    point = MmPoint::new(
        point.x_mm * cos - point.y_mm * sin,
        point.x_mm * sin + point.y_mm * cos,
    );
    match transform.mirror {
        Mirror::None => point,
        Mirror::X => MmPoint::new(-point.x_mm, point.y_mm),
        Mirror::Y => MmPoint::new(point.x_mm, -point.y_mm),
        Mirror::Xy => MmPoint::new(-point.x_mm, -point.y_mm),
    }
}

fn geometry_covers(
    geometry: &SemanticGeometry,
    point: MmPoint,
    apertures: &std::collections::HashMap<&str, &ApertureShape>,
) -> bool {
    match geometry {
        SemanticGeometry::Flash {
            center,
            aperture_id,
            transform,
        } => apertures.get(aperture_id.as_str()).is_some_and(|shape| {
            aperture_covers(shape, apply_inverse_transform(point, *center, *transform))
        }),
        SemanticGeometry::Line {
            start,
            end,
            width_mm,
        } => capsule_covers(*start, *end, *width_mm, point),
        SemanticGeometry::RectangularSweep {
            start,
            end,
            width_mm,
            height_mm,
        } => {
            let min_x = start.x_mm.min(end.x_mm) - width_mm / 2.0;
            let max_x = start.x_mm.max(end.x_mm) + width_mm / 2.0;
            let min_y = start.y_mm.min(end.y_mm) - height_mm / 2.0;
            let max_y = start.y_mm.max(end.y_mm) + height_mm / 2.0;
            point.x_mm >= min_x - EPSILON_MM
                && point.x_mm <= max_x + EPSILON_MM
                && point.y_mm >= min_y - EPSILON_MM
                && point.y_mm <= max_y + EPSILON_MM
        }
        SemanticGeometry::Arc { path, width_mm } => arc_covers(*path, *width_mm, point),
        // Each contour is an independently filled region.  Holes are encoded
        // by a cut-in within that contour; RegionRole::Hole never subtracts a
        // separate contour from its siblings.
        SemanticGeometry::Region { contours } => contours
            .iter()
            .any(|contour| contour_covers(contour, point)),
        // Resolved by the document-level caller (`block::resolve_instance`)
        // before it reaches a per-geometry function; not reachable in practice.
        SemanticGeometry::BlockInstance { .. } => false,
    }
}

fn aperture_covers(shape: &ApertureShape, point: MmPoint) -> bool {
    match shape {
        ApertureShape::Circle {
            diameter_mm,
            hole_diameter_mm,
        } => radial_covers(point, *diameter_mm, *hole_diameter_mm),
        ApertureShape::Rectangle {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => rectangle_covers(point, *width_mm, *height_mm, *hole_diameter_mm),
        ApertureShape::Obround {
            width_mm,
            height_mm,
            hole_diameter_mm,
        } => obround_covers(point, *width_mm, *height_mm, *hole_diameter_mm),
        ApertureShape::Polygon {
            diameter_mm,
            vertices,
            rotation_deg,
            hole_diameter_mm,
        } => polygon_covers(
            point,
            *diameter_mm,
            *vertices,
            *rotation_deg,
            *hole_diameter_mm,
        ),
        ApertureShape::Macro { primitives } => {
            let mut covered = false;
            for primitive in primitives {
                let hit = match primitive {
                    MacroPrimitive::Circle {
                        diameter_mm,
                        center,
                        rotation_deg,
                        ..
                    } => radial_covers(
                        rotate(point, -*rotation_deg).sub(*center),
                        *diameter_mm,
                        None,
                    ),
                    MacroPrimitive::CenterLine {
                        width_mm,
                        height_mm,
                        center,
                        rotation_deg,
                        ..
                    } => {
                        let local = rotate(point, -*rotation_deg).sub(*center);
                        rectangle_covers(local, *width_mm, *height_mm, None)
                    }
                    MacroPrimitive::Outline {
                        points,
                        rotation_deg,
                        ..
                    } => {
                        let local = rotate(point, -*rotation_deg);
                        polygon_points_covers(points, local)
                    }
                };
                if hit {
                    match primitive_exposure(primitive) {
                        Exposure::Dark => covered = true,
                        Exposure::Clear => covered = false,
                    }
                }
            }
            covered
        }
    }
}

fn primitive_exposure(primitive: &MacroPrimitive) -> Exposure {
    match primitive {
        MacroPrimitive::Circle { exposure, .. }
        | MacroPrimitive::CenterLine { exposure, .. }
        | MacroPrimitive::Outline { exposure, .. } => *exposure,
    }
}

fn radial_covers(point: MmPoint, diameter: f64, hole: Option<f64>) -> bool {
    let radius = point.x_mm.hypot(point.y_mm);
    radius <= diameter / 2.0 + EPSILON_MM && hole.is_none_or(|inner| radius >= inner / 2.0)
}

fn rectangle_covers(point: MmPoint, width: f64, height: f64, hole: Option<f64>) -> bool {
    point.x_mm.abs() <= width / 2.0 + EPSILON_MM
        && point.y_mm.abs() <= height / 2.0 + EPSILON_MM
        && hole.is_none_or(|inner| point.x_mm.hypot(point.y_mm) >= inner / 2.0)
}

fn obround_covers(point: MmPoint, width: f64, height: f64, hole: Option<f64>) -> bool {
    let (long, short, along_x) = if width >= height {
        (width, height, true)
    } else {
        (height, width, false)
    };
    let half_segment = (long - short) / 2.0;
    let local = if along_x {
        point
    } else {
        MmPoint::new(point.y_mm, point.x_mm)
    };
    let distance = (local.x_mm.abs() - half_segment).max(0.0).hypot(local.y_mm);
    distance <= short / 2.0 + EPSILON_MM
        && hole.is_none_or(|inner| point.x_mm.hypot(point.y_mm) >= inner / 2.0)
}

fn polygon_covers(
    point: MmPoint,
    diameter: f64,
    vertices: u8,
    rotation_deg: f64,
    hole: Option<f64>,
) -> bool {
    if hole.is_some_and(|inner| point.x_mm.hypot(point.y_mm) < inner / 2.0) {
        return false;
    }
    let radius = diameter / 2.0;
    let local = rotate(point, -rotation_deg);
    let vertices = (0..vertices)
        .map(|index| {
            let angle = std::f64::consts::TAU * f64::from(index) / f64::from(vertices);
            MmPoint::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect::<Vec<_>>();
    polygon_points_covers(&vertices, local)
}

fn polygon_points_covers(points: &[MmPoint], point: MmPoint) -> bool {
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if ((a.y_mm > point.y_mm) != (b.y_mm > point.y_mm))
            && point.x_mm < (b.x_mm - a.x_mm) * (point.y_mm - a.y_mm) / (b.y_mm - a.y_mm) + a.x_mm
        {
            inside = !inside;
        }
    }
    inside
}

fn rotate(point: MmPoint, degrees: f64) -> MmPoint {
    let (sin, cos) = degrees.to_radians().sin_cos();
    MmPoint::new(
        point.x_mm * cos - point.y_mm * sin,
        point.x_mm * sin + point.y_mm * cos,
    )
}

fn capsule_covers(start: MmPoint, end: MmPoint, width: f64, point: MmPoint) -> bool {
    let dx = end.x_mm - start.x_mm;
    let dy = end.y_mm - start.y_mm;
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared <= EPSILON_MM * EPSILON_MM {
        0.0
    } else {
        ((point.x_mm - start.x_mm) * dx + (point.y_mm - start.y_mm) * dy) / length_squared
    }
    .clamp(0.0, 1.0);
    MmPoint::new(start.x_mm + t * dx, start.y_mm + t * dy).distance_mm(point)
        <= width / 2.0 + EPSILON_MM
}

fn arc_covers(path: ArcGeometry, width: f64, point: MmPoint) -> bool {
    if !width.is_finite() || width <= 0.0 || !path.is_valid() {
        return false;
    }
    if path.zero_sweep() {
        return point.distance_mm(path.start) <= width / 2.0 + EPSILON_MM;
    }
    let circle = path.canonical_circle();
    if capsule_covers(path.start, circle.start, width, point)
        || capsule_covers(circle.end, path.end, width, point)
    {
        return true;
    }
    (point.distance_mm(circle.center) - circle.radius()).abs() <= width / 2.0 + EPSILON_MM
        && (circle.full_circle
            || arc_parameter(
                circle,
                (point.y_mm - circle.center.y_mm).atan2(point.x_mm - circle.center.x_mm),
            )
            .is_some())
}

fn contour_covers(contour: &RegionContour, point: MmPoint) -> bool {
    let Ok(canonical) = canonical_region_contour(contour) else {
        return false;
    };
    let contour = &canonical;
    if !point.is_valid_geometry() {
        return false;
    }
    if contour.edges.len() == 1
        && let RegionEdge::Arc(arc) = contour.edges[0]
        && arc.full_circle
    {
        return point.distance_mm(arc.center) <= arc.radius() + EPSILON_MM;
    }
    if contour
        .edges
        .iter()
        .any(|edge| edge_contains_point(edge, point))
    {
        return true;
    }
    let winding = contour
        .edges
        .iter()
        .map(|edge| edge_ray_winding(edge, point))
        .sum::<i32>();
    winding != 0
}

fn point_on_arc(point: MmPoint, arc: ArcGeometry) -> bool {
    let radius = arc.radius();
    radius.is_finite()
        && (point.distance_mm(arc.center) - radius).abs() <= EPSILON_MM
        && (arc.full_circle
            || arc_parameter(
                arc,
                (point.y_mm - arc.center.y_mm).atan2(point.x_mm - arc.center.x_mm),
            )
            .is_some())
}

fn edge_contains_point(edge: &RegionEdge, point: MmPoint) -> bool {
    match edge {
        RegionEdge::Line { start, end } => point_on_segment(point, *start, *end),
        RegionEdge::Arc(arc) => point_on_arc(point, *arc),
    }
}

fn arc_parameter(arc: ArcGeometry, angle: f64) -> Option<f64> {
    let sweep = arc.sweep_radians()?;
    let start = (arc.start.y_mm - arc.center.y_mm).atan2(arc.start.x_mm - arc.center.x_mm);
    let mut delta = angle - start;
    match arc.direction {
        ArcDirection::CounterClockwise => {
            while delta < 0.0 {
                delta += std::f64::consts::TAU;
            }
        }
        ArcDirection::Clockwise => {
            while delta > 0.0 {
                delta -= std::f64::consts::TAU;
            }
            delta = -delta;
        }
    }
    (delta <= sweep + 1e-12).then_some((delta / sweep).clamp(0.0, 1.0))
}

fn arc_point_at(arc: ArcGeometry, parameter: f64) -> MmPoint {
    let start = (arc.start.y_mm - arc.center.y_mm).atan2(arc.start.x_mm - arc.center.x_mm);
    let sweep = arc.sweep_radians().unwrap_or(0.0);
    let angle = match arc.direction {
        ArcDirection::CounterClockwise => start + sweep * parameter,
        ArcDirection::Clockwise => start - sweep * parameter,
    };
    MmPoint::new(
        arc.center.x_mm + arc.radius() * angle.cos(),
        arc.center.y_mm + arc.radius() * angle.sin(),
    )
}

fn edge_ray_winding(edge: &RegionEdge, point: MmPoint) -> i32 {
    match edge {
        RegionEdge::Line { start, end } => ray_winding_line(*start, *end, point),
        RegionEdge::Arc(arc) => ray_winding_arc(*arc, point),
    }
}

fn ray_winding_line(start: MmPoint, end: MmPoint, point: MmPoint) -> i32 {
    if start.y_mm <= point.y_mm && end.y_mm > point.y_mm {
        (line_x_at_y(start, end, point.y_mm) > point.x_mm) as i32
    } else if start.y_mm > point.y_mm && end.y_mm <= point.y_mm {
        -((line_x_at_y(start, end, point.y_mm) > point.x_mm) as i32)
    } else {
        0
    }
}

fn line_x_at_y(start: MmPoint, end: MmPoint, y: f64) -> f64 {
    start.x_mm + (y - start.y_mm) * (end.x_mm - start.x_mm) / (end.y_mm - start.y_mm)
}

fn ray_winding_arc(arc: ArcGeometry, point: MmPoint) -> i32 {
    let dy = point.y_mm - arc.center.y_mm;
    let radius = arc.radius();
    let discriminant = radius * radius - dy * dy;
    if discriminant < 0.0 {
        return 0;
    }
    let root = discriminant.max(0.0).sqrt();
    let mut winding = 0;
    for x in [arc.center.x_mm - root, arc.center.x_mm + root] {
        if x <= point.x_mm {
            continue;
        }
        let angle = dy.atan2(x - arc.center.x_mm);
        let Some(parameter) = arc_parameter(arc, angle) else {
            continue;
        };
        let endpoint = 1e-12;
        let step = 1e-8_f64.min(0.25 / arc.sweep_radians().unwrap_or(1.0));
        let sign = if parameter <= endpoint {
            let after = arc_point_at(arc, step).y_mm;
            (after > point.y_mm) as i32
        } else if parameter >= 1.0 - endpoint {
            let before = arc_point_at(arc, 1.0 - step).y_mm;
            -((before > point.y_mm) as i32)
        } else {
            let before = arc_point_at(arc, parameter - step).y_mm;
            let after = arc_point_at(arc, parameter + step).y_mm;
            if before <= point.y_mm && after > point.y_mm {
                1
            } else if before > point.y_mm && after <= point.y_mm {
                -1
            } else {
                0
            }
        };
        winding += sign;
    }
    winding
}

impl MmPoint {
    fn sub(self, other: Self) -> Self {
        Self::new(self.x_mm - other.x_mm, self.y_mm - other.y_mm)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    InvalidGeometry(&'static str),
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGeometry(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_region_segment_tolerance_has_distance_units() {
        for scale in [0.00005, 0.001, 1., 1000.] {
            let a = MmPoint::new(0., 0.);
            let b = MmPoint::new(scale, 0.);
            assert!(!point_on_segment(
                MmPoint::new(scale * 0.5, 2. * EPSILON_MM),
                a,
                b
            ));
            assert!(point_on_segment(
                MmPoint::new(scale * 0.5, 0.5 * EPSILON_MM),
                a,
                b
            ));
        }
        let a = MmPoint::new(0., 0.);
        let b = MmPoint::new(0.0014, 0.);
        let c = MmPoint::new(0., 0.00005);
        let contour = RegionContour {
            role: RegionRole::Solid,
            edges: vec![
                RegionEdge::Line { start: a, end: b },
                RegionEdge::Line { start: b, end: c },
                RegionEdge::Line { start: c, end: a },
            ],
        };
        assert!(validate_contour(&contour).is_ok());
        assert!(
            validate_contour(&RegionContour {
                role: RegionRole::Solid,
                edges: vec![
                    RegionEdge::Line { start: a, end: b },
                    RegionEdge::Line {
                        start: b,
                        end: MmPoint::new(0.0007, 0.)
                    },
                    RegionEdge::Line {
                        start: MmPoint::new(0.0007, 0.),
                        end: a
                    }
                ]
            })
            .is_err()
        );
    }

    fn circle(diameter_mm: f64, hole_diameter_mm: Option<f64>) -> CircleAperture {
        CircleAperture::new(diameter_mm, hole_diameter_mm).unwrap()
    }

    #[test]
    fn dark_clear_dark_preserves_expected_annuli() {
        let mut layer = Layer::new("top", "S0 polarity");
        let origin = MmPoint::new(0.0, 0.0);
        layer.objects = vec![
            DrawObject {
                object_id: "outer".into(),
                geometry: Geometry::CircleFlash {
                    center: origin,
                    aperture: circle(10.0, None),
                },
                exposure: Exposure::Dark,
            },
            DrawObject {
                object_id: "clear".into(),
                geometry: Geometry::CircleFlash {
                    center: origin,
                    aperture: circle(6.0, None),
                },
                exposure: Exposure::Clear,
            },
            DrawObject {
                object_id: "inner".into(),
                geometry: Geometry::CircleFlash {
                    center: origin,
                    aperture: circle(2.0, None),
                },
                exposure: Exposure::Dark,
            },
        ];
        assert!(layer.coverage_at(origin));
        assert!(!layer.coverage_at(MmPoint::new(2.0, 0.0)));
        assert!(layer.coverage_at(MmPoint::new(4.0, 0.0)));
        assert!(!layer.coverage_at(MmPoint::new(6.0, 0.0)));
    }

    #[test]
    fn aperture_hole_is_local_and_does_not_clear_prior_line_model() {
        let origin = MmPoint::new(0.0, 0.0);
        let mut layer = Layer::new("top", "top");
        layer.objects = vec![
            DrawObject {
                object_id: "line".into(),
                geometry: Geometry::Line {
                    start: MmPoint::new(-5.0, 0.0),
                    end: MmPoint::new(5.0, 0.0),
                    width_mm: 0.5,
                },
                exposure: Exposure::Dark,
            },
            DrawObject {
                object_id: "ring".into(),
                geometry: Geometry::CircleFlash {
                    center: origin,
                    aperture: circle(10.0, Some(4.0)),
                },
                exposure: Exposure::Dark,
            },
        ];
        // The ring's local hole leaves the previous line visible at its center.
        assert!(layer.coverage_at(origin));
        assert!(layer.coverage_at(MmPoint::new(4.0, 0.0)));
        assert!(!layer.coverage_at(MmPoint::new(0.0, 1.5)));

        let mut document = Document::new("s0");
        document.layers.push(layer);
        let mut clear_layer = Layer::new("clear-layer", "clear-layer");
        clear_layer.objects.push(DrawObject {
            object_id: "clear".into(),
            geometry: Geometry::CircleFlash {
                center: origin,
                aperture: circle(10.0, None),
            },
            exposure: Exposure::Clear,
        });
        document.layers.push(clear_layer);
        assert_eq!(document.layer_coverage_at("top", origin), Some(true));
        assert_eq!(
            document.layer_coverage_at("clear-layer", origin),
            Some(false)
        );
    }

    #[test]
    fn points_are_measured_in_millimetres() {
        assert_eq!(
            MmPoint::new(0.0, 0.0).distance_mm(MmPoint::new(3.0, 4.0)),
            5.0
        );
    }

    #[test]
    fn coincident_arc_overlap_is_rejected_but_complementary_arcs_are_valid() {
        let upper = ArcGeometry {
            start: MmPoint::new(1.0, 0.0),
            end: MmPoint::new(-1.0, 0.0),
            center: MmPoint::new(0.0, 0.0),
            direction: ArcDirection::CounterClockwise,
            full_circle: false,
            source: None,
        };
        let same_path_backwards = ArcGeometry {
            start: MmPoint::new(-1.0, 0.0),
            end: MmPoint::new(1.0, 0.0),
            center: MmPoint::new(0.0, 0.0),
            direction: ArcDirection::Clockwise,
            full_circle: false,
            source: None,
        };
        let lower = ArcGeometry {
            start: MmPoint::new(-1.0, 0.0),
            end: MmPoint::new(1.0, 0.0),
            center: MmPoint::new(0.0, 0.0),
            direction: ArcDirection::CounterClockwise,
            full_circle: false,
            source: None,
        };
        assert!(
            validate_contour(&RegionContour {
                edges: vec![RegionEdge::Arc(upper), RegionEdge::Arc(same_path_backwards)],
                role: RegionRole::Solid,
            })
            .is_err()
        );
        assert!(
            validate_contour(&RegionContour {
                edges: vec![RegionEdge::Arc(upper), RegionEdge::Arc(lower)],
                role: RegionRole::Solid,
            })
            .is_ok()
        );
    }

    #[test]
    fn macro_outline_propagates_typed_intersection_resource_limit() {
        let edge_count = MAX_INTERSECTION_EDGES + 1;
        let mut points = (0..edge_count)
            .map(|index| MmPoint::new(index as f64, 0.0))
            .collect::<Vec<_>>();
        points.push(points[0]);
        let primitive = MacroPrimitive::Outline {
            exposure: Exposure::Dark,
            points,
            rotation_deg: 0.0,
        };
        assert!(matches!(
            validate_macro_primitive(&primitive),
            Err(SemanticError::ResourceLimit {
                resource: "region_intersection_edges",
                limit: MAX_INTERSECTION_EDGES,
                actual,
            }) if actual > MAX_INTERSECTION_EDGES
        ));
    }
}

#[cfg(test)]
mod cutin_direction_regression {
    use super::*;
    #[test]
    fn rotated_parallel_cutins_detected_but_mixed_directions_rejected() {
        let line = |a, b, c, d| RegionEdge::Line {
            start: MmPoint::new(a, b),
            end: MmPoint::new(c, d),
        };
        let mut edges = vec![
            line(0., 0., 1., 1.),
            line(1., 1., 0., 0.),
            line(2., 0., 4., 2.),
            line(4., 2., 2., 0.),
        ];
        let (points, _, mixed) = contour_cutins(&edges);
        assert_eq!(points.len(), 4);
        assert!(!mixed);
        edges[2] = line(2., 0., 2., 2.);
        edges[3] = line(2., 2., 2., 0.);
        assert!(contour_cutins(&edges).2);
        assert!(!legal_cutin_pair(&edges[0], &line(1., 1., 0.1, 0.)));
    }
}

#[cfg(test)]
mod text_cutin_regression {
    use super::*;
    #[test]
    fn retraced_cutin_arc_endpoint_is_legal_but_interior_crossing_is_not() {
        let p = |x, y| MmPoint::new(x, y);
        let outer = [
            p(-5., 0.),
            p(0., 5.0000001),
            p(5., 0.),
            p(0., -5.),
            p(-5., 0.),
        ];
        let mut edges = Vec::new();
        for w in outer.windows(2) {
            edges.push(RegionEdge::Arc(ArcGeometry {
                start: w[0],
                end: w[1],
                center: p(0., 0.),
                direction: ArcDirection::Clockwise,
                full_circle: false,
                source: Some(ArcSource {
                    resolution_mm: 1e-6,
                    single_quadrant: false,
                }),
            }));
        }
        for w in [
            p(-5., 0.),
            p(-2., 0.),
            p(-2., -2.),
            p(2., -2.),
            p(2., 2.),
            p(-2., 2.),
            p(-2., 0.),
            p(-5., 0.),
        ]
        .windows(2)
        {
            edges.push(RegionEdge::Line {
                start: w[0],
                end: w[1],
            });
        }
        let mut contour = RegionContour {
            role: RegionRole::Solid,
            edges,
        };
        validate_region_contour(&contour).unwrap();
        // Crossing the outer boundary is still rejected, regardless of cut-ins.
        contour.edges[6] = RegionEdge::Line {
            start: p(-2., -2.),
            end: p(6., -2.),
        };
        contour.edges[7] = RegionEdge::Line {
            start: p(6., -2.),
            end: p(2., 2.),
        };
        assert!(validate_region_contour(&contour).is_err());
    }
}
