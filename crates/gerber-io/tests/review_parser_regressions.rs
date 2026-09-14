use editor_core::{Exposure, Geometry, MmPoint};
use gerber_io::parse_s0;

const BASE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,10*%\n%ADD11C,6*%\nD10*\nX0Y0D03*\nM02*\n";

#[test]
fn repeated_flash_keeps_modal_position_aperture_polarity_and_order() {
    let input = BASE.replace(
        "X0Y0D03*",
        "X1000000Y2000000D03*\nD03*\nD11*\n%LPC*%\nD03*\nD10*\n%LPD*%\nD03*",
    );
    let scene = parse_s0(input.as_bytes(), "repeat").unwrap();
    let objects = &scene.document.layers[0].objects;
    assert_eq!(objects.len(), 4);
    for (i, object) in objects.iter().enumerate() {
        assert_eq!(object.object_id, format!("object-{}", i + 1));
        let Geometry::CircleFlash { center, aperture } = object.geometry else {
            panic!()
        };
        assert_eq!(center, MmPoint::new(1.0, 2.0));
        assert_eq!(aperture.diameter_mm, if i == 2 { 6.0 } else { 10.0 });
        assert_eq!(
            object.exposure,
            if i == 2 {
                Exposure::Clear
            } else {
                Exposure::Dark
            }
        );
    }
}

#[test]
fn first_flash_without_coordinates_is_rejected() {
    for coordinates in ["D03*", "X0D03*", "Y0D03*"] {
        assert!(parse_s0(BASE.replace("X0Y0D03*", coordinates).as_bytes(), "bad").is_err());
    }
}
