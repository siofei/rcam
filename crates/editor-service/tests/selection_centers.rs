use editor_service::*;
use serde_json::json;
fn setup() -> (ApplicationService, DocumentInfo, SelectionCentersParams) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s5i2/overlap_rectangles.gbr");
    let mut s = ApplicationService::new();
    s.grant_file_access(&path, false).unwrap();
    let d = s.open(path.to_str().unwrap()).unwrap();
    let snapshot = s.render_snapshot(&d.document_id).unwrap();
    let params = SelectionCentersParams {
        groups: snapshot
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect(),
        semantics: SelectionMaterialSemantics::SelectedLayerComposite,
    };
    (s, d, params)
}
fn center(result: &SelectionCentersResult) -> editor_core::MmPoint {
    let SelectionMaterialResult::Computed {
        value:
            CompositeMaterial::Ready {
                centroid_mm,
                area_mm2,
                perimeter_mm,
                ..
            },
    } = &result.material
    else {
        panic!("{result:?}")
    };
    assert!((area_mm2 - 5.).abs() < 1e-10);
    assert!((perimeter_mm - 10.).abs() < 1e-10);
    *centroid_mm
}
#[test]
fn readonly_composite_real_service_cache_edit_undo_and_json() {
    let (mut s, d, p) = setup();
    let id = &d.document_id;
    let before = s.document_get(id).unwrap();
    let snap = s.render_snapshot(id).unwrap();
    let a = s
        .geometry_selection_centers(id, &d.revision, p.clone())
        .unwrap();
    assert!(!a.cache_hit);
    assert_eq!(center(&a), editor_core::MmPoint::new(1.3, 0.9));
    let b = s
        .geometry_selection_centers(id, &d.revision, p.clone())
        .unwrap();
    assert!(b.cache_hit);
    assert_eq!(center(&a), center(&b));
    assert_eq!(s.document_get(id).unwrap(), before);
    assert_eq!(s.render_snapshot(id).unwrap(), snap);
    let encoded = serde_json::to_string(&p).unwrap();
    assert_eq!(
        serde_json::from_str::<SelectionCentersParams>(&encoded).unwrap(),
        p
    );
    let envelope = json!({"api_version":1,"request_id":"selection-test","op":"geometry.selection_centers","document_id":id,"expected_revision":d.revision,"params":p});
    let reply = s.execute_json(&envelope.to_string());
    assert_eq!(reply["status"], "completed", "{reply}");
    assert!(reply["result"]["cache_hit"].as_bool().unwrap());
    s.objects_edit_selection(
        id,
        &d.revision,
        EditSelectionParams {
            groups: p.groups.clone(),
            operation: SelectionEdit::Move {
                dx_mm: 7.,
                dy_mm: -2.,
            },
        },
    )
    .unwrap();
    let revision = s.document_get(id).unwrap().revision;
    assert_eq!(
        s.geometry_selection_centers(id, &d.revision, p.clone())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let moved = s
        .geometry_selection_centers(id, &revision, p.clone())
        .unwrap();
    assert!(!moved.cache_hit);
    assert!((center(&moved).x_mm - 8.3).abs() < 1e-10);
    assert!((center(&moved).y_mm + 1.1).abs() < 1e-10);
    s.history_undo(id, &revision).unwrap();
    let revision = s.document_get(id).unwrap().revision;
    let restored = s.geometry_selection_centers(id, &revision, p).unwrap();
    assert!(!restored.cache_hit);
    assert_eq!(center(&restored), center(&a));
}
#[test]
fn query_cancel_and_invalid_inputs_have_zero_manufacturing_effect() {
    let (mut s, d, p) = setup();
    let before = s.document_get(&d.document_id).unwrap();
    assert_eq!(
        s.geometry_selection_centers_cancellable(&d.document_id, &d.revision, p.clone(), || true)
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    let mut polls = 0;
    assert_eq!(
        s.geometry_selection_centers_cancellable(&d.document_id, &d.revision, p.clone(), || {
            polls += 1;
            polls > 30
        })
        .unwrap_err()
        .code,
        "CANCELLED"
    );
    let mut duplicate = p.clone();
    let first = duplicate.groups[0].object_ids[0].clone();
    duplicate.groups[0].object_ids.push(first);
    assert_eq!(
        s.geometry_selection_centers(&d.document_id, &d.revision, duplicate)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    let missing = json!({"api_version":1,"request_id":"bad","op":"geometry.selection_centers","document_id":d.document_id,"expected_revision":null,"params":p});
    assert_eq!(
        s.execute_json(&missing.to_string())["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let mut extra = missing;
    extra["expected_revision"] = json!(d.revision);
    extra["params"]["semantics"] = json!("object_sum");
    assert_eq!(
        s.execute_json(&extra.to_string())["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(s.document_get(&d.document_id).unwrap(), before);
}
#[test]
fn precision_selection_and_document_lifecycle_are_cache_bound() {
    let (mut s, d, p) = setup();
    let mut one = p.clone();
    one.groups[0].object_ids.truncate(1);
    let a = s
        .geometry_selection_centers(&d.document_id, &d.revision, one)
        .unwrap();
    assert!(!a.cache_hit);
    assert!(matches!(
        a.material,
        SelectionMaterialResult::Computed {
            value: CompositeMaterial::Ready { area_mm2: 4., .. }
        }
    ));
    let a = s
        .geometry_selection_centers(&d.document_id, &d.revision, p.clone())
        .unwrap();
    assert!(!a.cache_hit);
    s.set_manufacturing_precision(
        &d.document_id,
        &d.revision,
        ManufacturingPrecision {
            resolution_mm: 1e-6,
        },
    )
    .unwrap();
    let current = s.document_get(&d.document_id).unwrap();
    let a = s
        .geometry_selection_centers(&d.document_id, &current.revision, p.clone())
        .unwrap();
    assert!(!a.cache_hit);
    assert_eq!(a.resolution_mm, 1e-6);
    s.close(&d.document_id, &current.revision, true).unwrap();
    assert_eq!(
        s.geometry_selection_centers(&d.document_id, &current.revision, p)
            .unwrap_err()
            .code,
        "NOT_FOUND"
    );
}
#[test]
#[cfg(target_os = "macos")]
fn local_cjk_and_english_text_material_matches_independent_polygon_moments() {
    let (mut s, d, _) = setup();
    let font_path = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
    s.grant_file_access(std::path::Path::new(font_path), false)
        .unwrap();
    let font = s.font_inspect(font_path, 0).unwrap().identity;
    let text = s
        .text_create(
            &d.document_id,
            &d.revision,
            TextParams {
                layer_id: d.layer_ids[0].clone(),
                font,
                layout: TextLayout {
                    text: "口回8B".into(),
                    x_mm: 10.,
                    y_mm: 3.,
                    height_mm: 3.,
                    tracking_mm: 0.2,
                    h_align: HorizontalAlign::Left,
                    v_align: VerticalAlign::Bottom,
                    rotation_deg: 0.,
                    curve_tolerance_mm: editor_text::TOLERANCE_MM,
                    baseline_spacing_mm: 0.,
                    stroke_width_mm: 0.15,
                    outline_offset_mm: 0.,
                },
            },
        )
        .unwrap();
    let snapshot = s.render_snapshot(&d.document_id).unwrap();
    let p = SelectionCentersParams {
        groups: vec![SelectionGroup {
            layer_id: d.layer_ids[0].clone(),
            object_ids: text.generated_object_ids.clone(),
        }],
        semantics: SelectionMaterialSemantics::SelectedLayerComposite,
    };
    let before = s.document_get(&d.document_id).unwrap();
    let q = s
        .geometry_selection_centers(&d.document_id, &text.revision, p)
        .unwrap();
    let SelectionMaterialResult::Computed {
        value:
            CompositeMaterial::Ready {
                area_mm2,
                centroid_mm,
                centroid_error_mm,
                area_error_mm2,
                ..
            },
    } = q.material
    else {
        panic!("{q:?}")
    };
    let (mut area, mut mx, mut my) = (0., 0., 0.);
    for object in snapshot.layers[0]
        .objects
        .iter()
        .filter(|o| text.generated_object_ids.contains(&o.object_id))
    {
        let editor_core::SemanticGeometry::Region { contours } = &object.geometry else {
            panic!("outline text")
        };
        for c in contours {
            let (mut a, mut x, mut y) = (0., 0., 0.);
            for e in &c.edges {
                match e {
                    editor_core::RegionEdge::Line { start, end } => {
                        let dx = end.x_mm - start.x_mm;
                        let dy = end.y_mm - start.y_mm;
                        for t in [(1. - 1. / 3_f64.sqrt()) / 2., (1. + 1. / 3_f64.sqrt()) / 2.] {
                            let px = start.x_mm + dx * t;
                            let py = start.y_mm + dy * t;
                            a += (px * dy - py * dx) / 4.;
                            x += px * px * dy / 4.;
                            y -= py * py * dx / 4.;
                        }
                    }
                    editor_core::RegionEdge::Arc(arc) => {
                        // Independent numerical Green quadrature, rather than
                        // copying the production trigonometric antiderivatives.
                        // The frozen Region manufacturing circle passes both
                        // declared endpoints. Reconstruct it independently on
                        // the chord bisector, not the original fuzzy source arc.
                        let mut circle = *arc;
                        if !arc.full_circle {
                            let dx = arc.end.x_mm - arc.start.x_mm;
                            let dy = arc.end.y_mm - arc.start.y_mm;
                            let r0 = (arc.start.x_mm - arc.center.x_mm).powi(2)
                                + (arc.start.y_mm - arc.center.y_mm).powi(2);
                            let r1 = (arc.end.x_mm - arc.center.x_mm).powi(2)
                                + (arc.end.y_mm - arc.center.y_mm).powi(2);
                            let k = (r1 - r0) / (2. * (dx * dx + dy * dy));
                            circle.center.x_mm += k * dx;
                            circle.center.y_mm += k * dy;
                        }
                        let arc = &circle;
                        let sweep = arc.sweep_radians().unwrap()
                            * if arc.direction == editor_core::ArcDirection::CounterClockwise {
                                1.
                            } else {
                                -1.
                            };
                        let theta = (arc.start.y_mm - arc.center.y_mm)
                            .atan2(arc.start.x_mm - arc.center.x_mm);
                        let r = arc.radius();
                        let n = 8192usize;
                        let mut sums = [0.; 3];
                        for i in 0..=n {
                            let t = theta + sweep * i as f64 / n as f64;
                            let px = arc.center.x_mm + r * t.cos();
                            let py = arc.center.y_mm + r * t.sin();
                            let dx = -r * t.sin() * sweep;
                            let dy = r * t.cos() * sweep;
                            let weight = if i == 0 || i == n {
                                1.
                            } else if i % 2 == 0 {
                                2.
                            } else {
                                4.
                            };
                            for (sum, v) in sums.iter_mut().zip([
                                (px * dy - py * dx) / 2.,
                                px * px * dy / 2.,
                                -py * py * dx / 2.,
                            ]) {
                                *sum += weight * v / (3. * n as f64);
                            }
                        }
                        a += sums[0];
                        x += sums[1];
                        y += sums[2];
                    }
                }
            }
            area += a.abs();
            mx += x * a.signum();
            my += y * a.signum();
        }
    }
    eprintln!(
        "text area production={area_mm2} raw-integral={area} error={area_error_mm2} center={centroid_mm:?} independent=({}, {}) center_error={centroid_error_mm}",
        mx / area,
        my / area
    );
    assert!((area_mm2 - area).abs() <= area_error_mm2);
    assert!((centroid_mm.x_mm - mx / area).abs() <= centroid_error_mm);
    assert!((centroid_mm.y_mm - my / area).abs() <= centroid_error_mm);
    assert_eq!(s.document_get(&d.document_id).unwrap(), before);
}
#[test]
#[ignore = "explicit release cache measurement gate"]
fn release_thirty_hot_service_cache_queries() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s2b3_2/P10K_CIRCLES.gbr");
    let mut s = ApplicationService::new();
    s.grant_file_access(&path, false).unwrap();
    let d = s.open(path.to_str().unwrap()).unwrap();
    let snapshot = s.render_snapshot(&d.document_id).unwrap();
    let params = SelectionCentersParams {
        groups: snapshot
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect(),
        semantics: SelectionMaterialSemantics::SelectedLayerComposite,
    };
    let start = std::time::Instant::now();
    let cold = s
        .geometry_selection_centers(&d.document_id, &d.revision, params.clone())
        .unwrap();
    println!(
        "{{\"service_cold_ms\":{}}}",
        start.elapsed().as_secs_f64() * 1000.
    );
    assert!(
        matches!(
            cold.material,
            SelectionMaterialResult::Computed {
                value: CompositeMaterial::Ready { .. }
            }
        ),
        "{cold:?}"
    );
    let mut times = Vec::new();
    for i in 0..30 {
        let p = params.clone();
        let start = std::time::Instant::now();
        let q = s
            .geometry_selection_centers(&d.document_id, &d.revision, p)
            .unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.;
        assert!(q.cache_hit);
        assert_eq!(q.material, cold.material);
        times.push(ms);
        println!("{{\"sample\":{i},\"service_cache_ms\":{ms}}}");
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"service_cache_p95_ms\":{},\"service_cache_max_ms\":{}}}",
        times[28], times[29]
    );
    assert!(times[29] <= 10.);
}
#[test]
fn composite_query_edit_export_reopen_has_independent_same_material() {
    let (mut s, d, p) = setup();
    let root = std::env::temp_dir().join(format!(
        "rcam-i2-headless-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    s.grant_file_access(&root, true).unwrap();
    let before = s
        .geometry_selection_centers(&d.document_id, &d.revision, p.clone())
        .unwrap();
    s.objects_edit_selection(
        &d.document_id,
        &d.revision,
        EditSelectionParams {
            groups: p.groups.clone(),
            operation: SelectionEdit::Move {
                dx_mm: 7.,
                dy_mm: -2.,
            },
        },
    )
    .unwrap();
    let rev = s.document_get(&d.document_id).unwrap().revision;
    let changed = s
        .geometry_selection_centers(&d.document_id, &rev, p.clone())
        .unwrap();
    assert!((center(&changed).x_mm - 8.3).abs() < 1e-10);
    let output = root.join("moved.gbr");
    let export = s
        .export_layer(
            &d.document_id,
            &rev,
            ExportParams {
                layer_id: d.layer_ids[0].clone(),
                path: output.to_str().unwrap().into(),
                overwrite: OverwritePolicy {
                    mode: "deny".into(),
                    expected_sha256: None,
                },
                metadata_policy: MetadataPolicy {
                    mode: "require_confirmation".into(),
                    categories: None,
                },
                compatibility_precision_override_mm: None,
            },
        )
        .unwrap();
    assert_eq!(export.exported_revision, rev);
    s.grant_file_access(&output, false).unwrap();
    let reopened = s.open(output.to_str().unwrap()).unwrap();
    let snapshot = s.render_snapshot(&reopened.document_id).unwrap();
    let query = SelectionCentersParams {
        groups: snapshot
            .layers
            .iter()
            .map(|l| SelectionGroup {
                layer_id: l.id.clone(),
                object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
            })
            .collect(),
        semantics: SelectionMaterialSemantics::SelectedLayerComposite,
    };
    let reimport = s
        .geometry_selection_centers(&reopened.document_id, &reopened.revision, query)
        .unwrap();
    assert_eq!(center(&reimport), center(&changed));
    s.history_undo(&d.document_id, &rev).unwrap();
    let revision = s.document_get(&d.document_id).unwrap().revision;
    let restored = s
        .geometry_selection_centers(&d.document_id, &revision, p)
        .unwrap();
    assert_eq!(center(&restored), center(&before));
    // Preserve this run's public generated files; no directory deletion.
    println!("headless_evidence={}", root.display());
}

#[test]
fn canonical_target_order_cache_capacity_and_cancelled_hits_are_bound() {
    let (mut s, d, p) = setup();
    let first = s
        .geometry_selection_centers(&d.document_id, &d.revision, p.clone())
        .unwrap();
    assert!(!first.cache_hit);
    let mut reversed = p.clone();
    reversed.groups[0].object_ids.reverse();
    assert!(
        s.geometry_selection_centers(&d.document_id, &d.revision, reversed)
            .unwrap()
            .cache_hit
    );
    let before = s.document_get(&d.document_id).unwrap();
    assert_eq!(
        s.geometry_selection_centers_cancellable(&d.document_id, &d.revision, p.clone(), || true)
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    assert_eq!(s.document_get(&d.document_id).unwrap(), before);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/s5i2/overlap_rectangles.gbr");
    for _ in 0..64 {
        let other = s.open(path.to_str().unwrap()).unwrap();
        let snapshot = s.render_snapshot(&other.document_id).unwrap();
        let params = SelectionCentersParams {
            groups: snapshot
                .layers
                .iter()
                .map(|l| SelectionGroup {
                    layer_id: l.id.clone(),
                    object_ids: l.objects.iter().map(|o| o.object_id.clone()).collect(),
                })
                .collect(),
            semantics: SelectionMaterialSemantics::SelectedLayerComposite,
        };
        assert!(
            !s.geometry_selection_centers(&other.document_id, &other.revision, params)
                .unwrap()
                .cache_hit
        );
    }
    assert!(
        !s.geometry_selection_centers(&d.document_id, &d.revision, p)
            .unwrap()
            .cache_hit
    );
}

#[test]
fn every_warm_cache_checkpoint_cancels_without_publishing_or_mutation() {
    let (mut s, d, p) = setup();
    s.geometry_selection_centers(&d.document_id, &d.revision, p.clone())
        .unwrap();
    let before = s.document_get(&d.document_id).unwrap();
    let snapshot = s.render_snapshot(&d.document_id).unwrap();
    let mut total = 0;
    assert!(
        s.geometry_selection_centers_cancellable(&d.document_id, &d.revision, p.clone(), || {
            total += 1;
            false
        })
        .unwrap()
        .cache_hit
    );
    for cut in 1..=total {
        let mut polls = 0;
        let result = s.geometry_selection_centers_cancellable(
            &d.document_id,
            &d.revision,
            p.clone(),
            || {
                polls += 1;
                polls == cut
            },
        );
        assert_eq!(
            result.unwrap_err().code,
            "CANCELLED",
            "warm cache checkpoint {cut}/{total}"
        );
        assert_eq!(s.document_get(&d.document_id).unwrap(), before);
        assert_eq!(s.render_snapshot(&d.document_id).unwrap(), snapshot);
        assert!(
            s.geometry_selection_centers(&d.document_id, &d.revision, p.clone())
                .unwrap()
                .cache_hit
        );
    }
}
