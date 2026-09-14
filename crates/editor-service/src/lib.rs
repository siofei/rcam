//! UI-free application boundary used by both the S0 app and future commands.
//!
//! S0 intentionally exposes read-only open/query/analyze operations only. It
//! does not pretend to provide the V1 editor or a production writer.

use editor_core::{
    CircleAperture, DocumentSnapshot, DrawObject, Exposure, Geometry, Layer, MmPoint,
};
use gerber_io::{S0Error, S0Scene, parse_s0};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub api_version: u32,
    pub stage: String,
    pub read_only: bool,
    pub supported_operations: Vec<String>,
    pub unsupported_operations: Vec<String>,
    pub supported_gerber_subset: Vec<String>,
    pub unsupported_gerber_features: Vec<String>,
    pub precision: PrecisionCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecisionCapabilities {
    pub coordinate_unit: String,
    pub coordinate_type: String,
    pub compare_tolerance_mm: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenResult {
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub layer_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageSample {
    pub point: MmPoint,
    pub layer_id: String,
    pub covered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub api_version: u32,
    pub document_id: String,
    pub revision: String,
    pub samples: Vec<CoverageSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestEnvelope {
    api_version: u32,
    request_id: String,
    op: String,
    document_id: Option<String>,
    params: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenS0Params {
    source: String,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

pub struct ApplicationService {
    scenes: HashMap<String, S0Scene>,
}

impl Default for ApplicationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationService {
    pub fn new() -> Self {
        Self {
            scenes: HashMap::new(),
        }
    }

    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            api_version: API_VERSION,
            stage: "S0 technology validation".into(),
            read_only: true,
            supported_operations: vec![
                "system.capabilities".into(),
                "document.open_s0".into(),
                "document.snapshot".into(),
                "document.analyze_s0".into(),
            ],
            unsupported_operations: vec![
                "objects.move".into(),
                "objects.duplicate".into(),
                "objects.delete".into(),
                "history.undo".into(),
                "gerber.export_layer".into(),
            ],
            supported_gerber_subset: vec![
                "FS absolute coordinates".into(),
                "MO MM/IN".into(),
                "AD C circle aperture".into(),
                "LPD/LPC ordered flash exposure".into(),
                "D03 circle flash".into(),
            ],
            unsupported_gerber_features: vec![
                "D01/D02 line and arc interpolation".into(),
                "regions".into(),
                "AM/AB/SR".into(),
                "deprecated image transforms".into(),
                "production export".into(),
            ],
            precision: PrecisionCapabilities {
                coordinate_unit: "mm".into(),
                coordinate_type: "f64".into(),
                compare_tolerance_mm: "1e-6".into(),
            },
        }
    }

    pub fn open_s0(
        &mut self,
        document_id: impl Into<String>,
        bytes: &[u8],
    ) -> Result<OpenResult, ServiceError> {
        let document_id = document_id.into();
        if document_id.trim().is_empty() {
            return Err(ServiceError::invalid("document_id must not be empty"));
        }
        if self.scenes.contains_key(&document_id) {
            return Err(ServiceError {
                code: "ALREADY_EXISTS".into(),
                message: format!("document {document_id:?} is already open"),
            });
        }
        let scene = parse_s0(bytes, &document_id).map_err(map_parse_error)?;
        let layer_ids = scene
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        let diagnostics = scene.diagnostics.clone();
        self.scenes.insert(document_id.clone(), scene);
        Ok(OpenResult {
            api_version: API_VERSION,
            document_id,
            revision: "0".into(),
            layer_ids,
            diagnostics,
        })
    }

    /// Opens the fixed, local S0 risk scene used by the demo window. The
    /// ordinary `open_s0` path never appends these constructed fixtures.
    pub fn open_demo_s0(
        &mut self,
        document_id: impl Into<String>,
        bytes: &[u8],
    ) -> Result<OpenResult, ServiceError> {
        let mut result = self.open_s0(document_id, bytes)?;
        let scene = self
            .scenes
            .get_mut(&result.document_id)
            .ok_or_else(|| ServiceError::not_found(&result.document_id))?;
        add_s0_risk_layers(scene);
        result.layer_ids = scene
            .document
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect();
        result.diagnostics = scene.diagnostics.clone();
        Ok(result)
    }

    /// Strict JSON boundary for the four read-only S0 operations.
    pub fn execute_json(&mut self, raw: &str) -> Result<Value, ServiceError> {
        let request: RequestEnvelope = serde_json::from_str(raw)
            .map_err(|error| ServiceError::invalid(format!("invalid request: {error}")))?;
        if request.api_version != API_VERSION {
            return Err(ServiceError {
                code: "UNSUPPORTED_API_VERSION".into(),
                message: format!("api_version {} is not supported", request.api_version),
            });
        }
        if request.request_id.trim().is_empty() {
            return Err(ServiceError::invalid("request_id must not be empty"));
        }
        let result = match request.op.as_str() {
            "system.capabilities" => {
                parse_empty_params(&request.params)?;
                serde_json::to_value(self.capabilities()).map_err(serialize_error)?
            }
            "document.open_s0" => {
                let document_id = request
                    .document_id
                    .clone()
                    .ok_or_else(|| ServiceError::invalid("document_id is required"))?;
                let params: OpenS0Params = parse_params(&request.params)?;
                serde_json::to_value(self.open_s0(document_id, params.source.as_bytes())?)
                    .map_err(serialize_error)?
            }
            "document.snapshot" => {
                parse_empty_params(&request.params)?;
                let document_id = request
                    .document_id
                    .clone()
                    .ok_or_else(|| ServiceError::invalid("document_id is required"))?;
                serde_json::to_value(self.snapshot(&document_id)?).map_err(serialize_error)?
            }
            "document.analyze_s0" => {
                parse_empty_params(&request.params)?;
                let document_id = request
                    .document_id
                    .clone()
                    .ok_or_else(|| ServiceError::invalid("document_id is required"))?;
                serde_json::to_value(self.analyze_s0(&document_id)?).map_err(serialize_error)?
            }
            _ => {
                return Err(ServiceError {
                    code: "UNSUPPORTED_OPERATION".into(),
                    message: request.op,
                });
            }
        };
        Ok(serde_json::json!({
            "api_version": API_VERSION,
            "request_id": request.request_id,
            "status": "completed",
            "document_id": request.document_id,
            "revision": "0",
            "result": result,
            "warnings": [],
            "error": null,
            "job_id": null
        }))
    }

    pub fn snapshot(&self, document_id: &str) -> Result<DocumentSnapshot, ServiceError> {
        self.scenes
            .get(document_id)
            .map(|scene| scene.document.snapshot(0))
            .ok_or_else(|| ServiceError::not_found(document_id))
    }

    pub fn analyze_s0(&self, document_id: &str) -> Result<AnalysisResult, ServiceError> {
        let scene = self
            .scenes
            .get(document_id)
            .ok_or_else(|| ServiceError::not_found(document_id))?;
        let layer_id = scene
            .document
            .layers
            .first()
            .map(|layer| layer.id.clone())
            .ok_or_else(|| ServiceError::invalid("document has no layer"))?;
        let points = [0.0, 2.0, 4.0, 6.0].map(|x| MmPoint::new(x, 0.0));
        let samples = points
            .into_iter()
            .map(|point| CoverageSample {
                point,
                layer_id: layer_id.clone(),
                covered: scene
                    .document
                    .layer_coverage_at(&layer_id, point)
                    .unwrap_or(false),
            })
            .collect();
        Ok(AnalysisResult {
            api_version: API_VERSION,
            document_id: document_id.into(),
            revision: "0".into(),
            samples,
        })
    }
}

impl ServiceError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_ARGUMENT".into(),
            message: message.into(),
        }
    }

    fn not_found(document_id: &str) -> Self {
        Self {
            code: "NOT_FOUND".into(),
            message: format!("document {document_id:?} is not open"),
        }
    }
}

