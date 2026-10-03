//! Internal-only native batch drag recorder. Production input/worker/Metal path.
use crate::{EditorApp, camera::Camera, gpu::PrepareStats, state::Action};
use editor_core::{
    BoundsMm, MmPoint, SemanticGeometry, hash::sha256_hex, hit_test::SelectRectMode,
};
use eframe::egui::{self, Pos2, vec2};
use egui_wgpu::wgpu;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static ORIGIN: OnceLock<Instant> = OnceLock::new();
pub fn worker_observation(start: Instant, view: &crate::state::View) -> Option<Value> {
    let origin = *ORIGIN.get()?;
    let d = view.info.as_ref();
    Some(
        json!({"started_ns":start.checked_duration_since(origin)?.as_nanos() as u64,"finished_ns":origin.elapsed().as_nanos() as u64,"state":{"document_id":d.map(|d|&d.document_id),"revision":d.map(|d|&d.revision),"workspace_revision":d.map(|d|&d.workspace_revision),"dirty":d.map(|d|d.dirty),"project_dirty":d.map(|d|d.project_dirty),"undo":d.map(|d|d.undo_entries),"redo":d.map(|d|d.redo_entries),"selected":view.selected.ordered.len(),"scene_serial":view.scene.as_ref().map(|s|s.serial),"scene_objects":view.scene.as_ref().map_or(0,|s|s.objects.len())}}),
    )
}
fn view_parameters(app: &EditorApp) -> Value {
    json!({"center_mm":[app.camera.center.x_mm,app.camera.center.y_mm],"scale":app.camera.scale,"rect":[app.canvas_rect.min.x,app.canvas_rect.min.y,app.canvas_rect.max.x,app.canvas_rect.max.y],"ppp":app.reported_ppp,"grid_snap":app.grid.snap_enabled,"object_snap":app.object_snap.enabled,"threshold_physical_px":crate::drag::THRESHOLD_PX})
}

