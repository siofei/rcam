//! Internal-only, synthetic native observations. Uses EditorApp's normal input,
//! worker, viewport and production callback. Never substitutes query results.
use crate::{
    EditorApp,
    camera::Camera,
    gpu::PrepareStats,
    state::{Action, View},
};
use editor_core::{MmPoint, hash::sha256_hex};
use eframe::egui::{self, vec2};
use egui_wgpu::wgpu;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static COUNTERS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());
static WORKER: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static HIT_MS: Mutex<Vec<f64>> = Mutex::new(Vec::new());
pub fn directory() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("RCAM_S5M1_NATIVE_DIR")?).ok()?;
    (dir.parent() == Some(Path::new("/private/tmp"))
        && dir.file_name()?.to_str()?.starts_with("rcam-s5m1-native-"))
    .then_some(dir)
}
pub fn activate_counters() {
    ACTIVE.store(true, Ordering::Relaxed);
}
pub fn counter_snapshot() -> Value {
    counters()
}
pub fn worker_snapshot() -> Value {
    json!(*WORKER.lock().unwrap())
}
pub fn gpu_event(label: &'static str, n: u64) {
    if ACTIVE.load(Ordering::Relaxed) {
        *COUNTERS.lock().unwrap().entry(label).or_default() += n;
    }
}
pub fn gpu_bytes(n: u64) {
    if ACTIVE.load(Ordering::Relaxed) {
        let mut c = COUNTERS.lock().unwrap();
        c.insert("custom-buffer-live-bytes", n);
        let peak = c
            .get("custom-buffer-largest-observed-bytes")
            .copied()
            .unwrap_or(0)
            .max(n);
        c.insert("custom-buffer-largest-observed-bytes", peak);
    }
}
pub fn gpu_device_allocation(device: &wgpu::Device) {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: read-only query under wgpu's live HAL guard and Metal device
        // mutex; no handle escapes and no resources/device are mutated.
        if let Some(hal) = unsafe { device.as_hal::<wgpu::hal::api::Metal>() } {
            let n = hal.raw_device().lock().current_allocated_size();
            let mut c = COUNTERS.lock().unwrap();
            c.insert("metal-device-current-allocated-bytes", n);
            let peak = c
                .get("metal-device-max-observed-allocated-bytes")
                .copied()
                .unwrap_or(0)
                .max(n);
            c.insert("metal-device-max-observed-allocated-bytes", peak);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = device;
}
pub fn count_rebuild(patch: bool) {
    gpu_event(
        if patch {
            "geometry-patch-attempt"
        } else {
            "geometry-full-build-attempt"
        },
        1,
    );
}
pub struct HitTimer(Option<Instant>);
impl HitTimer {
    pub fn start() -> Self {
        Self(ACTIVE.load(Ordering::Relaxed).then(Instant::now))
    }
}
impl Drop for HitTimer {
    fn drop(&mut self) {
        if let Some(t) = self.0 {
            HIT_MS
                .lock()
                .unwrap()
                .push(t.elapsed().as_secs_f64() * 1000.);
        }
    }
}
pub fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Open(_) => "open",
        Action::DiscardNewWorkspace => "new-project",
        Action::Viewport(..) => "viewport",
        Action::ProbeDrag(..) => "probe-drag",
        Action::Select(..) | Action::CanvasSelect(..) => "select",
        Action::SelectRect(..) => "select-rect",
        Action::Close(..) => "close",
        Action::Move(..) => "move",
        Action::DragMove(..) => "drag-move",
        Action::History(false) => "undo",
        Action::History(true) => "redo",
        Action::Save(..) => "export",
        _ => "other",
    }
}
pub fn worker_result(id: u64, label: &str, start: Instant, view: &View) {
    if ACTIVE.load(Ordering::Relaxed) {
        gpu_event(
            if label == "open" {
                "file-open-parse"
            } else {
                "worker-action"
            },
            1,
        );
        WORKER.lock().unwrap().push(json!({"sequence":id,"action":label,"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"selected_count":view.selected.ordered.len(),"scene_count":view.scene.as_ref().map(|s|s.ids.len()),"error":view.error.as_ref().map(|e|&e.code),"batch":crate::native_batch_drag::worker_observation(start, view)}));
    }
}
fn counters() -> Value {
    json!(*COUNTERS.lock().unwrap())
}
fn state(app: &EditorApp) -> Value {
    let info = app.view.info.as_ref();
    json!({"document_id":info.map(|i|&i.document_id),"revision":info.map(|i|&i.revision),"workspace_revision":info.map(|i|&i.workspace_revision),"dirty":info.map(|i|i.dirty),"project_dirty":info.map(|i|i.project_dirty),"undo":info.map(|i|i.undo_entries),"redo":info.map(|i|i.redo_entries),"scene_count":app.view.scene.as_ref().map_or(0,|s|s.ids.len()),"manufacturing_count":app.view.snap_snapshot.as_ref().map_or(0,|s|s.layers.iter().map(|l|l.objects.len()).sum::<usize>()),"scene_serial":app.view.scene.as_ref().map(|s|s.serial),"selected_count":app.view.selected.ordered.len(),"selected_primary":app.view.selected.primary().map(|o|&o.object.object_id),"busy":app.busy,"display_pending":app.display_pending,"blocked":app.view.blocked,"display_error":app.display_error,"error":app.view.error.as_ref().map(|e|&e.code),"ui_error":app.ui_error,"camera_center":app.camera.center,"camera_scale":app.camera.scale,"canvas_physical":[app.canvas_rect.width()*app.reported_ppp,app.canvas_rect.height()*app.reported_ppp]})
}
struct Pending {
    id: u64,
    observed: Value,
    at: Instant,
}
pub struct Run {
    dir: PathBuf,
    mode: String,
    fixture: String,
    expected: usize,
    path: PathBuf,
    device: wgpu::Device,
    pub painted: Arc<AtomicU64>,
    pub frame_id: u64,
    phase: u32,
    since: Instant,
    started: Instant,
    stable: u32,
    round: u32,
    pending: Option<Pending>,
    complete: Option<Value>,
    previous_nav: Option<Instant>,
    frame_interval: Option<f64>,
    frame_input: Instant,
    point_sequence: u64,
    point_before: Value,
    focused: bool,
    frames: Vec<Value>,
    records: Vec<Value>,
    failures: Vec<String>,
    prepared: Option<Value>,
    base: Camera,
    baseline: Value,
    open_at: Instant,
    point: usize,
    input_at: Option<Instant>,
    point_step: u32,
    marquee_step: u32,
    wake: Arc<AtomicBool>,
    screenshots: u32,
    original_geometry: String,
    moved_geometry: String,
    wake_confirmation_requested: bool,
    single_geometry: String,
}
impl Run {
    pub fn from_env(device: wgpu::Device) -> Option<Self> {
        let dir = directory()?;
        let request: Value =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).ok()?).ok()?;
        let mode = request["mode"].as_str()?.to_owned();
        if !["nav", "load", "select", "idle", "lifecycle"].contains(&mode.as_str()) {
            return None;
        }
        let fixture = request["fixture"].as_str()?.to_owned();
        let (bytes, expected) = match fixture.as_str() {
            "P10K" => (
                include_bytes!("../../../fixtures/synthetic/s5m1/P10K_CROP.gbr").as_slice(),
                10000,
            ),
            "P100K" => (
                include_bytes!("../../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr").as_slice(),
                100000,
            ),
            _ => return None,
        };
        assert!(!cfg!(debug_assertions), "native evidence requires release");
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join("native-run.lock"))
            .ok()?;
        let path = dir.join(format!("{fixture}.gbr"));
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .ok()?
            .write_all(bytes)
            .ok()?;
        ACTIVE.store(true, Ordering::Relaxed);
        Some(Self {
            dir,
            mode,
            fixture,
            expected,
            path,
            device,
            painted: Arc::new(AtomicU64::new(0)),
            frame_id: 0,
            phase: 0,
            since: Instant::now(),
            started: Instant::now(),
            stable: 0,
            round: 0,
            pending: None,
            complete: None,
            previous_nav: None,
            frame_interval: None,
            frame_input: Instant::now(),
            point_sequence: 0,
            point_before: Value::Null,
            focused: true,
            frames: vec![],
            records: vec![],
            failures: vec![],
            prepared: None,
            base: Camera::default(),
            baseline: Value::Null,
            open_at: Instant::now(),
            point: 0,
            input_at: None,
            point_step: 0,
            marquee_step: 0,
            wake: Arc::new(AtomicBool::new(false)),
            screenshots: 0,
            original_geometry: String::new(),
            moved_geometry: String::new(),
            wake_confirmation_requested: false,
            single_geometry: String::new(),
        })
    }
    fn enter(&mut self, phase: u32) {
        self.phase = phase;
        self.since = Instant::now();
        self.complete = None;
        self.point_step = 0;
    }
    #[cfg(target_os = "macos")]
    fn metal_allocation_bytes(&self) -> Option<u64> {
        // SAFETY: read-only query of the same live Metal device held by eframe.
        // The HAL guard and device mutex outlive this call; no resource mutation.
        let hal = unsafe { self.device.as_hal::<wgpu::hal::api::Metal>() }?;
        Some(hal.raw_device().lock().current_allocated_size())
    }
    #[cfg(not(target_os = "macos"))]
    fn metal_allocation_bytes(&self) -> Option<u64> {
        None
    }
    fn record(&mut self, app: &EditorApp, label: &str) {
        let at = self.started.elapsed();
        self.records.push(json!({"label":label,"frame_id":self.frame_id,"input_monotonic_ns":self.frame_input.duration_since(self.started).as_nanos() as u64,"monotonic_ns":at.as_nanos() as u64,"phase_origin_ns":self.since.duration_since(self.started).as_nanos() as u64,"elapsed_seconds":at.as_secs_f64(),"wall_time_ns":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos().to_string(),"round":self.round,"state":state(app),"completed_frame":self.complete,"counters":counters(),"metal_device_current_allocated_bytes":self.metal_allocation_bytes()}));
        self.progress();
    }
    fn completed_current(&self, app: &EditorApp) -> bool {
        let current = state(app);
        self.complete.as_ref().is_some_and(|frame| {
            [
                "document_id",
                "revision",
                "scene_serial",
                "selected_count",
                "selected_primary",
            ]
            .iter()
            .all(|key| frame[*key] == current[*key])
                && frame["display_pending"] == false
                && frame["focused"] == true
        })
    }
    fn progress(&self) {
        std::fs::write(self.dir.join("native-progress.json"),serde_json::to_vec_pretty(&json!({"phase":self.phase,"mode":self.mode,"point":self.point,"round":self.round,"records":self.records,"failures":self.failures})).unwrap()).unwrap();
    }
    pub fn prepare(&mut self, stats: &PrepareStats) {
        self.prepared = Some(
            json!({"cpu_prepare_ms":stats.cpu_prepare_ms,"object_visits":stats.object_visits,"candidate_count":stats.candidate_count,"cell_references_visited":stats.cell_references_visited,"preview_index_ms":stats.preview_index_ms}),
        );
    }
    fn finish(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        self.record(app, "finished");
        let exe = std::env::current_exe().unwrap();
        let report = json!({"schema_version":2,"stage":"S5-M1","status":"OBSERVED","mode":self.mode,"fixture":self.fixture,"commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":sha256_hex(&std::fs::read(exe).unwrap()),"fixture_sha256":sha256_hex(&std::fs::read(&self.path).unwrap()),"protocol_sha256":sha256_hex(include_bytes!("../../../fixtures/synthetic/s5m1/protocol.json")),"adapter":app.adapter,"evidence_kind":"controlled synthetic egui RawInput -> EditorApp -> real worker/ApplicationService -> production Metal callback; app frame intervals, no scanout/physical-input claim","load_origin":"before normal Action::Open enqueue (conservative upper bound including access checks, actual file read/parse, display build/upload and complete production GPU frame); OS page cache uncontrolled","records":self.records,"frames":self.frames,"worker":*WORKER.lock().unwrap(),"exact_hit_cpu_ms":*HIT_MS.lock().unwrap(),"failures":self.failures,"counters":counters(),"gpu_counter_scope":"actual sizes of six custom production buffers; largest live observation, not whole-process GPU peak; egui/surface textures and transient overlap require separate budget evidence"});
        std::fs::write(
            self.dir.join("native-observations.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        self.phase = 100;
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn input(&mut self, app: &EditorApp, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let now = Instant::now();
        // One monotonic input clock also links the previous paint observation
        // to this next input; navigation intervals never use wall time.
        self.frame_input = now;
        self.focused = raw.focused;
        self.complete = None;
        if let Some(pending) = self.pending.take() {
            let painted = self.painted.load(Ordering::Acquire) == pending.id;
            if painted
                && matches!(
                    self.phase,
                    1 | 5
                        | 7
                        | 11
                        | 13
                        | 20
                        | 22
                        | 30
                        | 31
                        | 32
                        | 41
                        | 42
                        | 43
                        | 44
                        | 45
                        | 47
                        | 49
                        | 50
                        | 51
                )
            {
                let wait = Instant::now();
                let fence = self.device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(Duration::from_secs(10)),
                });
                if let Err(e) = &fence {
                    self.failures.push(format!("GPU fence: {e}"));
                }
                let mut observation = pending.observed;
                // Callback::prepare/upload and paint occurred after update.
                // Capture actual counters after the matching frame GPU fence.
                observation["painted"] = json!(true);
                observation["gpu_completed"] = json!(fence.is_ok());
                observation["acknowledged_monotonic_ns"] =
                    json!(self.started.elapsed().as_nanos() as u64);
                observation["counters"] = counters();
                observation["gpu_complete_upper_bound_ms"] =
                    json!(pending.at.elapsed().as_secs_f64() * 1000.);
                observation["gpu_fence_wait_ms"] = json!(wait.elapsed().as_secs_f64() * 1000.);
                self.complete = Some(observation);
            } else if pending.observed["phase"] == 3 {
                let mut observation = pending.observed;
                observation["painted"] = json!(painted);
                observation["acknowledged_monotonic_ns"] =
                    json!(now.duration_since(self.started).as_nanos() as u64);
                self.frames.push(observation);
            }
        }
        for event in &raw.events {
            if let egui::Event::Screenshot { image, .. } = event {
                self.screenshots += 1;
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                std::fs::write(
                    self.dir.join(format!("surface-{}.ppm", self.screenshots)),
                    bytes,
                )
                .unwrap();
            }
        }
        self.frame_id += 1;
        if self.phase == 3 {
            if let Some(previous) = self.previous_nav {
                self.frame_interval = Some(now.duration_since(previous).as_secs_f64() * 1000.);
            }
            self.previous_nav = Some(now);
        }
        if matches!(self.phase, 2 | 3) {
            let t = self.since.elapsed().as_secs_f64();
            let target_scale =
                self.base.scale * (1.05 + 0.35 * (std::f64::consts::TAU * t / 17.).sin());
            let target = MmPoint::new(
                self.base.center.x_mm + 80. * (std::f64::consts::TAU * t / 11.).sin(),
                self.base.center.y_mm + 8. * (std::f64::consts::TAU * t / 7.).sin(),
            );
            raw.events
                .push(egui::Event::PointerMoved(app.canvas_rect.center()));
            raw.events.push(egui::Event::Zoom(
                (target_scale / app.camera.scale).powf(1.25) as f32,
            ));
            raw.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(
                    ((app.camera.center.x_mm - target.x_mm) * app.camera.scale) as f32,
                    ((target.y_mm - app.camera.center.y_mm) * app.camera.scale) as f32,
                ),
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.phase == 5
            && self.point_step == 0
            && !app.busy
            && !app.display_pending
            && app.view.render_viewport.is_some_and(|bounds| {
                crate::display::covers_view(
                    bounds,
                    app.view.render_ppm,
                    app.camera
                        .world(app.canvas_rect.left_bottom(), app.canvas_rect),
                    app.camera
                        .world(app.canvas_rect.right_top(), app.canvas_rect),
                    app.camera.scale * f64::from(app.reported_ppp),
                )
            })
        {
            let pos = app.canvas_rect.center();
            self.input_at = Some(Instant::now());
            self.point_sequence = app.sequence;
            self.point_before = json!({"state":state(app),"counters":counters()});
            raw.events.extend([
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            self.point_step = 1;
        }
        if self.phase == 7 {
            let start = app.camera.screen(MmPoint::new(-40., -20.), app.canvas_rect);
            let end = app
                .camera
                .screen(MmPoint::new(1040., 120.), app.canvas_rect);
            match self.marquee_step {
                0 if !app.busy && !app.display_pending => {
                    raw.events.extend([
                        egui::Event::PointerMoved(start),
                        egui::Event::PointerButton {
                            pos: start,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                    self.marquee_step = 1;
                }
                1 if !app.busy => {
                    raw.events.push(egui::Event::PointerMoved(end));
                    self.marquee_step = 2;
                }
                2 if !app.busy => {
                    self.input_at = Some(Instant::now());
                    raw.events.push(egui::Event::PointerButton {
                        pos: end,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                    self.marquee_step = 3;
                }
                _ => {}
            }
        }
    }
    fn open(&mut self, app: &mut EditorApp) {
        self.open_at = Instant::now();
        app.send(Action::Open(self.path.clone()));
        self.enter(1);
        self.record(app, "open-enqueued");
    }
    fn pure(&mut self, app: &EditorApp) {
        let s = state(app);
        for key in [
            "revision",
            "workspace_revision",
            "dirty",
            "project_dirty",
            "undo",
            "redo",
        ] {
            if s[key] != self.baseline[key] {
                self.failures.push(format!("read-only changed {key}"));
            }
        }
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        if self.phase == 100 {
            return;
        }
        if self.started.elapsed() > Duration::from_secs(900) {
            self.failures.push("native run exceeded 900 seconds".into());
            self.finish(app, ctx);
            return;
        }
        if app.view.error.is_some() {
            self.failures.push(format!(
                "unexpected worker error {:?}",
                app.view.error.as_ref().map(|e| &e.code)
            ));
            self.finish(app, ctx);
            return;
        }
        let mut observed = state(app);
        observed["frame_id"] = json!(self.frame_id);
        observed["input_monotonic_ns"] =
            json!(self.frame_input.duration_since(self.started).as_nanos() as u64);
        observed["observed_monotonic_ns"] = json!(self.started.elapsed().as_nanos() as u64);
        observed["selected_primary_in_scene"] =
            json!(app.view.selected.primary().is_some_and(|o| {
                app.view
                    .scene
                    .as_ref()
                    .is_some_and(|s| s.ids.contains(&o.object.object_id))
            }));
        observed["focused"] = json!(self.focused);
        observed["phase"] = json!(self.phase);
        observed["elapsed_seconds"] =
            json!(self.frame_input.duration_since(self.since).as_secs_f64());
        observed["counters"] = counters();
        observed["prepare"] = self.prepared.take().unwrap_or(Value::Null);
        observed["frame_interval_ms"] = json!(self.frame_interval.take());
        self.pending = Some(Pending {
            id: self.frame_id,
            observed,
            at: Instant::now(),
        });
        // True idle: one future wake, no polling repaint loop or injected events.
        if !matches!(self.phase, 11 | 13 | 35) {
            ctx.request_repaint();
        }
        if self.phase == 5 && self.since.elapsed() > Duration::from_secs(10) {
            self.failures.push(format!(
                "point {} did not complete ordinary input path; input step {}",
                self.point, self.point_step
            ));
            self.finish(app, ctx);
            return;
        }
        if app.busy {
            return;
        }
        match self.phase {
            0 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                let actual = app.canvas_rect.size() * ctx.pixels_per_point();
                if !app.canvas_rect.is_positive() {
                    return;
                }
                if (actual.x - 1600.).abs() > 0.1 || (actual.y - 900.).abs() > 0.1 {
                    self.stable = 0;
                    if self.since.elapsed() > Duration::from_millis(250) {
                        if let Some(window) = ctx.input(|i| i.viewport().inner_rect) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                                window.size()
                                    + (vec2(1600., 900.) - actual) / ctx.pixels_per_point(),
                            ));
                        }
                        self.since = Instant::now();
                    }
                    return;
                }
                self.stable += 1;
                if self.stable >= 3 {
                    self.open(app);
                }
            }
            1 if !app.display_pending
                && app
                    .view
                    .scene
                    .as_ref()
                    .is_some_and(|s| s.ids.len() == self.expected)
                && self
                    .complete
                    .as_ref()
                    .is_some_and(|v| v["scene_count"] == self.expected) =>
            {
                self.records.push(json!({"label":"complete-operable-load","round":self.round,"elapsed_ms":self.open_at.elapsed().as_secs_f64()*1000.,"frame":self.complete,"state":state(app),"counters":counters()}));
                self.progress();
                self.base = app.camera;
                self.baseline = state(app);
                match self.mode.as_str() {
                    "nav" => self.enter(23),
                    "select" => self.enter(23),
                    "idle" => self.enter(10),
                    "load" => self.enter(20),
                    "lifecycle" => self.enter(30),
                    _ => unreachable!(),
                }
            }
            2 if self.since.elapsed() >= Duration::from_secs(10) => {
                // Freeze (start frame, end frame], including the final input
                // crossing 60s. Its pending paint is acknowledged next frame.
                self.enter(3);
                self.record(app, "navigation-start");
                self.previous_nav = Some(self.frame_input);
                std::fs::write(self.dir.join("navigation-started"), b"60 seconds").unwrap();
            }
            3 if self.frame_input.duration_since(self.since) >= Duration::from_secs(60) => {
                self.record(app, "navigation-end");
                self.pure(app);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.enter(25);
            }
            9 if self.since.elapsed() > Duration::from_secs(1) => self.finish(app, ctx),
            4 => {
                if self.point == 200 {
                    app.fit = true;
                    self.enter(6);
                } else {
                    // Deterministic view setup outside latency measurement;
                    // click itself takes the complete ordinary gesture path.
                    let i = self.point as i32;
                    app.camera.center =
                        MmPoint::new(f64::from(i * 499 % 1000 + 1), f64::from(i * 37 % 100 + 1));
                    app.camera.scale = 20.;
                    self.enter(5);
                }
            }
            5 if self.point_step == 1
                && app.sequence > self.point_sequence
                && !app.display_pending
                && app.drag.is_none()
                && self.complete.as_ref().is_some_and(|v| {
                    v["selected_count"] == 1
                        && app
                            .view
                            .selected
                            .primary()
                            .is_some_and(|o| v["selected_primary"] == o.object.object_id)
                        && v["scene_serial"] == self.point_before["state"]["scene_serial"]
                }) =>
            {
                let i = self.point as i32;
                let ordinal = (i * 37 % 100) * 1000 + (i * 499 % 1000) + 1;
                let suffix = format!("object-{ordinal}");
                let actual = app
                    .view
                    .selected
                    .primary()
                    .map(|o| o.object.object_id.as_str());
                if actual.is_none_or(|id| id != suffix) {
                    self.failures.push(format!(
                        "point {} expected {suffix}, got {actual:?}",
                        self.point
                    ));
                }
                self.records.push(json!({"label":"point-highlight","before":self.point_before,"point":self.point,"ordinal":ordinal,"expected_id_suffix":suffix,"actual_id":actual,"input_to_gpu_complete_upper_bound_ms":self.input_at.take().unwrap().elapsed().as_secs_f64()*1000.,"frame":self.complete,"state":state(app)}));
                self.pure(app);
                self.point += 1;
                self.progress();
                self.enter(4);
            }
            6 if !app.display_pending && self.since.elapsed() > Duration::from_millis(300) => {
                self.enter(7);
                self.marquee_step = 0;
                self.record(app, "marquee-start");
            }
            7 if self.marquee_step == 3
                && app.drag.is_none()
                && self
                    .complete
                    .as_ref()
                    .is_some_and(|v| v["selected_count"] == self.expected) =>
            {
                let ready = self.input_at.take().unwrap().elapsed().as_secs_f64() * 1000.;
                let ids: Vec<_> = app
                    .view
                    .selected
                    .ordered
                    .iter()
                    .map(|o| o.object.object_id.clone())
                    .collect();
                let ordered = ids
                    .iter()
                    .enumerate()
                    .all(|(i, id)| id == &format!("object-{}", i + 1));
                if ids.len() != 100000 || !ordered {
                    self.failures
                        .push("full marquee source order/count mismatch".into());
                }
                std::fs::write(
                    self.dir.join("marquee-selected-ids.json"),
                    serde_json::to_vec(&ids).unwrap(),
                )
                .unwrap();
                self.records.push(json!({"label":"marquee-highlight","release_to_gpu_complete_upper_bound_ms":ready,"selected_count":ids.len(),"ordered":ordered,"frame":self.complete,"state":state(app)}));
                self.pure(app);
                self.enter(25);
            }
            10 if self.since.elapsed() > Duration::from_secs(2) => {
                app.send(Action::SaveProject(
                    Some(self.dir.join("idle-baseline.rcam")),
                    false,
                    None,
                ));
                self.enter(14);
            }
            14 if self.since.elapsed() > Duration::from_secs(2) => {
                app.send(Action::Save(
                    self.dir.join("read-only-before.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
                self.enter(15);
            }
            15 => {
                self.baseline = state(app);
                self.record(app, "idle-start");
                self.enter(11);
                let ctx = ctx.clone();
                let wake = self.wake.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(60));
                    wake.store(true, Ordering::Release);
                    ctx.request_repaint();
                });
                std::fs::write(
                    self.dir.join("idle-started"),
                    b"60 seconds no harness repaint",
                )
                .unwrap();
            }
            11 => {
                self.records.push(json!({"label":"idle-frame","seconds":self.since.elapsed().as_secs_f64(),"frame_id":self.frame_id,"counters":counters()}));
                if self.wake.load(Ordering::Acquire) {
                    self.record(app, "idle-end");
                    self.pure(app);
                    self.open_at = Instant::now();
                    app.send(Action::Select(
                        MmPoint::new(1., 1.),
                        0.,
                        crate::selection::SelectionMode::Replace,
                    ));
                    self.enter(13);
                }
            }
            13 if self
                .complete
                .as_ref()
                .is_some_and(|v| v["selected_count"] == 1) =>
            {
                self.records.push(json!({"label":"worker-real-wake-highlight","enqueue_to_complete_ms":self.open_at.elapsed().as_secs_f64()*1000.,"frame":self.complete}));
                self.pure(app);
                self.enter(25);
            }
            13 if app.view.selected.ordered.len() == 1 && !self.wake_confirmation_requested => {
                // Only confirm after the real worker wake has delivered the
                // result; do not poll the receive channel with harness redraws.
                self.wake_confirmation_requested = true;
                ctx.request_repaint();
            }
            20 if self.since.elapsed() > Duration::from_secs(1) => {
                self.record(app, "load-settled");
                if self.round >= 3 {
                    self.finish(app, ctx);
                } else {
                    app.send(Action::Close(true));
                    self.enter(22);
                }
            }
            22 if self.since.elapsed() > Duration::from_secs(1) && app.view.scene.is_none() => {
                self.round += 1;
                self.open(app);
            }
            23 => {
                app.send(Action::Save(
                    self.dir.join("read-only-before.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
                self.enter(24);
            }
            24 => {
                self.record(app, "read-only-baseline-export");
                self.baseline = state(app);
                if self.mode == "nav" {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                    self.enter(2);
                } else {
                    self.enter(4);
                }
            }
            25 => {
                app.send(Action::Save(
                    self.dir.join("read-only-after.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
                self.enter(26);
            }
            26 => {
                let before = std::fs::read(self.dir.join("read-only-before.gbr")).unwrap();
                let after = std::fs::read(self.dir.join("read-only-after.gbr")).unwrap();
                self.records.push(json!({"label":"read-only-writer-parity","before_sha256":sha256_hex(&before),"after_sha256":sha256_hex(&after),"byte_identical":before==after,"source_sha256":sha256_hex(&std::fs::read(&self.path).unwrap())}));
                if before != after {
                    self.failures
                        .push("read-only operation changed real Gerber export bytes".into());
                }
                if self.mode == "select" {
                    self.enter(40);
                } else {
                    self.finish(app, ctx);
                }
            }
            30 if self.since.elapsed() > Duration::from_secs(1) => {
                app.camera.scale = self.base.scale * 2.;
                self.enter(31);
            }
            31 if !app.display_pending && self.since.elapsed() > Duration::from_secs(1) => {
                self.record(app, "life-lod2");
                app.camera.scale = self.base.scale * 4.;
                self.enter(32);
            }
            32 if !app.display_pending && self.since.elapsed() > Duration::from_secs(1) => {
                self.record(app, "life-lod4");
                app.fit = true;
                self.enter(33);
            }
            33 if !app.display_pending && self.since.elapsed() > Duration::from_secs(1) => {
                self.record(app, "life-fit");
                app.send(Action::Close(true));
                self.enter(34);
            }
            34 if app.view.scene.is_none() => {
                self.record(app, "life-closed");
                self.enter(35);
                self.wake.store(false, Ordering::Release);
                let ctx = ctx.clone();
                let wake = self.wake.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(5));
                    wake.store(true, Ordering::Release);
                    ctx.request_repaint();
                });
            }
            35 if self.wake.load(Ordering::Acquire) => {
                self.record(app, "life-closed-idle5");
                std::fs::write(
                    self.dir.join(format!("life-{}-closed", self.round + 1)),
                    b"RSS sample checkpoint",
                )
                .unwrap();
                if self.round >= 19 {
                    self.finish(app, ctx);
                } else {
                    self.round += 1;
                    self.open(app);
                }
            }
            40 => {
                self.original_geometry = geometry_hash(app);
                app.send(Action::SelectRect(
                    editor_core::BoundsMm {
                        min_x_mm: 0.5,
                        min_y_mm: 0.5,
                        max_x_mm: 3.5,
                        max_y_mm: 1.5,
                    },
                    editor_core::hit_test::SelectRectMode::Window,
                ));
                self.enter(41);
            }
            41 if self.completed_current(app) => {
                if app.view.selected.ordered.len() != 3 {
                    self.failures.push(
                        "bounded move selection must contain exactly first three source objects"
                            .into(),
                    );
                    self.finish(app, ctx);
                    return;
                }
                self.record(app, "bounded-move-before");
                app.send(Action::Move("0.25".into(), "0.125".into()));
                self.enter(42);
            }
            42 if self.completed_current(app) => {
                self.moved_geometry = geometry_hash(app);
                self.record(app, "bounded-move-after");
                if app.view.info.as_ref().unwrap().undo_entries != 1
                    || !manufacturing_truth(app, true)
                {
                    self.failures.push(
                        "bounded move changed wrong objects/order or was not one Undo".into(),
                    );
                }
                app.send(Action::History(false));
                self.enter(43);
            }
            43 if self.completed_current(app) => {
                self.record(app, "bounded-undo");
                if geometry_hash(app) != self.original_geometry || !manufacturing_truth(app, false)
                {
                    self.failures
                        .push("Undo did not restore exact ordered manufacturing snapshot".into());
                }
                app.send(Action::History(true));
                self.enter(44);
            }
            44 if self.completed_current(app) => {
                self.record(app, "bounded-redo");
                if geometry_hash(app) != self.moved_geometry || !manufacturing_truth(app, true) {
                    self.failures
                        .push("Redo did not restore moved snapshot".into());
                }
                app.send(Action::Save(
                    self.dir.join("bounded-move-export.gbr"),
                    app.view.layers[0].layer_id.clone(),
                    None,
                ));
                self.enter(45);
            }
            45 if self.completed_current(app) => {
                self.record(app, "bounded-export");
                if geometry_hash(app) != self.moved_geometry
                    || sha256_hex(&std::fs::read(&self.path).unwrap())
                        != sha256_hex(include_bytes!(
                            "../../../fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr"
                        ))
                {
                    self.failures.push(
                        "Export changed manufacturing snapshot or original source file".into(),
                    );
                }
                app.send(Action::Close(true));
                self.enter(46);
            }
            46 => {
                app.send(Action::Open(self.dir.join("bounded-move-export.gbr")));
                self.enter(47);
            }
            47 if self.completed_current(app)
                && !app.display_pending
                && app
                    .view
                    .scene
                    .as_ref()
                    .is_some_and(|s| s.ids.len() == 100000) =>
            {
                self.record(app, "bounded-export-reopen");
                if !manufacturing_truth(app, true) {
                    self.failures.push(
                        "Export/Reopen did not preserve independent f64 coordinates/exposure/order"
                            .into(),
                    );
                }
                self.enter(48);
            }
            48 => {
                app.send(Action::Select(
                    MmPoint::new(1.25, 1.125),
                    0.,
                    crate::selection::SelectionMode::Replace,
                ));
                self.enter(49);
            }
            49 if self.completed_current(app) => {
                if app.view.selected.ordered.len() != 1 {
                    self.failures
                        .push("single move selection is not exactly one object".into());
                    self.finish(app, ctx);
                    return;
                }
                self.single_geometry = geometry_hash(app);
                self.record(app, "single-move-before");
                app.send(Action::Move("0.1".into(), "0".into()));
                self.enter(50);
            }
            50 if self.completed_current(app) => {
                self.record(app, "single-move-after");
                if app.view.info.as_ref().unwrap().undo_entries != 1 {
                    self.failures
                        .push("single move was not one undo transaction".into());
                }
                app.send(Action::History(false));
                self.enter(51);
            }
            51 if self.completed_current(app) => {
                self.record(app, "single-move-undo");
                if geometry_hash(app) != self.single_geometry {
                    self.failures
                        .push("single move Undo did not restore exact snapshot".into());
                }
                self.finish(app, ctx);
            }
            _ => {}
        }
    }
}

fn geometry_hash(app: &EditorApp) -> String {
    let s = app.view.snap_snapshot.as_ref().unwrap();
    sha256_hex(&serde_json::to_vec(&(&s.apertures, &s.layers, &s.block_definitions)).unwrap())
}
fn manufacturing_truth(app: &EditorApp, moved: bool) -> bool {
    let Some(s) = app.view.snap_snapshot.as_ref() else {
        return false;
    };
    if s.layers.len() != 1 || s.layers[0].objects.len() != 100000 {
        return false;
    }
    s.layers[0].objects.iter().enumerate().all(|(i, o)| {
        let editor_core::SemanticGeometry::Flash { center, .. } = o.geometry else {
            return false;
        };
        let dx = if moved && i < 3 { 0.25 } else { 0. };
        let dy = if moved && i < 3 { 0.125 } else { 0. };
        o.exposure == editor_core::Exposure::Dark
            && (center.x_mm - ((i % 1000 + 1) as f64 + dx)).abs() < 1e-10
            && (center.y_mm - ((i / 1000 + 1) as f64 + dy)).abs() < 1e-10
    })
}
