use editor_core::{ApertureShape, MmPoint, SemanticGeometry, grip::GripFeatureId};
use editor_service::{
    ApplicationService, CreateEmptyLayerParams, ExportParams, FileAccessPolicy, LayerUpdateParams,
    MetadataPolicy, OverwritePolicy, QueryParams, SetActiveLayerParams,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,2X0.5*%\nD10*\nX10000000Y20000000D03*\nX20000000Y20000000D03*\nM02*\n";
const ROTATED_RECTANGLE: &str =
    "%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,4X2*%\nD10*\n%LR90*%\nX10000000Y20000000D03*\nM02*\n";
const LINE: &str =
    "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX0Y0D02*\nG01X1000000Y0D01*\nM02*\n";
const ARC: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG75*\nX1000000Y0D02*\nG03X0Y1000000I-1000000J0D01*\nM02*\n";
const REGION: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nG36*\nX0Y0D02*\nG01X4000000Y0D01*\nX4000000Y4000000D01*\nX0Y4000000D01*\nX0Y0D01*\nG37*\nM02*\n";

struct Run {
    service: ApplicationService,
    dir: PathBuf,
    document: String,
    layer: String,
    objects: Vec<String>,
    source_hash: String,
}

impl Run {
    fn new() -> Self {
        Self::from_source(SOURCE, 2)
    }

    fn from_source(contents: &str, expected_objects: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rcam-s4c2-grip-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.gbr");
        fs::write(&source, contents).unwrap();
        let source_hash = editor_core::hash::sha256_hex(&fs::read(&source).unwrap());
        let policy = FileAccessPolicy::new(dir.clone(), [dir.clone()], [dir.clone()]);
        let mut service = ApplicationService::with_file_access(policy);
        let opened = service.open("source.gbr").unwrap();
        let document = opened.document_id;
        let layer = opened.layer_ids[0].clone();
        let objects: Vec<String> = service
            .objects_query(
                &document,
                QueryParams {
                    layer_id: layer.clone(),
                    geometry_type: None,
                    region_mm: None,
                    relation: None,
                    limit: None,
                    cursor: None,
                },
            )
            .unwrap()
            .objects
            .into_iter()
            .map(|item| item.object.object_id)
            .collect();
        assert_eq!(objects.len(), expected_objects);
        Self {
            service,
            dir,
            document,
            layer,
            objects,
            source_hash,
        }
    }

    fn call(&mut self, op: &str, revision: Option<&str>, params: Value) -> Value {
        self.service.execute_json(&json!({"api_version":1,"request_id":op,"op":op,"document_id":self.document,"expected_revision":revision,"params":params}).to_string())
    }

    fn grips(&mut self) -> Value {
        self.call(
            "objects.grips",
            None,
            json!({"layer_id":self.layer,"object_id":self.objects[0]}),
        )
    }

    fn edit(&mut self, revision: &str, target_x: f64) -> Value {
        self.edit_grip(
            0,
            revision,
            json!({"kind":"radius"}),
            MmPoint::new(target_x, 20.0),
        )
    }

    fn edit_grip(
        &mut self,
        index: usize,
        revision: &str,
        grip_id: Value,
        target: MmPoint,
    ) -> Value {
        self.call("objects.grip_edit", Some(revision), json!({"layer_id":self.layer,"object_id":self.objects[index],"grip_id":grip_id,"target_mm":target}))
    }

    fn info(&self) -> editor_service::DocumentInfo {
        self.service.document_get(&self.document).unwrap()
    }

    fn shape(&self, object_id: &str) -> (MmPoint, ApertureShape, String) {
        let snapshot = self.service.render_snapshot(&self.document).unwrap();
        let object = snapshot.layers[0]
            .objects
            .iter()
            .find(|o| o.object_id == object_id)
            .unwrap();
        let SemanticGeometry::Flash {
            center,
            aperture_id,
            ..
        } = &object.geometry
        else {
            panic!("expected Flash")
        };
        let shape = snapshot
            .apertures
            .iter()
            .find(|a| a.id == *aperture_id)
            .unwrap()
            .shape
            .clone();
        (*center, shape, aperture_id.clone())
    }

    fn patch(&self) -> LayerUpdateParams {
        LayerUpdateParams {
            layer_id: self.layer.clone(),
            expected_workspace_revision: self.info().workspace_revision,
            ..Default::default()
        }
    }
}

#[test]
fn public_s4c2_fixture_parses_obround_polygon_sweep_and_full_circle_arc() {
    let scene = gerber_io::parse_s1(
        include_bytes!("../../../fixtures/synthetic/s4c2/grips.gbr"),
        "s4c2-grips",
    )
    .unwrap();
    let document = scene.document;
    let objects = &document.layers[0].objects;
    assert_eq!(objects.len(), 12);
    let has_flash = |center, accepts: fn(&ApertureShape) -> bool| {
        objects.iter().any(|object| {
            let SemanticGeometry::Flash {
                center: actual,
                aperture_id,
                ..
            } = &object.geometry
            else {
                return false;
            };
            *actual == center
                && document
                    .apertures
                    .iter()
                    .find(|aperture| aperture.id == *aperture_id)
                    .is_some_and(|aperture| accepts(&aperture.shape))
        })
    };
    assert!(has_flash(MmPoint::new(10., 3.), |shape| matches!(
        shape,
        ApertureShape::Obround { .. }
    )));
    assert!(has_flash(MmPoint::new(14., 3.), |shape| matches!(
        shape,
        ApertureShape::Polygon { .. }
    )));
    assert!(objects.iter().any(|object| matches!(
        &object.geometry,
        SemanticGeometry::RectangularSweep { start, end, .. }
            if *start == MmPoint::new(10., 8.) && *end == MmPoint::new(14., 8.)
    )));
    assert!(objects.iter().any(|object| matches!(
        &object.geometry,
        SemanticGeometry::Arc { path, .. }
            if path.full_circle
                && path.center == MmPoint::new(17., 8.)
                && (path.radius() - 1.).abs() < 1e-9
    )));
    let rectangle = objects
        .iter()
        .find(|object| matches!(&object.geometry, SemanticGeometry::Region { contours } if contours.len() == 1 && contours[0].edges.len() == 4))
        .expect("fixture must include a four-edge Region");
    let SemanticGeometry::Region { contours } = &rectangle.geometry else {
        unreachable!();
    };
    assert_eq!(contours[0].role, editor_core::RegionRole::Solid);
    let expected_edges = [
        (MmPoint::new(10., 18.), MmPoint::new(14., 18.)),
        (MmPoint::new(14., 18.), MmPoint::new(14., 22.)),
        (MmPoint::new(14., 22.), MmPoint::new(10., 22.)),
        (MmPoint::new(10., 22.), MmPoint::new(10., 18.)),
    ];
    for (edge, (expected_start, expected_end)) in contours[0].edges.iter().zip(expected_edges) {
        assert!(matches!(
            edge,
            editor_core::RegionEdge::Line { start, end }
                if *start == expected_start && *end == expected_end
        ));
    }
}

fn exported(run: &mut Run) -> editor_core::SemanticDocument {
    let output = run.dir.join("geometry.gbr");
    run.service
        .export_layer(
            &run.document,
            &run.info().revision,
            ExportParams {
                layer_id: run.layer.clone(),
                path: output.to_string_lossy().into_owned(),
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
    gerber_io::parse_s1(&fs::read(output).unwrap(), "exported")
        .unwrap()
        .document
}

fn near(a: MmPoint, b: MmPoint) {
    assert!(
        (a.x_mm - b.x_mm).abs() <= 0.000001 && (a.y_mm - b.y_mm).abs() <= 0.000001,
        "{a:?} != {b:?}"
    );
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn json_grip_edit_is_atomic_cow_and_survives_project_and_gerber_roundtrip() {
    let mut run = Run::new();
    let before = run.service.render_snapshot(&run.document).unwrap();
    let info = run.info();
    let first = run.grips();
    assert_eq!(first["status"], "completed", "{first}");
    assert_eq!(first["result"], run.grips()["result"]);
    assert_eq!(first["result"][0]["id"], json!({"kind":"radius"}));
    assert_eq!(
        run.info(),
        info,
        "query must not change revision, dirty or history"
    );

    let preview = editor_core::grip::preview_grip_edit(
        &before.layers[0].objects[0],
        Some(&before.apertures[0].shape),
        GripFeatureId::Radius,
        MmPoint::new(12.0, 20.0),
    )
    .unwrap();
    assert!(matches!(
        preview.aperture_shape,
        Some(ApertureShape::Circle {
            diameter_mm: 4.0,
            hole_diameter_mm: Some(0.5)
        })
    ));
    assert_eq!(
        run.service.render_snapshot(&run.document).unwrap(),
        before,
        "preview is read-only"
    );
    assert_eq!(run.info(), info);

    let original_id = run.shape(&run.objects[0]).2;
    let changed = run.edit(&info.revision, 12.0);
    assert_eq!(changed["status"], "completed", "{changed}");
    assert_eq!(run.info().revision, "1");
    assert_eq!(run.info().undo_entries, 1);
    assert!(run.info().dirty);
    let (center, shape, generated_id) = run.shape(&run.objects[0]);
    assert_eq!(center, MmPoint::new(10.0, 20.0));
    assert!(matches!(
        shape,
        ApertureShape::Circle {
            diameter_mm: 4.0,
            hole_diameter_mm: Some(0.5)
        }
    ));
    assert_ne!(generated_id, original_id);
    assert_eq!(run.shape(&run.objects[1]).2, original_id);
    assert!(matches!(
        run.shape(&run.objects[1]).1,
        ApertureShape::Circle {
            diameter_mm: 2.0,
            ..
        }
    ));

    let stale = run.edit("0", 13.0);
    assert_eq!(stale["error"]["code"], "REVISION_CONFLICT", "{stale}");
    assert_eq!(run.info().revision, "1");
    run.service.history_undo(&run.document, "1").unwrap();
    assert_eq!(run.shape(&run.objects[0]).2, original_id);
    assert_eq!(
        run.service
            .render_snapshot(&run.document)
            .unwrap()
            .apertures,
        before.apertures
    );
    run.service.history_redo(&run.document, "2").unwrap();
    assert_eq!(run.shape(&run.objects[0]).2, generated_id);

    let project = run.dir.join("edited.rcam");
    run.service
        .project_save(&run.document, "3", Some(project.to_str().unwrap()), false)
        .unwrap();
    let reopened = run.service.project_open(project.to_str().unwrap()).unwrap();
    let restored = run.service.render_snapshot(&reopened.document_id).unwrap();
    let restored_object = restored.layers[0]
        .objects
        .iter()
        .find(|o| o.object_id == run.objects[0])
        .unwrap();
    assert!(
        matches!(&restored_object.geometry, SemanticGeometry::Flash { aperture_id, center, .. } if aperture_id == &generated_id && *center == MmPoint::new(10.0, 20.0))
    );
    assert!(restored.apertures.iter().any(|a| a.id == generated_id
        && matches!(
            a.shape,
            ApertureShape::Circle {
                diameter_mm: 4.0,
                ..
            }
        )));

    let output = run.dir.join("export.gbr");
    run.service
        .export_layer(
            &reopened.document_id,
            &reopened.revision,
            ExportParams {
                layer_id: run.layer.clone(),
                path: output.to_string_lossy().into_owned(),
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
    let exported = gerber_io::parse_s1(&fs::read(output).unwrap(), "exported").unwrap();
    let mut diameters = exported.document.layers[0]
        .objects
        .iter()
        .map(|object| {
            let SemanticGeometry::Flash {
                aperture_id,
                center,
                ..
            } = &object.geometry
            else {
                panic!("expected Flash")
            };
            let aperture = exported
                .document
                .apertures
                .iter()
                .find(|a| a.id == *aperture_id)
                .unwrap();
            let ApertureShape::Circle { diameter_mm, .. } = aperture.shape else {
                panic!("expected Circle")
            };
            (center.x_mm, center.y_mm, diameter_mm)
        })
        .collect::<Vec<_>>();
    diameters.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(diameters, [(10.0, 20.0, 4.0), (20.0, 20.0, 2.0)]);
    assert_eq!(
        editor_core::hash::sha256_hex(&fs::read(run.dir.join("source.gbr")).unwrap()),
        run.source_hash
    );
}

#[test]
fn json_grips_enforce_workspace_permissions_without_manufacturing_changes() {
    let mut run = Run::new();
    let baseline = run.service.render_snapshot(&run.document).unwrap();
    for (field, value) in [("visible", false), ("selectable", false), ("locked", true)] {
        let mut patch = run.patch();
        match field {
            "visible" => patch.visible = Some(value),
            "selectable" => patch.selectable = Some(value),
            _ => patch.locked = Some(value),
        }
        let revision = run.info().revision;
        run.service
            .layer_update(&run.document, &revision, patch)
            .unwrap();
        let query = run.grips();
        assert_eq!(query["result"], json!([]), "{field}: {query}");
        let denied = run.edit(&revision, 12.0);
        assert!(denied["error"]["code"].is_string(), "{field}: {denied}");
        let after = run.service.render_snapshot(&run.document).unwrap();
        assert_eq!(after.layers, baseline.layers);
        assert_eq!(after.apertures, baseline.apertures);
        assert_eq!(after.revision, baseline.revision);
        let mut restore = run.patch();
        match field {
            "visible" => restore.visible = Some(true),
            "selectable" => restore.selectable = Some(true),
            _ => restore.locked = Some(false),
        }
        run.service
            .layer_update(&run.document, &revision, restore)
            .unwrap();
    }

    let revision = run.info().revision;
    let second = run
        .service
        .create_empty_layer(&run.document, &revision, CreateEmptyLayerParams::default())
        .unwrap();
    let current = run.info();
    run.service
        .layers_set_active(
            &run.document,
            &current.revision,
            SetActiveLayerParams {
                expected_workspace_revision: current.workspace_revision,
                layer_id: Some(second.layer_id),
            },
        )
        .unwrap();
    assert_eq!(run.grips()["result"], json!([]));
    let denied = run.edit(&run.info().revision, 12.0);
    assert!(denied["error"]["code"].is_string(), "{denied}");
    assert_eq!(
        run.shape(&run.objects[0]).1,
        ApertureShape::Circle {
            diameter_mm: 2.0,
            hole_diameter_mm: Some(0.5)
        }
    );
}

#[test]
fn rotated_rectangle_grip_exports_local_axis_resize() {
    let mut run = Run::from_source(ROTATED_RECTANGLE, 1);
    let reply = run.edit_grip(0, "0", json!({"kind":"right"}), MmPoint::new(10.0, 23.0));
    assert_eq!(reply["status"], "completed", "{reply}");
    let document = exported(&mut run);
    let SemanticGeometry::Flash {
        center,
        aperture_id,
        transform,
    } = &document.layers[0].objects[0].geometry
    else {
        panic!("expected rectangle Flash")
    };
    near(*center, MmPoint::new(10.0, 20.5));
    assert!((transform.rotation_deg - 90.0).abs() <= 1e-9);
    let shape = &document
        .apertures
        .iter()
        .find(|a| a.id == *aperture_id)
        .unwrap()
        .shape;
    assert_eq!(
        *shape,
        ApertureShape::Rectangle {
            width_mm: 5.0,
            height_mm: 2.0,
            hole_diameter_mm: None
        }
    );
}

#[test]
fn line_endpoint_grip_exports_independent_path_geometry() {
    let mut run = Run::from_source(LINE, 1);
    let reply = run.edit_grip(0, "0", json!({"kind":"end"}), MmPoint::new(2.0, 0.0));
    assert_eq!(reply["status"], "completed", "{reply}");
    let document = exported(&mut run);
    let SemanticGeometry::Line {
        start,
        end,
        width_mm,
    } = document.layers[0].objects[0].geometry
    else {
        panic!("expected Line")
    };
    near(start, MmPoint::new(0.0, 0.0));
    near(end, MmPoint::new(2.0, 0.0));
    assert!((width_mm - 0.2).abs() <= 1e-9);
}

#[test]
fn arc_radius_grip_exports_center_radius_and_direction() {
    let mut run = Run::from_source(ARC, 1);
    let reply = run.edit_grip(0, "0", json!({"kind":"radius"}), MmPoint::new(2.0, 0.0));
    assert_eq!(reply["status"], "completed", "{reply}");
    let document = exported(&mut run);
    let SemanticGeometry::Arc { path, width_mm } = &document.layers[0].objects[0].geometry else {
        panic!("expected Arc")
    };
    near(path.center, MmPoint::new(0.0, 0.0));
    near(path.start, MmPoint::new(2.0, 0.0));
    near(path.end, MmPoint::new(0.0, 2.0));
    assert!((path.radius() - 2.0).abs() <= 0.000001);
    assert_eq!(path.direction, editor_core::ArcDirection::CounterClockwise);
    assert!(!path.full_circle);
    assert!((*width_mm - 0.2).abs() <= 1e-9);
}

#[test]
fn region_vertex_grip_exports_closed_line_only_contour() {
    let mut run = Run::from_source(REGION, 1);
    let reply = run.edit_grip(
        0,
        "0",
        json!({"kind":"vertex","contour":0,"vertex":0}),
        MmPoint::new(-1.0, -1.0),
    );
    assert_eq!(reply["status"], "completed", "{reply}");
    let document = exported(&mut run);
    let SemanticGeometry::Region { contours } = &document.layers[0].objects[0].geometry else {
        panic!("expected Region")
    };
    assert_eq!(contours.len(), 1);
    assert_eq!(contours[0].edges.len(), 4);
    let editor_core::RegionEdge::Line {
        start: first,
        end: second,
    } = contours[0].edges[0]
    else {
        panic!("expected first line")
    };
    let editor_core::RegionEdge::Line {
        start: fourth,
        end: closing,
    } = contours[0].edges[3]
    else {
        panic!("expected closing line")
    };
    near(first, MmPoint::new(-1.0, -1.0));
    near(second, MmPoint::new(4.0, 0.0));
    near(fourth, MmPoint::new(0.0, 4.0));
    near(closing, first);
}
