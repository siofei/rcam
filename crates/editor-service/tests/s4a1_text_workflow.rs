//! Real macOS CJK font identity is explicit; no font bytes are distributed.
use editor_core::{Exposure, MmPoint, ObjectOrigin, RegionEdge, SemanticGeometry};
use editor_service::*;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nX-1000000Y1500000D02*\nX10000000Y1500000D01*\nM02*\n";
struct Run {
    service: ApplicationService,
    dir: PathBuf,
    id: String,
    layer: String,
    font: FontIdentity,
}
impl Run {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-s4a1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.gbr"), SOURCE).unwrap();
        let path = std::env::var("RCAM_TEXT_FONT")
            .unwrap_or_else(|_| "/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into());
        let sha256 = std::env::var("RCAM_TEXT_FONT_SHA256").unwrap_or_else(|_| {
            "876af2cd4854644e7f3e7feb2f688997fdb3343c6df6693611209c9dfb47ccec".into()
        });
        let font = FontIdentity {
            path: path.clone(),
            sha256,
            face_index: 0,
            license_status: "local OS font; user-selected outline use; not redistributed".into(),
            redistribution_allowed: false,
        };
        let policy = FileAccessPolicy::new(
            &dir,
            [dir.clone(), PathBuf::from(&path).parent().unwrap().into()],
            [dir.clone()],
        );
        let mut service = ApplicationService::with_file_access(policy);
        let opened = service.open("source.gbr").unwrap();
        Self {
            service,
            dir,
            id: opened.document_id,
            layer: opened.layer_ids[0].clone(),
            font,
        }
    }
    fn params(&self, text: &str) -> TextParams {
        TextParams {
            layer_id: self.layer.clone(),
            font: self.font.clone(),
            layout: TextLayout {
                text: text.into(),
                x_mm: 0.,
                y_mm: 0.,
                height_mm: 3.,
                tracking_mm: 0.,
                h_align: HorizontalAlign::Left,
                v_align: VerticalAlign::Bottom,
                rotation_deg: 0.,
                curve_tolerance_mm: editor_text::TOLERANCE_MM,
                baseline_spacing_mm: 0.,
                stroke_width_mm: 0.15,
                outline_offset_mm: 0.,
            },
        }
    }
    fn objects(&self) -> Vec<editor_core::SemanticObject> {
        self.service.render_snapshot(&self.id).unwrap().layers[0]
            .objects
            .clone()
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn text_transaction_undo_redo_export_reopen_and_metrics() {
    for (n, (text, angle, align)) in [
        ("W1234567 8B", 0., HorizontalAlign::Left),
        ("钢网测试口回", 0., HorizontalAlign::Center),
        ("O田中回", 90., HorizontalAlign::Right),
        ("中文ABC123", 37., HorizontalAlign::Left),
    ]
    .into_iter()
    .enumerate()
    {
        let mut r = Run::new();
        let original = r.objects();
        let mut p = r.params(text);
        p.layout.rotation_deg = angle;
        p.layout.h_align = align;
        let result = r
            .service
            .text_create(&r.id, "0", p)
            .unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(result.revision, "1");
        assert_eq!(result.undo_entries_added, 1);
        assert_eq!(r.service.document_get(&r.id).unwrap().undo_entries, 1);
        let expected = r.objects();
        assert_eq!(&expected[..1], original.as_slice());
        assert_eq!(expected.len(), result.generated_object_ids.len() + 1);
        let mut op = None;
        for o in &expected[1..] {
            assert_eq!(o.exposure, Exposure::Dark);
            let ObjectOrigin::Generated { operation_id } = &o.origin else {
                panic!()
            };
            if let Some(previous) = op {
                assert_eq!(previous, operation_id);
            }
            op = Some(operation_id);
        }
        let metrics = r
            .service
            .objects_metrics(
                &r.id,
                MetricsParams {
                    layer_id: r.layer.clone(),
                    object_ids: result.generated_object_ids.clone(),
                },
            )
            .unwrap();
        assert_eq!(metrics.summary.unsupported_count, 0, "{metrics:?}");
        assert!(metrics.summary.object_area_sum_mm2 > 0.);
        r.service.history_undo(&r.id, "1").unwrap();
        assert_eq!(r.objects(), original);
        r.service.history_redo(&r.id, "2").unwrap();
        assert_eq!(r.objects(), expected);
        assert!(r.service.validate(&r.id).unwrap().valid);
        let output = r.dir.join("text.gbr");
        r.service
            .export_layer(
                &r.id,
                "3",
                ExportParams {
                    layer_id: r.layer.clone(),
                    path: output.to_string_lossy().into_owned(),
                    overwrite: OverwritePolicy {
                        mode: "deny".into(),
                        expected_sha256: None,
                    },
                    metadata_policy: MetadataPolicy {
                        mode: "require_confirmation".into(),
                        categories: None,
                    },
                },
            )
            .unwrap_or_else(|e| panic!("{text} export: {e:?}"));
        let bytes = std::fs::read(&output).unwrap();
        let reopened = gerber_io::parse_s1(&bytes, "reopened").unwrap();
        let actual = &reopened.document.layers[0].objects;
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(&expected).skip(1) {
            assert_eq!(a.exposure, e.exposure);
            let (
                SemanticGeometry::Region { contours: a },
                SemanticGeometry::Region { contours: e },
            ) = (&a.geometry, &e.geometry)
            else {
                panic!()
            };
            assert_eq!(a.len(), e.len());
            assert_eq!(
                a.iter().map(|c| c.edges.len()).sum::<usize>(),
                e.iter().map(|c| c.edges.len()).sum::<usize>()
            );
            for (a, e) in a
                .iter()
                .flat_map(|c| &c.edges)
                .zip(e.iter().flat_map(|c| &c.edges))
            {
                match (a, e) {
                    (
                        RegionEdge::Line { start: a, end: b },
                        RegionEdge::Line { start: c, end: d },
                    ) => assert!(
                        a.distance_mm(output_grid(*c)) < 1e-6
                            && b.distance_mm(output_grid(*d)) < 1e-6
                    ),
                    (RegionEdge::Arc(a), RegionEdge::Arc(b)) => {
                        assert!(
                            a.start.distance_mm(output_grid(b.start)) < 1e-6
                                && a.end.distance_mm(output_grid(b.end)) < 1e-6
                        );
                        assert!(a.center.distance_mm(output_grid(b.center)) < 1e-6);
                        assert_eq!(a.direction, b.direction);
                        assert_eq!(a.full_circle, b.full_circle);
                    }
                    _ => panic!("manufacturing edge kind changed on reopen"),
                }
            }
        }
        assert_eq!(
            std::fs::read(r.dir.join("source.gbr")).unwrap(),
            SOURCE.as_bytes()
        );
        if let Ok(out) = std::env::var("RCAM_TEXT_EVIDENCE") {
            let out = PathBuf::from(out);
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(out.join(format!("text-{n}.gbr")), bytes).unwrap();
            std::fs::write(out.join("input.gbr"), SOURCE).unwrap();
            std::fs::write(out.join(format!("text-{n}.json")),serde_json::to_vec_pretty(&json!({"text":text,"angle":angle,"font_sha256":r.font.sha256,"font_name":PathBuf::from(&r.font.path).file_name().unwrap().to_string_lossy(),"face_index":0,"license_status":r.font.license_status,"redistribution_allowed":false,"objects":expected.len(),"metrics":metrics.summary,"geometry_coordinate_error_bound_mm":0.000001,"result":"PASS"})).unwrap()).unwrap();
        }
    }
}
#[test]
fn hole_keeps_background_and_empty_hole_after_reopen() {
    let mut r = Run::new();
    let p = r.params("口");
    r.service.text_create(&r.id, "0", p).unwrap();
    // Compare material at every scanline sample against the original line. A local
    // hole contains the original line but is empty immediately above/below it.
    let objects = r.objects();
    let mut document = gerber_io::parse_s1(SOURCE.as_bytes(), "test")
        .unwrap()
        .document;
    document.layers[0].objects = objects.clone();
    let output = r.dir.join("hole.gbr");
    r.service
        .export_layer(
            &r.id,
            "1",
            ExportParams {
                layer_id: r.layer.clone(),
                path: output.to_string_lossy().into_owned(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
            },
        )
        .unwrap();
    let reopened = gerber_io::parse_s1(&std::fs::read(output).unwrap(), "hole-reopened")
        .unwrap()
        .document;
    let layer = document.layers[0].id.clone();
    let mut hole_found = false;
    for i in 1..300 {
        let x = i as f64 / 100.;
        let center = MmPoint::new(x, 1.5);
        assert_eq!(document.layer_coverage_at(&layer, center), Some(true));
        assert_eq!(
            reopened.layer_coverage_at(&reopened.layers[0].id, center),
            Some(true)
        );
        let mut text_only = document.clone();
        text_only.layers[0].objects.remove(0);
        if text_only.layer_coverage_at(&layer, center) == Some(false) && i > 50 && i < 200 {
            hole_found = true;
            assert_eq!(
                reopened.layer_coverage_at(&reopened.layers[0].id, MmPoint::new(x, 1.6)),
                Some(false)
            );
            assert_eq!(
                document.layer_coverage_at(&layer, MmPoint::new(x, 1.6)),
                Some(false)
            );
        }
    }
    assert!(hole_found);
}
#[test]
fn all_rejections_leave_document_history_and_revision_unchanged() {
    let mut r = Run::new();
    let original = r.objects();
    let before = r.service.document_get(&r.id).unwrap();
    let mut failures = vec![];
    for text in ["", " ", "A\tB", "A\u{9fff}"] {
        failures.push(r.params(text));
    }
    let mut p = r.params("A");
    p.font.sha256 = "0".repeat(64);
    failures.push(p);
    let mut p = r.params("A");
    p.font.face_index = u32::MAX;
    failures.push(p);
    let mut p = r.params("A");
    p.font.path = r.dir.join("source.gbr").to_string_lossy().into_owned();
    failures.push(p);
    for field in 0..5 {
        let mut p = r.params("A");
        match field {
            0 => p.layout.height_mm = f64::NAN,
            1 => p.layout.x_mm = f64::INFINITY,
            2 => p.layout.y_mm = f64::NAN,
            3 => p.layout.tracking_mm = f64::NAN,
            _ => p.layout.rotation_deg = f64::NAN,
        };
        failures.push(p);
    }
    for p in failures {
        assert!(r.service.text_create(&r.id, "0", p).is_err());
        assert_eq!(r.objects(), original);
        assert_eq!(r.service.document_get(&r.id).unwrap(), before);
    }
    let p = r.params("A");
    assert_eq!(
        r.service.text_create(&r.id, "9", p).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    r.service
        .layer_update(
            &r.id,
            "0",
            LayerUpdateParams {
                layer_id: r.layer.clone(),
                expected_workspace_revision: "0".into(),
                display_name: None,
                visible: None,
                locked: Some(true),
            },
        )
        .unwrap();
    let p = r.params("A");
    assert!(r.service.text_create(&r.id, "0", p).is_err());
    assert_eq!(r.objects(), original);
    assert_eq!(r.service.document_get(&r.id).unwrap().revision, "0");
}
#[test]
fn json_contract_advertises_and_dispatches_text() {
    let mut r = Run::new();
    assert!(
        r.service
            .capabilities()
            .supported_operations
            .iter()
            .any(|op| op == "text.create")
    );
    let reply=r.service.execute_json(&json!({"api_version":1,"request_id":"text","op":"text.create","document_id":r.id,"expected_revision":"0","params":r.params("A")}).to_string());
    assert!(reply["status"] == "completed", "{reply}");
    assert_eq!(r.service.document_get(&r.id).unwrap().revision, "1");
}

#[test]
fn multiline_stroke_and_outline_share_atomic_export_workflow() {
    for stroke in [true, false] {
        let mut r = Run::new();
        let before = r.objects();
        let mut params = r.params(if stroke { "abcABC\n0123" } else { "中文\nAB" });
        params.layout.baseline_spacing_mm = 4.5;
        if stroke {
            params.font = builtin_stroke_font().identity;
        }
        let preview = r.service.text_preview(&r.id, "0", params.clone()).unwrap();
        assert_eq!(r.objects(), before);
        let made = r.service.text_create(&r.id, "0", params.clone()).unwrap();
        assert_eq!(made.undo_entries_added, 1);
        let expected = r.objects();
        assert_eq!(
            expected[1..]
                .iter()
                .map(|o| o.geometry.clone())
                .collect::<Vec<_>>(),
            preview.geometries
        );
        r.service.history_undo(&r.id, "1").unwrap();
        assert_eq!(r.objects(), before);
        r.service.history_redo(&r.id, "2").unwrap();
        assert_eq!(r.objects(), expected);
        let output = r.dir.join("multiline.gbr");
        r.service
            .export_layer(
                &r.id,
                "3",
                ExportParams {
                    layer_id: r.layer.clone(),
                    path: output.to_string_lossy().into(),
                    overwrite: OverwritePolicy {
                        mode: "deny".into(),
                        expected_sha256: None,
                    },
                    metadata_policy: MetadataPolicy {
                        mode: "require_confirmation".into(),
                        categories: None,
                    },
                },
            )
            .unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let parsed = gerber_io::parse_s1(&bytes, "multiline-reopen").unwrap();
        assert_eq!(parsed.document.layers[0].objects.len(), expected.len());
        if stroke {
            for (actual, expected) in parsed.document.layers[0]
                .objects
                .iter()
                .zip(&expected)
                .skip(1)
            {
                let (
                    SemanticGeometry::Line {
                        start: a,
                        end: b,
                        width_mm: w,
                    },
                    SemanticGeometry::Line {
                        start: c,
                        end: d,
                        width_mm: v,
                    },
                ) = (&actual.geometry, &expected.geometry)
                else {
                    panic!()
                };
                assert!(
                    a.distance_mm(output_grid(*c)) < 1e-6
                        && b.distance_mm(output_grid(*d)) < 1e-6
                        && (w - v).abs() < 1e-6
                );
            }
            params.layout.text = "中文".into();
            assert!(r.service.text_create(&r.id, "3", params.clone()).is_err());
            assert_eq!(r.objects(), expected);
            params.layout.text = "ABC".into();
            params.font.sha256 = "0".repeat(64);
            assert!(r.service.text_create(&r.id, "3", params).is_err());
            assert_eq!(r.service.document_get(&r.id).unwrap().revision, "3");
        }
    }
}

// Independent default manufacturing-grid oracle; encoding comparison stays 1 nm.
fn output_grid(p: editor_core::MmPoint) -> editor_core::MmPoint {
    editor_core::MmPoint::new(
        (p.x_mm * 10000.).round() / 10000.,
        (p.y_mm * 10000.).round() / 10000.,
    )
}
