//! §57 of the S4-B2 brief: a definition with 400 openings placed as 100
//! instances must not make `.rcam` logical size (or project memory) scale
//! like 40,000 independent flattened primitives.
use editor_core::block::{BlockDefinitionId, BlockObject, BlockObjectGeometry, BlockTransform};
use editor_core::snap::SnapKind;
use editor_core::units::ManufacturingPrecision;
use editor_core::workspace::{Color, LayerKind, LayerWorkspaceState};
use editor_core::{
    ApertureDefinition, ApertureShape, Exposure, LocalTransform, Mirror, MmPoint, ObjectOrigin,
    SemanticGeometry, SemanticLayer, SemanticObject,
};
use rcam_project::*;

pub fn big_project(openings: usize, instances: usize) -> RCamProject {
    let aperture = ApertureDefinition {
        id: "shared::opening".into(),
        source_dcode: 10,
        shape: ApertureShape::Circle {
            diameter_mm: 0.1,
            hole_diameter_mm: None,
        },
    };
    let objects = (0..openings)
        .map(|i| BlockObject {
            geometry: BlockObjectGeometry::Flash {
                center: MmPoint::new((i % 20) as f64 * 0.2, (i / 20) as f64 * 0.2),
                aperture_id: aperture.id.clone(),
                transform: LocalTransform {
                    mirror: Mirror::None,
                    rotation_deg: 0.,
                    scale: 1.,
                },
            },
            exposure: Exposure::Dark,
        })
        .collect();
    let definition = editor_core::block::BlockDefinition {
        id: BlockDefinitionId("perf-def".into()),
        name: "perf".into(),
        local_origin: MmPoint::new(0., 0.),
        objects,
        revision: 0,
    };
    let layer_objects = (0..instances)
        .map(|i| SemanticObject {
            object_id: format!("inst-{i}"),
            geometry: SemanticGeometry::BlockInstance {
                definition_id: definition.id.clone(),
                transform: BlockTransform {
                    translation: MmPoint::new(i as f64 * 5.0, 0.0),
                    rotation_deg: 0.,
                    mirror: false,
                },
            },
            exposure: Exposure::Dark,
            origin: ObjectOrigin::Generated {
                operation_id: "perf".into(),
            },
        })
        .collect();
    RCamProject {
        format_version: FORMAT_VERSION,
        project_id: ProjectId("perf-project".into()),
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
                enabled_kinds: vec![SnapKind::Endpoint],
                radius_px: 8.0,
            },
            active_layer_id: None,
            camera: None,
        },
        layer_order: vec!["l1".into()],
        layers: vec![LayerProjectState {
            layer: SemanticLayer {
                id: "l1".into(),
                objects: layer_objects,
            },
            workspace: LayerWorkspaceState::new(LayerKind::Gerber, "l1", Color::rgb(0, 0, 0)),
            provenance: None,
        }],
        apertures: vec![aperture],
        block_definitions: vec![definition],
        board: None,
    }
}
