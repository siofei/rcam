//! Opt-in native surface benchmark. Drives the existing egui gesture/service path.
use crate::{
    EditorApp,
    camera::Camera,
    gpu::PrepareStats,
    state::{Action, MirrorDirection, PivotInput},
};
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
    tools_gate: bool,
    navigation_shot: bool,
    tool_input_phase: Option<u32>,
    tool_baseline: Option<serde_json::Value>,
    tools_records: Vec<Value>,
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
        if !cfg!(feature = "internal-evidence") {
            return None;
        }

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
            tools_gate: std::env::var_os("RCAM_S2C1_GATE").is_some(),
            navigation_shot: false,
            tool_input_phase: None,
            tool_baseline: None,
            tools_records: vec![],
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
        if self.tools_gate
            && self.tool_input_phase != Some(self.phase)
            && ((20..=28).contains(&self.phase) || self.phase == 221)
            && !app.busy
        {
            let point = match self.phase {
                20 => Some(MmPoint::new(20., 13.)),
                21 | 22 => Some(MmPoint::new(23., 17.)),
                26 => Some(MmPoint::new(20., 13.)),
                27 | 28 => Some(MmPoint::new(21.23, 13.77)),
                _ => None,
            };
            if let Some(point) = point {
                let pos = app.camera.screen(point, app.canvas_rect);
                raw.events.push(egui::Event::PointerMoved(pos));
                if matches!(self.phase, 20 | 22 | 26 | 28) {
                    raw.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: self.phase != 28,
                        modifiers: egui::Modifiers::NONE,
                    });
                    if matches!(self.phase, 20 | 22) {
                        raw.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            }
            if self.phase == 23 || self.phase == 221 {
                raw.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
                raw.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            self.tool_input_phase = Some(self.phase);
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
                if app.view.selected.ordered.len() != 1000 {
                    self.failures
                        .push("Window selection did not select P1K".into());
                }
                self.tools_records.push(json!({"phase":"window_selection","selected":app.view.selected.ordered.len(),"info":app.view.info}));
                app.send(Action::SelectRect(
                    app.view.bounds.unwrap(),
                    editor_core::hit_test::SelectRectMode::Crossing,
                ));
                self.enter(200);
            }
            200 if !app.busy => {
                if app.view.selected.ordered.len() != 1000 {
                    self.failures
                        .push("Crossing selection did not select P1K".into());
                }
                self.tools_records.push(json!({"phase":"crossing_selection","selected":app.view.selected.ordered.len(),"info":app.view.info}));
                self.baseline = app.view.selected.ordered.to_vec();
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
                if self.tools_gate {
                    app.grid.visible = true;
                    app.grid.spacing_mm = 0.5;
                    app.spacing = "0.5".into();
                    Self::screenshot(ctx);
                }
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
                if self.tools_gate && t >= 15. && !self.navigation_shot {
                    Self::screenshot(ctx);
                    self.navigation_shot = true;
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
                if self.tools_gate {
                    app.grid.snap_enabled = self.round > 0;
                }
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
                    self.expected_delta = app
                        .grid
                        .point(app.camera.world(self.last_pointer, app.canvas_rect))
                        .unwrap();
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
            14 if self.since.elapsed().as_secs_f64() > 0.5 => {
                if self.tools_gate {
                    app.tool = crate::tools::ActiveTool::Measure;
                    self.tool_baseline =
                        Some(serde_json::to_value(app.view.info.as_ref().unwrap()).unwrap());
                    self.enter(20);
                } else {
                    self.finish(app, ctx);
                }
            }
            20..=23 if self.tools_gate && self.since.elapsed().as_secs_f64() > 0.5 => {
                let info = serde_json::to_value(app.view.info.as_ref().unwrap()).unwrap();
                if Some(&info) != self.tool_baseline.as_ref() {
                    self.failures.push("Measure changed document info".into());
                }
                match self.phase {
                    20 if app.measure.a != Some(MmPoint::new(20., 13.)) => {
                        self.failures.push("measure A".into())
                    }
                    21 | 22 if app.measure.values() != Some((3., 4., 5.)) => {
                        self.failures.push("measure 3-4-5".into())
                    }
                    21 | 22
                        if app
                            .measure
                            .angle_deg()
                            .is_none_or(|angle| (angle - 53.130_102_354_155_98).abs() > 1e-12) =>
                    {
                        self.failures.push("measure angle".into())
                    }
                    21 if app.measure.fixed => self.failures.push("dynamic B fixed early".into()),
                    22 if !app.measure.fixed || app.measure.completed.len() != 1 => self
                        .failures
                        .push("fixed/retained measurement missing".into()),
                    23 if app.measure.a.is_some() || !app.measure.completed.is_empty() => {
                        self.failures.push("Escape did not clear ruler".into())
                    }
                    _ => {}
                }
                self.tools_records.push(json!({"phase":self.phase,"measure":app.measure.label(),"values":app.measure.values(),"angle_deg":app.measure.angle_deg(),"retained":app.measure.completed.len(),"fixed":app.measure.fixed,"info":info,"ppp":ctx.pixels_per_point(),"canvas_physical":[app.canvas_rect.width()*ctx.pixels_per_point(),app.canvas_rect.height()*ctx.pixels_per_point()]}));
                Self::screenshot(ctx);
                self.enter(if self.phase == 22 {
                    220
                } else {
                    self.phase + 1
                });
            }
            220 if self.tools_gate => {
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("grid-spacing")));
                if self.since.elapsed().as_secs_f64() > 0.5 {
                    self.enter(221);
                }
            }
            221 if self.tools_gate && self.since.elapsed().as_secs_f64() > 0.5 => {
                let preserved = app.measure.fixed
                    && app.measure.values() == Some((3., 4., 5.))
                    && app.measure.completed.len() == 1;
                if !preserved {
                    self.failures
                        .push("text-focused Escape cleared measurement".into());
                }
                self.tools_records.push(json!({"phase":221,"text_escape_preserved_measure":preserved,"info":app.view.info}));
                Self::screenshot(ctx);
                self.enter(23);
            }
            24 if self.tools_gate => {
                self.save(app, "after-tools.gbr");
                app.tool = crate::tools::ActiveTool::Select;
                self.enter(25);
            }
            25 if self.tools_gate && !app.busy => {
                if std::fs::read(self.out.join("after-tools.gbr")).unwrap() != self.baseline_bytes {
                    self.failures.push("tools changed writer bytes".into());
                }
                app.send(Action::Select(
                    MmPoint::new(20., 13.),
                    0.,
                    crate::selection::SelectionMode::Replace,
                ));
                self.enter(26);
            }
            26 if self.tools_gate
                && !app.busy
                && app.drag.as_ref().is_some_and(|d| d.confirmed) =>
            {
                self.revision = app.view.info.as_ref().unwrap().revision.clone();
                self.undo = app.view.info.as_ref().unwrap().undo_entries;
                self.enter(27);
            }
            27 if self.tools_gate && self.since.elapsed().as_secs_f64() > 0.5 => {
                if app.view.info.as_ref().unwrap().revision != self.revision
                    || app.view.info.as_ref().unwrap().undo_entries != self.undo
                {
                    self.failures.push("single preview mutated document".into());
                }
                Self::screenshot(ctx);
                self.enter(28);
            }
            28 if self.tools_gate && !app.busy && self.since.elapsed().as_secs_f64() > 0.5 => {
                let info = app.view.info.as_ref().unwrap();
                let coords = app.view.selected.ordered.len() == 1
                    && matches!(app.view.selected.primary().unwrap().object.geometry,SemanticGeometry::Flash{center,..} if center.distance_mm(MmPoint::new(21.,14.))<1e-6);
                if !coords
                    || info.undo_entries != self.undo + 1
                    || info.revision.parse::<u64>().unwrap()
                        != self.revision.parse::<u64>().unwrap() + 1
                {
                    self.failures
                        .push("single snap commit coordinates/history".into());
                }
                self.tools_records.push(json!({"phase":28,"coordinates_correct":coords,"info":info,"selected":app.view.selected.ordered}));
                Self::screenshot(ctx);
                self.save(app, "single-snapped.gbr");
                self.enter(29);
            }
            29 if self.tools_gate && !app.busy => {
                app.send(Action::History(false));
                self.enter(30);
            }
            30 if self.tools_gate && !app.busy => {
                if !matches!(app.view.selected.primary().unwrap().object.geometry,SemanticGeometry::Flash{center,..} if center==MmPoint::new(20.,13.))
                {
                    self.failures.push("single Undo".into());
                }
                Self::screenshot(ctx);
                app.send(Action::Close(true));
                self.enter(31);
            }
            31 if self.tools_gate && !app.busy => {
                app.send(Action::Open(self.out.join("single-snapped.gbr")));
                self.enter(32);
            }
            32 if self.tools_gate && !app.busy && !app.fit => {
                app.send(Action::Select(
                    MmPoint::new(21., 14.),
                    0.,
                    crate::selection::SelectionMode::Replace,
                ));
                self.enter(33);
            }
            33 if self.tools_gate && !app.busy => {
                let coords = app.view.selected.ordered.len() == 1
                    && matches!(app.view.selected.primary().unwrap().object.geometry,SemanticGeometry::Flash{center,..} if center==MmPoint::new(21.,14.));
                if !coords || app.measure.a.is_some() || !app.measure.completed.is_empty() {
                    self.failures.push("reopen coordinate/measure reset".into());
                }
                self.tools_records.push(
                    json!({"phase":33,"reopen_coordinates_correct":coords,"info":app.view.info}),
                );
                Self::screenshot(ctx);
                self.enter(34);
            }
            34 if self.tools_gate && self.since.elapsed().as_secs_f64() > 0.5 => {
                app.send(Action::Duplicate);
                self.enter(35);
            }
            35 if self.tools_gate && !app.busy => {
                let total = app
                    .view
                    .scene
                    .as_ref()
                    .map_or(0, |scene| scene.objects.len());
                if total != 1001 || app.view.selected.ordered.len() != 1 {
                    self.failures.push("native Duplicate state".into());
                }
                self.tools_records.push(json!({"phase":"duplicate","scene_total":total,"selected":app.view.selected.ordered,"info":app.view.info}));
                app.send(Action::Delete);
                self.enter(36);
            }
            36 if self.tools_gate && !app.busy => {
                let total = app
                    .view
                    .scene
                    .as_ref()
                    .map_or(0, |scene| scene.objects.len());
                if total != 1000 || !app.view.selected.ordered.is_empty() {
                    self.failures.push("native Delete state".into());
                }
                self.tools_records
                    .push(json!({"phase":"delete","scene_total":total,"info":app.view.info}));
                app.send(Action::History(false));
                self.enter(37);
            }
            37 if self.tools_gate && !app.busy => {
                let total = app
                    .view
                    .scene
                    .as_ref()
                    .map_or(0, |scene| scene.objects.len());
                if total != 1001 {
                    self.failures.push("native Delete Undo state".into());
                }
                app.send(Action::History(true));
                self.enter(38);
            }
            38 if self.tools_gate && !app.busy => {
                let total = app
                    .view
                    .scene
                    .as_ref()
                    .map_or(0, |scene| scene.objects.len());
                if total != 1000 {
                    self.failures.push("native Delete Redo state".into());
                }
                self.tools_records.push(
                    json!({"phase":"delete_undo_redo","scene_total":total,"info":app.view.info}),
                );
                app.send(Action::Select(
                    MmPoint::new(21., 14.),
                    0.,
                    crate::selection::SelectionMode::Replace,
                ));
                self.enter(39);
            }
            39 if self.tools_gate && !app.busy => {
                if app.view.selected.ordered.len() != 1 {
                    self.failures.push("native transform selection".into());
                    self.finish(app, ctx);
                    return;
                }
                app.send(Action::Rotate("37".into(), PivotInput::WorldOrigin));
                self.enter(40);
            }
            40 if self.tools_gate && !app.busy => {
                let (sine, cosine) = 37_f64.to_radians().sin_cos();
                let expected = MmPoint::new(21. * cosine - 14. * sine, 21. * sine + 14. * cosine);
                let correct = matches!(app.view.selected.primary().map(|o| &o.object.geometry),Some(SemanticGeometry::Flash{center,..}) if center.distance_mm(expected)<1e-8);
                if !correct {
                    self.failures.push("native Rotate coordinates".into());
                }
                self.tools_records.push(json!({"phase":"rotate","coordinates_correct":correct,"expected_center_mm":[expected.x_mm,expected.y_mm],"selected":app.view.selected.ordered,"info":app.view.info}));
                app.send(Action::Mirror(MirrorDirection::Horizontal));
                self.enter(41);
            }
            41 if self.tools_gate && !app.busy => {
                let (sine, cosine) = 37_f64.to_radians().sin_cos();
                let expected = MmPoint::new(21. * cosine - 14. * sine, 21. * sine + 14. * cosine);
                let correct = matches!(app.view.selected.primary().map(|o| &o.object.geometry),Some(SemanticGeometry::Flash{center,..}) if center.distance_mm(expected)<1e-8);
                if !correct {
                    self.failures.push("native Mirror coordinates".into());
                }
                self.tools_records.push(json!({"phase":"mirror","coordinates_correct":correct,"selected":app.view.selected.ordered,"info":app.view.info}));
                app.send(Action::SetFlashSize("0.75".into(), None));
                self.enter(42);
            }
            42 if self.tools_gate && !app.busy => {
                let selected_aperture =
                    match app.view.selected.primary().map(|o| &o.object.geometry) {
                        Some(SemanticGeometry::Flash { aperture_id, .. }) => Some(aperture_id),
                        _ => None,
                    };
                let cow = selected_aperture.is_some_and(|id| {
                    app.view.apertures.iter().any(|aperture| {
                        aperture.id == *id
                            && matches!(aperture.shape, editor_core::ApertureShape::Circle { diameter_mm, .. } if diameter_mm == 0.75)
                    })
                }) && app.view.apertures.iter().any(|aperture| {
                    matches!(aperture.shape, editor_core::ApertureShape::Circle { diameter_mm, .. } if diameter_mm == 0.5)
                });
                if !cow {
                    self.failures.push("native Flash size COW".into());
                }
                self.tools_records.push(json!({"phase":"flash_size_cow","cow_correct":cow,"apertures":app.view.apertures,"selected":app.view.selected.ordered,"info":app.view.info}));
                self.save(app, "s3-final.gbr");
                self.enter(43);
            }
            43 if self.tools_gate && !app.busy => {
                app.send(Action::Close(true));
                self.enter(44);
            }
            44 if self.tools_gate && !app.busy => {
                app.send(Action::Open(self.out.join("s3-final.gbr")));
                self.enter(45);
            }
            45 if self.tools_gate && !app.busy && !app.fit => {
                let (sine, cosine) = 37_f64.to_radians().sin_cos();
                let expected = MmPoint::new(21. * cosine - 14. * sine, 21. * sine + 14. * cosine);
                app.send(Action::Select(
                    expected,
                    1e-6,
                    crate::selection::SelectionMode::Replace,
                ));
                self.enter(46);
            }
            46 if self.tools_gate && !app.busy => {
                let selected_aperture =
                    match app.view.selected.primary().map(|o| &o.object.geometry) {
                        Some(SemanticGeometry::Flash { aperture_id, .. }) => Some(aperture_id),
                        _ => None,
                    };
                let reopened = selected_aperture.is_some_and(|id| {
                    app.view.apertures.iter().any(|aperture| {
                        aperture.id == *id
                            && matches!(aperture.shape, editor_core::ApertureShape::Circle { diameter_mm, .. } if (diameter_mm - 0.75).abs() < 1e-9)
                    })
                });
                if !reopened {
                    self.failures
                        .push("native Save As/Reopen COW geometry".into());
                }
                self.tools_records.push(json!({"phase":"s3_save_reopen","reopened_geometry_correct":reopened,"selected":app.view.selected.ordered,"info":app.view.info}));
                Self::screenshot(ctx);
                self.finish(app, ctx)
            }
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
        if self.tools_gate {
            self.write("s2c1-tools.json", &json!({"schema_version":2,"status":if self.failures.is_empty(){"PASS"}else{"FAIL"},"records":self.tools_records,"note":"native egui pointer events; toolbar state configured by harness; rounds 0 snap OFF, 1/2 ON; grid ON during navigation"}));
        }
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
