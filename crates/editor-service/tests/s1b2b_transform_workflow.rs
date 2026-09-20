//! S1-B2b: real JSON workflows and independent rigid-transform geometry assertions.
use editor_core::edit::{EditError, EditHistory};
use editor_core::{MmPoint, SemanticGeometry, SemanticObject};
use editor_service::{ApplicationService, FileAccessPolicy, ObjectInfo};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);
// F_POL: radii 5 Dark, 3 Clear, 1 Dark, all centered at (2,2).
const POL: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,10*%\n%ADD11C,6*%\n%ADD12C,2*%\nD10*\nX2000000Y2000000D03*\n%LPC*%\nD11*\nX2000000Y2000000D03*\n%LPD*%\nD12*\nX2000000Y2000000D03*\nM02*\n";
struct Run {
    service: ApplicationService,
    dir: PathBuf,
    id: String,
    layer: String,
    calls: Vec<Value>,
}
impl Run {
    fn new() -> Self {
        Self::source(POL.as_bytes())
    }
    fn source(source: &[u8]) -> Self {
        let base = std::env::var_os("RCAM_S1B2B_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "s1b2b-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.gbr"), source).unwrap();
        let service = ApplicationService::with_file_access(FileAccessPolicy::new(
            dir.clone(),
            [dir.clone()],
            [dir.clone()],
        ));
        let mut run = Self {
            service,
            dir,
            id: String::new(),
            layer: String::new(),
            calls: vec![],
        };
        let opened = run.ok("document.open", None, json!({"path":"source.gbr"}));
        run.id = opened["document_id"].as_str().unwrap().into();
        run.layer = opened["layer_ids"][0].as_str().unwrap().into();
        run
    }
    fn call(&mut self, op: &str, rev: Option<&str>, params: Value) -> Value {
        let mut req = json!({"api_version":1,"request_id":format!("b2-{}",self.calls.len()),"op":op,"params":params});
        if !matches!(op, "document.open" | "system.capabilities") {
            req["document_id"] = json!(self.id);
        }
        if let Some(rev) = rev {
            req["expected_revision"] = json!(rev);
        }
        let reply = self.service.execute_json(&req.to_string());
        assert_eq!(reply["request_id"], req["request_id"]);
        self.calls.push(json!({"request":req,"response":reply}));
        reply
    }
    fn ok(&mut self, op: &str, rev: Option<&str>, params: Value) -> Value {
        let r = self.call(op, rev, params);
        assert_eq!(r["status"], "completed", "{r}");
        r["result"].clone()
    }
    fn info(&mut self) -> Value {
        self.ok("document.get", None, json!({}))
    }
    fn objects(&mut self) -> Vec<SemanticObject> {
        let result = self.ok("objects.query", None, json!({"layer_id":self.layer}));
        let objects: Vec<ObjectInfo> = serde_json::from_value(result["objects"].clone()).unwrap();
        objects.into_iter().map(|o| o.object).collect()
    }
    fn ids(&mut self) -> Vec<String> {
        self.objects().into_iter().map(|o| o.object_id).collect()
    }
    fn rotate(&mut self, rev: &str, degrees: f64, pivot: MmPoint) -> Value {
        let ids = self.ids();
        self.ok(
            "objects.rotate",
            Some(rev),
            json!({"layer_id":self.layer,"object_ids":ids,"angle_deg":degrees,"pivot_mm":pivot}),
        )
    }
    fn mirror(&mut self, rev: &str, kind: &str, coordinate: f64) -> Value {
        let ids = self.ids();
        self.ok("objects.mirror", Some(rev), json!({"layer_id":self.layer,"object_ids":ids,"axis":{"kind":kind,"coordinate_mm":coordinate}}))
    }
    fn history(&mut self, op: &str, rev: &str) {
        self.ok(op, Some(rev), json!({}));
    }
    fn export_params(&self, path: &str) -> Value {
        json!({"layer_id":self.layer,"path":path,"overwrite":{"mode":"deny"},"metadata_policy":{"mode":"require_confirmation"}})
    }
    fn roundtrip(&mut self, rev: &str) -> gerber_io::S1Scene {
        self.ok("document.validate", None, json!({}));
        let source = std::fs::read(self.dir.join("source.gbr")).unwrap();
        let source_hash = self.info()["source_sha256"].clone();
        self.ok(
            "gerber.export_layer",
            Some(rev),
            self.export_params("edited.gbr"),
        );
        let opened = self.ok("document.open", None, json!({"path":"edited.gbr"}));
        assert!(
            self.service
                .validate(opened["document_id"].as_str().unwrap())
                .unwrap()
                .valid
        );
        assert_eq!(std::fs::read(self.dir.join("source.gbr")).unwrap(), source);
        assert_eq!(self.info()["source_sha256"], source_hash);
        gerber_io::parse_s1(
            &std::fs::read(self.dir.join("edited.gbr")).unwrap(),
            "reopened",
        )
        .unwrap()
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        if std::env::var_os("RCAM_S1B2B_EVIDENCE").is_some() {
            std::fs::write(
                self.dir.join("requests.json"),
                serde_json::to_vec_pretty(&self.calls).unwrap(),
            )
            .unwrap();
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

use editor_core::{LocalTransform, Mirror, RegionEdge};
use editor_service::{MirrorAxis, MirrorParams, PivotMm, RotateParams};
const RECT: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,4X2X0.5*%\nD10*\nX10000000Y0D03*\nM02*\n";
const SWEEP: &str = "%FSLAX26Y26*%\n%MOMM*%\n%ADD10R,2X1*%\n%ADD11C,1*%\nD11*\nX0Y0D03*\nD10*\nX0Y0D02*\nG01X3000000Y0D01*\nM02*\n";
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic")
            .join(name),
    )
    .unwrap()
}
fn near(actual: MmPoint, expected: MmPoint) {
    assert!(
        actual.distance_mm(expected) < 1e-6,
        "{actual:?} != {expected:?}"
    );
}
// Independent formula, no production transformation helpers used here.
fn rotated(p: MmPoint, a: f64, q: MmPoint) -> MmPoint {
    let theta = a * std::f64::consts::PI / 180.;
    MmPoint::new(
        q.x_mm + (p.x_mm - q.x_mm) * theta.cos() - (p.y_mm - q.y_mm) * theta.sin(),
        q.y_mm + (p.x_mm - q.x_mm) * theta.sin() + (p.y_mm - q.y_mm) * theta.cos(),
    )
}
fn flash(g: &SemanticGeometry) -> (MmPoint, LocalTransform) {
    if let SemanticGeometry::Flash {
        center, transform, ..
    } = g
    {
        (*center, *transform)
    } else {
        panic!("not flash")
    }
}
fn arc(g: &SemanticGeometry) -> editor_core::ArcGeometry {
    if let SemanticGeometry::Arc { path, .. } = g {
        *path
    } else {
        panic!("not arc")
    }
}
fn local_point(p: MmPoint, t: LocalTransform, center: MmPoint) -> MmPoint {
    let x = if matches!(t.mirror, Mirror::X | Mirror::Xy) {
        -p.x_mm
    } else {
        p.x_mm
    } * t.scale;
    let y = if matches!(t.mirror, Mirror::Y | Mirror::Xy) {
        -p.y_mm
    } else {
        p.y_mm
    } * t.scale;
    let p = rotated(MmPoint::new(x, y), t.rotation_deg, MmPoint::new(0., 0.));
    MmPoint::new(center.x_mm + p.x_mm, center.y_mm + p.y_mm)
}

#[test]
fn rotate_point_90_about_origin() {
    let mut r = Run::source(RECT.as_bytes());
    r.rotate("0", 90., MmPoint::new(0., 0.));
    assert_eq!(flash(&r.objects()[0].geometry).0, MmPoint::new(0., 10.));
}
#[test]
fn rotate_point_37_about_nonzero_pivot() {
    let mut r = Run::source(RECT.as_bytes());
    let pivot = MmPoint::new(10., 20.);
    r.rotate("0", 37., pivot);
    near(
        flash(&r.objects()[0].geometry).0,
        rotated(MmPoint::new(10., 0.), 37., pivot),
    );
}
#[test]
fn rotate_flash_updates_local_transform() {
    for ad in ["R,4X2X0.5", "O,4X2X0.5", "P,4X5X17X0.5"] {
        let src = RECT.replace("R,4X2X0.5", ad);
        let mut r = Run::source(src.as_bytes());
        let (center, t) = flash(&r.objects()[0].geometry);
        r.rotate("0", 37., MmPoint::new(0., 0.));
        let (new, nt) = flash(&r.objects()[0].geometry);
        assert_eq!(nt.rotation_deg, 37.);
        assert_eq!(nt.scale, t.scale);
        near(
            local_point(MmPoint::new(1.7, 0.3), nt, new),
            rotated(
                local_point(MmPoint::new(1.7, 0.3), t, center),
                37.,
                MmPoint::new(0., 0.),
            ),
        );
        r.roundtrip("1");
    }
}
#[test]
fn rotate_existing_mirrored_flash_composes_correctly() {
    for lm in ["N", "X", "Y", "XY"] {
        let src = RECT.replace("D10*", &format!("D10*\n%LM{lm}*%\n%LR23*%\n%LS1.5*%"));
        let mut r = Run::source(src.as_bytes());
        let (c, t) = flash(&r.objects()[0].geometry);
        let pivot = MmPoint::new(3., -4.);
        r.rotate("0", 37., pivot);
        let (nc, nt) = flash(&r.objects()[0].geometry);
        assert_eq!(nt.mirror, t.mirror);
        assert_eq!(nt.scale, 1.5);
        near(
            local_point(MmPoint::new(1.3, 0.7), nt, nc),
            rotated(local_point(MmPoint::new(1.3, 0.7), t, c), 37., pivot),
        );
        r.roundtrip("1");
    }
}
#[test]
fn rotate_line_roundtrip() {
    let src = b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX0Y0D02*\nX10000000Y0D01*\nM02*\n";
    let mut r = Run::source(src);
    r.rotate("0", 37., MmPoint::new(2., 3.));
    let scene = r.roundtrip("1");
    let grid = |p: MmPoint| {
        MmPoint::new(
            (p.x_mm * 10000.).round() / 10000.,
            (p.y_mm * 10000.).round() / 10000.,
        )
    };
    if let SemanticGeometry::Line {
        start,
        end,
        width_mm,
    } = scene.document.layers[0].objects[0].geometry
    {
        near(
            start,
            grid(rotated(MmPoint::new(0., 0.), 37., MmPoint::new(2., 3.))),
        );
        near(
            end,
            grid(rotated(MmPoint::new(10., 0.), 37., MmPoint::new(2., 3.))),
        );
        assert_eq!(width_mm, 0.2);
    } else {
        panic!("not line");
    }
}
fn check_arc(name: &str, mirror: bool) {
    let mut r = Run::source(&fixture(name));
    let old = r.objects();
    if mirror {
        r.mirror("0", "horizontal", 2.);
    } else {
        r.rotate("0", 37., MmPoint::new(2., 3.));
    }
    for (a, b) in old.iter().zip(r.objects()) {
        let (a, b) = (arc(&a.geometry), arc(&b.geometry));
        for (p, q) in [(a.start, b.start), (a.end, b.end), (a.center, b.center)] {
            near(
                q,
                if mirror {
                    MmPoint::new(p.x_mm, 4. - p.y_mm)
                } else {
                    rotated(p, 37., MmPoint::new(2., 3.))
                },
            );
        }
        assert_eq!(a.full_circle, b.full_circle);
        assert_eq!(a.zero_sweep(), b.zero_sweep());
        assert_eq!(a.source, b.source);
        assert!((a.arc_deviation() - b.arc_deviation()).abs() < 1e-6);
        if mirror {
            assert_ne!(a.direction, b.direction);
        } else {
            assert_eq!(a.direction, b.direction);
        }
    }
    r.roundtrip("1");
}
#[test]
fn rotate_arc_preserves_direction_and_source_semantics() {
    for name in [
        "s1a1/g75_small_deviation.gbr",
        "s1a1/g74_cw_quarter.gbr",
        "s1a1/g74_ccw_quarter.gbr",
        "s1a1/g74_zero.gbr",
    ] {
        check_arc(name, false);
    }
}
#[test]
fn rotate_full_circle_remains_full_circle() {
    check_arc("s1a1/g75_full.gbr", false);
}
#[test]
fn rotate_g74_zero_sweep_remains_zero_sweep() {
    check_arc("s1a1/g74_zero.gbr", false);
}
#[test]
fn mirror_arc_flips_direction() {
    for name in [
        "s1a1/g75_full.gbr",
        "s1a1/g75_small_deviation.gbr",
        "s1a1/g74_cw_quarter.gbr",
        "s1a1/g74_ccw_quarter.gbr",
        "s1a1/g74_zero.gbr",
    ] {
        check_arc(name, true);
    }
}
fn check_region(mirror: bool) {
    for name in ["s1a/region_arc.gbr", "s1a/region_cutin.gbr"] {
        let mut r = Run::source(&fixture(name));
        let before = r.objects();
        if mirror {
            r.mirror("0", "vertical", 2.);
        } else {
            r.rotate("0", 37., MmPoint::new(2., 3.));
        }
        let after = r.objects();
        let (SemanticGeometry::Region { contours: a }, SemanticGeometry::Region { contours: b }) =
            (&before[0].geometry, &after[0].geometry)
        else {
            panic!("not region")
        };
        assert_eq!(a.len(), b.len());
        let expected = |p: MmPoint| {
            if mirror {
                MmPoint::new(4. - p.x_mm, p.y_mm)
            } else {
                rotated(p, 37., MmPoint::new(2., 3.))
            }
        };
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.role, b.role);
            assert_eq!(a.edges.len(), b.edges.len());
            for (a, b) in a.edges.iter().zip(&b.edges) {
                match (a, b) {
                    (
                        RegionEdge::Line { start: a, end: b },
                        RegionEdge::Line { start: c, end: d },
                    ) => {
                        near(*c, expected(*a));
                        near(*d, expected(*b));
                    }
                    (RegionEdge::Arc(a), RegionEdge::Arc(b)) => {
                        near(b.start, expected(a.start));
                        near(b.end, expected(a.end));
                        near(b.center, expected(a.center));
                        assert_eq!(a.source, b.source);
                        assert_eq!(a.direction == b.direction, !mirror);
                    }
                    _ => panic!("edge topology changed"),
                }
            }
        }
        r.roundtrip("1");
    }
}
#[test]
fn rotate_region_preserves_edge_order_and_topology() {
    check_region(false);
}
#[test]
fn mirror_region_flips_all_arc_directions() {
    check_region(true);
}
#[test]
fn rectangular_sweep_rotates_exact_90_and_swaps_dimensions() {
    let mut r = Run::source(SWEEP.as_bytes());
    r.rotate("0", 90., MmPoint::new(0., 0.));
    assert!(
        matches!(r.objects()[1].geometry,SemanticGeometry::RectangularSweep{start,end,width_mm:1.,height_mm:2.} if start==MmPoint::new(0.,0.) && end==MmPoint::new(0.,3.))
    );
    let output = r.roundtrip("1");
    assert_eq!(
        output
            .document
            .layer_coverage_at(&output.document.layers[0].id, MmPoint::new(0.49, 3.99)),
        Some(true)
    );
    assert_eq!(
        output
            .document
            .layer_coverage_at(&output.document.layers[0].id, MmPoint::new(0.51, 3.99)),
        Some(false)
    );
}
#[test]
fn rectangular_sweep_rejects_37_deg_atomically() {
    let mut r = Run::source(SWEEP.as_bytes());
    let old = r.objects();
    let info = r.info();
    let ids = r.ids();
    let reply = r.call(
        "objects.rotate",
        Some("0"),
        json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":37,"pivot_mm":{"x_mm":0,"y_mm":0}}),
    );
    assert_eq!(reply["error"]["code"], "UNSUPPORTED_FEATURE");
    assert_eq!(r.objects(), old);
    assert_eq!(r.info(), info);
}
#[test]
fn mirror_flash_composes_local_transform() {
    for lm in ["N", "X", "Y", "XY"] {
        for kind in ["horizontal", "vertical"] {
            let src = RECT.replace("D10*", &format!("D10*\n%LM{lm}*%\n%LR23*%\n%LS1.5*%"));
            let mut r = Run::source(src.as_bytes());
            let (c, t) = flash(&r.objects()[0].geometry);
            r.mirror("0", kind, 3.);
            let (nc, nt) = flash(&r.objects()[0].geometry);
            let p = local_point(MmPoint::new(1.3, 0.7), t, c);
            near(
                local_point(MmPoint::new(1.3, 0.7), nt, nc),
                if kind == "horizontal" {
                    MmPoint::new(p.x_mm, 6. - p.y_mm)
                } else {
                    MmPoint::new(6. - p.x_mm, p.y_mm)
                },
            );
            r.roundtrip("1");
        }
    }
}
#[test]
fn mirror_twice_restores_exact_state() {
    for src in [
        RECT.as_bytes().to_vec(),
        SWEEP.as_bytes().to_vec(),
        fixture("s1a/region_arc.gbr"),
        fixture("s1a1/g75_full.gbr"),
    ] {
        for kind in ["horizontal", "vertical"] {
            let mut r = Run::source(&src);
            let old = r.objects();
            r.mirror("0", kind, 0.);
            r.mirror("1", kind, 0.);
            assert_eq!(
                serde_json::to_vec(&r.objects()).unwrap(),
                serde_json::to_vec(&old).unwrap()
            );
            r.roundtrip("2");
        }
    }
}
#[test]
fn four_quarter_turns_restore_exact_or_frozen_canonical_state() {
    for src in [
        RECT.as_bytes().to_vec(),
        SWEEP.as_bytes().to_vec(),
        fixture("s1a/region_arc.gbr"),
        fixture("s1a1/g75_full.gbr"),
    ] {
        let mut r = Run::source(&src);
        let old = r.objects();
        for n in 0..4 {
            r.rotate(&n.to_string(), 90., MmPoint::new(0., 0.));
        }
        assert_eq!(
            serde_json::to_vec(&r.objects()).unwrap(),
            serde_json::to_vec(&old).unwrap()
        );
        r.roundtrip("4");
    }
}
fn undo_redo(mirror: bool) {
    let mut r = Run::source(&fixture("s1a/region_arc.gbr"));
    let old = serde_json::to_vec(&r.objects()).unwrap();
    let result = if mirror {
        r.mirror("0", "horizontal", 2.)
    } else {
        r.rotate("0", 37., MmPoint::new(2., 3.))
    };
    assert_eq!(result["undo_entries_added"], 1);
    assert_eq!(result["undo_entries"], 1);
    let after = serde_json::to_vec(&r.objects()).unwrap();
    r.history("history.undo", "1");
    assert_eq!(serde_json::to_vec(&r.objects()).unwrap(), old);
    r.history("history.redo", "2");
    assert_eq!(serde_json::to_vec(&r.objects()).unwrap(), after);
    r.roundtrip("3");
}
#[test]
fn rotate_undo_redo_restores_exact_state() {
    undo_redo(false);
}
#[test]
fn mirror_undo_redo_restores_exact_state() {
    undo_redo(true);
}

