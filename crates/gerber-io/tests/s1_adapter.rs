use gerber_io::{S1Budget, S1Error, parse_s1, write_s1, write_s1_with_budget};

fn simple_source() -> &'static [u8] {
    b"%FSLAX26Y26*%%MOMM*%%ADD10C,1*%D10*X0Y0D03*M02*"
}

#[test]
fn tokenizes_multiple_commands_on_one_line() {
    let scene = parse_s1(simple_source(), "one-line").expect("valid one-line source");
    assert_eq!(scene.document.object_count(), 1);
}

#[test]
fn rejects_aperture_use_before_definition() {
    let source = b"%FSLAX26Y26*%\n%MOMM*%\nD10*\nX0Y0D03*\n%ADD10C,1*%\nM02*\n";
    assert!(matches!(
        parse_s1(source, "order"),
        Err(S1Error::Semantic { .. })
    ));
}

#[test]
fn scans_unused_macro_definitions() {
    let source =
        b"%FSLAX26Y26*%\n%MOMM*%\n%AMBAD*$1=1/0*1,1,$1,0,0*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    assert!(matches!(
        parse_s1(source, "unused-am"),
        Err(S1Error::Semantic { .. })
    ));
}

#[test]
fn rejects_self_intersecting_unused_macro() {
    let source = b"%FSLAX26Y26*%\n%MOMM*%\n%AMBAD*4,1,4,0,0,2,2,0,2,2,0,0,0,0*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n";
    assert!(matches!(
        parse_s1(source, "bowtie"),
        Err(S1Error::Semantic { .. })
    ));
}

#[test]
fn writer_uses_local_macro_definition_and_honors_budget() {
    let source =
        b"%FSLAX26Y26*%\n%MOMM*%\n%AMRCAM*1,1,1,0,0,0*%\n%ADD10RCAM*%\nD10*\nX0Y0D03*\nM02*\n";
    let scene = parse_s1(source, "macro").expect("valid macro source");
    let bytes = write_s1(&scene.document).expect("macro output");
    assert!(String::from_utf8_lossy(&bytes).contains("%AMRCAM10*"));
    let error = write_s1_with_budget(
        &scene.document,
        S1Budget {
            max_writer_bytes: 1,
            ..S1Budget::default()
        },
    )
    .expect_err("small writer budget must fail");
    assert!(matches!(
        error,
        S1Error::ResourceLimit {
            resource: "writer_bytes",
            ..
        }
    ));
}