fn map_parse_error(error: S0Error) -> ServiceError {
    let code = match &error {
        S0Error::Unsupported { .. } => "UNSUPPORTED_FEATURE",
        S0Error::DuplicateModal { .. }
        | S0Error::ContentAfterEnd { .. }
        | S0Error::MissingModal(_)
        | S0Error::Parser(_)
        | S0Error::ParserCommand(_) => "VALIDATION_FAILED",
        S0Error::InvalidGeometry(message) if message.contains("limit") => "RESOURCE_LIMIT",
        S0Error::InvalidGeometry(_) | S0Error::InvalidUtf8 | S0Error::Empty => "INVALID_ARGUMENT",
    };
    ServiceError {
        code: code.into(),
        message: error.to_string(),
    }
}

fn add_s0_risk_layers(scene: &mut S0Scene) {
    let mut line_ring = Layer::new("line-ring", "S0 line + local-hole ring");
    line_ring.objects.push(DrawObject {
        object_id: "line-ring-line".into(),
        geometry: Geometry::Line {
            start: MmPoint::new(7.0, 0.0),
            end: MmPoint::new(17.0, 0.0),
            width_mm: 0.5,
        },
        exposure: Exposure::Dark,
    });
    line_ring.objects.push(DrawObject {
        object_id: "line-ring-ring".into(),
        geometry: Geometry::CircleFlash {
            center: MmPoint::new(12.0, 0.0),
            aperture: CircleAperture::new(10.0, Some(4.0)).expect("fixed S0 geometry"),
        },
        exposure: Exposure::Dark,
    });
    let mut cross_dark = Layer::new("cross-dark", "S0 cross-layer dark");
    cross_dark.objects.push(DrawObject {
        object_id: "cross-dark-flash".into(),
        geometry: Geometry::CircleFlash {
            center: MmPoint::new(-12.0, 0.0),
            aperture: CircleAperture::new(5.0, None).expect("fixed S0 geometry"),
        },
        exposure: Exposure::Dark,
    });
    let mut cross_clear = Layer::new("cross-clear", "S0 cross-layer clear");
    cross_clear.objects.push(DrawObject {
        object_id: "cross-clear-flash".into(),
        geometry: Geometry::CircleFlash {
            center: MmPoint::new(-12.0, 0.0),
            aperture: CircleAperture::new(5.0, None).expect("fixed S0 geometry"),
        },
        exposure: Exposure::Clear,
    });
    // The clear layer is independent, so its operation cannot erase the dark
    // circle on the other layer.
    scene
        .document
        .layers
        .extend([line_ring, cross_dark, cross_clear]);
    scene
        .diagnostics
        .push("S0 also displays independent line/ring and cross-layer Clear fixtures".into());
}