#[test]
fn mixed_selection_failure_is_atomic() {
    for op in ["objects.rotate", "objects.mirror"] {
        let mut r = Run::new();
        let ids = r.ids();
        r.rotate("0", 90., MmPoint::new(0., 0.));
        r.history("history.undo", "1");
        let old = r.objects();
        let info = r.info();
        let base = if op == "objects.rotate" {
            json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":37,"pivot_mm":{"x_mm":0,"y_mm":0}})
        } else {
            json!({"layer_id":r.layer,"object_ids":ids,"axis":{"kind":"horizontal","coordinate_mm":0}})
        };
        let mut invalid = vec![base.clone(); 5];
        invalid[0]["object_ids"] = json!([]);
        invalid[1]["object_ids"] = json!([ids[0], "missing"]);
        invalid[2]["object_ids"] = json!([ids[0], ids[0]]);
        invalid[3]["layer_id"] = json!("missing");
        invalid[4]["object_ids"] = json!((0..10001).map(|i| format!("x{i}")).collect::<Vec<_>>());
        for params in invalid {
            assert_eq!(r.call(op, Some("2"), params)["status"], "error");
            assert_eq!(r.objects(), old);
            assert_eq!(r.info(), info);
        }
        assert_eq!(
            r.call(op, Some("0"), base.clone())["error"]["code"],
            "REVISION_CONFLICT"
        );
        assert_eq!(
            r.call(op, None, base.clone())["error"]["code"],
            "INVALID_ARGUMENT"
        );
        assert_eq!(r.objects(), old);
        assert_eq!(r.info(), info);
        r.ok(op, Some("2"), base);
        assert_eq!(r.info()["redo_entries"], 0);
    }
}
#[test]
fn rotate_mirror_export_reopen_matches_independent_geometry() {
    // Expected local ring/line, circle polarity, and rectangle-with-hole formulas
    // are independent of the production geometry evaluator and writer.
    for (src, kind) in [
        (fixture("s1a/macro_hole_over_line.gbr"), 0),
        (POL.as_bytes().to_vec(), 1),
        (RECT.as_bytes().to_vec(), 2),
    ] {
        let mut r = Run::source(&src);
        let pivot = MmPoint::new(2., 3.);
        r.rotate("0", 37., pivot);
        r.mirror("1", "vertical", 4.);
        let out = r.roundtrip("2");
        for ix in -36..=56 {
            for iy in -32..=32 {
                let p = MmPoint::new(ix as f64 * 0.23 + 0.017, iy as f64 * 0.23 + 0.013);
                let expected = match kind {
                    0 => {
                        let rad = p.x_mm.hypot(p.y_mm);
                        let dx = (p.x_mm.abs() - 3.).max(0.);
                        (dx * dx + p.y_mm * p.y_mm <= 0.01) || (1. ..=2.).contains(&rad)
                    }
                    1 => {
                        let rad = (p.x_mm - 2.).hypot(p.y_mm - 2.);
                        rad <= 1. || (rad > 3. && rad <= 5.)
                    }
                    _ => {
                        (p.x_mm - 10.).abs() <= 2.
                            && p.y_mm.abs() <= 1.
                            && (p.x_mm - 10.).hypot(p.y_mm) > 0.25
                    }
                };
                let q = rotated(p, 37., pivot);
                let q = MmPoint::new(8. - q.x_mm, q.y_mm);
                assert_eq!(
                    out.document
                        .layer_coverage_at(&out.document.layers[0].id, q),
                    Some(expected),
                    "kind={kind} p={p:?}"
                );
            }
        }
    }
}
#[test]
fn strict_transform_dtos_and_finite_checks_preserve_redo() {
    let mut r = Run::source(RECT.as_bytes());
    let ids = r.ids();
    r.rotate("0", 90., MmPoint::new(0., 0.));
    r.history("history.undo", "1");
    let old = r.objects();
    let info = r.info();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (a, x, y) in [(bad, 0., 0.), (37., bad, 0.), (37., 0., bad)] {
            let error = r
                .service
                .objects_rotate(
                    &r.id,
                    "2",
                    RotateParams {
                        layer_id: r.layer.clone(),
                        object_ids: ids.clone(),
                        angle_deg: a,
                        pivot_mm: PivotMm { x_mm: x, y_mm: y },
                    },
                )
                .unwrap_err();
            assert_eq!(error.code, "INVALID_ARGUMENT");
        }
        for axis in [
            MirrorAxis::Horizontal { coordinate_mm: bad },
            MirrorAxis::Vertical { coordinate_mm: bad },
        ] {
            assert_eq!(
                r.service
                    .objects_mirror(
                        &r.id,
                        "2",
                        MirrorParams {
                            layer_id: r.layer.clone(),
                            object_ids: ids.clone(),
                            axis
                        }
                    )
                    .unwrap_err()
                    .code,
                "INVALID_ARGUMENT"
            );
        }
    }
    for a in [0., 360., -360., 720.] {
        assert_eq!(r.call("objects.rotate",Some("2"),json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":a,"pivot_mm":{"x_mm":0,"y_mm":0}}))["error"]["code"],"INVALID_ARGUMENT");
    }
    for (op, params) in [
        (
            "objects.rotate",
            json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":37,"pivot_mm":{"x_mm":0,"y_mm":0,"z_mm":0}}),
        ),
        (
            "objects.rotate",
            json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":37,"pivot_mm":{"x_mm":0,"y_mm":0},"extra":0}),
        ),
        (
            "objects.mirror",
            json!({"layer_id":r.layer,"object_ids":ids,"axis":{"kind":"diagonal","coordinate_mm":0}}),
        ),
        (
            "objects.mirror",
            json!({"layer_id":r.layer,"object_ids":ids,"axis":{"kind":"horizontal","coordinate_mm":0,"angle":12}}),
        ),
    ] {
        assert_eq!(
            r.call(op, Some("2"), params)["error"]["code"],
            "INVALID_ARGUMENT"
        );
    }
    assert_eq!(r.objects(), old);
    assert_eq!(r.info(), info);
    r.history("history.redo", "2");
}
#[test]
fn core_history_limits_are_atomic() {
    let mut doc = gerber_io::parse_s1(RECT.as_bytes(), "limits")
        .unwrap()
        .document;
    let layer = doc.layers[0].id.clone();
    let ids = vec![doc.layers[0].objects[0].object_id.clone()];
    let mut h = EditHistory::default();
    // Workspace locks are checked through the public service in S1-B2c.
    for _ in 0..100 {
        h.rotate_objects(&mut doc, &layer, &ids, 90., MmPoint::new(0., 0.))
            .unwrap();
    }
    h.rotate_objects(&mut doc, &layer, &ids, 37., MmPoint::new(0., 0.))
        .unwrap();
    h.mirror_objects(
        &mut doc,
        &layer,
        &ids,
        MirrorAxis::Vertical { coordinate_mm: 0. },
    )
    .unwrap();
    assert_eq!(h.undo_len(), 100);
    assert_eq!(h.truncated_entries(), 2);
    assert!(h.bytes() <= editor_core::edit::MAX_HISTORY_BYTES);
}
#[test]
fn overflow_and_roundoff_reject_without_history() {
    let mut r = Run::source(RECT.as_bytes());
    let ids = r.ids();
    let old = r.objects();
    let info = r.info();
    for op in ["objects.rotate", "objects.mirror"] {
        let params = if op == "objects.rotate" {
            json!({"layer_id":r.layer,"object_ids":ids,"angle_deg":180,"pivot_mm":{"x_mm":1e9,"y_mm":1e9}})
        } else {
            json!({"layer_id":r.layer,"object_ids":ids,"axis":{"kind":"vertical","coordinate_mm":1e9}})
        };
        assert_eq!(r.call(op, Some("0"), params)["status"], "error");
        assert_eq!(r.objects(), old);
        assert_eq!(r.info(), info);
    }
}
#[test]
fn invariant_line_mirror_is_noop_and_preserves_history() {
    let src = b"%FSLAX26Y26*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\nX0Y0D02*\nX10000000Y0D01*\nM02*\n";
    let mut r = Run::source(src);
    let ids = r.ids();
    let old = r.objects();
    let info = r.info();
    assert_eq!(r.call("objects.mirror",Some("0"),json!({"layer_id":r.layer,"object_ids":ids,"axis":{"kind":"horizontal","coordinate_mm":0}}))["error"]["code"],"INVALID_ARGUMENT");
    assert_eq!(r.objects(), old);
    assert_eq!(r.info(), info);
}
#[test]
fn structural_order_guard_scalability_baseline() {
    // Measurement only: retains the existing guard and budget, no performance threshold waived.
    let mut rows = vec![];
    for count in [10_000, 100_000, 150_000, 500_000] {
        let mut doc = gerber_io::parse_s1(RECT.as_bytes(), "baseline")
            .unwrap()
            .document;
        let template = doc.layers[0].objects[0].clone();
        doc.layers[0].objects = (0..count)
            .map(|i| {
                let mut o = template.clone();
                o.object_id = format!("baseline-object-{i}");
                o
            })
            .collect();
        let layer = doc.layers[0].id.clone();
        let ids = vec![doc.layers[0].objects[0].object_id.clone()];
        let start = std::time::Instant::now();
        let mut h = EditHistory::default();
        let result = h.duplicate_objects(&mut doc, &layer, &ids, 1., 0.);
        let seconds = start.elapsed().as_secs_f64();
        assert!(result.is_ok() || result == Err(EditError::ResourceLimit));
        if result.is_err() {
            assert_eq!(doc.object_count(), count);
            assert_eq!(h.undo_len(), 0);
        }
        rows.push(json!({"objects":count,"seconds":seconds,"result":format!("{result:?}"),"build":"test debug, local cache","max_history_bytes":editor_core::edit::MAX_HISTORY_BYTES}));
    }
    println!("structural order baseline: {}", json!(rows));
    if let Some(base) = std::env::var_os("RCAM_S1B2B_EVIDENCE") {
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            PathBuf::from(base).join("order-guard-baseline.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn two_edge_region_envelope_checks_both_shared_endpoints_but_not_interior() {
    let mut doc = gerber_io::parse_s1(&fixture("s1a/region_arc.gbr"), "envelope")
        .unwrap()
        .document;
    for (offset, valid) in [(0.0000002, true), (0.00001, false)] {
        let SemanticGeometry::Region { contours } = &mut doc.layers[0].objects[0].geometry else {
            panic!()
        };
        let RegionEdge::Arc(a) = &mut contours[0].edges[0] else {
            panic!()
        };
        a.center.x_mm = offset;
        assert_eq!(doc.validate().is_ok(), valid, "offset {offset}");
    }
    assert!(
        gerber_io::parse_s1(&fixture("s1a1/g75_region_deviation_unsafe.gbr"), "unsafe").is_err()
    );
}
#[test]
fn rotated_cutin_preserves_independent_square_hole_truth() {
    let mut r = Run::source(&fixture("s1a/region_cutin.gbr"));
    r.rotate("0", 37., MmPoint::new(2., 3.));
    r.mirror("1", "horizontal", 0.);
    let out = r.roundtrip("2");
    for ix in -2..=42 {
        for iy in -2..=42 {
            let p = MmPoint::new(ix as f64 * 0.1 + 0.017, iy as f64 * 0.1 + 0.023);
            let expected = p.x_mm > 0.
                && p.x_mm < 4.
                && p.y_mm > 0.
                && p.y_mm < 4.
                && !(p.x_mm > 1. && p.x_mm < 3. && p.y_mm > 1. && p.y_mm < 3.);
            let q = rotated(p, 37., MmPoint::new(2., 3.));
            assert_eq!(
                out.document
                    .layer_coverage_at(&out.document.layers[0].id, MmPoint::new(q.x_mm, -q.y_mm)),
                Some(expected),
                "{p:?}"
            );
        }
    }
}
#[test]
fn transform_byte_budget_failure_retains_redo() {
    let mut doc = gerber_io::parse_s1(RECT.as_bytes(), "bytes")
        .unwrap()
        .document;
    let template = doc.layers[0].objects[0].clone();
    doc.layers[0].objects = (0..3000)
        .map(|i| {
            let mut o = template.clone();
            o.object_id = format!("{i}{}", "x".repeat(8000));
            o
        })
        .collect();
    let ids: Vec<_> = doc.layers[0]
        .objects
        .iter()
        .map(|o| o.object_id.clone())
        .collect();
    let layer = doc.layers[0].id.clone();
    let mut h = EditHistory::default();
    h.rotate_objects(&mut doc, &layer, &ids[..1], 90., MmPoint::new(0., 0.))
        .unwrap();
    h.undo(&mut doc).unwrap();
    let old = doc.clone();
    assert_eq!(
        h.rotate_objects(&mut doc, &layer, &ids, 37., MmPoint::new(0., 0.)),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(
        h.mirror_objects(
            &mut doc,
            &layer,
            &ids,
            MirrorAxis::Vertical { coordinate_mm: 0. }
        ),
        Err(EditError::ResourceLimit)
    );
    assert_eq!(doc, old);
    assert_eq!(h.undo_len(), 0);
    assert_eq!(h.redo_len(), 1);
}

#[test]
fn asymmetric_macro_orientation_and_local_hole_survive_composed_transform() {
    let src=b"%FSLAX26Y26*%\n%MOMM*%\n%AMASYM*21,1,4,2,1,0,0*1,0,0.5,1,0*%\n%ADD10ASYM*%\nD10*\n%LMX*%\n%LR23*%\n%LS1.5*%\nX10000000Y0D03*\nM02*\n";
    for axis in ["horizontal", "vertical"] {
        let mut r = Run::source(src);
        let (c, t) = flash(&r.objects()[0].geometry);
        r.rotate("0", 37., MmPoint::new(2., 3.));
        r.mirror("1", axis, 4.);
        let out = r.roundtrip("2");
        for ix in -15..=35 {
            for iy in -15..=15 {
                let p = MmPoint::new(ix as f64 * 0.1 + 0.017, iy as f64 * 0.1 + 0.013);
                let expected = p.x_mm >= -1.
                    && p.x_mm <= 3.
                    && p.y_mm.abs() <= 1.
                    && (p.x_mm - 1.).hypot(p.y_mm) > 0.25;
                let q = rotated(local_point(p, t, c), 37., MmPoint::new(2., 3.));
                let q = if axis == "horizontal" {
                    MmPoint::new(q.x_mm, 8. - q.y_mm)
                } else {
                    MmPoint::new(8. - q.x_mm, q.y_mm)
                };
                assert_eq!(
                    out.document
                        .layer_coverage_at(&out.document.layers[0].id, q),
                    Some(expected),
                    "{p:?}"
                );
            }
        }
    }
}
#[test]
fn negative_and_large_finite_angles_normalize_with_canonical_repetition() {
    for a in [-37., 397., 1e12] {
        let mut r = Run::source(RECT.as_bytes());
        r.rotate("0", a, MmPoint::new(0., 0.));
        let (p, t) = flash(&r.objects()[0].geometry);
        assert_eq!(t.rotation_deg, a.rem_euclid(360.));
        near(
            p,
            rotated(
                MmPoint::new(10., 0.),
                a.rem_euclid(360.),
                MmPoint::new(0., 0.),
            ),
        );
    }
    for lm in ["N", "X", "Y", "XY"] {
        let src = RECT.replace("D10*", &format!("D10*\n%LM{lm}*%\n%LR-23*%"));
        let mut r = Run::source(src.as_bytes());
        let (c, t) = flash(&r.objects()[0].geometry);
        r.mirror("0", "vertical", 0.);
        r.mirror("1", "vertical", 0.);
        let (nc, nt) = flash(&r.objects()[0].geometry);
        assert_eq!(nt.rotation_deg, 337.);
        assert_eq!(nt.mirror, t.mirror);
        near(
            local_point(MmPoint::new(1.7, 0.3), nt, nc),
            local_point(MmPoint::new(1.7, 0.3), t, c),
        );
    }
}

#[test]
fn capabilities_publish_only_tested_transform_scope() {
    let mut r = Run::new();
    let caps = r.ok("system.capabilities", None, json!({}));
    for op in ["objects.rotate", "objects.mirror"] {
        assert!(
            caps["supported_operations"]
                .as_array()
                .unwrap()
                .contains(&json!(op))
        );
        assert!(
            !caps["unsupported_operations"]
                .as_array()
                .unwrap()
                .contains(&json!(op))
        );
    }
    assert!(
        caps["unsupported_gerber_features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("RectangularSweep"))
    );
    let mut readonly = ApplicationService::new();
    for op in ["objects.rotate", "objects.mirror"] {
        assert!(
            !readonly
                .capabilities()
                .supported_operations
                .contains(&op.into())
        );
        let response = readonly.execute_json(&json!({"api_version":1,"request_id":"readonly","op":op,"document_id":"missing","expected_revision":"0","params":{}}).to_string());
        assert_eq!(response["error"]["code"], "UNSUPPORTED_OPERATION");
    }
}
