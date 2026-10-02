//! Opt-in native A2 observations. No product automation API. All setup uses the
//! ordinary app queue; cancellation uses actual egui pointer input and button.
use crate::{
    EditorApp,
    state::{Action, View},
};
use editor_service::task::{TaskContext, TaskState};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static ORIGIN: OnceLock<(Instant, u64)> = OnceLock::new();
static EVENTS: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static TARGET: AtomicU64 = AtomicU64::new(0);
static COMMIT_BARRIER: AtomicBool = AtomicBool::new(false);
static DELIVERY_BARRIER: AtomicBool = AtomicBool::new(false);
static WAITING: AtomicBool = AtomicBool::new(false);
static RELEASE: AtomicBool = AtomicBool::new(false);
pub fn directory() -> Option<PathBuf> {
    let p = std::fs::canonicalize(std::env::var_os("RCAM_A2_NATIVE_DIR")?).ok()?;
    (p.parent() == Some(Path::new("/private/tmp"))
        && p.file_name()?.to_str()?.starts_with("rcam-a2-native-"))
    .then_some(p)
}
fn stamp() -> u64 {
    let (start, epoch) = ORIGIN.get_or_init(|| {
        (
            Instant::now(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        )
    });
    epoch + start.elapsed().as_nanos() as u64
}
fn event(label: &str, task: u64, data: Value) -> u64 {
    if ORIGIN.get().is_none() {
        return 0;
    }
    let at = stamp();
    EVENTS
        .lock()
        .unwrap()
        .push(json!({"at_ns":at,"event":label,"task_id":task,"data":data}));
    at
}
fn state(view: &View) -> Value {
    let info = view.info.as_ref();
    json!({"document_id":info.map(|d|&d.document_id),"revision":info.map(|d|&d.revision),"workspace_revision":info.map(|d|&d.workspace_revision),"generation":view.task_generation,"dirty":info.map(|d|d.dirty),"project_dirty":info.map(|d|d.project_dirty),"undo_entries":info.map(|d|d.undo_entries),"redo_entries":info.map(|d|d.redo_entries),"layers":view.layers.len(),"objects":view.snap_snapshot.as_ref().map_or(0,|s|s.layers.iter().map(|l|l.objects.len()).sum::<usize>()),"content_sha256":view.snap_snapshot.as_ref().map(|s|editor_core::hash::sha256_hex(&serde_json::to_vec(&(&s.layers,&s.apertures,&s.block_definitions)).unwrap())),"selection":view.selected.ids()})
}
pub fn worker_begin(task: &TaskContext, view: &View) {
    if ORIGIN.get().is_none() {
        return;
    }
    event(
        "worker_begin",
        task.task_id,
        json!({"version":task.input,"before":state(view)}),
    );
}
pub fn worker_finished(id: u64, view: &View) {
    if ORIGIN.get().is_none() {
        return;
    }
    event(
        "worker_finished",
        id,
        json!({"receipt":view.task_receipt,"after":state(view),"error":view.error.as_ref().map(|e|&e.code)}),
    );
}
pub fn reply(id: u64, installed: bool) {
    event("reply_received", id, json!({"installed":installed}));
}
fn wait(id: u64, label: &str) {
    event(label, id, json!({"injected":true,"timeout_ms":5000}));
    WAITING.store(true, Ordering::Release);
    let start = Instant::now();
    while !RELEASE.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(2));
    }
    event(
        "barrier_released",
        id,
        json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"released":RELEASE.load(Ordering::Acquire)}),
    );
}
pub fn committing(task: &TaskContext) {
    if TARGET.load(Ordering::Acquire) == task.task_id && COMMIT_BARRIER.load(Ordering::Acquire) {
        wait(task.task_id, "commit_barrier");
    }
}
pub fn returning(id: u64) {
    if TARGET.load(Ordering::Acquire) == id && DELIVERY_BARRIER.load(Ordering::Acquire) {
        wait(id, "delivery_barrier");
    }
    event("worker_returned", id, json!({}));
}
pub fn cancel_clicked(
    task: u64,
    outcome: editor_service::task::CancelOutcome,
    ctx: &egui::Context,
) {
    event(
        "cancel_button",
        task,
        json!({"outcome":format!("{outcome:?}")}),
    );
    RELEASE.store(true, Ordering::Release);
    if ORIGIN.get().is_some() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
            "a2-feedback",
        )));
    }
}
pub struct Run {
    dir: PathBuf,
    mode: String,
    request: Value,
    phase: u32,
    since: Instant,
    pub cancel_rect: egui::Rect,
    pub cancel_state: Option<TaskState>,
    target: u64,
    input: Option<u64>,
    screenshot: Option<u64>,
    baseline_frame: Option<u64>,
    selection_prepared: bool,
    before: Value,
    setup: PathBuf,
    project: PathBuf,
    frames: Vec<Value>,
    failures: Vec<String>,
    finished: bool,
}
impl Run {
    pub fn from_env() -> Option<Self> {
        let dir = directory()?;
        let request: Value =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).ok()?).ok()?;
        assert!(!cfg!(debug_assertions), "native A2 requires release");
        assert!(matches!(
            request["case"].as_str(),
            Some("A" | "B" | "C" | "D" | "E-undo" | "E-redo")
        ));
        let mode = request["case"].as_str()?.to_owned();
        let setup = dir.join("baseline.gbr");
        std::fs::write(
            &setup,
            include_bytes!("../../../fixtures/synthetic/s4c3/blocks.gbr"),
        )
        .ok()?;
        stamp();
        Some(Self {
            project: dir.join("baseline.rcam"),
            dir,
            mode,
            request,
            phase: 0,
            since: Instant::now(),
            cancel_rect: egui::Rect::NOTHING,
            cancel_state: None,
            target: 0,
            input: None,
            screenshot: None,
            baseline_frame: None,
            selection_prepared: false,
            before: Value::Null,
            setup,
            frames: vec![],
            failures: vec![],
            finished: false,
        })
    }
    fn enter(&mut self, phase: u32) {
        self.phase = phase;
        self.since = Instant::now();
    }
    fn key(raw: &mut egui::RawInput, key: egui::Key, shift: bool) {
        let mods = egui::Modifiers {
            mac_cmd: true,
            command: true,
            shift,
            ..Default::default()
        };
        raw.modifiers = mods;
        for pressed in [true, false] {
            raw.events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: mods,
            });
        }
    }
    pub fn input(&mut self, app: &mut EditorApp, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.finished {
            return;
        }
        for e in &raw.events {
            if let egui::Event::Screenshot { image, .. } = e {
                let at = stamp();
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                std::fs::write(self.dir.join(format!("frame-{at}.ppm")), bytes).unwrap();
                let observed = event(
                    "screenshot_received",
                    self.target,
                    json!({"path":format!("frame-{at}.ppm"),"focused":raw.focused}),
                );
                if self.input.is_some() {
                    self.screenshot.get_or_insert(observed);
                } else {
                    self.baseline_frame = Some(at);
                }
            }
        }
        if self.phase == 10 && self.input.is_none() {
            let ready = if self.mode == "A" {
                app.pending_task.as_ref().is_some_and(|t| {
                    t.task_id == self.target
                        && t.cancel_token.state() == TaskState::Running
                        && self.cancel_state == Some(TaskState::Running)
                })
            } else {
                WAITING.load(Ordering::Acquire)
                    && (self.mode != "B" || self.cancel_state == Some(TaskState::Committing))
            };
            if ready && self.cancel_rect.is_positive() && matches!(self.mode.as_str(), "A" | "B") {
                self.input = Some(stamp());
                event(
                    "native_cancel_input",
                    self.target,
                    json!({"phase":self.mode,"rect":[self.cancel_rect.min.x,self.cancel_rect.min.y,self.cancel_rect.max.x,self.cancel_rect.max.y]}),
                );
                let pos = self.cancel_rect.center();
                raw.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    raw.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
            } else if ready && !matches!(self.mode.as_str(), "A" | "B") {
                self.input = Some(stamp());
                event(
                    "native_transition_input",
                    self.target,
                    json!({"kind":self.mode}),
                );
                match self.mode.as_str() {
                    "C" => Self::key(raw, egui::Key::W, false),
                    "E-undo" => Self::key(raw, egui::Key::Z, false),
                    "E-redo" => Self::key(raw, egui::Key::Z, true),
                    "D" => app.begin_transition(crate::project_ui::Transition::Open(
                        self.project.clone(),
                    )),
                    _ => {}
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    "a2-transition",
                )));
            }
        }
    }
    fn target(&mut self, app: &mut EditorApp, action: Action) {
        self.cancel_rect = egui::Rect::NOTHING;
        self.cancel_state = None;
        self.before = state(&app.view);
        self.target = app.sequence + 1;
        TARGET.store(self.target, Ordering::Release);
        WAITING.store(false, Ordering::Release);
        RELEASE.store(false, Ordering::Release);
        COMMIT_BARRIER.store(self.mode == "B", Ordering::Release);
        DELIVERY_BARRIER.store(!matches!(self.mode.as_str(), "A" | "B"), Ordering::Release);
        app.send(action);
        self.enter(10);
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        self.frames.push(json!({"at_ns":stamp(),"phase":self.phase,"sequence":app.sequence,"busy":app.busy,"display_pending":app.display_pending,"has_last_good":app.last_good.is_some(),"pending":app.pending_task.as_ref().map(|t|json!({"id":t.task_id,"state":t.cancel_token.state()})),"receipt":app.view.task_receipt,"state":state(&app.view)}));
        ctx.request_repaint_after(Duration::from_millis(8));
        if self.since.elapsed() > Duration::from_secs(30) {
            self.failures.push(format!("phase {} timeout", self.phase));
            self.finish(app, ctx);
            return;
        }
        if self.phase == 10 && self.input.is_some() && app.sequence > self.target {
            RELEASE.store(true, Ordering::Release);
        }
        match self.phase {
            0 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                app.send(Action::Open(self.setup.clone()));
                self.enter(1);
            }
            1 if !app.busy && app.view.info.is_some() => {
                app.send(Action::SaveProject(Some(self.project.clone()), false, None));
                self.enter(2);
            }
            2 if !app.busy => {
                if app.view.error.is_some() {
                    self.failures.push("setup failed".into());
                    self.finish(app, ctx);
                    return;
                }
                if self.mode.starts_with("E-") {
                    app.send(Action::CreateEmptyLayer(Some("history-marker".into())));
                    self.enter(3);
                } else {
                    self.enter(5);
                }
            }
            3 if !app.busy => {
                if self.mode == "E-redo" {
                    app.send(Action::History(false));
                    self.enter(4);
                } else {
                    self.enter(5);
                }
            }
            4 if !app.busy => {
                self.enter(5);
            }
            5 if !app.busy
                && app.viewport_sequence.is_none()
                && !app.display_pending
                && self.mode == "A"
                && !self.selection_prepared =>
            {
                self.selection_prepared = true;
                app.send(Action::SelectRect(
                    app.view.bounds.unwrap(),
                    editor_core::hit_test::SelectRectMode::Crossing,
                ));
                self.enter(7);
            }
            7 if !app.busy => {
                if app.view.selected.ids().len() != 5 {
                    self.failures
                        .push("nonempty selection baseline failed".into());
                    self.finish(app, ctx);
                    return;
                }
                self.enter(5);
            }
            5 if !app.busy
                && app.viewport_sequence.is_none()
                && !app.display_pending
                && app.last_good.is_some() =>
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    "a2-baseline",
                )));
                self.enter(6);
            }
            6 if self.baseline_frame.is_some() => {
                let action = match self.mode.as_str() {
                    "A" => Action::ImportGerbers(vec![PathBuf::from(
                        self.request["fixture"].as_str().unwrap(),
                    )]),
                    "B" => Action::CreateEmptyLayer(Some("committed-layer".into())),
                    _ => Action::Viewport(
                        app.camera.center,
                        app.view.bounds.unwrap(),
                        app.view.render_ppm,
                    ),
                };
                self.target(app, action);
            }
            10 if self.input.is_some()
                && !app.busy
                && app.viewport_sequence.is_none()
                && self.screenshot.is_some() =>
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    "a2-final",
                )));
                self.enter(11);
            }
            11 if self.since.elapsed() > Duration::from_millis(200) => {
                self.finish(app, ctx);
            }
            _ => {}
        }
    }
    fn finish(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        RELEASE.store(true, Ordering::Release);
        self.finished = true;
        let events = EVENTS.lock().unwrap().clone();
        let target_events: Vec<_> = events
            .iter()
            .filter(|e| e["task_id"] == self.target)
            .collect();
        let find = |name: &str| target_events.iter().find(|e| e["event"] == name).copied();
        let after = state(&app.view);
        let mut feedback_ms = None;
        let mut terminal_ms = None;
        if let (Some(input), Some(frame)) = (self.input, self.screenshot) {
            feedback_ms = Some((frame - input) as f64 / 1e6);
        }
        if let (Some(input), Some(done)) = (self.input, find("worker_returned")) {
            terminal_ms = Some(done["at_ns"].as_u64().unwrap().saturating_sub(input) as f64 / 1e6);
        }
        if self.mode == "A" {
            if find("cancel_button").is_none_or(|e| e["data"]["outcome"] != "Requested") {
                self.failures
                    .push("real Cancel button did not request cancellation".into());
            }
            if self.before != after {
                self.failures
                    .push("cancel changed document/history/selection".into());
            }
            if feedback_ms.is_none_or(|v| v > 500.) || terminal_ms.is_none_or(|v| v > 2000.) {
                self.failures.push("cancel SLA missing/exceeded".into());
            }
        } else if self.mode == "B" {
            if find("cancel_button").is_none_or(|e| e["data"]["outcome"] != "TooLate") {
                self.failures.push("native TooLate not observed".into());
            }
            if after["undo_entries"].as_u64() != self.before["undo_entries"].as_u64().map(|n| n + 1)
            {
                self.failures.push("commit did not produce one undo".into());
            }
        } else {
            if find("reply_received").is_none_or(|e| e["data"]["installed"] != false) {
                self.failures
                    .push("old reply was not actually rejected".into());
            }
            let expected = match self.mode.as_str() {
                "C" => after["document_id"].is_null(),
                "D" => {
                    after["document_id"] != self.before["document_id"]
                        && after["content_sha256"] == self.before["content_sha256"]
                }
                _ => after["revision"] != self.before["revision"],
            };
            if !expected {
                self.failures
                    .push("transition/history result incorrect".into());
            }
        }
        let exe = std::env::current_exe().unwrap();
        let report = json!({"schema_version":2,"stage":"S5-M2-A2","status":"OBSERVED","request":self.request,"commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":editor_core::hash::sha256_hex(&std::fs::read(exe).unwrap()),"profile":"release","adapter":app.adapter,"input_kind":"synthetic native egui RawInput -> real Cancel widget / Command keymap; D uses production Open transition with explicit path; no physical input/scanout claim","clock":"process Instant mapped once to Unix epoch; T1 is the screenshot_received event after writing the completed surface readback (conservative upper bound)","barriers":"A none; B holds Committing until real Cancel widget returns TooLate; C/D/E hold completed viewport reply until next production command is accepted; waits are not production latency","target":self.target,"before":self.before,"after":after,"baseline_frame_ns":self.baseline_frame,"input_ns":self.input,"feedback_upper_bound_ms":feedback_ms,"worker_return_upper_bound_ms":terminal_ms,"events":events,"frames":self.frames,"failures":self.failures});
        std::fs::write(
            self.dir.join("observations.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