fn parse_empty_params(value: &Value) -> Result<(), ServiceError> {
    parse_params::<EmptyParams>(value).map(|_| ())
}

fn parse_params<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, ServiceError> {
    serde_json::from_value(value.clone())
        .map_err(|error| ServiceError::invalid(format!("invalid params: {error}")))
}

fn serialize_error(error: serde_json::Error) -> ServiceError {
    ServiceError::invalid(format!("response serialization failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"G04 service S0*
%FSLAX24Y24*%
%MOMM*%
%ADD10C,10*%
%ADD11C,6*%
%ADD12C,2*%
%LPD*%
D10*
X0Y0D03*
%LPC*%
D11*
X0Y0D03*
%LPD*%
D12*
X0Y0D03*
M02*
"#;

    #[test]
    fn service_returns_real_parser_snapshot_and_analysis() {
        let mut service = ApplicationService::new();
        let opened = service.open_s0("demo", SAMPLE).unwrap();
        assert_eq!(opened.revision, "0");
        let snapshot = service.snapshot("demo").unwrap();
        assert_eq!(snapshot.layers[0].objects.len(), 3);
        let analysis = service.analyze_s0("demo").unwrap();
        assert_eq!(
            analysis
                .samples
                .iter()
                .map(|sample| sample.covered)
                .collect::<Vec<_>>(),
            vec![true, false, true, false]
        );
    }

    #[test]
    fn capability_dto_is_json_round_trippable() {
        let service = ApplicationService::new();
        let capabilities = service.capabilities();
        let json = serde_json::to_string(&capabilities).unwrap();
        assert_eq!(
            serde_json::from_str::<Capabilities>(&json).unwrap(),
            capabilities
        );
    }

    #[test]
    fn json_boundary_rejects_unknown_versions_operations_and_fields() {
        let mut service = ApplicationService::new();
        let request = serde_json::json!({
            "api_version": 1,
            "request_id": "cap-1",
            "op": "system.capabilities",
            "params": {}
        });
        let response = service.execute_json(&request.to_string()).unwrap();
        assert_eq!(response["request_id"], "cap-1");
        assert_eq!(response["status"], "completed");

        let unknown = serde_json::json!({
            "api_version": 1,
            "request_id": "bad",
            "op": "system.capabilities",
            "params": {},
            "extra": true
        });
        assert_eq!(
            service.execute_json(&unknown.to_string()).unwrap_err().code,
            "INVALID_ARGUMENT"
        );

        let op = serde_json::json!({
            "api_version": 1,
            "request_id": "bad-op",
            "op": "objects.move",
            "params": {}
        });
        assert_eq!(
            service.execute_json(&op.to_string()).unwrap_err().code,
            "UNSUPPORTED_OPERATION"
        );
    }

    #[test]
    fn duplicate_document_id_is_rejected_without_revision_reset() {
        let mut service = ApplicationService::new();
        service.open_s0("demo", SAMPLE).unwrap();
        let error = service.open_s0("demo", SAMPLE).unwrap_err();
        assert_eq!(error.code, "ALREADY_EXISTS");
        assert_eq!(service.snapshot("demo").unwrap().revision, "0");
    }
}
