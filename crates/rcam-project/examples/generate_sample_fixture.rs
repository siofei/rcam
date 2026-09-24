//! Generates `fixtures/synthetic/s4b2/sample.rcam` (§56/§65 of the S4-B2
//! brief): a steel-mesh-opening-like `BlockDefinition` (40 openings) placed
//! as 5 instances at 0/90/37 degrees and Mirror X/Y-equivalent, all sharing
//! the same definition. Synthetic data only, no private Gerber.
//!
//! Run with: `cargo run -p rcam-project --example generate_sample_fixture`

use editor_core::block::{BlockDefinitionId, BlockObject, BlockObjectGeometry, BlockTransform};
use editor_core::snap::SnapKind;
use editor_core::units::ManufacturingPrecision;
use editor_core::workspace::{Color, LayerKind, LayerWorkspaceState};
use editor_core::{
    ApertureDefinition, ApertureShape, Exposure, LocalTransform, Mirror, MmPoint, ObjectOrigin,
    SemanticGeometry, SemanticLayer, SemanticObject,
};
use rcam_project::*;
use std::path::Path;

fn build_definition() -> editor_core::block::BlockDefinition {
    let mut objects = Vec::new();
    // A 5x8 grid of small circular steel-mesh openings, 0.3mm pitch each way.
    for row in 0..5 {
        for col in 0..8 {
            objects.push(BlockObject {
                geometry: BlockObjectGeometry::Flash {
                    center: MmPoint::new(f64::from(col) * 0.3, f64::from(row) * 0.3),
                    aperture_id: "shared::opening".into(),
                    transform: LocalTransform {
                        mirror: Mirror::None,
                        rotation_deg: 0.,
                        scale: 1.,
                    },
                },
                exposure: Exposure::Dark,
            });
        }
    }
    editor_core::block::BlockDefinition {
        id: BlockDefinitionId("bga400-opening-array".into()),
        name: "BGA400 opening array".into(),
        local_origin: MmPoint::new(0.6, 0.6),
        objects,
        revision: 0,
    }
}

fn build_project() -> RCamProject {
    let aperture = ApertureDefinition {
        id: "shared::opening".into(),
        source_dcode: 10,
        shape: ApertureShape::Circle {
            diameter_mm: 0.15,
            hole_diameter_mm: None,
        },
    };
    let definition = build_definition();
    let placements = [
        (0.0, false),
        (90.0, false),
        (37.0, false),
        (0.0, true),   // Mirror X (reflect_x)
        (180.0, true), // Mirror X + rotate 180 = canonical Mirror Y (ADR 0032)
    ];
    let mut objects = Vec::new();
    for (index, (rotation_deg, mirror)) in placements.iter().enumerate() {
        objects.push(SemanticObject {
            object_id: format!("instance-{index}"),
            geometry: SemanticGeometry::BlockInstance {
                definition_id: definition.id.clone(),
                transform: BlockTransform {
                    translation: MmPoint::new(f64::from(index as i32) * 4.0, 0.0),
                    rotation_deg: *rotation_deg,
                    mirror: *mirror,
                },
            },
            exposure: Exposure::Dark,
            origin: ObjectOrigin::Generated {
                operation_id: "sample-fixture".into(),
            },
        });
    }

    let layer = LayerProjectState {
        layer: SemanticLayer {
            id: "steel-mesh".into(),
            objects,
        },
        workspace: LayerWorkspaceState::new(
            LayerKind::Gerber,
            "Steel Mesh",
            Color::rgb(120, 160, 220),
        ),
        provenance: None,
        compatibility_issues: Vec::new(),
    };

    RCamProject {
        format_version: FORMAT_VERSION,
        project_id: ProjectId("s4b2-sample-project".into()),
        manufacturing: ManufacturingProjectSettings {
            precision: ManufacturingPrecision::default(),
        },
        workspace: WorkspaceProjectState {
            display_unit: DisplayUnit::Millimeters,
            grid: GridSettings {
                spacing_mm: 0.5,
                visible: true,
                snap: true,
            },
            snap: SnapSettingsState {
                enabled: true,
                enabled_kinds: vec![SnapKind::Endpoint, SnapKind::Center, SnapKind::Midpoint],
                radius_px: 8.0,
            },
            active_layer_id: Some("steel-mesh".into()),
            camera: None,
        },
        layer_order: vec!["steel-mesh".into()],
        layers: vec![layer],
        apertures: vec![aperture],
        block_definitions: vec![definition],
        board: None,
    }
}

fn main() {
    let project = build_project();
    let bytes = encode_v1(&project).expect("sample fixture project must encode");
    // Round-trip sanity check before writing anything to disk.
    let decoded = decode(&bytes).expect("sample fixture must decode");
    decoded.validate().expect("sample fixture must validate");
    assert_eq!(
        bytes,
        encode_v1(&decoded).unwrap(),
        "encode must be deterministic"
    );

    let out =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/s4b2/sample.rcam");
    std::fs::write(&out, &bytes).expect("write sample.rcam");
    println!(
        "wrote {} ({} bytes, {} instances sharing 1 definition of {} objects)",
        out.display(),
        bytes.len(),
        decoded.layers[0].layer.objects.len(),
        decoded.block_definitions[0].objects.len(),
    );
}
