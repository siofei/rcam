//! Small, UI-free S0 semantic model.
//!
//! This is deliberately only the geometry needed by the S0 risk fixture. It is
//! not the V1 editor model and has no writer or editing commands.

use serde::{Deserialize, Serialize};

pub const EPSILON_MM: f64 = 1e-6;

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
        if !diameter_mm.is_finite() || diameter_mm <= 0.0 {
            return Err(CoreError::InvalidGeometry(
                "circle diameter must be positive",
            ));
        }
        if hole_diameter_mm
            .is_some_and(|hole| !hole.is_finite() || hole <= 0.0 || hole >= diameter_mm)
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
            .is_some_and(|hole| radius < hole / 2.0 - EPSILON_MM);
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
}
