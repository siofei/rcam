//! Real system fonts, not mock shapes. Writes each result, including rejections.
use editor_core::{RegionEdge, SemanticGeometry};
use editor_service::*;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
#[test]
#[ignore = "release evidence matrix; needs authorized local macOS fonts and a new evidence directory"]
fn geometry_matrix() {
    let out = PathBuf::from(std::env::var("RCAM_S4A21_MATRIX").expect("new output directory"));
    std::fs::create_dir_all(&out).unwrap();
    let mut rows = vec![];
    for (fi, font_path) in [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
    ]
    .iter()
    .enumerate()
    {
        for (ti, text) in [
            "A",
            "O",
            "8",
            "W1234567 8B",
            "中",
            "口",
            "回",
            "田",
            "钢网测试口回",
            "中文ABC123",
        ]
        .iter()
        .enumerate()
        {
            let mut service = ApplicationService::new();
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/s1a/ordered_local_hole.gbr");
            service.grant_file_access(&source, false).unwrap();
            service
                .grant_file_access(Path::new(font_path), false)
                .unwrap();
            service.grant_file_access(&out, true).unwrap();
            let d = service.open(source.to_str().unwrap()).unwrap();
            let font = service.font_inspect(font_path, 0).unwrap();
            let params = TextParams {
                layer_id: d.layer_ids[0].clone(),
                font: font.identity.clone(),
                layout: TextLayout {
                    text: (*text).into(),
                    height_mm: 3.,
                    x_mm: 12.34567,
                    y_mm: -6.78901,
                    tracking_mm: 0.,
                    rotation_deg: 37.,
                    curve_tolerance_mm: 0.00025,
                    outline_offset_mm: 0.,
                    h_align: HorizontalAlign::Left,
                    v_align: VerticalAlign::Bottom,
                },
            };
            let start = Instant::now();
            let preview = service.text_preview(&d.document_id, &d.revision, params.clone());
            let preview_ms = start.elapsed().as_secs_f64() * 1000.;
            let mut row = json!({"font":Path::new(font_path).file_name().unwrap().to_string_lossy(),"font_sha256":font.identity.sha256,"face_index":0,"text":text,"glyph_count":text.chars().filter(|c|!c.is_whitespace()).count(),"preview_ms":preview_ms});
            match preview {
                Err(e) => {
                    row["status"] = json!("FAIL");
                    row["error"] = json!(e);
                }
                Ok(p) => {
                    let mut contours = 0;
                    let mut edges = 0;
                    let mut arcs = 0;
                    for g in &p.geometries {
                        if let SemanticGeometry::Region { contours: cs } = g {
                            contours += cs.len();
                            for c in cs {
                                edges += c.edges.len();
                                arcs += c
                                    .edges
                                    .iter()
                                    .filter(|e| matches!(e, RegionEdge::Arc(_)))
                                    .count();
                            }
                        }
                    }
                    row["objects"] = json!(p.geometries.len());
                    row["contours"] = json!(contours);
                    row["edges"] = json!(edges);
                    row["arcs"] = json!(arcs);
                    let created = service
                        .text_create(&d.document_id, &d.revision, params.clone())
                        .unwrap();
                    let path = out.join(format!("{fi}-{ti}.gbr"));
                    let start = Instant::now();
                    let result = service.export_layer(
                        &d.document_id,
                        &created.revision,
                        ExportParams {
                            layer_id: params.layer_id,
                            path: path.to_string_lossy().into(),
                            overwrite: OverwritePolicy {
                                mode: "deny".into(),
                                expected_sha256: None,
                            },
                            metadata_policy: MetadataPolicy {
                                mode: "require_confirmation".into(),
                                categories: None,
                            },
                        },
                    );
                    row["writer_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
                    match result {
                        Err(e) => {
                            row["status"] = json!("FAIL");
                            row["error"] = json!(e);
                        }
                        Ok(_) => {
                            let bytes = std::fs::read(&path).unwrap();
                            row["gerber_bytes"] = json!(bytes.len());
                            row["writer_commands"] =
                                json!(bytes.iter().filter(|b| **b == b'*').count());
                            let start = Instant::now();
                            let reopened = gerber_io::parse_s1(&bytes, "matrix-reopened");
                            row["reopen_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
                            row["status"] = json!(if reopened.is_ok() { "PASS" } else { "FAIL" });
                        }
                    }
                }
            }
            rows.push(row);
            std::fs::write(
                out.join("matrix.json"),
                serde_json::to_vec_pretty(
                    &json!({"schema_version":2,"platform":"macOS arm64","rows":rows}),
                )
                .unwrap(),
            )
            .unwrap();
        }
    }
    assert!(
        rows.iter().all(|r| r["status"] == "PASS"),
        "see matrix.json; rejections do not count as success"
    );
}
