//! Opt-in native surface benchmark. Drives the existing egui gesture/service path.
use crate::{EditorApp, camera::Camera, gpu::PrepareStats, state::Action};
use editor_core::{MmPoint, SemanticGeometry};
use eframe::egui::{self, pos2, vec2};
use egui_wgpu::wgpu;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

pub struct NativeBench {
    out: PathBuf,
    device: wgpu::Device,
    pub painted: Arc<AtomicU64>,
    pub frame_id: u64,
    pending: Option<Pending>,
    records: Vec<Value>,
    releases: Vec<Value>,
    failures: Vec<String>,
    phase: u32,
    since: Instant,
    started: Instant,
    previous_active: Option<(String, Instant)>,
    frame_phase: String,
    frame_focused: bool,
    pan_duration: f64,
    durations: Vec<f64>,
    baseline: Vec<editor_service::ObjectInfo>,
    baseline_bytes: Vec<u8>,
    base_camera: Camera,
    round: usize,
    pressed: bool,
    released: bool,
    drag_start: egui::Pos2,
    last_pointer: egui::Pos2,
    release_at: Option<Instant>,
    expected_delta: MmPoint,
    revision: String,
    undo: usize,
    last_completed_revision: Option<String>,
    last_completed_at: Option<Instant>,
    screenshot: u32,
    stable_size: usize,
}
struct Pending {
    id: u64,
    start: Instant,
    phase: String,
    revision: String,
    record: Value,
}
impl NativeBench {
    pub fn from_env(device: wgpu::Device) -> Option<Self> {
        if std::env::var("RCAM_NATIVE_BENCH").ok().as_deref() != Some("s2b32") {
            return None;
        }
        assert!(!cfg!(debug_assertions), "native benchmark requires release");
        let out =
            PathBuf::from(std::env::var_os("RCAM_BENCH_OUT").expect("RCAM_BENCH_OUT required"));
        std::fs::create_dir_all(&out).expect("benchmark output directory");
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(out.join("native-run.lock"))
            .expect("new run directory required");
        Some(Self {
            out,
            device,
            painted: Arc::new(AtomicU64::new(0)),
            frame_id: 0,
            pending: None,
            records: vec![],
            releases: vec![],
            failures: vec![],
            phase: 0,
            since: Instant::now(),
            started: Instant::now(),
            previous_active: None,
            frame_phase: String::new(),
            frame_focused: false,
            pan_duration: 0.,
            durations: vec![],
            baseline: vec![],
            baseline_bytes: vec![],
            base_camera: Camera::default(),
            round: 0,
            pressed: false,
            released: false,
            drag_start: pos2(0., 0.),
            last_pointer: pos2(0., 0.),
            release_at: None,
            expected_delta: MmPoint::new(0., 0.),
            revision: String::new(),
            undo: 0,
            last_completed_revision: None,
            last_completed_at: None,
            screenshot: 0,
            stable_size: 0,
        })
    }
    fn enter(&mut self, phase: u32) {
        self.phase = phase;
        self.since = Instant::now();
    }
    fn active(&self) -> String {
        match self.phase {
            5 => "pan_zoom".into(),
            9 => format!("drag_{}", self.round),
            _ => String::new(),
        }
    }
    pub fn input(&mut self, app: &EditorApp, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let wait = Instant::now();
        if let Err(e) = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        }) {
            self.failures.push(format!("GPU fence: {e}"));
        }
        let now = Instant::now();
        if let Some(mut pending) = self.pending.take() {
            let painted = self.painted.load(Ordering::Acquire) == pending.id;
            pending.record["painted"] = json!(painted);
            pending.record["previous_surface_gpu_fence_wait_ms"] =
                json!(now.duration_since(wait).as_secs_f64() * 1000.);
            pending.record["update_to_surface_gpu_complete_ms"] =
                json!(now.duration_since(pending.start).as_secs_f64() * 1000.);
            if painted {
                self.last_completed_revision = Some(pending.revision);
                self.last_completed_at = Some(now);
            }
            if !pending.phase.is_empty() {
                if !painted {
                    self.failures
                        .push(format!("active frame {} was not painted", pending.id));
                }
                self.records.push(pending.record);
            }
        }
        self.frame_id += 1;
        self.frame_phase = self.active();
        self.frame_focused = raw.focused;
        if !self.frame_phase.is_empty() && !raw.focused {
            self.failures
                .push("active benchmark window lost focus".into());
        }
        for event in &raw.events {
            if let egui::Event::Screenshot { image, .. } = event {
                self.screenshot += 1;
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                std::fs::write(
                    self.out.join(format!("surface-{}.ppm", self.screenshot)),
                    bytes,
                )
                .expect("screenshot write");
            }
        }
        if self.phase == 8 && !self.pressed {
            self.drag_start = app.camera.screen(MmPoint::new(20., 13.), app.canvas_rect);
            self.last_pointer = self.drag_start;
            raw.events.push(egui::Event::PointerMoved(self.drag_start));
            raw.events.push(egui::Event::PointerButton {
                pos: self.drag_start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            self.pressed = true;
        } else if self.phase == 9 {
            let t = self.since.elapsed().as_secs_f64().min(10.);
            let delta = vec2(
                (1.5 + 0.5 * (t * 0.7).sin()) as f32,
                -(0.8 + 0.3 * (t * 0.9).cos()) as f32,
            ) * app.camera.scale as f32;
            self.last_pointer = self.drag_start + delta;
            raw.events
                .push(egui::Event::PointerMoved(self.last_pointer));
        } else if self.phase == 10 && !self.released {
            self.release_at = Some(Instant::now());
            raw.events
                .push(egui::Event::PointerMoved(self.last_pointer));
            raw.events.push(egui::Event::PointerButton {
                pos: self.last_pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            self.released = true;
        }
    }
    fn save(&self, app: &mut EditorApp, name: &str) {
        app.send(Action::Save(
            self.out.join(name),
            app.view.layers[0].layer_id.clone(),
            None,
        ));
    }
    fn check_metrics(&mut self, app: &EditorApp) {
        let mut area = 0.;
        let mut perimeter = 0.;
        let mut exact = 0;
        for item in &app.view.metrics {
            if let editor_service::MetricValue::Exact {
                area_mm2,
                perimeter_mm,
            } = item.value
            {
                exact += 1;
                area += area_mm2;
                perimeter += perimeter_mm;
            }
        }
        if exact != 1000
            || app.view.metrics.len() != 1000
            || (area - 1000. * std::f64::consts::PI / 16.).abs() > 1e-8
            || (perimeter - 500. * std::f64::consts::PI).abs() > 1e-8
        {
            self.failures.push("1000 metrics invariant".into());
        }
    }
    fn screenshot(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        ctx.request_repaint();
        if self.started.elapsed().as_secs() > 180 {
            self.failures.push("native harness timeout".into());
            self.finish(app, ctx);
            return;
        }
        if app.view.error.is_some() {
            self.failures
                .push(format!("service error {:?}", app.view.error));
            self.finish(app, ctx);
            return;
        }
        if self.phase >= 4 && self.phase <= 12 {
            let physical = app.canvas_rect.size() * ctx.pixels_per_point();
            if (physical.x - 1600.).abs() > 0.1 || (physical.y - 900.).abs() > 0.1 {
                self.failures.push(format!("canvas changed {physical:?}"));
                self.finish(app, ctx);
                return;
            }
        }
        match self.phase {
            0 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                app.send(Action::Open(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../fixtures/synthetic/s2b3_1/P1K_CIRCLES.gbr"),
                ));
                self.enter(1);
            }
            1 if !app.busy && app.view.info.is_some() && app.canvas_rect.is_positive() => {
                if self.since.elapsed().as_secs_f64() < 0.25 {
                    return;
                }
                let ppp = ctx.pixels_per_point();
                let actual = app.canvas_rect.size() * ppp;
                if (actual.x - 1600.).abs() > 0.1 || (actual.y - 900.).abs() > 0.1 {
                    self.stable_size = 0;
                    eprintln!(
                        "BENCH_RESIZE physical={actual:?} inner={:?} ppp={ppp}",
                        ctx.input(|i| i.viewport().inner_rect)
                    );
                    self.since = Instant::now();
                    if let Some(window) = ctx.input(|i| i.viewport().inner_rect) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                            window.size() + (vec2(1600., 900.) - actual) / ppp,
                        ));
                    }
                } else {
                    self.stable_size += 1;
                    if self.stable_size >= 3 {
                        app.fit = true;
                        app.send(Action::SelectRect(
                            app.view.bounds.unwrap(),
                            editor_core::hit_test::SelectRectMode::Window,
                        ));
                        self.enter(2);
                    }
                }
            }
            2 if !app.busy => {
                self.check_metrics(app);
                self.baseline = app.view.selected.ordered.clone();
                self.save(app, "baseline.gbr");
                self.enter(3);
            }
            3 if !app.busy => {
                self.baseline_bytes = std::fs::read(self.out.join("baseline.gbr")).unwrap();
                self.base_camera = app.camera;
                self.write("1000-selection-metrics.json", &json!(app.view.metrics));
                Self::screenshot(ctx);
                self.enter(4);
            }
            4 if self.since.elapsed().as_secs_f64() >= 10. => {
                self.enter(5);
            }
            5 => {
                let t = self.since.elapsed().as_secs_f64();
                app.camera = self.base_camera;
                app.camera.center.x_mm += 2. * (t * 0.7).sin();
                app.camera.center.y_mm += 1.5 * (t * 0.5).sin();
                app.camera.scale *= 1. + 0.15 * (t * 0.4).sin();
                if app.view.selected.ordered != self.baseline
                    || app.view.info.as_ref().unwrap().undo_entries != 0
                {
                    self.failures
                        .push("navigation changed manufacturing".into());
                }
                if t >= 30. {
                    self.pan_duration = t;
                    self.save(app, "after-navigation.gbr");
                    self.enter(6);
                }
            }
            6 if !app.busy => {
                if std::fs::read(self.out.join("after-navigation.gbr")).unwrap()
                    != self.baseline_bytes
                {
                    self.failures.push("navigation writer bytes changed".into());
                }
                app.camera = self.base_camera;
                self.enter(7);
            }
            7 if !app.busy => {
                Self::screenshot(ctx);
                self.pressed = false;
                self.released = false;
                self.enter(8);
            }
            8 if !app.busy
                && app
                    .drag
                    .as_ref()
                    .is_some_and(|d| d.confirmed && !d.box_select) =>
            {
                let info = app.view.info.as_ref().unwrap();
                self.revision = info.revision.clone();
                self.undo = info.undo_entries;
                self.enter(9);
            }
            9 => {
                let info = app.view.info.as_ref().unwrap();
                if info.revision != self.revision
                    || info.undo_entries != self.undo
                    || app.view.selected.ordered != self.baseline
                {
                    self.failures
                        .push("preview changed manufacturing/history".into());
                }
                if app.drag.is_none() {
                    self.failures.push("preview gesture lost".into());
                    self.finish(app, ctx);
                    return;
                }
                if self.since.elapsed().as_secs_f64() >= 10. {
                    self.durations.push(self.since.elapsed().as_secs_f64());
                    self.expected_delta = app.camera.world(self.last_pointer, app.canvas_rect);
                    let start = app.camera.world(self.drag_start, app.canvas_rect);
                    self.expected_delta = MmPoint::new(
                        self.expected_delta.x_mm - start.x_mm,
                        self.expected_delta.y_mm - start.y_mm,
                    );
                    self.enter(10);
                }
            }
            10 if self.released && !app.busy => {
                let info = app.view.info.as_ref().unwrap();
                if info.revision != self.revision
                    && self.last_completed_revision.as_ref() == Some(&info.revision)
                {
                    let latency = self
                        .last_completed_at
                        .unwrap()
                        .duration_since(self.release_at.unwrap())
                        .as_secs_f64()
                        * 1000.;
                    let mut coords = true;
                    for (a, b) in self.baseline.iter().zip(&app.view.selected.ordered) {
                        if let (
                            SemanticGeometry::Flash { center: a, .. },
                            SemanticGeometry::Flash { center: b, .. },
                        ) = (&a.object.geometry, &b.object.geometry)
                        {
                            coords &= (b.x_mm - a.x_mm - self.expected_delta.x_mm).abs() < 1e-8
                                && (b.y_mm - a.y_mm - self.expected_delta.y_mm).abs() < 1e-8;
                        } else {
                            coords = false;
                        }
                    }
                    let history = info.undo_entries == self.undo + 1;
                    self.releases.push(json!({"round":self.round,"release_to_final_surface_gpu_complete_ms":latency,"one_undo":history,"all_coordinates_correct":coords,"revision":info.revision,"delta_mm":[self.expected_delta.x_mm,self.expected_delta.y_mm]}));
                    if !history || !coords {
                        self.failures.push("release transaction/coordinates".into());
                    }
                    self.check_metrics(app);
                    Self::screenshot(ctx);
                    self.enter(11);
                }
            }
            11 if self.since.elapsed().as_secs_f64() > 0.5 => {
                app.send(Action::History(false));
                self.enter(12);
            }
            12 if !app.busy => {
                if app.view.selected.ordered != self.baseline
                    || app.view.info.as_ref().unwrap().undo_entries != self.undo
                {
                    self.failures
                        .push("Undo did not restore geometry/history".into());
                }
                self.save(app, &format!("after-undo-{}.gbr", self.round));
                self.enter(13);
            }
            13 if !app.busy => {
                if std::fs::read(self.out.join(format!("after-undo-{}.gbr", self.round))).unwrap()
                    != self.baseline_bytes
                {
                    self.failures.push("Undo writer bytes changed".into());
                }
                self.round += 1;
                if self.round == 3 {
                    self.enter(14);
                    Self::screenshot(ctx);
                } else {
                    self.enter(7);
                }
            }
            14 if self.since.elapsed().as_secs_f64() > 0.5 => self.finish(app, ctx),
            _ => {}
        }
    }
    pub fn record(
        &mut self,
        app: &EditorApp,
        stats: &PrepareStats,
        ppp: f32,
        frame_start: Instant,
    ) {
        let phase = self.frame_phase.clone();
        let interval = self
            .previous_active
            .as_ref()
            .filter(|(p, _)| p == &phase && !phase.is_empty())
            .map(|(_, t)| frame_start.duration_since(*t).as_secs_f64() * 1000.);
        self.previous_active = Some((phase.clone(), frame_start));
        let revision = app
            .view
            .info
            .as_ref()
            .map_or(String::new(), |i| i.revision.clone());
        self.pending = Some(Pending {
            id: self.frame_id,
            start: frame_start,
            phase: phase.clone(),
            revision: revision.clone(),
            record: json!({"frame":self.frame_id,"focused":self.frame_focused,"phase":phase,"revision":revision,"frame_interval_ms":interval,"canvas_physical":[app.canvas_rect.width()*ppp,app.canvas_rect.height()*ppp],"cpu_prepare_ms":stats.cpu_prepare_ms,"preview_index_ms":stats.preview_index_ms,"candidate_count":stats.candidate_count,"object_visits":stats.object_visits,"cell_references_visited":stats.cell_references_visited,"max_candidates_in_view":stats.max_candidates_in_view,"scene_total":app.view.scene.as_ref().map_or(0,|s|s.objects.len())}),
        });
    }
    pub fn ensure_record(&mut self, app: &EditorApp, ppp: f32, start: Instant) {
        if !self.frame_phase.is_empty()
            && self.pending.as_ref().is_none_or(|p| p.id != self.frame_id)
        {
            self.failures.push(format!(
                "no production callback for active frame {}: {:?}",
                self.frame_id, app.display_error
            ));
            self.record(app, &PrepareStats::default(), ppp, start);
        }
    }
    fn write(&self, name: &str, value: &Value) {
        std::fs::write(
            self.out.join(name),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
    fn finish(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        if self.phase == 99 {
            return;
        }
        let pan: Vec<_> = self
            .records
            .iter()
            .filter(|r| r["phase"] == "pan_zoom")
            .cloned()
            .collect();
        let mut drags = vec![];
        for round in 0..3 {
            let frames: Vec<_> = self
                .records
                .iter()
                .filter(|r| r["phase"] == format!("drag_{round}"))
                .cloned()
                .collect();
            let mut intervals: Vec<_> = frames
                .iter()
                .filter_map(|f| f["frame_interval_ms"].as_f64())
                .collect();
            intervals.sort_by(f64::total_cmp);
            let p95 = percentile(&intervals, 0.95);
            if p95.is_none_or(|v| v > 50.) {
                self.failures
                    .push(format!("drag {round} frame p95 {p95:?}"));
            }
            drags.push(json!({"round":round,"p95_frame_interval_ms":p95,"active_duration_seconds":self.durations.get(round),"frames":frames}));
        }
        if self.durations.len() != 3 || self.durations.iter().any(|d| *d < 10.) {
            self.failures
                .push("three full 10s intervals required".into());
        }
        if self.releases.len() != 3
            || self.releases.iter().any(|r| {
                r["release_to_final_surface_gpu_complete_ms"]
                    .as_f64()
                    .unwrap()
                    > 300.
            })
        {
            self.failures.push("release <=300ms gate".into());
        }
        self.write(
            "native-pan-zoom.json",
            &json!({"schema_version":2,"duration_seconds":30,"active_duration_seconds":self.pan_duration,"frames":pan}),
        );
        self.write(
            "native-drag-3x10s.json",
            &json!({"schema_version":2,"rounds":drags}),
        );
        self.write("release-latency.json",&json!({"schema_version":2,"definition":"release raw input to target revision surface GPU complete after present call; conservative, not display scanout","rounds":self.releases}));
        self.write("native-results.json",&json!({"schema_version":2,"benchmark":"s2b32","status":if self.failures.is_empty(){"PASS"}else{"FAIL"},"failures":self.failures,"elapsed_seconds":self.started.elapsed().as_secs_f64(),"surface_screenshots":self.screenshot}));
        eprintln!(
            "NATIVE_BENCH_RESULT {} {:?}",
            if self.failures.is_empty() {
                "PASS"
            } else {
                "FAIL"
            },
            self.failures
        );
        self.phase = 99;
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
pub fn percentile(sorted: &[f64], q: f64) -> Option<f64> {
    (!sorted.is_empty()).then(|| sorted[(sorted.len() as f64 * q).ceil().max(1.) as usize - 1])
}
#[cfg(test)]
mod tests {
    #[test]
    fn percentile_keeps_slow_tail() {
        assert_eq!(super::percentile(&[1., 2., 3., 100.], 0.95), Some(100.));
        assert_eq!(super::percentile(&[], 0.95), None);
    }
}