// UI update allocation count, excluding this recorder's serialization, GPU
// callback, other threads and framework work outside input->end-of-update.
struct Counted;
thread_local! { static COUNT: std::cell::Cell<Option<(u64,u64)>> = const {std::cell::Cell::new(None)}; }
fn allocated(bytes: usize) {
    let _ = COUNT.try_with(|c| {
        if let Some((n, b)) = c.get() {
            c.set(Some((n + 1, b + bytes as u64)));
        }
    });
}
unsafe impl std::alloc::GlobalAlloc for Counted {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        allocated(l.size());
        unsafe { std::alloc::System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {
        allocated(l.size());
        unsafe { std::alloc::System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, n: usize) -> *mut u8 {
        allocated(n);
        unsafe { std::alloc::System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;

pub fn directory() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("RCAM_BATCH_DRAG_NATIVE_DIR")?).ok()?;
    (dir.parent() == Some(Path::new("/private/tmp"))
        && dir.file_name()?.to_str()?.starts_with("rcam-batch-drag-"))
    .then_some(dir)
}
fn state(app: &EditorApp) -> Value {
    let d = app.view.info.as_ref();
    json!({"document_id":d.map(|d|&d.document_id),"revision":d.map(|d|&d.revision),"workspace_revision":d.map(|d|&d.workspace_revision),"dirty":d.map(|d|d.dirty),"project_dirty":d.map(|d|d.project_dirty),"undo":d.map(|d|d.undo_entries),"redo":d.map(|d|d.redo_entries),"selected":app.view.selected.ordered.len(),"scene_serial":app.view.scene.as_ref().map(|s|s.serial),"scene_objects":app.view.scene.as_ref().map_or(0,|s|s.objects.len()),"busy":app.busy,"display_pending":app.display_pending,"delta":app.drag.as_ref().map(|g|g.delta),"canvas_physical":[app.canvas_rect.width()*app.reported_ppp,app.canvas_rect.height()*app.reported_ppp]})
}
struct Pending {
    id: u64,
    input: Instant,
    observed: Value,
}
pub struct Run {
    dir: PathBuf,
    request: Value,
    count: usize,
    mode: String,
    phase: u32,
    started: Instant,
    since: Instant,
    input_at: Instant,
    previous: Option<Instant>,
    frame_interval: Option<f64>,
    operation_at: Option<Instant>,
    device: wgpu::Device,
    pub painted: Arc<AtomicU64>,
    pub frame_id: u64,
    pending: Option<Pending>,
    complete: Option<Value>,
    prepared: Option<Value>,
    frames: Vec<Value>,
    events: Vec<Value>,
    failures: Vec<String>,
    press: Pos2,
    baseline: Value,
    baseline_snapshot: Option<Arc<editor_service::RenderSnapshot>>,
    baseline_scene: Option<Arc<crate::display::Scene>>,
    pub focused: bool,
    finished: bool,
    snapshot_files: Vec<Value>,
    delta: MmPoint,
    injected: Value,
    trajectory_origin: Option<Instant>,
}
impl Run {
    pub fn from_env(device: wgpu::Device) -> Option<Self> {
        let dir = directory()?;
        let request: Value =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).ok()?).ok()?;
        let count = request["selected"].as_u64()? as usize;
        assert!([100, 500, 1000, 5000].contains(&count));
        assert!(!cfg!(debug_assertions));
        let mode = request["mode"].as_str()?.to_owned();
        assert!(["move", "escape", "blur", "pointergone", "new-project"].contains(&mode.as_str()));
        crate::native_s5m1::activate_counters();
        let started = *ORIGIN.get_or_init(Instant::now);
        Some(Self {
            dir,
            request,
            count,
            mode,
            phase: 0,
            started,
            since: Instant::now(),
            input_at: Instant::now(),
            previous: None,
            frame_interval: None,
            operation_at: None,
            device,
            painted: Arc::new(AtomicU64::new(0)),
            frame_id: 0,
            pending: None,
            complete: None,
            prepared: None,
            frames: vec![],
            events: vec![],
            failures: vec![],
            press: Pos2::ZERO,
            baseline: Value::Null,
            baseline_snapshot: None,
            baseline_scene: None,
            focused: true,
            finished: false,
            snapshot_files: vec![],
            delta: MmPoint::new(0., 0.),
            injected: Value::Null,
            trajectory_origin: None,
        })
    }
    fn enter(&mut self, p: u32) {
        self.phase = p;
        self.since = Instant::now();
    }
    fn event(&mut self, label: &str, data: Value) {
        self.events.push(json!({"label":label,"at_ns":self.started.elapsed().as_nanos() as u64,"frame_id":self.frame_id,"data":data}));
    }
    pub fn prepare(&mut self, s: &PrepareStats) {
        self.prepared = Some(
            json!({"cpu_prepare_ms":s.cpu_prepare_ms,"preview_index_ms":s.preview_index_ms,"object_visits":s.object_visits,"candidates":s.candidate_count}),
        );
    }
    fn pointer(raw: &mut egui::RawInput, p: Pos2, pressed: Option<bool>) {
        raw.events.push(egui::Event::PointerMoved(p));
        if let Some(pressed) = pressed {
            raw.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    fn key(raw: &mut egui::RawInput, key: egui::Key, command: bool, shift: bool) {
        let modifiers = egui::Modifiers {
            mac_cmd: command,
            command,
            shift,
            ..Default::default()
        };
        raw.modifiers = modifiers;
        for pressed in [true, false] {
            raw.events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            });
        }
    }
    pub fn input(&mut self, app: &mut EditorApp, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.finished {
            return;
        }
        self.complete = None;
        if let Some(p) = self.pending.take() {
            let painted = self.painted.load(Ordering::Acquire) == p.id;
            let mut frame = p.observed;
            frame["painted"] = json!(painted);
            // Fence only the actually submitted production callback. This is a
            // conservative GPU completion bound, not GPU execution or scanout.
            if painted {
                let okay = self
                    .device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(5)),
                    })
                    .is_ok();
                frame["gpu_completed"] = json!(okay);
                frame["input_gpu_complete_ms"] = json!(p.input.elapsed().as_secs_f64() * 1000.);
                frame["completed_ns"] = json!(self.started.elapsed().as_nanos() as u64);
                if !okay {
                    self.failures.push("GPU fence failed".into());
                }
                self.complete = Some(frame.clone());
            }
            frame["counters"] = crate::native_s5m1::counter_snapshot();
            self.frames.push(frame);
        }
        for e in &raw.events {
            if let egui::Event::Screenshot {
                image, user_data, ..
            } = e
            {
                let name = format!("surface-{}.ppm", self.frame_id);
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                std::fs::write(self.dir.join(&name), &bytes).unwrap();
                self.event("screenshot", json!({"path":name,"width":image.size[0],"height":image.size[1],"sha256":sha256_hex(&bytes),"request":user_data.data.as_ref().and_then(|d|d.downcast_ref::<Value>())}));
            }
        }
        self.frame_id += 1;
        self.input_at = Instant::now();
        self.focused = raw.focused;
        match self.phase {
            4 => {
                Self::pointer(raw, self.press, Some(true));
                self.event("press", json!({"position":[self.press.x,self.press.y]}));
                self.enter(5);
            }
            6 => {
                let seconds = if self.mode == "move" { 10. } else { 1. };
                let t = (self.input_at.duration_since(self.since).as_secs_f64() / seconds).min(1.)
                    as f32;
                let pos =
                    self.press + vec2(36. * t, -18. * t + 8. * (std::f32::consts::TAU * t).sin());
                if t < 1. {
                    Self::pointer(raw, pos, None);
                } else {
                    let pos = self.press + vec2(36., -18.);
                    Self::pointer(raw, pos, Some(false));
                    match self.mode.as_str() {
                        "escape" => Self::key(raw, egui::Key::Escape, false, false),
                        "blur" => {
                            raw.focused = false;
                            self.focused = false;
                        }
                        "pointergone" => raw.events.push(egui::Event::PointerGone),
                        "new-project" => app.send(Action::DiscardNewWorkspace),
                        _ => {}
                    }
                    self.operation_at = Some(self.input_at);
                    self.delta = MmPoint::new(36. / app.camera.scale, 18. / app.camera.scale);
                    self.event(
                        "release",
                        json!({"mode":self.mode,"position":[pos.x,pos.y],"delta_mm":self.delta}),
                    );
                    self.enter(7);
                }
            }
            8 => {
                Self::key(raw, egui::Key::Z, true, false);
                self.operation_at = Some(self.input_at);
                self.event("undo-input", Value::Null);
                self.enter(9);
            }
            10 => {
                Self::key(raw, egui::Key::Z, true, true);
                self.operation_at = Some(self.input_at);
                self.event("redo-input", Value::Null);
                self.enter(11);
            }
            _ => {}
        }
        self.injected = json!({"view":view_parameters(app),"pointer":raw.events.iter().rev().find_map(|e|if let egui::Event::PointerMoved(p)=e {Some([p.x,p.y])} else {None}),"pressed":raw.events.iter().any(|e|matches!(e,egui::Event::PointerButton {pressed:true,..})),"released":raw.events.iter().any(|e|matches!(e,egui::Event::PointerButton {pressed:false,..})),"escape":raw.events.iter().any(|e|matches!(e,egui::Event::Key {key:egui::Key::Escape,pressed:true,..})),"pointer_gone":raw.events.iter().any(|e|matches!(e,egui::Event::PointerGone)),"focused":raw.focused,"trajectory_origin_ns":self.trajectory_origin.map(|t|t.duration_since(self.started).as_nanos() as u64)});
        // Frame interval includes recorder overhead, scheduling and preceding
        // GPU completion; CPU update time below explicitly excludes recorder.
        self.frame_interval = self
            .previous
            .replace(self.input_at)
            .map(|t| self.input_at.duration_since(t).as_secs_f64() * 1000.);
        if self.mode == "blur" && self.phase == 7 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        COUNT.with(|c| c.set(Some((0, 0))));
    }
    fn complete_current(&self, app: &EditorApp) -> bool {
        let current = state(app);
        !app.busy
            && !app.display_pending
            && self.complete.as_ref().is_some_and(|v| {
                v["painted"] == true && v["gpu_completed"] == true && v["state"] == current
            })
    }
    fn completion(&mut self, app: &EditorApp, label: &str, action: Option<&str>) {
        let frame = self.complete.as_ref().unwrap();
        let workers = crate::native_s5m1::worker_snapshot();
        let worker = action
            .and_then(|a| {
                workers
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|w| w["action"] == a)
            })
            .map(|w| w["sequence"].clone());
        self.event(label,json!({"state":state(app),"duration_ms":self.operation_at.unwrap().elapsed().as_secs_f64()*1000.,"visible_frame_id":frame["id"],"visible_completed_ns":frame["completed_ns"],"worker_sequence":worker}));
    }
    fn screenshot(&mut self, ctx: &egui::Context, label: &str) {
        let request = json!({"label":label,"frame_id":self.frame_id});
        self.event("screenshot-request", request.clone());
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
            request,
        )));
    }

    fn snapshot(&mut self, app: &EditorApp, label: &str) {
        let Some(s) = &app.view.snap_snapshot else {
            return;
        };
        let mut bytes = vec![];
        for o in s.layers.iter().flat_map(|l| &l.objects) {
            if let SemanticGeometry::Flash { center, .. } = &o.geometry {
                bytes.extend_from_slice(&center.x_mm.to_le_bytes());
                bytes.extend_from_slice(&center.y_mm.to_le_bytes());
            } else {
                self.failures
                    .push("fixed fixture contains non-Flash".into());
            }
        }
        let name = format!("{label}-positions.f64le");
        std::fs::write(self.dir.join(&name), &bytes).unwrap();
        self.snapshot_files.push(json!({"label":label,"path":name,"sha256":sha256_hex(&bytes),"count":bytes.len()/16,"content_sha256":sha256_hex(&serde_json::to_vec(&(&s.layers,&s.apertures,&s.block_definitions)).unwrap()),"state":state(app)}));
    }
    fn finish(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        self.finished = true;
        let binary = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let fixture = Path::new(self.request["fixture"].as_str().unwrap());
        let report = json!({"schema_version":2,"stage":"S5-M2-B","profile":"release","observation_version":2,"last_observed_frame_id":self.frame_id-1,"request":self.request,"commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":sha256_hex(&binary),"fixture_sha256":sha256_hex(&std::fs::read(fixture).unwrap()),"protocol_sha256":sha256_hex(include_bytes!("../../../fixtures/synthetic/s5m2b/protocol.json")),"adapter":app.adapter,"frames":self.frames,"events":self.events,"snapshots":self.snapshot_files,"worker":crate::native_s5m1::worker_snapshot(),"failures":self.failures,"counters":crate::native_s5m1::counter_snapshot(),"measurement_scope":"synthetic egui input; production Metal callback completion upper bound, no physical input/scanout claim; UI allocator only raw-input-end to tick-start; RSS/CPU externally sampled"});
        std::fs::write(
            self.dir.join("observations.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        let allocation = COUNT.with(|c| c.replace(None));
        if self.finished {
            return;
        }
        let cpu_ms = self.input_at.elapsed().as_secs_f64() * 1000.;
        if self.started.elapsed() > Duration::from_secs(120) {
            self.failures.push(format!("timeout phase {}", self.phase));
            self.finish(app, ctx);
            return;
        }
        if app.view.error.is_some() || app.display_error.is_some() {
            self.failures.push(format!(
                "app error {:?} {:?}",
                app.view.error, app.display_error
            ));
            self.finish(app, ctx);
            return;
        }
        let stable = self.baseline_snapshot.as_ref().is_none_or(|s| {
            app.view
                .snap_snapshot
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(s, v))
        });
        let scene_stable = self
            .baseline_scene
            .as_ref()
            .is_none_or(|s| app.view.scene.as_ref().is_some_and(|v| Arc::ptr_eq(s, v)));
        let interval = self
            .previous
            .map(|_| self.input_at.duration_since(self.started).as_secs_f64());
        self.pending = Some(Pending {
            id: self.frame_id,
            input: self.input_at,
            observed: json!({"id":self.frame_id,"phase":self.phase,"injected":self.injected,"view":view_parameters(app),"gesture":app.drag.as_ref().map(|d|json!({"last":[d.last.x,d.last.y],"confirmed":d.confirmed,"dragging":d.evidence_dragging(),"error":d.error()})),"input_ns":self.input_at.duration_since(self.started).as_nanos() as u64,"input_seconds":interval,"observed_ns":self.started.elapsed().as_nanos() as u64,"state":state(app),"cpu_update_ms":cpu_ms,"frame_interval_ms":self.frame_interval,"focused":self.focused,"ui_allocation_count_bytes":allocation,"prepare":self.prepared.take(),"snapshot_identity_unchanged":stable,"scene_identity_unchanged":scene_stable,"preview_index_identity_unchanged":app.last_good.as_ref().is_none_or(|l|Arc::ptr_eq(&l.scene.index,&l.index)),"paint_delta":app.last_good.as_ref().map(|l|l.uniforms.preview)}),
        });
        ctx.request_repaint_after(Duration::from_millis(16));
        if app.busy {
            return;
        }
        match self.phase {
            0 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                let actual = app.canvas_rect.size() * ctx.pixels_per_point();
                if (actual - vec2(1600., 900.)).length() > 0.2 {
                    if self.since.elapsed() > Duration::from_millis(250) {
                        if let Some(w) = ctx.input(|i| i.viewport().inner_rect) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                                w.size() + (vec2(1600., 900.) - actual) / ctx.pixels_per_point(),
                            ));
                        }
                        self.since = Instant::now();
                    }
                    return;
                }
                app.send(Action::Open(PathBuf::from(
                    self.request["fixture"].as_str().unwrap(),
                )));
                self.enter(1);
            }
            1 if self.complete_current(app) => {
                let n = self.count;
                app.send(Action::SelectRect(
                    BoundsMm {
                        min_x_mm: 0.5,
                        min_y_mm: 0.5,
                        max_x_mm: n.min(1000) as f64 + 0.5,
                        max_y_mm: n.div_ceil(1000) as f64 + 0.5,
                    },
                    SelectRectMode::Window,
                ));
                self.enter(2);
            }
            2 if self.complete_current(app) => {
                if app.view.selected.ordered.len() != self.count {
                    self.failures.push("selection count mismatch".into());
                    self.finish(app, ctx);
                    return;
                }
                app.camera = Camera {
                    center: MmPoint::new(500.5, 50.5),
                    scale: 0.74,
                };
                app.fit = false;
                app.grid.snap_enabled = false;
                app.object_snap.enabled = false;
                self.event("warmup-begin", state(app));
                self.enter(3);
            }
            3 if self.since.elapsed() >= Duration::from_secs(10) && self.complete_current(app) => {
                self.baseline = state(app);
                self.baseline_snapshot = app.view.snap_snapshot.clone();
                self.baseline_scene = app.view.scene.clone();
                self.snapshot(app, "before");
                self.press = app.camera.screen(MmPoint::new(1., 1.), app.canvas_rect);
                self.event("baseline",json!({"state":self.baseline,"counters":crate::native_s5m1::counter_snapshot()}));
                self.screenshot(ctx, "batch-baseline");
                self.enter(4);
            }
            5 if app.drag.as_ref().is_some_and(|d| d.confirmed) => {
                self.enter(6);
                self.trajectory_origin = Some(self.since);
                self.event("confirmed", json!({"state":state(app),"trajectory_origin_ns":self.since.duration_since(self.started).as_nanos() as u64}));
            }
            6 => {
                if !stable || !scene_stable {
                    self.failures
                        .push("preview changed immutable model/scene".into());
                }
                if self.since.elapsed() > Duration::from_secs(2)
                    && self.since.elapsed() < Duration::from_millis(2050)
                {
                    self.screenshot(ctx, "batch-preview");
                }
            }
            7 if self.mode != "move"
                && app.drag.is_none()
                && self.complete_current(app)
                && (self.mode != "new-project"
                    || state(app)["document_id"] != self.baseline["document_id"]) =>
            {
                self.completion(
                    app,
                    "cancel-complete",
                    (self.mode == "new-project").then_some("new-project"),
                );
                self.snapshot(app, "cancelled");
                self.enter(12);
                self.screenshot(ctx, "batch-cancel");
            }
            7 if self.complete_current(app)
                && state(app)["revision"] != self.baseline["revision"] =>
            {
                self.completion(app, "commit-complete", Some("drag-move"));
                self.snapshot(app, "moved");
                self.enter(8);
            }
            9 if self.complete_current(app) && state(app)["undo"] == self.baseline["undo"] => {
                self.completion(app, "undo-complete", Some("undo"));
                self.snapshot(app, "undo");
                self.enter(10);
            }
            11 if self.complete_current(app) && state(app)["undo"] != self.baseline["undo"] => {
                self.completion(app, "redo-complete", Some("redo"));
                self.snapshot(app, "redo");
                self.enter(12);
                self.screenshot(ctx, "batch-redo");
            }
            12 if self.since.elapsed() > Duration::from_millis(300) => self.finish(app, ctx),
            _ => {}
        }
    }
}
