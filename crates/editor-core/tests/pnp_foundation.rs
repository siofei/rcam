use editor_core::{MmPoint, board::*, pnp::*};
fn mapping() -> PnpMapping {
    PnpMapping {
        delimiter: Delimiter::Csv,
        unit: PnpUnit::Mm,
        refdes: 0,
        x: 1,
        y: 2,
        rotation: 3,
        side: 4,
        footprint: Some(5),
        value: Some(6),
        top_token: "Top".into(),
        bottom_token: "Bottom".into(),
        clockwise: false,
        rotation_offset_deg: 0.,
        invert_y: false,
    }
}

#[test]
fn explicit_mapping_bom_quotes_units_direction_side_unicode() {
    let bytes = include_bytes!("../../../fixtures/synthetic/s4d1/pnp.csv");
    let mut m = mapping();
    let preview = parse_pnp(bytes, &m);
    assert!(preview.valid(), "{preview:?}");
    assert_eq!(preview.components.len(), 3);
    assert_eq!(
        preview.components[2].footprint.as_deref(),
        Some("封装,测试")
    );
    assert_eq!(preview.components[1].side, BoardSide::Bottom);
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend_from_slice(bytes);
    assert_eq!(parse_pnp(&bom, &m), preview);
    m.unit = PnpUnit::Inch;
    m.clockwise = true;
    m.invert_y = true;
    m.rotation_offset_deg = 15.;
    let p = parse_pnp(bytes, &m);
    assert_eq!(p.components[1].position.x_mm, 254.);
    assert_eq!(p.components[1].rotation_deg, 105.);
    assert_eq!(p.components[2].position.y_mm, -254.);
    let tsv = "Side\tRotation\tY\tRef\tX\nBottom\t37\t2\t\"测试\"\t1\n";
    m.delimiter = Delimiter::Tsv;
    m.refdes = 3;
    m.x = 4;
    m.y = 2;
    m.rotation = 1;
    m.side = 0;
    m.footprint = None;
    m.value = None;
    m.unit = PnpUnit::Mm;
    m.invert_y = false;
    m.clockwise = true;
    m.rotation_offset_deg = 0.;
    let p = parse_pnp(tsv.as_bytes(), &m);
    assert!(p.valid());
    assert_eq!(p.components[0].rotation_deg, 323.);
    let mut m = mapping();
    m.footprint = None;
    m.value = None;
    let escaped = parse_pnp(b"Ref,X,Y,A,S\r\n\"R\"\"1\",1,2,30,Top\r\n", &m);
    assert!(escaped.valid());
    assert_eq!(escaped.components[0].refdes, "R\"1");
}
#[test]
fn failures_have_physical_lines_and_no_partial_components() {
    let m = mapping();
    let p = parse_pnp(
        include_bytes!("../../../fixtures/synthetic/s4d1/pnp-invalid.csv"),
        &m,
    );
    assert!(!p.valid());
    assert!(p.components.is_empty());
    assert_eq!(p.diagnostic_count, 3);
    assert_eq!(
        p.diagnostics.iter().map(|d| d.line).collect::<Vec<_>>(),
        vec![3, 3, 4]
    );
    for bytes in [
        b"\xff".as_slice(),
        b"Ref,X,Y,A,S\n\"unclosed".as_slice(),
        b"Ref,X,Y,A,S\nR,1,2,3,Top".as_slice(),
    ] {
        assert!(!parse_pnp(bytes, &m).valid());
    }
    for value in ["NaN", "inf", "-inf", "1e309", "1000001"] {
        let text = format!("R,X,Y,A,S,F,V\nR1,{value},0,0,Top,a,b\n");
        let p = parse_pnp(text.as_bytes(), &m);
        assert!(!p.valid());
        assert_eq!(p.diagnostics[0].line, 2);
    }
    let mut dup = m.clone();
    dup.x = dup.refdes;
    assert!(!parse_pnp(b"", &dup).valid());
    let text = format!(
        "R,X,Y,A,S,F,V\n{},0,0,0,Top,a,b\n",
        "R".repeat(MAX_FIELD_BYTES)
    );
    assert!(parse_pnp(text.as_bytes(), &m).valid());
    let text = format!(
        "R,X,Y,A,S,F,V\n{},0,0,0,Top,a,b\n",
        "R".repeat(MAX_FIELD_BYTES + 1)
    );
    assert_eq!(
        parse_pnp(text.as_bytes(), &m).diagnostics[0].code,
        "field_budget"
    );
    let input = vec![b'x'; MAX_PNP_BYTES + 1];
    assert_eq!(parse_pnp(&input, &m).diagnostics[0].code, "byte_budget");
    let text = "R,X,Y,A,S,F,V\nR1,0,0,0,Top,a,b\nR1,0,0,0,Bottom,a,b\n";
    assert!(parse_pnp(text.as_bytes(), &m).valid());
    let p = parse_pnp(
        b"R,X,Y,A,S,F,V\n\"R\n1\",0,0,0,Top,a,b\nR2,NaN,0,0,Top,a,b\n",
        &m,
    );
    assert_eq!(p.diagnostics.last().unwrap().line, 4);
}
#[test]
fn analytic_rigid_registration_and_rejection() {
    let board = [MmPoint::new(1., 2.), MmPoint::new(11., 2.)];
    for reflect_x in [false, true] {
        for rotation_deg in [0., 90., 37., -128.5] {
            let t = CoordinateTransform2D {
                reflect_x,
                rotation_deg,
                translation: MmPoint::new(12., -8.),
            };
            let world = board.map(|p| t.apply(p));
            let r = registration(RegistrationInput::TwoPoint {
                board,
                world,
                reflect_x,
            })
            .unwrap();
            for p in [board[0], board[1], MmPoint::new(7., 11.)] {
                assert!(r.transform.apply(p).distance_mm(t.apply(p)) < 1e-9);
                assert!(
                    r.transform
                        .inverse()
                        .apply(r.transform.apply(p))
                        .distance_mm(p)
                        < 1e-9
                );
            }
            assert!(r.residual_mm < 1e-9);
            let expected = (rotation_deg + if reflect_x { 150. } else { 30. }).rem_euclid(360.);
            assert!((t.apply_direction_deg(30.) - expected).abs() < 1e-9);
        }
    }
    assert!(
        registration(RegistrationInput::TwoPoint {
            board: [board[0]; 2],
            world: board,
            reflect_x: false
        })
        .is_err()
    );
    assert_eq!(
        registration(RegistrationInput::TwoPoint {
            board,
            world: [MmPoint::new(0., 0.), MmPoint::new(20., 0.)],
            reflect_x: false
        })
        .unwrap_err(),
        "distance_mismatch"
    );
    assert!(
        registration(RegistrationInput::Manual {
            transform: CoordinateTransform2D {
                rotation_deg: f64::NAN,
                ..Default::default()
            }
        })
        .is_err()
    );
}
