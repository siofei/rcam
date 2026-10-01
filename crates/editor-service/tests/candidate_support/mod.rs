#![allow(dead_code)]
#[path = "../array_support/mod.rs"]
pub mod array_support;
pub use array_support::Run;
use editor_core::{board::*, pnp::*};
use editor_service::*;
use serde_json::json;
pub fn mapping() -> PnpMapping {
    serde_json::from_value(json!({"delimiter":"csv","unit":"mm","refdes":0,"x":1,"y":2,"rotation":3,"side":4,"footprint":5,"value":6,"top_token":"Top","bottom_token":"Bottom","clockwise":false,"rotation_offset_deg":0,"invert_y":false})).unwrap()
}
pub fn import(r: &mut Run) {
    std::fs::write(r.dir.join("pnp.csv"),b"RefDes,X,Y,Rotation,Side,Footprint,Value\nC15,0,0,37,Top,unused,secret\nC16,0,0,37,Bottom,unused,secret\n").unwrap();
    let p = r
        .service
        .components_preview_pnp("pnp.csv", &mapping())
        .unwrap();
    r.service
        .components_import_pnp(
            &r.document,
            &r.info().revision,
            ImportPnpParams {
                path: "pnp.csv".into(),
                mapping: mapping(),
                preview_sha256: p.sha256,
                allow_replace: false,
            },
        )
        .unwrap();
}
pub fn register(r: &mut Run, t: CoordinateTransform2D) {
    r.service
        .board_set_registration(
            &r.document,
            &r.info().revision,
            RegistrationInput::Manual { transform: t },
        )
        .unwrap();
}
pub fn query(r: &Run) -> NearbyManufacturingQuery {
    NearbyManufacturingQuery {
        revision: r.info().revision,
        component_id: r
            .service
            .board_state(&r.document)
            .unwrap()
            .unwrap()
            .components[0]
            .id
            .0
            .clone(),
        layer_ids: vec![r.layer.clone()],
        window: ManufacturingSearchWindow::ComponentLocalRect {
            width_mm: 10.,
            height_mm: 10.,
        },
        offset: 0,
        limit: 500,
    }
}
pub fn setup(count: usize) -> Run {
    let mut r = Run::new(count);
    import(&mut r);
    register(&mut r, CoordinateTransform2D::IDENTITY);
    r
}
pub fn nearby(
    r: &Run,
    q: &NearbyManufacturingQuery,
) -> Result<ManufacturingCandidatePage, ServiceError> {
    r.service.components_nearby_manufacturing(&r.document, q)
}
