//! Local diagnostics. Typed summaries only: never pass payloads or error messages.
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub mod context;
pub use context::*;

const MAX_EVENT: usize = 4096;
const RING_COUNT: usize = 1000;
static GLOBAL: OnceLock<Arc<Runtime>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Menu,
    Context,
    Toolbar,
    Shortcut,
    Canvas,
    Modal,
    #[default]
    Automation,
    Recovery,
    System,
}
thread_local! { static SOURCE: std::cell::Cell<Source> = const { std::cell::Cell::new(Source::Automation) }; }
pub fn with_source<T>(source: Source, f: impl FnOnce() -> T) -> T {
    struct Reset(Source);
    impl Drop for Reset {
        fn drop(&mut self) {
            SOURCE.set(self.0);
        }
    }
    let _reset = Reset(SOURCE.replace(source));
    f()
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}
impl Level {
    fn rank(self) -> u64 {
        match self {
            Self::Off => 0,
            Self::Error => 1,
            Self::Warn => 2,
            Self::Info => 3,
            Self::Debug => 4,
            Self::Trace => 5,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp_ms: u64,
    pub session_id: String,
    pub app_version: String,
    pub commit: String,
    pub operation_id: u64,
    #[serde(default)]
    pub document_id_hash: Option<String>,
    #[serde(default)]
    pub content_sha256_prefix: Option<String>,
    #[serde(default)]
    pub layer_id_hash: Option<String>,
    pub command_id: String,
    pub source: Source,
    pub phase: String,
    pub revision_before: Option<u64>,
    pub revision_after: Option<u64>,
    pub duration_us: u64,
    #[serde(default)]
    pub metrics: std::collections::BTreeMap<String, u64>,
    pub error_code: Option<String>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn token(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(96)
        .collect()
}

type LogSnapshot = Vec<(String, Vec<u8>)>;

enum Message {
    Event(bool, Vec<u8>),
    Flush(mpsc::Sender<()>),
    Snapshot(mpsc::Sender<io::Result<LogSnapshot>>),
    Stop,
}
pub struct Runtime {
    dir: PathBuf,
    session: String,
    version: String,
    commit: String,
    ring: Mutex<VecDeque<Event>>,
    gpu: Mutex<Option<(String, String)>>,
    summaries: Mutex<std::collections::BTreeMap<String, Event>>,
    tx: SyncSender<Message>,
    dropped: AtomicU64,
    level: AtomicU64,
    io_errors: Arc<AtomicU64>,
}
pub struct Guard {
    runtime: Arc<Runtime>,
    worker: Option<JoinHandle<()>>,
}
impl Runtime {
    pub fn start(dir: PathBuf, version: &str, commit: &str) -> io::Result<Guard> {
        fs::create_dir_all(dir.join("crashes"))?;
        let runtime_log = Rolling::new(&dir, "rcam", 20 * 1024 * 1024, 5)?;
        let operations = Rolling::new(&dir, "operations", 10 * 1024 * 1024, 5)?;
        let (tx, rx) = mpsc::sync_channel(2048);
        let errors = Arc::new(AtomicU64::new(0));
        let worker_errors = errors.clone();
        let worker = std::thread::Builder::new()
            .name("rcam-diagnostics".into())
            .spawn(move || {
                let (mut runtime_log, mut operations) = (runtime_log, operations);
                loop {
                    let result = match rx.recv_timeout(Duration::from_millis(250)) {
                        Ok(Message::Event(operation, bytes)) => {
                            if operation {
                                operations.write(&bytes)
                            } else {
                                runtime_log.write(&bytes)
                            }
                        }
                        Ok(Message::Snapshot(ack)) => {
                            let snapshot = runtime_log.snapshot().and_then(|first| {
                                operations
                                    .snapshot()
                                    .map(|second| first.into_iter().chain(second).collect())
                            });
                            let _ = ack.send(snapshot);
                            Ok(())
                        }
                        Ok(Message::Flush(ack)) => {
                            let r = runtime_log.flush().and_then(|_| operations.flush());
                            let _ = ack.send(());
                            r
                        }
                        Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            let _ = runtime_log.flush();
                            let _ = operations.flush();
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            runtime_log.flush().and_then(|_| operations.flush())
                        }
                    };
                    if result.is_err() {
                        worker_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })?;
        let runtime = Arc::new(Self {
            dir,
            session: format!(
                "{}-{}-{}",
                now(),
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            version: token(version),
            commit: token(commit),
            ring: Mutex::new(VecDeque::with_capacity(RING_COUNT)),
            gpu: Mutex::new(None),
            summaries: Mutex::new(Default::default()),
            tx,
            dropped: AtomicU64::new(0),
            level: AtomicU64::new(3),
            io_errors: errors,
        });
        runtime.runtime_event(Level::Info, "session.start");
        Ok(Guard {
            runtime,
            worker: Some(worker),
        })
    }
    pub fn set_gpu(&self, name: &str, backend: &str) {
        if let Ok(mut gpu) = self.gpu.lock() {
            *gpu = Some((token(name), token(backend)));
        }
        self.runtime_event(Level::Info, "gpu.initialized");
    }
    pub fn set_level(&self, level: Level) {
        self.level.store(level.rank(), Ordering::Relaxed);
        if level != Level::Off {
            let mut event = self.event("logging.level", "Info", 0);
            event.metrics.insert("level_rank".into(), level.rank());
            self.emit(false, event);
        }
    }
    pub fn directory(&self) -> &Path {
        &self.dir
    }
    fn emit(&self, operation: bool, event: Event) {
        if !operation
            && !event.metrics.is_empty()
            && let Ok(mut summaries) = self.summaries.lock()
            && (summaries.contains_key(&summary_key(&event)) || summaries.len() < 512)
        {
            summaries.insert(summary_key(&event), event.clone());
        }
        if operation && let Ok(mut ring) = self.ring.lock() {
            if ring.len() == RING_COUNT {
                ring.pop_front();
            }
            ring.push_back(event.clone());
        }
        if let Ok(mut bytes) = serde_json::to_vec(&event) {
            if bytes.len() > MAX_EVENT {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }
            bytes.push(b'\n');
            if self.tx.try_send(Message::Event(operation, bytes)).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn event(&self, command: &str, phase: &str, id: u64) -> Event {
        Event {
            timestamp_ms: now(),
            session_id: self.session.clone(),
            app_version: self.version.clone(),
            commit: self.commit.clone(),
            operation_id: id,
            document_id_hash: None,
            content_sha256_prefix: None,
            layer_id_hash: None,
            command_id: token(command),
            source: SOURCE.get(),
            phase: token(phase),
            revision_before: None,
            revision_after: None,
            duration_us: 0,
            metrics: Default::default(),
            error_code: None,
        }
    }
    pub fn runtime_event(&self, level: Level, command: &str) {
        if level != Level::Off && level.rank() <= self.level.load(Ordering::Relaxed) {
            self.emit(false, self.event(command, &format!("{level:?}"), 0));
        }
    }
    pub fn recent(&self) -> Vec<Event> {
        self.ring
            .lock()
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default()
    }
    pub fn flush(&self) -> bool {
        let (tx, rx) = mpsc::channel();
        self.tx.try_send(Message::Flush(tx)).is_ok()
            && rx.recv_timeout(Duration::from_secs(2)).is_ok()
    }
    pub fn export(&self, path: &Path) -> io::Result<()> {
        self.export_with_context(path, DiagnosticContext::default())
    }
    pub fn export_with_context(
        &self,
        path: &Path,
        mut context: DiagnosticContext,
    ) -> io::Result<()> {
        context.layers.truncated |= context.layers.layers.len() > 256;
        context.layers.layers.truncate(256);
        if !self.flush() {
            return Err(io::Error::other("diagnostic flush timed out"));
        }
        // Read only explicit diagnostic names; never recursively walk project or log directories.
        let mut files: Vec<(String, Vec<u8>)> = vec![
            (
                "environment.json".into(),
                serde_json::to_vec(
                    &json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH, "profile": if cfg!(debug_assertions) { "debug" } else { "release" }, "gpu": self.gpu.lock().ok().and_then(|g| g.clone()), "app_version": self.version, "commit": self.commit}),
                )?,
            ),
            (
                "diagnostic.json".into(),
                serde_json::to_vec(
                    &json!({"schema_version": 1, "session_id": self.session, "dropped_events": self.dropped.load(Ordering::Relaxed), "io_errors": self.io_errors.load(Ordering::Relaxed), "recent_operations": self.recent()}),
                )?,
            ),
        ];
        let summary = self.summaries.lock().map(|s| s.clone()).unwrap_or_default();
        files.push((
            "project_summary.json".into(),
            serde_json::to_vec(&json!({"schema_version": 1, "project": context.project}))?,
        ));
        files.push((
            "layer_summary.json".into(),
            serde_json::to_vec(&context.layers)?,
        ));
        for (name, compatibility) in [
            ("compatibility_summary.json", true),
            ("performance_summary.json", false),
        ] {
            let selected: Vec<_> = summary
                .values()
                .filter(|e| {
                    if compatibility {
                        e.command_id == "gerber.compatibility.categories"
                    } else {
                        performance_command(&e.command_id)
                    }
                })
                .collect();
            files.push((
                name.into(),
                serde_json::to_vec(
                    &json!({"schema_version":1,"events": selected,"max_events":512}),
                )?,
            ));
        }
        let (tx, rx) = mpsc::channel();
        self.tx
            .try_send(Message::Snapshot(tx))
            .map_err(|_| io::Error::other("diagnostic writer busy or unavailable"))?;
        files.extend(
            rx.recv_timeout(Duration::from_secs(5))
                .map_err(|_| io::Error::other("diagnostic snapshot timed out"))??,
        );
        let mut crashes: Vec<_> = fs::read_dir(self.dir.join("crashes"))?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with("crash-")
                    && entry.path().extension().is_some_and(|ext| ext == "json")
            })
            .collect();
        crashes.sort_by_key(|entry| entry.file_name());
        for entry in crashes.into_iter().rev().take(3) {
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_file() || metadata.len() > 5 * 1024 * 1024 {
                continue;
            }
            // Reconstruct from known typed fields; never copy arbitrary crash payload strings.
            let value: serde_json::Value = serde_json::from_slice(&fs::read(entry.path())?)?;
            let events: Vec<Event> = serde_json::from_value(value["recent_operations"].clone())?;
            let report = json!({"session_id": token(value["session_id"].as_str().unwrap_or_default()), "error_code":"RUST_PANIC", "panic_payload":"redacted", "location": value["location"].as_array().filter(|a| a.len() == 3).map(|a| (token(a[0].as_str().unwrap_or_default()), a[1].as_u64(), a[2].as_u64())), "panic_hash": value["panic_hash"].as_str().map(token), "recent_operations": events.into_iter().take(RING_COUNT).collect::<Vec<_>>()});
            files.push((
                format!("crashes/{}", entry.file_name().to_string_lossy()),
                serde_json::to_vec(&report)?,
            ));
        }
        const MAX_CONTENT: usize = 100 * 1024 * 1024;
        let mut used = files
            .iter()
            .filter(|(name, _)| !name.ends_with(".log"))
            .map(|(_, b)| b.len())
            .sum::<usize>();
        let mut truncated_logs = false;
        files.retain(|(name, bytes)| {
            if !name.ends_with(".log") {
                return true;
            }
            if used.saturating_add(bytes.len()) > MAX_CONTENT - 64 * 1024 {
                truncated_logs = true;
                false
            } else {
                used += bytes.len();
                true
            }
        });
        files.push(("manifest.json".into(), serde_json::to_vec(&json!({"app_version": self.version, "commit": self.commit, "session_id": self.session, "created_at_ms": now(), "redaction_mode": "typed-summary-only", "max_content_bytes": MAX_CONTENT, "truncated_logs": truncated_logs, "included_files": files.iter().map(|(n,_)| n).chain(std::iter::once(&"manifest.json".to_string())).collect::<Vec<_>>()}))?));
        let entries: Vec<_> = files
            .iter()
            .map(|(name, bytes)| rcam_project::zip_codec::ZipEntry {
                path: name,
                data: bytes,
            })
            .collect();
        if entries.iter().map(|e| e.data.len()).sum::<usize>() > MAX_CONTENT {
            return Err(io::Error::other("diagnostic content budget exceeded"));
        }
        let bytes = rcam_project::zip_codec::write_zip(&entries);
        let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
        if let Err(error) = output.write_all(&bytes).and_then(|_| output.sync_all()) {
            drop(output);
            let _ = fs::remove_file(path);
            return Err(error);
        }
        Ok(())
    }
    pub fn write_crash(&self) -> io::Result<()> {
        self.write_crash_details(None, None)
    }
    fn write_crash_details(
        &self,
        location: Option<(String, u32, u32)>,
        panic_hash: Option<String>,
    ) -> io::Result<()> {
        // Intentionally omit arbitrary panic payload/location/backtrace: all can contain private paths or text.
        let recent = self
            .ring
            .try_lock()
            .map(|r| r.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let bytes = serde_json::to_vec(
            &json!({"session_id": self.session, "timestamp_ms": now(), "app_version": self.version, "commit": self.commit, "error_code": "RUST_PANIC", "panic_payload": "redacted", "location": location, "panic_hash": panic_hash, "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "recent_operations": recent}),
        )?;
        let dir = self.dir.join("crashes");
        let path = dir.join(format!(
            "crash-{}-{}.json",
            now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut out = OpenOptions::new().create_new(true).write(true).open(path)?;
        out.write_all(&bytes)?;
        let mut paths: Vec<_> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("crash-")
                    && e.path().extension().is_some_and(|s| s == "json")
            })
            .map(|e| e.path())
            .collect();
        paths.sort();
        let excess = paths.len().saturating_sub(20);
        for path in paths.into_iter().take(excess) {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }
}
impl Guard {
    pub fn runtime(&self) -> Arc<Runtime> {
        self.runtime.clone()
    }
    pub fn install_sink(&self) -> bool {
        GLOBAL.set(self.runtime.clone()).is_ok()
    }
    pub fn install(&self) -> bool {
        if GLOBAL.set(self.runtime.clone()).is_err() {
            return false;
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(runtime) = GLOBAL.get() {
                let location = info.location().map(|l| {
                    (
                        token(l.file().rsplit(['/', '\\']).next().unwrap_or("unknown")),
                        l.line(),
                        l.column(),
                    )
                });
                let message = info
                    .payload()
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str));
                let hash = message
                    .map(|s| editor_core::hash::sha256_hex(&s.as_bytes()[..s.len().min(4096)]));
                let _ = runtime.write_crash_details(location, hash);
            }
            // Preserve the standard panic/backtrace path even when reporting fails.
            previous(info);
        }));
        true
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.runtime.runtime_event(
            Level::Info,
            if std::thread::panicking() {
                "session.end.panic"
            } else {
                "session.end.clean"
            },
        );
        let _ = self.runtime.tx.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn summary_key(event: &Event) -> String {
    format!(
        "{}:{}:{}",
        event.command_id,
        event.content_sha256_prefix.as_deref().unwrap_or_default(),
        event.layer_id_hash.as_deref().unwrap_or_default()
    )
}
fn performance_command(command: &str) -> bool {
    [
        "project.open.",
        "project.save.",
        "gerber.import.",
        "gerber.export.",
        "snap.query.",
    ]
    .iter()
    .any(|prefix| command.starts_with(prefix))
        || command == "render_index.build"
}
pub fn measurements(level: Level, command: &'static str, fields: &[(&'static str, u64)]) {
    identified_measurements(level, command, None, None, fields)
}
/// Identities are hex-only, bounded hashes; arbitrary paths/text cannot enter events.
pub fn identified_measurements(
    level: Level,
    command: &'static str,
    content_hash: Option<&str>,
    layer_id: Option<&str>,
    fields: &[(&'static str, u64)],
) {
    if let Some(runtime) = global() {
        if level == Level::Off || level.rank() > runtime.level.load(Ordering::Relaxed) {
            return;
        }
        let mut event = runtime.event(command, &format!("{level:?}"), 0);
        event.content_sha256_prefix = content_hash
            .filter(|h| h.len() >= 16 && h.bytes().all(|c| c.is_ascii_hexdigit()))
            .map(|h| h[..16].to_ascii_lowercase());
        event.layer_id_hash = layer_id.map(hash_identity);
        event.metrics = fields
            .iter()
            .take(24)
            .map(|(k, v)| (token(k), *v))
            .collect();
        runtime.emit(false, event);
    }
}
#[derive(Clone, Copy)]
pub enum RenderException {
    DisplayPrepareFailed,
    LastGoodFrameFallback,
    ResourceLimit,
    SurfaceError,
    DeviceLost,
}
pub fn render_exception(kind: RenderException) {
    static LAST: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
    let (index, command) = match kind {
        RenderException::DisplayPrepareFailed => (0, "render.display_prepare_failed"),
        RenderException::LastGoodFrameFallback => (1, "render.last_good_frame_fallback"),
        RenderException::ResourceLimit => (2, "render.resource_limit"),
        RenderException::SurfaceError => (3, "render.surface_error"),
        RenderException::DeviceLost => (4, "render.device_lost"),
    };
    rate_limited(&LAST[index], Level::Warn, command, &[("window_ms", 1000)]);
}
/// Monotonic, aggregate stage timing; never includes user strings.
pub struct Timing {
    command: &'static str,
    start: Instant,
}
impl Timing {
    pub fn start(command: &'static str) -> Self {
        Self {
            command,
            start: Instant::now(),
        }
    }
}
impl Drop for Timing {
    fn drop(&mut self) {
        measurements(
            Level::Info,
            self.command,
            &[(
                "duration_us",
                self.start.elapsed().as_micros().min(u64::MAX as u128) as u64,
            )],
        );
    }
}
/// Per-call-site suppression; hot paths never write to disk or acquire the writer lock.
pub fn rate_limited(
    last: &AtomicU64,
    level: Level,
    command: &'static str,
    fields: &[(&'static str, u64)],
) {
    let current = now();
    let old = last.load(Ordering::Relaxed);
    if current.saturating_sub(old) >= 1000
        && last
            .compare_exchange(old, current, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        measurements(level, command, fields);
    }
}
pub fn global() -> Option<&'static Arc<Runtime>> {
    GLOBAL.get()
}
pub fn runtime_event(level: Level, command: &str) {
    if let Some(runtime) = global() {
        runtime.runtime_event(level, command);
    }
}
pub struct Operation {
    runtime: Option<Arc<Runtime>>,
    event: Option<Event>,
    start: Instant,
}
impl Operation {
    pub fn begin(command: &str, revision: Option<u64>) -> Self {
        Self::with_runtime(
            global()
                .filter(|r| r.level.load(Ordering::Relaxed) != 0)
                .cloned(),
            command,
            revision,
        )
    }
    pub fn begin_document(command: &str, document_id: &str, revision: Option<u64>) -> Self {
        let runtime = global()
            .filter(|r| r.level.load(Ordering::Relaxed) != 0)
            .cloned();
        let event = runtime.as_ref().map(|r| {
            let mut event = r.event(command, "begin", NEXT.fetch_add(1, Ordering::Relaxed));
            event.revision_before = revision;
            event.document_id_hash =
                Some(editor_core::hash::sha256_hex(document_id.as_bytes())[..16].into());
            r.emit(true, event.clone());
            event
        });
        Self {
            runtime,
            event,
            start: Instant::now(),
        }
    }
    pub fn with_runtime(
        runtime: Option<Arc<Runtime>>,
        command: &str,
        revision: Option<u64>,
    ) -> Self {
        let event = runtime.as_ref().map(|r| {
            let mut event = r.event(command, "begin", NEXT.fetch_add(1, Ordering::Relaxed));
            event.revision_before = revision;
            r.emit(true, event.clone());
            event
        });
        Self {
            runtime,
            event,
            start: Instant::now(),
        }
    }
    /// Fixed numeric categories only; never accept manufacturing payloads.
    pub fn grip_metadata(&mut self, geometry_kind: u64, grip_kind: u64) {
        if let Some(event) = &mut self.event {
            event.metrics.insert("geometry_kind".into(), geometry_kind);
            event.metrics.insert("grip_kind".into(), grip_kind);
        }
    }
    /// Fixed arrangement categories and counts; anchor identity is hashed, never logged raw.
    pub fn arrangement_metadata(
        &mut self,
        mode: Option<u64>,
        axis: Option<u64>,
        anchor: Option<&str>,
        count: usize,
        moved: usize,
    ) {
        if let Some(event) = &mut self.event {
            if let Some(mode) = mode {
                event.metrics.insert("mode".into(), mode);
            }
            if let Some(axis) = axis {
                event.metrics.insert("axis".into(), axis);
            }
            if let Some(anchor) = anchor {
                let hash = editor_core::hash::sha256_hex(anchor.as_bytes());
                event.metrics.insert(
                    "anchor_hash".into(),
                    u64::from_str_radix(&hash[..16], 16).expect("hex SHA256"),
                );
            }
            event.metrics.insert("object_count".into(), count as u64);
            event.metrics.insert("moved_count".into(), moved as u64);
            event.metrics.insert("changed_objects".into(), moved as u64);
        }
    }
    pub fn selection_count(&mut self, count: usize) {
        if let Some(event) = &mut self.event {
            event.metrics.insert("selection_count".into(), count as u64);
        }
    }
    /// Array dimensions/counts and pitch categories only; never numeric geometry.
    pub fn array_metadata(
        &mut self,
        spec: editor_core::edit::RectangularArray,
        source: usize,
        created: usize,
        contains_block: bool,
    ) {
        let sign = |v: f64| {
            if !v.is_finite() {
                3
            } else if v == 0.0 {
                0
            } else if v > 0.0 {
                1
            } else {
                2
            }
        };
        if let Some(event) = &mut self.event {
            for (key, value) in [
                ("rows", spec.rows),
                ("columns", spec.columns),
                (
                    "cell_count",
                    spec.rows.checked_mul(spec.columns).unwrap_or(0),
                ),
                ("source_object_count", source as u64),
                ("created_object_count", created as u64),
                ("source_contains_block", u64::from(contains_block)),
                ("pitch_x_sign", sign(spec.pitch_x_mm)),
                ("pitch_y_sign", sign(spec.pitch_y_mm)),
            ] {
                event.metrics.insert(key.into(), value);
            }
        }
    }
    pub fn end(mut self, revision: Option<u64>, error: Option<&str>) {
        if let (Some(runtime), Some(mut event)) = (&self.runtime, self.event.take()) {
            event.phase = if error.is_some() { "error" } else { "ok" }.into();
            event.revision_after = revision;
            event.error_code = error.map(token);
            event.duration_us = self.start.elapsed().as_micros().min(u64::MAX as u128) as u64;
            runtime.emit(true, event);
        }
    }
}
// An unwound operation keeps its BEGIN in the ring, without inventing a successful END.
struct Rolling {
    dir: PathBuf,
    name: String,
    limit: u64,
    count: usize,
    bytes: u64,
    file: Option<io::BufWriter<File>>,
}
impl Rolling {
    fn new(dir: &Path, name: &str, limit: u64, count: usize) -> io::Result<Self> {
        let path = dir.join(format!("{name}.log"));
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            dir: dir.into(),
            name: name.into(),
            limit,
            count,
            bytes,
            file: Some(io::BufWriter::new(file)),
        })
    }
    fn snapshot(&mut self) -> io::Result<LogSnapshot> {
        self.flush()?;
        let mut files = Vec::new();
        for n in 0..=1 {
            let path = self.path(n);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) if n > 0 && e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if !metadata.is_file() || metadata.len() > self.limit {
                return Err(io::Error::other("invalid diagnostic log"));
            }
            files.push((
                if n == 0 {
                    format!("{}.log", self.name)
                } else {
                    format!("{}.{n}.log", self.name)
                },
                fs::read(path)?,
            ));
        }
        Ok(files)
    }
    fn path(&self, n: usize) -> PathBuf {
        self.dir.join(if n == 0 {
            format!("{}.log", self.name)
        } else {
            format!("{}.{n}.log", self.name)
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("log unavailable"))?
            .flush()
    }
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() as u64 > self.limit {
            return Err(io::Error::other("oversized log event"));
        }
        if self.bytes + bytes.len() as u64 > self.limit {
            self.flush()?;
            self.file.take();
            for n in (1..self.count).rev() {
                let from = self.path(n - 1);
                if from.exists() {
                    let to = self.path(n);
                    if to.exists() {
                        fs::remove_file(&to)?;
                    }
                    fs::rename(from, to)?;
                }
            }
            self.file = Some(io::BufWriter::new(
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(self.path(0))?,
            ));
            self.bytes = 0;
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("log unavailable"))?
            .write_all(bytes)?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rcam-diagnostics-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn performance_summary_is_an_explicit_allowlist() {
        for name in [
            "project.open.read",
            "project.save.codec",
            "gerber.import.summary",
            "gerber.export.summary",
            "render_index.build",
            "snap.query.slow_or_high_pairs",
        ] {
            assert!(performance_command(name));
        }
        for name in [
            "logging.level",
            "future.metrics",
            "gerber.compatibility.categories",
            "project.unrelated",
            "snap.acquire",
        ] {
            assert!(!performance_command(name));
        }
    }
    #[test]
    fn rotation_and_retention() {
        let dir = dir();
        let mut log = Rolling::new(&dir, "test", 16, 3).unwrap();
        for _ in 0..20 {
            log.write(b"12345678\n").unwrap();
        }
        log.flush().unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 3);
        for f in fs::read_dir(&dir).unwrap() {
            assert!(f.unwrap().metadata().unwrap().len() <= 16);
        }
        assert_eq!(log.snapshot().unwrap().len(), 2);
        drop(log);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ring_and_export_are_bounded_and_private() {
        let dir = dir();
        let guard = Runtime::start(dir.clone(), "1", "test").unwrap();
        let runtime = guard.runtime();
        for _ in 0..1100 {
            Operation::with_runtime(Some(runtime.clone()), "objects.move", Some(1))
                .end(Some(2), None);
        }
        assert_eq!(runtime.recent().len(), 1000);
        fs::write(dir.join("secret.gbr"), b"PRIVATE").unwrap();
        let zip = dir.join("diagnostics.zip");
        runtime.export(&zip).unwrap();
        let entries = rcam_project::zip_codec::read_zip(
            &fs::read(zip).unwrap(),
            &rcam_project::zip_codec::ReadPolicy {
                max_entries: 10,
                max_uncompressed_bytes: 40 * 1024 * 1024,
                max_entry_bytes: 20 * 1024 * 1024,
                max_path_len: 100,
            },
        )
        .unwrap();
        let names: std::collections::BTreeSet<_> =
            entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(
            names,
            [
                "environment.json",
                "diagnostic.json",
                "project_summary.json",
                "layer_summary.json",
                "compatibility_summary.json",
                "performance_summary.json",
                "rcam.log",
                "operations.log",
                "manifest.json"
            ]
            .into_iter()
            .collect()
        );
        for e in entries {
            assert!(!String::from_utf8_lossy(&e.data).contains("PRIVATE"));
        }
        drop(guard);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn crash_retention_and_incomplete_operation() {
        let dir = dir();
        let guard = Runtime::start(dir.clone(), "1", "test").unwrap();
        let runtime = guard.runtime();
        let _operation = Operation::with_runtime(Some(runtime.clone()), "objects.rotate", Some(4));
        for _ in 0..22 {
            runtime.write_crash().unwrap();
        }
        assert_eq!(fs::read_dir(dir.join("crashes")).unwrap().count(), 20);
        assert_eq!(runtime.recent().last().unwrap().phase, "begin");
        drop(guard);
        fs::remove_dir_all(dir).unwrap();
    }
}
