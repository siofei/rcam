mod array_support;
use array_support::Run;
use editor_core::pnp::*;
use editor_service::*;
use rcam_project::zip_codec::{ZipEntry, write_zip};
use serde_json::json;
fn mapping() -> PnpMapping {
    serde_json::from_value(json!({"delimiter":"csv","unit":"mm","refdes":1,"x":6,"y":7,"rotation":5,"side":4,"footprint":2,"value":0,"top_token":"Top","bottom_token":"Bottom","clockwise":false,"rotation_offset_deg":0,"invert_y":false,"source":{"kind":"xlsx","worksheet":"Data","header_row":3}})).unwrap()
}
fn xlsx(sheet: &str, shared: &str) -> Vec<u8> {
    let wb = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="r1"/></sheets><extLst><ext uri="calcfeatures"><x:calcFeatures xmlns:x="http://schemas.microsoft.com/office/spreadsheetml/2018/calcfeatures"><x:feature name="RD"/></x:calcFeatures></ext></extLst></workbook>"#;
    let rel = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    write_zip(&[
        ZipEntry {
            path: "xl/workbook.xml",
            data: wb.as_bytes(),
        },
        ZipEntry {
            path: "xl/_rels/workbook.xml.rels",
            data: rel.as_bytes(),
        },
        ZipEntry {
            path: "xl/sharedStrings.xml",
            data: shared.as_bytes(),
        },
        ZipEntry {
            path: "xl/worksheets/sheet1.xml",
            data: sheet.as_bytes(),
        },
    ])
}
fn data(formula: bool) -> Vec<u8> {
    let headers = [
        "PartType",
        "RefDes",
        "PartDecal",
        "Pins",
        "Layer",
        "Orient.",
        "X",
        "Y",
        "SMD",
        "Glued",
    ];
    let mut sheet = String::from(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>说明</t></is></c></row><row r="3">"#,
    );
    for (i, h) in headers.iter().enumerate() {
        sheet += &format!(
            r#"<c r="{}3" t="inlineStr"><is><t>{h}</t></is></c>"#,
            (b'A' + i as u8) as char
        );
    }
    sheet += r#"</row><row r="5"><c r="A5" t="s"><v>0</v></c><c r="B5" t="s"><v>1</v></c><c r="C5" t="inlineStr"><is><r><t>封装</t></r><r><t>&amp;名称</t></r></is></c><c r="E5" t="s"><v>2</v></c><c r="F5"><v>-90</v></c><c r="G5" t="inlineStr"><is><t>9.78266666666667</t></is></c><c r="H5"><v>-2.248</v></c><c r="I5" t="b"><v>1</v></c>"#;
    if formula {
        sheet += r#"<c r="J5"><f>1+1</f><v>2</v></c>"#;
    }
    sheet += "</row><row r=\"6\"/><row r=\"7\"><c r=\"A7\"/></row></sheetData></worksheet>";
    let shared = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>数值 &lt; 10</t></si><si><t>R测试1</t></si><si><t>Bottom</t></si></sst>"#;
    xlsx(&sheet, shared)
}
fn import(r: &mut Run, file: &str, m: &PnpMapping) {
    let p = r.service.components_preview_pnp(file, m).unwrap();
    assert!(p.preview.valid(), "{:?}", p.preview.diagnostics);
    r.service
        .components_import_pnp(
            &r.document,
            &r.info().revision,
            ImportPnpParams {
                path: file.into(),
                mapping: m.clone(),
                preview_sha256: p.sha256,
                allow_replace: false,
            },
        )
        .unwrap();
}
#[test]
fn data_only_xlsx_rows_precision_proposal_source_hash_and_project_history() {
    let mut r = Run::new(2);
    let bytes = data(false);
    std::fs::write(r.dir.join("data.xlsx"), &bytes).unwrap();
    let mut m = mapping();
    m.source = Some(PnpSource::Xlsx {
        worksheet: String::new(),
        header_row: 3,
    });
    let p = r.service.components_preview_pnp("data.xlsx", &m).unwrap();
    assert_eq!(p.worksheets, vec!["Data"]);
    assert_eq!(
        p.preview.diagnostics[0].code,
        "worksheet_selection_required"
    );
    assert!(!p.preview.valid());
    m = mapping();
    m.refdes = 0;
    m.x = 1;
    let p = r.service.components_preview_pnp("data.xlsx", &m).unwrap();
    assert!(!p.preview.valid());
    let m = p.suggested_mapping.unwrap();
    assert_eq!(m.refdes, 1);
    assert_eq!(m.x, 6);
    assert_eq!(m.side, 4);
    let p = r.service.components_preview_pnp("data.xlsx", &m).unwrap();
    assert!(p.preview.valid());
    assert_eq!(p.preview.row_count, 1);
    assert_eq!(p.preview.components[0].position.x_mm, 9.78266666666667);
    assert_eq!(p.preview.components[0].rotation_deg, 270.);
    assert_eq!(
        p.preview.components[0].footprint.as_deref(),
        Some("封装&名称")
    );
    assert_eq!(p.preview.components[0].value.as_deref(), Some("数值 < 10"));
    let before = r.snapshot();
    let info = r.info();
    import(&mut r, "data.xlsx", &m);
    let board = r.service.board_state(&r.document).unwrap().unwrap();
    assert_eq!(board.mapping, m);
    assert_eq!(
        board.provenance.sha256,
        editor_core::hash::sha256_hex(&bytes)
    );
    assert_eq!(r.snapshot().layers, before.layers);
    assert_eq!(r.info().dirty, info.dirty);
    assert_eq!(r.info().undo_entries, info.undo_entries + 1);
    r.service
        .history_undo(&r.document, &r.info().revision)
        .unwrap();
    assert!(r.service.board_state(&r.document).unwrap().is_none());
    r.service
        .history_redo(&r.document, &r.info().revision)
        .unwrap();
    assert_eq!(r.service.board_state(&r.document).unwrap().unwrap(), board);
    r.service
        .project_save(
            &r.document,
            &r.info().revision,
            Some(r.dir.join("board.rcam").to_str().unwrap()),
            false,
        )
        .unwrap();
    let restored = r.service.project_open("board.rcam").unwrap();
    assert_eq!(
        r.service
            .board_state(&restored.document_id)
            .unwrap()
            .unwrap(),
        board
    );
}
#[test]
fn bad_archive_formula_sheet_and_limits_are_atomic() {
    let mut r = Run::new(1);
    let before = r.info();
    let geometry = r.snapshot();
    for (bytes, m, code, line) in [
        (data(true), mapping(), "formula_unsupported", 5),
        (b"not a ZIP".to_vec(), mapping(), "xlsx_archive_rejected", 0),
    ] {
        std::fs::write(r.dir.join("bad.xlsx"), &bytes).unwrap();
        let p = r.service.components_preview_pnp("bad.xlsx", &m).unwrap();
        assert!(!p.preview.valid());
        assert_eq!(p.preview.diagnostics[0].code, code);
        assert_eq!(p.preview.diagnostics[0].line, line);
        let e = r
            .service
            .components_import_pnp(
                &r.document,
                &r.info().revision,
                ImportPnpParams {
                    path: "bad.xlsx".into(),
                    mapping: m,
                    preview_sha256: p.sha256,
                    allow_replace: false,
                },
            )
            .unwrap_err();
        assert_eq!(e.code, "VALIDATION_FAILED");
        assert_eq!(r.info(), before);
        assert_eq!(r.snapshot().layers, geometry.layers);
        assert!(r.service.board_state(&r.document).unwrap().is_none());
    }
    let shared = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"/>"#;
    for (sheet,code) in [(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1000000"/></sheetData></worksheet>"#.to_owned(),"row_budget"),(r#"<!DOCTYPE worksheet [<!ENTITY x "secret">]><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"/>"#.to_owned(),"xml_doctype"),(r#"<worksheet xmlns="evil"/>"#.to_owned(),"xml_namespace"),(format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="3"><c r="A3" t="inlineStr"><is><t>{}</t></is></c></row></sheetData></worksheet>"#,"x".repeat(MAX_FIELD_BYTES+1)),"field_budget")] {
        std::fs::write(r.dir.join("bad.xlsx"),xlsx(&sheet,shared)).unwrap();let p=r.service.components_preview_pnp("bad.xlsx",&mapping()).unwrap();assert_eq!(p.preview.diagnostics[0].code,code);
    }
    let mut m = mapping();
    m.source = Some(PnpSource::Xlsx {
        worksheet: "Missing".into(),
        header_row: 3,
    });
    std::fs::write(r.dir.join("bad.xlsx"), data(false)).unwrap();
    assert_eq!(
        r.service
            .components_preview_pnp("bad.xlsx", &m)
            .unwrap()
            .preview
            .diagnostics[0]
            .code,
        "worksheet_not_found"
    );
    let mut b = data(false);
    b[70] ^= 1;
    std::fs::write(r.dir.join("bad.xlsx"), b).unwrap();
    assert_eq!(
        r.service
            .components_preview_pnp("bad.xlsx", &mapping())
            .unwrap()
            .preview
            .diagnostics[0]
            .code,
        "xlsx_archive_rejected"
    );
}
#[test]
fn fixed_width_blank_side_declared_unit_physical_line_and_character_boundaries() {
    let mut r = Run::new(1);
    let text = format!(
        "UUNITS = MILLIMETERS\r\n{:<20}{:>13}{:>13}{:>5}{:>4}{}\r\n{:<20}{:>13}{:>13}{:>5}{:>4}{}\r\n",
        "R1", 88.1100, 24.1050, 270, "m", "封装 one", "C1", 51.2450, 28.3366, 90, "", "C0201"
    );
    std::fs::write(r.dir.join("coords.txt"), &text).unwrap();
    let mut m = mapping();
    m.refdes = 0;
    m.x = 1;
    m.y = 2;
    m.rotation = 3;
    m.side = 4;
    m.footprint = Some(5);
    m.value = None;
    m.top_token = String::new();
    m.bottom_token = "m".into();
    m.source = Some(PnpSource::FixedWidth {
        skip_lines: 1,
        columns: vec![
            PnpColumnSpan {
                start: 0,
                end: Some(20),
            },
            PnpColumnSpan {
                start: 20,
                end: Some(33),
            },
            PnpColumnSpan {
                start: 33,
                end: Some(46),
            },
            PnpColumnSpan {
                start: 46,
                end: Some(51),
            },
            PnpColumnSpan {
                start: 51,
                end: Some(55),
            },
            PnpColumnSpan {
                start: 55,
                end: None,
            },
        ],
    });
    let p = r.service.components_preview_pnp("coords.txt", &m).unwrap();
    assert!(p.preview.valid(), "{:?}", p.preview.diagnostics);
    assert_eq!(p.preview.components.len(), 2);
    assert_eq!(
        p.preview.components[0].side,
        editor_core::board::BoardSide::Bottom
    );
    assert_eq!(
        p.preview.components[1].side,
        editor_core::board::BoardSide::Top
    );
    assert_eq!(
        p.preview.components[0].footprint.as_deref(),
        Some("封装 one")
    );
    let mut inches = m.clone();
    inches.unit = PnpUnit::Inch;
    assert_eq!(
        r.service
            .components_preview_pnp("coords.txt", &inches)
            .unwrap()
            .preview
            .diagnostics[0]
            .code,
        "declared_unit_mismatch"
    );
    let bad = text.replace("51.245", "NaN   ");
    std::fs::write(r.dir.join("bad.txt"), bad).unwrap();
    let p = r.service.components_preview_pnp("bad.txt", &m).unwrap();
    assert!(!p.preview.valid());
    assert_eq!(p.preview.diagnostics[0].line, 3);
    assert!(p.preview.components.is_empty());
    let mut cuts = m.clone();
    if let Some(PnpSource::FixedWidth { columns, .. }) = &mut cuts.source {
        columns[5].start = 56;
    }
    assert_eq!(
        r.service
            .components_preview_pnp("coords.txt", &cuts)
            .unwrap()
            .preview
            .diagnostics[0]
            .code,
        "fixed_width_boundary"
    );
    import(&mut r, "coords.txt", &m);
    let board = r.service.board_state(&r.document).unwrap().unwrap();
    let mut legacy = serde_json::to_value(mapping()).unwrap();
    legacy.as_object_mut().unwrap().remove("source");
    let legacy: PnpMapping = serde_json::from_value(legacy).unwrap();
    assert!(legacy.source.is_none());
    assert!(
        serde_json::to_value(legacy)
            .unwrap()
            .get("source")
            .is_none()
    );
    assert_eq!(board.mapping, m);
}
#[test]
fn headerless_five_columns_manual_mapping_samples_and_ignored_extra_values() {
    let mut r = Run::new(1);
    let mut m = mapping();
    m.source = Some(PnpSource::Delimited {
        skip_lines: 1,
        has_header: false,
    });
    m.refdes = 32;
    let bytes = b"comment\n90,Top,R1,1.25,-2\n-90,Bottom,R2,3,4\n";
    std::fs::write(r.dir.join("five.csv"), bytes).unwrap();
    let p = r.service.components_preview_pnp("five.csv", &m).unwrap();
    assert!(!p.preview.valid());
    assert_eq!(p.preview.headers.len(), 5);
    assert_eq!(p.preview.sample_rows[0].line, 2);
    assert_eq!(p.preview.sample_rows[0].fields[2], "R1");
    m.refdes = 2;
    m.x = 3;
    m.y = 4;
    m.rotation = 0;
    m.side = 1;
    m.footprint = None;
    m.value = None;
    let p = r.service.components_preview_pnp("five.csv", &m).unwrap();
    assert!(p.preview.valid());
    assert_eq!(p.preview.row_count, 2); // First data row is not a discarded header.
    assert_eq!(p.preview.components[0].refdes, "R1");
    assert_eq!(p.preview.components[0].position.x_mm, 1.25);
    import(&mut r, "five.csv", &m);
    let board = r.service.board_state(&r.document).unwrap().unwrap();
    assert!(
        board
            .components
            .iter()
            .all(|c| c.footprint.is_none() && c.value.is_none())
    );
    let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>90</v></c><c r="B1" t="inlineStr"><is><t>Top</t></is></c><c r="C1" t="inlineStr"><is><t>R1</t></is></c><c r="D1"><v>1.25</v></c><c r="E1"><v>-2</v></c><c r="F1" t="inlineStr"><is><t>unused private text</t></is></c></row></sheetData></worksheet>"#;
    std::fs::write(
        r.dir.join("no-header.xlsx"),
        xlsx(
            sheet,
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"/>"#,
        ),
    )
    .unwrap();
    m.source = Some(PnpSource::Xlsx {
        worksheet: "Data".into(),
        header_row: 0,
    });
    let p = r
        .service
        .components_preview_pnp("no-header.xlsx", &m)
        .unwrap();
    assert!(p.preview.valid());
    assert_eq!(p.preview.row_count, 1);
    assert_eq!(p.preview.sample_rows[0].line, 1);
    assert_eq!(p.preview.sample_rows[0].fields.len(), 6);
    assert_eq!(p.preview.components[0].footprint, None);
    assert_eq!(p.preview.components[0].value, None);
}
#[test]
#[ignore = "requires explicit private RCAM_PNP_REAL_INPUT_MANIFEST and RCAM_PNP_REAL_OUTPUT"]
fn real_coordinate_files_read_only_service_preview() {
    let manifest =
        std::path::PathBuf::from(std::env::var_os("RCAM_PNP_REAL_INPUT_MANIFEST").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("RCAM_PNP_REAL_OUTPUT").unwrap());
    let cases: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let mut r = Run::new(1);
    let info = r.info();
    let geometry = r.snapshot();
    let mut reports = vec![];
    for case in cases {
        let path = std::path::Path::new(case["path"].as_str().unwrap());
        let bytes = std::fs::read(path).unwrap();
        let hash = editor_core::hash::sha256_hex(&bytes);
        assert_eq!(hash, case["sha256"].as_str().unwrap());
        r.service.grant_file_access(path, false).unwrap();
        let m: PnpMapping = serde_json::from_value(case["draft_mapping"].clone()).unwrap();
        let p = r
            .service
            .components_preview_pnp(path.to_str().unwrap(), &m)
            .unwrap();
        assert!(
            p.preview.valid(),
            "file {}: {:?}",
            case["id"],
            p.preview.diagnostics
        );
        assert_eq!(p.sha256, hash);
        assert_eq!(
            p.preview.row_count,
            case["expected_rows"].as_u64().unwrap() as usize
        );
        assert_eq!(
            editor_core::hash::sha256_hex(&std::fs::read(path).unwrap()),
            hash
        );
        assert_eq!(r.info(), info);
        assert_eq!(r.snapshot().layers, geometry.layers);
        assert!(r.service.board_state(&r.document).unwrap().is_none());
        println!(
            "file {}: {} rows read-only preview PASS",
            case["id"], p.preview.row_count
        );
        reports.push(json!({"id":case["id"],"sha256":hash,"original_unchanged":true,"read_only":true,"draft_conventions_unconfirmed":true,"result":p}));
    }
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schema_version":2,"status":"PARSE_PASS / CONVENTIONS_PENDING","read_only":true,"files":reports})).unwrap()).unwrap();
}
