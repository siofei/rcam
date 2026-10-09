//! Opt-in local source-clock observation. This never submits or waits for GPU work.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const CAPACITY: usize = 4096;
const WRITER_BATCH_CAPACITY: usize = 16;
const META_LIMIT: u64 = 4 * 1024 * 1024;
const OUTPUT_LIMIT: u64 = 128 * 1024 * 1024;
const FOOTER_RESERVE: u64 = 8192;

/// Decimal strings preserve integer nanoseconds in consumers using IEEE doubles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct Ns(pub u64);
impl TryFrom<String> for Ns {
    type Error = std::num::ParseIntError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse().map(Self)
    }
}
impl Serialize for Ns {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Window {
    pub id: String,
    pub start_ns: Ns,
    pub end_ns: Ns,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    run_id: String,
    output: PathBuf,
    expected_commit: String,
    expected_manifest_sha256: String,
    expected_binary_sha256: String,
    clock_id: String,
    windows: Vec<Window>,
}
impl Config {
    fn validate(&self, clock_id: &str) -> Result<(), String> {
        if self.run_id.is_empty()
            || self.run_id.len() > 64
            || !self
                .run_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            || !self.output.is_absolute()
            || self.clock_id != clock_id
            || self.windows.len() > 16
        {
            return Err("invalid run, output, clock or window count".into());
        }
        for hash in [&self.expected_manifest_sha256, &self.expected_binary_sha256] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err("expected SHA256 must be 64 lowercase hex digits".into());
            }
        }
        for (index, window) in self.windows.iter().enumerate() {
            if window.id.is_empty()
                || window.id.len() > 64
                || !window
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                || window.start_ns >= window.end_ns
                || self.windows[..index].iter().any(|w| w.id == window.id)
                || index > 0 && self.windows[index - 1].end_ns > window.start_ns
            {
                return Err("windows must be named, unique, ordered and nonoverlapping".into());
            }
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", test))]
fn convert_ticks(ticks: u64, numer: u32, denom: u32) -> Option<u64> {
    if numer == 0 || denom == 0 {
        return None;
    }
    u64::try_from(u128::from(ticks) * u128::from(numer) / u128::from(denom)).ok()
}
struct Clock {
    failed: AtomicBool,
    #[cfg(not(target_os = "macos"))]
    origin: Instant,
    #[cfg(target_os = "macos")]
    timebase: [u32; 2],
    #[cfg(target_os = "macos")]
    origin_ticks: Ns,
    #[cfg(target_os = "macos")]
    origin_ns: Ns,
}
impl Clock {
    fn new() -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            #[repr(C)]
            struct Timebase {
                numer: u32,
                denom: u32,
            }
            unsafe extern "C" {
                fn mach_timebase_info(info: *mut Timebase) -> i32;
                fn mach_absolute_time() -> u64;
            }
            let mut info = Timebase { numer: 0, denom: 0 };
            // Public libSystem API writes exactly this initialized C layout.
            if unsafe { mach_timebase_info(&mut info) } != 0 {
                return Err("mach_timebase_info unavailable".into());
            }
            let ticks = unsafe { mach_absolute_time() };
            let ns = convert_ticks(ticks, info.numer, info.denom)
                .ok_or("Mach clock conversion out of range")?;
            Ok(Self {
                failed: AtomicBool::new(false),
                timebase: [info.numer, info.denom],
                origin_ticks: Ns(ticks),
                origin_ns: Ns(ns),
            })
        }
        #[cfg(not(target_os = "macos"))]
        Ok(Self {
            failed: AtomicBool::new(false),
            origin: Instant::now(),
        })
    }
    fn id(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "mach_absolute_time_ns"
        } else {
            "rust_instant_run_relative_ns"
        }
    }
    fn now(&self) -> Ns {
        #[cfg(target_os = "macos")]
        let value = {
            unsafe extern "C" {
                fn mach_absolute_time() -> u64;
            }
            convert_ticks(
                unsafe { mach_absolute_time() },
                self.timebase[0],
                self.timebase[1],
            )
        };
        #[cfg(not(target_os = "macos"))]
        let value = u64::try_from(self.origin.elapsed().as_nanos()).ok();
        match value {
            Some(ns) => Ns(ns),
            None => {
                self.failed.store(true, Ordering::Release);
                Ns(0)
            }
        }
    }
    fn metadata(&self) -> serde_json::Value {
        #[cfg(target_os = "macos")]
        let (conversion, ticks, origin) = (
            serde_json::json!({"numer":self.timebase[0],"denom":self.timebase[1]}),
            Some(self.origin_ticks),
            self.origin_ns,
        );
        #[cfg(not(target_os = "macos"))]
        let (conversion, ticks, origin) = (serde_json::Value::Null, None::<Ns>, Ns(0));
        serde_json::json!({"clock_id":self.id(),"unit":"ns","integer_encoding":"decimal_string",
            "epoch":if cfg!(target_os="macos") {"Mach absolute tick epoch; external domains require calibration"} else {"this recorder's Instant origin; no cross-process equivalence"},
            "conversion":conversion,"origin_raw_ticks":ticks,"origin_source_ns":origin,"metadata_sample_source_ns":self.now(),
            "clock_failed":self.failed.load(Ordering::Acquire),"sleep_wake":"not validated; classify suspension independently"})
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage {
    InputBatch,
    Update,
    Validation,
    RealPrepare,
    CanvasPanel,
    CallbackPrepare,
    CallbackPaint,
    RequestAttempt,
}
const STAGES: usize = 8;
#[derive(Clone, Copy, Default, Serialize)]
pub(crate) struct Binding {
    pub egui_identity_known: bool,
    pub viewport_id: u64,
    pub update_id: u64,
    pub egui_frame_nr: u64,
    pub egui_pass_nr: u64,
    pub pass_index: usize,
    pub input_batch_id: u64,
    pub version_id: u64,
    pub render_callback_id: u64,
    pub rendered_source_version_id: u64,
    pub rendered_source_update_id: u64,
    pub rendered_scene_serial: u64,
}
#[derive(Clone, Copy, Default, Serialize)]
pub(crate) struct Snapshot {
    pub input_focused: bool,
    pub selected_count: usize,
    pub move_phase: &'static str,
    pub drag_active: bool,
    pub canvas_physical: Option<[f32; 2]>,
    pub effective_ppp: f32,
    pub native_ppp: Option<f32>,
    pub ui_zoom: f32,
    pub canvas_layout: &'static str,
    pub pending_task_id: Option<std::num::NonZeroU64>,
    pub viewport_task_id: Option<std::num::NonZeroU64>,
    pub geometry_task_id: Option<std::num::NonZeroU64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MoveExitReason {
    FocusLost,
    PointerGone,
    WindowFocusLost,
    Escape,
    SecondaryClick,
    PointPickCancelled,
    ModalCancelled,
    ModalReplaced,
    ToolChanged,
    ModalTaskCompleted,
    ContextChanged,
    Transition,
    InitialAdmissionRejected,
    TaskIdentityMismatch,
    ApplyTerminal,
    TaskCancelled,
    TaskError,
    PlacementMissing,
    InvalidBase,
    PreviewPhaseMismatch,
    PreviewMissing,
    WorkerDisconnected,
    RequestInvalid,
    AdmissionRejected,
}
#[derive(Default, Serialize)]
struct InputDetail {
    focused: bool,
    pointer_moved: usize,
    pointer_button: usize,
    pointer_gone: usize,
    window_focus_gained: usize,
    window_focus_lost: usize,
}
#[derive(Clone, Copy, Serialize)]
pub(crate) struct RenderSource {
    pub update_id: u64,
    pub version_id: u64,
    pub scene_serial: u64,
    pub input_batch_id: u64,
    pub canvas_physical: [f32; 2],
    pub effective_ppp: f32,
    pub selection_epoch: u64,
}
#[derive(Clone, Serialize, PartialEq, Eq)]
struct Version {
    project_id: Option<String>,
    document_id: Option<String>,
    revision: Option<String>,
    workspace_revision: Option<String>,
    task_generation: u64,
    rule_revision: u64,
    selection_epoch: u64,
    scene_serial: Option<u64>,
    installed_open_generation: u64,
}
impl Version {
    fn matches(&self, view: &crate::state::View, open_generation: u64) -> bool {
        let info = view.info.as_ref();
        self.project_id.as_deref() == info.map(|i| i.project_id.as_str())
            && self.document_id.as_deref() == info.map(|i| i.document_id.as_str())
            && self.revision.as_deref() == info.map(|i| i.revision.as_str())
            && self.workspace_revision.as_deref() == info.map(|i| i.workspace_revision.as_str())
            && self.task_generation == view.task_generation
            && self.rule_revision == view.rule_revision
            && self.selection_epoch == view.selection_epoch
            && self.scene_serial == view.scene.as_ref().map(|s| s.serial)
            && self.installed_open_generation == open_generation
    }
    fn capture(view: &crate::state::View, open_generation: u64) -> Self {
        let info = view.info.as_ref();
        Self {
            project_id: info.map(|i| i.project_id.clone()),
            document_id: info.map(|i| i.document_id.clone()),
            revision: info.map(|i| i.revision.clone()),
            workspace_revision: info.map(|i| i.workspace_revision.clone()),
            task_generation: view.task_generation,
            rule_revision: view.rule_revision,
            selection_epoch: view.selection_epoch,
            scene_serial: view.scene.as_ref().map(|s| s.serial),
            installed_open_generation: open_generation,
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Metadata {
    Version {
        version_id: u64,
        version: Version,
    },
    Task {
        status: &'static str,
        attempt_id: Option<u64>,
        task_id: u64,
        input: editor_service::task::TaskVersion,
        result: Option<editor_service::task::TaskVersion>,
        state: Option<editor_service::task::TaskState>,
    },
}
impl Metadata {
    fn strings(&self) -> Vec<&str> {
        match self {
            Self::Version { version, .. } => [
                &version.project_id,
                &version.document_id,
                &version.revision,
                &version.workspace_revision,
            ]
            .into_iter()
            .filter_map(|v| v.as_deref())
            .collect(),
            Self::Task { input, result, .. } => std::iter::once(input)
                .chain(result.iter())
                .flat_map(|v| {
                    [
                        v.document_id.as_deref(),
                        v.document_revision.as_deref(),
                        v.workspace_revision.as_deref(),
                        Some(v.geometry_policy_hash.as_str()),
                    ]
                })
                .flatten()
                .collect(),
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Payload {
    RenderSource {
        source: RenderSource,
    },
    CallbackEnqueued {
        binding: Binding,
    },
    RequestAttempt {
        attempt_id: u64,
        task_id: Option<u64>,
        binding: Binding,
        start_ns: Ns,
        end_ns: Ns,
        wall_duration_ns: Ns,
        outcome: &'static str,
    },
    Span {
        stage: Stage,
        binding: Binding,
        start_ns: Ns,
        end_ns: Ns,
        wall_duration_ns: Ns,
        outcome: &'static str,
    },
    UpdateStart {
        binding: Binding,
        previous: Option<Endpoint>,
        snapshot: Snapshot,
    },
    UpdateEnd {
        binding: Binding,
        end_version_id: u64,
        snapshot: Snapshot,
    },
    InputCounts {
        input_batch_id: u64,
        viewport_id: u64,
        counts: [usize; 4],
        detail: InputDetail,
    },
    MoveExit {
        binding: Binding,
        phase: &'static str,
        reason: MoveExitReason,
    },
    Metadata {
        metadata: Box<Metadata>,
    },
}
#[derive(Clone, Copy, Serialize)]
struct Endpoint {
    update_id: u64,
    source_ns: Ns,
    version_id: u64,
    input_batch_id: u64,
}
#[derive(Serialize)]
struct Record {
    record_seq: u64,
    source_ns: Ns,
    #[serde(flatten)]
    payload: Payload,
}

struct Shared {
    queue: Mutex<VecDeque<Record>>,
    wake: Condvar,
    clock: Clock,
    closing: AtomicBool,
    close_ready: AtomicBool,
    failed: AtomicBool,
    inflight: AtomicU64,
    sequence: AtomicU64,
    callback_sequence: AtomicU64,
    accepted: AtomicU64,
    consumed: AtomicU64,
    dropped: [AtomicU64; STAGES],
    dropped_metadata: AtomicU64,
    dropped_other: AtomicU64,
    full: AtomicU64,
    contention: AtomicU64,
    late: AtomicU64,
    high_water: AtomicU64,
    first_drop: AtomicU64,
    last_drop: AtomicU64,
    metadata_bytes: AtomicU64,
    cutoff: AtomicU64,
    supervisor_timeout: AtomicBool,
}
impl Shared {
    fn new(clock: Clock, capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            wake: Condvar::new(),
            clock,
            closing: AtomicBool::new(false),
            close_ready: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            inflight: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            callback_sequence: AtomicU64::new(0),
            accepted: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
            dropped: std::array::from_fn(|_| AtomicU64::new(0)),
            dropped_metadata: AtomicU64::new(0),
            dropped_other: AtomicU64::new(0),
            full: AtomicU64::new(0),
            contention: AtomicU64::new(0),
            late: AtomicU64::new(0),
            high_water: AtomicU64::new(0),
            first_drop: AtomicU64::new(u64::MAX),
            last_drop: AtomicU64::new(0),
            metadata_bytes: AtomicU64::new(0),
            cutoff: AtomicU64::new(0),
            supervisor_timeout: AtomicBool::new(false),
        })
    }
    fn active(&self) -> bool {
        !self.closing.load(Ordering::Acquire)
            && !self.failed.load(Ordering::Acquire)
            && !self.clock.failed.load(Ordering::Acquire)
    }
    fn drop_record(&self, payload: &Payload, at: Ns) {
        match payload {
            Payload::Span { stage, .. } => &self.dropped[*stage as usize],
            Payload::RequestAttempt { .. } => &self.dropped[Stage::RequestAttempt as usize],
            Payload::Metadata { .. } => &self.dropped_metadata,
            _ => &self.dropped_other,
        }
        .fetch_add(1, Ordering::Relaxed);
        self.first_drop.fetch_min(at.0, Ordering::Relaxed);
        self.last_drop.fetch_max(at.0, Ordering::Relaxed);
    }
    fn emit(&self, at: Ns, payload: Payload) {
        self.emit_inner(at, payload, false);
    }
    fn emit_inner(&self, at: Ns, payload: Payload, admitted_span: bool) {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        if self.clock.failed.load(Ordering::Acquire) {
            self.failed.store(true, Ordering::Release);
        }
        if self.closing.load(Ordering::SeqCst) && !admitted_span {
            self.late.fetch_add(1, Ordering::Relaxed);
            self.inflight.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let mut wake = false;
        if self.failed.load(Ordering::Acquire) {
            self.drop_record(&payload, at);
        } else if let Ok(mut queue) = self.queue.try_lock() {
            if self.closing.load(Ordering::SeqCst) && !admitted_span {
                self.late.fetch_add(1, Ordering::Relaxed);
            } else if queue.len() == queue.capacity() {
                self.full.fetch_add(1, Ordering::Relaxed);
                self.drop_record(&payload, at);
            } else {
                wake = queue.is_empty();
                queue.push_back(Record {
                    record_seq: seq,
                    source_ns: at,
                    payload,
                });
                self.high_water
                    .fetch_max(queue.len() as u64, Ordering::Relaxed);
                self.accepted.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.contention.fetch_add(1, Ordering::Relaxed);
            self.drop_record(&payload, at);
        }
        self.inflight.fetch_sub(1, Ordering::SeqCst);
        // Notify after releasing the queue lock, only for a new nonempty queue.
        // Close and admitted span completion have their own independent wakes.
        if wake {
            self.wake.notify_one();
        }
    }
    fn writer_batch(&self) -> Result<([Option<Record>; WRITER_BATCH_CAPACITY], bool), String> {
        let mut records = std::array::from_fn(|_| None);
        let mut queue = self.queue.lock().map_err(|_| "trace queue poisoned")?;
        if queue.is_empty()
            && (!self.close_ready.load(Ordering::Acquire)
                || self.inflight.load(Ordering::SeqCst) != 0)
        {
            queue = self
                .wake
                .wait_timeout(queue, Duration::from_millis(250))
                .map_err(|_| "trace queue poisoned")?
                .0;
        }
        for slot in &mut records {
            let Some(record) = queue.pop_front() else {
                break;
            };
            *slot = Some(record);
        }
        // An empty queue after draining this batch is not a finished writer:
        // every record in the batch must first be written and counted.
        let drained = records[0].is_none()
            && self.close_ready.load(Ordering::Acquire)
            && self.inflight.load(Ordering::SeqCst) == 0;
        drop(queue);
        Ok((records, drained))
    }
    fn metadata(&self, metadata: Metadata) {
        if !self.active() {
            return;
        }
        let strings = metadata.strings();
        let capacities = match &metadata {
            Metadata::Version { version, .. } => [
                &version.project_id,
                &version.document_id,
                &version.revision,
                &version.workspace_revision,
            ]
            .into_iter()
            .flatten()
            .map(String::capacity)
            .sum::<usize>(),
            Metadata::Task { input, result, .. } => std::iter::once(input)
                .chain(result.iter())
                .map(|v| {
                    [&v.document_id, &v.document_revision, &v.workspace_revision]
                        .into_iter()
                        .flatten()
                        .map(String::capacity)
                        .sum::<usize>()
                        + v.geometry_policy_hash.capacity()
                })
                .sum(),
        };
        // Cumulative conservative charge: queued value, transient/cache clone, scratch.
        let cost = (std::mem::size_of::<Metadata>() + capacities) * 2
            + strings.capacity() * std::mem::size_of::<&str>();
        if strings.iter().any(|v| v.len() > 256)
            || self
                .metadata_bytes
                .fetch_add(cost as u64, Ordering::Relaxed)
                + cost as u64
                > META_LIMIT
        {
            self.failed.store(true, Ordering::Release);
        }
        self.emit(
            self.clock.now(),
            Payload::Metadata {
                metadata: Box::new(metadata),
            },
        );
    }
    fn counters(&self) -> serde_json::Value {
        let dropped: Vec<_> = self
            .dropped
            .iter()
            .map(|v| v.load(Ordering::Relaxed))
            .collect();
        serde_json::json!({"attempted":self.sequence.load(Ordering::Relaxed),"accepted":self.accepted.load(Ordering::Relaxed),
            "consumed":self.consumed.load(Ordering::Relaxed),"drop_by_stage":dropped,"stage_order":["input_batch","update","validation","real_prepare","canvas_panel","callback_prepare","callback_paint","request_attempt"],
            "dropped_metadata":self.dropped_metadata.load(Ordering::Relaxed),"dropped_other":self.dropped_other.load(Ordering::Relaxed),
            "queue_full":self.full.load(Ordering::Relaxed),"queue_contention":self.contention.load(Ordering::Relaxed),
            "queue_high_water":self.high_water.load(Ordering::Relaxed),"queue_capacity":CAPACITY,
            "first_drop_source_ns":(self.first_drop.load(Ordering::Relaxed)!=u64::MAX).then(||Ns(self.first_drop.load(Ordering::Relaxed))),"last_drop_source_ns":Ns(self.last_drop.load(Ordering::Relaxed)),
            "metadata_bytes_charged":self.metadata_bytes.load(Ordering::Relaxed),"record_size_bytes":std::mem::size_of::<Record>(),
            "producer_inflight":self.inflight.load(Ordering::SeqCst),"late_attempts_at_footer":self.late.load(Ordering::Relaxed),
            "clock_failed":self.clock.failed.load(Ordering::Acquire),"failed":self.failed.load(Ordering::Acquire),"supervisor_timeout":self.supervisor_timeout.load(Ordering::Acquire)})
    }
    fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.cutoff.store(self.clock.now().0, Ordering::Relaxed);
        self.close_ready.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}

#[derive(Clone)]
pub(crate) struct CallbackBinding {
    shared: Arc<Shared>,
    binding: Binding,
}
impl CallbackBinding {
    pub fn span(&self, stage: Stage) -> Span {
        Span::new(self.shared.clone(), self.binding, stage)
    }
}
pub(crate) struct Span {
    shared: Arc<Shared>,
    binding: Binding,
    stage: Stage,
    start: Ns,
    pub outcome: &'static str,
    pub attempt_id: Option<u64>,
    pub task_id: Option<u64>,
    end: Option<Ns>,
    admitted: bool,
}
impl Span {
    fn new(shared: Arc<Shared>, binding: Binding, stage: Stage) -> Self {
        let start = shared.clock.now();
        Self::new_at(shared, binding, stage, start)
    }
    fn new_at(shared: Arc<Shared>, binding: Binding, stage: Stage, start: Ns) -> Self {
        shared.inflight.fetch_add(1, Ordering::SeqCst);
        let admitted =
            !shared.closing.load(Ordering::SeqCst) && !shared.failed.load(Ordering::Acquire);
        if !admitted {
            shared.inflight.fetch_sub(1, Ordering::SeqCst);
        }
        Self {
            shared,
            binding,
            stage,
            start,
            outcome: "returned",
            attempt_id: None,
            task_id: None,
            end: None,
            admitted,
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if !self.admitted {
            return;
        }
        let end = self.end.unwrap_or_else(|| self.shared.clock.now());
        if let Some(attempt_id) = self.attempt_id {
            self.shared.emit_inner(
                end,
                Payload::RequestAttempt {
                    attempt_id,
                    task_id: self.task_id,
                    binding: self.binding,
                    start_ns: self.start,
                    end_ns: end,
                    wall_duration_ns: Ns(end.0.saturating_sub(self.start.0)),
                    outcome: self.outcome,
                },
                true,
            );
            self.shared.inflight.fetch_sub(1, Ordering::SeqCst);
            self.shared.wake.notify_one();
            return;
        }
        self.shared.emit_inner(
            end,
            Payload::Span {
                stage: self.stage,
                binding: self.binding,
                start_ns: self.start,
                end_ns: end,
                wall_duration_ns: Ns(end.0.saturating_sub(self.start.0)),
                outcome: self.outcome,
            },
            true,
        );
        self.shared.inflight.fetch_sub(1, Ordering::SeqCst);
        self.shared.wake.notify_one();
    }
}
pub(crate) struct Update {
    span: Span,
    canvas: std::cell::Cell<Option<[f32; 2]>>,
    layout_seen: std::cell::Cell<bool>,
}
impl Update {
    pub fn stage(&self, stage: Stage, version_id: u64) -> Span {
        let mut binding = self.span.binding;
        binding.version_id = version_id;
        Span::new(self.span.shared.clone(), binding, stage)
    }
    pub fn callback(&self, enqueue_version: u64, source: Option<RenderSource>) -> CallbackBinding {
        let mut binding = self.span.binding;
        binding.version_id = enqueue_version;
        binding.render_callback_id = self
            .span
            .shared
            .callback_sequence
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        binding.rendered_source_version_id = source.map_or(0, |s| s.version_id);
        binding.rendered_source_update_id = source.map_or(0, |s| s.update_id);
        binding.rendered_scene_serial = source.map_or(0, |s| s.scene_serial);
        self.span.shared.emit(
            self.span.shared.clock.now(),
            Payload::CallbackEnqueued { binding },
        );
        CallbackBinding {
            shared: self.span.shared.clone(),
            binding,
        }
    }
    pub fn current_canvas(&self, rect: eframe::egui::Rect, ppp: f32) {
        self.layout_seen.set(true);
        self.canvas.set(
            (rect.is_positive() && ppp.is_finite() && ppp > 0.)
                .then(|| [rect.width() * ppp, rect.height() * ppp]),
        );
    }
    pub fn finish(mut self, end_version: u64, mut snapshot: Snapshot) {
        snapshot.canvas_physical = self.canvas.get();
        snapshot.canvas_layout = if self.layout_seen.get() {
            "current_update_layout"
        } else {
            "layout_not_reached"
        };
        let end = self.span.shared.clock.now();
        self.span.end = Some(end);
        self.span.shared.emit_inner(
            end,
            Payload::UpdateEnd {
                binding: self.span.binding,
                end_version_id: end_version,
                snapshot,
            },
            true,
        );
        self.span.outcome = "normal_end";
    }
    pub fn render_source(&self, mut source: RenderSource) -> RenderSource {
        source.update_id = self.span.binding.update_id;
        source.input_batch_id = self.span.binding.input_batch_id;
        self.span.shared.emit(
            self.span.shared.clock.now(),
            Payload::RenderSource { source },
        );
        source
    }
}

pub(crate) struct Recorder {
    shared: Arc<Shared>,
    run_id: String,
    done: Option<mpsc::Receiver<Result<(), String>>>,
    writer_thread: Option<std::thread::JoinHandle<()>>,
    version: Option<Version>,
    version_id: u64,
    input_id: u64,
    root_input_id: u64,
    update_id: u64,
    previous: Option<Endpoint>,
    open_generation: u64,
    open_task: Option<u64>,
    attempt_id: u64,
    current_binding: Option<Binding>,
}
impl Recorder {
    pub fn from_env() -> Option<Self> {
        match std::env::var("RCAM_FRAME_TRACE") {
            Err(std::env::VarError::NotPresent) => return None,
            Ok(value) if value == "0" => return None,
            Ok(value) if value == "1" => {}
            _ => {
                eprintln!("RCAM_FRAME_TRACE initialization failed: expected explicit 0 or 1");
                return None;
            }
        }
        match Self::initialize() {
            Ok(recorder) => Some(recorder),
            Err(error) => {
                eprintln!("RCAM_FRAME_TRACE initialization failed: {error}");
                None
            }
        }
    }
    fn initialize() -> Result<Self, String> {
        let path =
            std::env::var_os("RCAM_FRAME_TRACE_CONFIG").ok_or("configuration path is required")?;
        let file = File::open(path).map_err(|_| "cannot read trace configuration")?;
        let mut bytes = Vec::new();
        file.take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read trace configuration")?;
        if bytes.len() > 65536 {
            return Err("configuration exceeds 64KiB".into());
        }
        let config: Config =
            serde_json::from_slice(&bytes).map_err(|_| "invalid trace configuration schema")?;
        let clock = Clock::new()?;
        config.validate(clock.id())?;
        let commit = option_env!("RCAM_BUILD_COMMIT").unwrap_or("unknown");
        let source = option_env!("RCAM_BUILD_SOURCE").unwrap_or("unknown");
        let manifest_sha =
            editor_core::hash::sha256_hex(include_bytes!("../../../MANIFEST.sha256"));
        if config.expected_commit != commit || config.expected_manifest_sha256 != manifest_sha {
            return Err("source identity mismatch".into());
        }
        if std::mem::size_of::<Record>() > 256
            || std::mem::size_of::<[Option<Record>; WRITER_BATCH_CAPACITY]>() > 4096
        {
            return Err("numeric queue budget exceeded".into());
        }
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config.output)
            .map_err(|_| "trace output must be a new writable file")?;
        let shared = Shared::new(clock, CAPACITY);
        let (done_tx, done) = mpsc::sync_channel(1);
        let sink = shared.clone();
        let run_id = config.run_id.clone();
        let writer_thread = std::thread::Builder::new()
            .name("rcam-frame-trace".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let result = writer(
                    output,
                    &sink,
                    config,
                    editor_core::hash::sha256_hex(&bytes),
                    commit,
                    source,
                    manifest_sha,
                );
                if result.is_err() {
                    sink.failed.store(true, Ordering::Release);
                    eprintln!("RCAM_FRAME_TRACE writer failed; evidence incomplete");
                }
                let _ = done_tx.send(result);
            })
            .map_err(|_| "cannot start trace writer")?;
        Ok(Self {
            shared,
            run_id,
            done: Some(done),
            writer_thread: Some(writer_thread),
            version: None,
            version_id: 0,
            input_id: 0,
            root_input_id: 0,
            update_id: 0,
            previous: None,
            open_generation: 0,
            open_task: None,
            attempt_id: 0,
            current_binding: None,
        })
    }
    pub fn version(&mut self, view: &crate::state::View) -> u64 {
        if !self.shared.active() {
            return self.version_id;
        }
        if view.info.as_ref().is_some_and(|i| {
            [
                &i.project_id,
                &i.document_id,
                &i.revision,
                &i.workspace_revision,
            ]
            .iter()
            .any(|v| v.len() > 256)
        }) {
            self.shared.failed.store(true, Ordering::Release);
            return self.version_id;
        }
        if self
            .version
            .as_ref()
            .is_none_or(|v| !v.matches(view, self.open_generation))
        {
            let version = Version::capture(view, self.open_generation);
            self.version_id += 1;
            self.shared.metadata(Metadata::Version {
                version_id: self.version_id,
                version: version.clone(),
            });
            self.version = Some(version);
        }
        self.version_id
    }
    pub fn input(&mut self, raw: &eframe::egui::RawInput) -> Option<Span> {
        if !self.shared.active() {
            return None;
        }
        self.input_id += 1;
        if raw.viewport_id == eframe::egui::ViewportId::ROOT {
            self.root_input_id = self.input_id;
        }
        let binding = Binding {
            input_batch_id: self.input_id,
            viewport_id: raw.viewport_id.0.value(),
            ..Default::default()
        };
        let span = Span::new(self.shared.clone(), binding, Stage::InputBatch);
        let mut counts = [0; 4];
        let mut detail = InputDetail {
            focused: raw.focused,
            ..Default::default()
        };
        for event in &raw.events {
            match event {
                eframe::egui::Event::PointerMoved(..) => detail.pointer_moved += 1,
                eframe::egui::Event::PointerButton { .. } => detail.pointer_button += 1,
                eframe::egui::Event::PointerGone => detail.pointer_gone += 1,
                eframe::egui::Event::WindowFocused(true) => detail.window_focus_gained += 1,
                eframe::egui::Event::WindowFocused(false) => detail.window_focus_lost += 1,
                _ => {}
            }
            let index = match event {
                eframe::egui::Event::PointerMoved(..)
                | eframe::egui::Event::PointerButton { .. }
                | eframe::egui::Event::PointerGone => 0,
                eframe::egui::Event::Key { .. } => 1,
                eframe::egui::Event::MouseWheel { .. } => 2,
                _ => 3,
            };
            counts[index] += 1;
        }
        self.shared.emit(
            span.start,
            Payload::InputCounts {
                input_batch_id: self.input_id,
                viewport_id: raw.viewport_id.0.value(),
                counts,
                detail,
            },
        );
        Some(span)
    }
    pub fn move_exit(
        &mut self,
        view: &crate::state::View,
        phase: &'static str,
        reason: MoveExitReason,
    ) {
        if !self.shared.active() {
            return;
        }
        let version_id = self.version(view);
        self.shared.emit(
            self.shared.clock.now(),
            Payload::MoveExit {
                binding: Binding {
                    version_id,
                    ..self.current_binding.unwrap_or_default()
                },
                phase,
                reason,
            },
        );
    }
    pub fn update(
        &mut self,
        start: Ns,
        ctx: &eframe::egui::Context,
        view: &crate::state::View,
        snapshot: Snapshot,
    ) -> Option<Update> {
        if !self.shared.active() {
            return None;
        }
        let version_id = self.version(view);
        self.update_id += 1;
        let binding = Binding {
            update_id: self.update_id,
            egui_frame_nr: ctx.cumulative_frame_nr(),
            egui_pass_nr: ctx.cumulative_pass_nr(),
            pass_index: ctx.current_pass_index(),
            egui_identity_known: true,
            viewport_id: ctx.viewport_id().0.value(),
            input_batch_id: self.root_input_id,
            version_id,
            ..Default::default()
        };
        let mut span = Span::new_at(self.shared.clone(), binding, Stage::Update, start);
        if !span.admitted {
            return None;
        }
        self.current_binding = Some(binding);
        self.shared.emit(
            start,
            Payload::UpdateStart {
                binding,
                previous: self.previous,
                snapshot,
            },
        );
        self.previous = Some(Endpoint {
            update_id: self.update_id,
            source_ns: start,
            version_id,
            input_batch_id: self.root_input_id,
        });
        span.outcome = "unfinished_or_unwind";
        Some(Update {
            span,
            canvas: std::cell::Cell::new(None),
            layout_seen: std::cell::Cell::new(false),
        })
    }
    pub fn request_attempt(&mut self, view: &crate::state::View) -> Option<Span> {
        if !self.shared.active() {
            return None;
        }
        let version_id = self.version(view);
        let mut span = Span::new(
            self.shared.clone(),
            Binding {
                version_id,
                ..self.current_binding.unwrap_or_default()
            },
            Stage::RequestAttempt,
        );
        self.attempt_id += 1;
        span.attempt_id = Some(self.attempt_id);
        Some(span)
    }
    fn valid_task(&self, version: &editor_service::task::TaskVersion) -> bool {
        let valid = [
            version.document_id.as_deref(),
            version.document_revision.as_deref(),
            version.workspace_revision.as_deref(),
            Some(version.geometry_policy_hash.as_str()),
        ]
        .into_iter()
        .flatten()
        .all(|value| value.len() <= 256);
        if !valid {
            self.shared.failed.store(true, Ordering::Release);
        }
        valid
    }
    pub fn accepted(
        &mut self,
        task: &editor_service::task::TaskContext,
        opens_project: bool,
        attempt_id: Option<u64>,
    ) {
        if !self.shared.active() {
            return;
        }
        if !self.valid_task(&task.input) {
            return;
        }
        if opens_project {
            self.open_task = Some(task.task_id);
        }
        self.shared.metadata(Metadata::Task {
            status: "accepted",
            attempt_id,
            task_id: task.task_id,
            input: task.input.clone(),
            result: None,
            state: None,
        });
    }
    pub fn received(&self, id: u64, view: &crate::state::View) {
        if !self.shared.active() {
            return;
        }
        if let Some(receipt) = &view.task_receipt {
            if !self.valid_task(&receipt.input) || !self.valid_task(&receipt.result_version) {
                return;
            }
            self.shared.metadata(Metadata::Task {
                status: "received",
                attempt_id: None,
                task_id: id,
                input: receipt.input.clone(),
                result: Some(receipt.result_version.clone()),
                state: Some(receipt.state),
            });
        }
    }
    pub fn installed(&mut self, id: u64, view: &crate::state::View) {
        if !self.shared.active() {
            return;
        }
        if self.open_task == Some(id) {
            if view.error.is_none() {
                self.open_generation += 1;
            }
            self.open_task = None;
        }
        self.version(view);
    }
    pub fn finish(&mut self) {
        if let Some(done) = self.done.take() {
            self.shared.close();
            let joined = self.writer_thread.take().is_some_and(|thread| {
                supervise_writer(&self.shared, done, thread, Duration::from_secs(2))
            });
            match joined {
                true => {
                    eprintln!(
                        "RCAM_FRAME_TRACE finalization_ack=ok run_id={}",
                        self.run_id
                    );
                }
                _ => {
                    eprintln!(
                        "RCAM_FRAME_TRACE finalization_ack=unconfirmed run_id={}; evidence incomplete",
                        self.run_id
                    );
                }
            }
        }
    }
    pub fn clear_update(&mut self) {
        self.current_binding = None;
    }
    pub fn source_now(&self) -> Option<Ns> {
        self.shared.active().then(|| self.shared.clock.now())
    }
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self {
            shared: Shared::new(Clock::new().unwrap(), CAPACITY),
            run_id: "test".into(),
            done: None,
            writer_thread: None,
            version: None,
            version_id: 0,
            input_id: 0,
            root_input_id: 0,
            update_id: 0,
            previous: None,
            open_generation: 0,
            open_task: None,
            attempt_id: 0,
            current_binding: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn take_test_records(&self) -> Vec<serde_json::Value> {
        self.shared
            .queue
            .lock()
            .unwrap()
            .drain(..)
            .map(|record| serde_json::to_value(record).unwrap())
            .collect()
    }
}
fn supervise_writer(
    shared: &Shared,
    done: mpsc::Receiver<Result<(), String>>,
    thread: std::thread::JoinHandle<()>,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    let result = done.recv_timeout(timeout);
    if matches!(result, Err(mpsc::RecvTimeoutError::Timeout)) {
        shared.supervisor_timeout.store(true, Ordering::Release);
    }
    let flushed = matches!(result, Ok(Ok(())));
    // Never join a live thread: a blocked file write cannot be interrupted here.
    while !thread.is_finished() && Instant::now() < deadline {
        if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            std::thread::park_timeout(remaining.min(Duration::from_millis(1)));
        }
    }
    if Instant::now() >= deadline || !thread.is_finished() {
        shared.supervisor_timeout.store(true, Ordering::Release);
        if thread.is_finished() {
            let _ = thread.join();
        }
        return false; // Detach; a later footer cannot replace the failed supervision.
    }
    let joined = thread.join().is_ok();
    if Instant::now() >= deadline {
        shared.supervisor_timeout.store(true, Ordering::Release);
        return false;
    }
    if !flushed || !joined {
        shared.failed.store(true, Ordering::Release);
        return false;
    }
    true
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.finish();
    }
}

fn line<T: Serialize>(
    writer: &mut impl Write,
    value: &T,
    bytes: &mut u64,
    limit: u64,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(value).map_err(|_| "trace serialization failed")?;
    if bytes.saturating_add(encoded.len() as u64 + 1) > limit {
        return Err("trace output budget exhausted".into());
    }
    writer
        .write_all(&encoded)
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|_| "trace I/O failed")?;
    *bytes += encoded.len() as u64 + 1;
    Ok(())
}
fn executable_sha256() -> Result<String, String> {
    let mut executable =
        File::open(std::env::current_exe().map_err(|_| "cannot identify executable")?)
            .map_err(|_| "cannot read executable")?;
    let mut hash = editor_core::hash::Sha256::new();
    let mut chunk = [0u8; 65536];
    loop {
        let count = executable
            .read(&mut chunk)
            .map_err(|_| "cannot hash executable")?;
        if count == 0 {
            break;
        }
        hash.update(&chunk[..count]);
    }
    Ok(hash.finish())
}
fn writer(
    output: File,
    shared: &Shared,
    config: Config,
    config_sha: String,
    commit: &str,
    source: &str,
    manifest_sha: String,
) -> Result<(), String> {
    let binary_sha = executable_sha256()?;
    if binary_sha != config.expected_binary_sha256 {
        return Err("binary identity mismatch".into());
    }
    let mut output = BufWriter::with_capacity(65536, output);
    let mut bytes = 0;
    let header = serde_json::json!({"kind":"header","schema_version":2,"run_id":config.run_id,"commit":commit,"build_source":source,
        "source_manifest_sha256":manifest_sha,"binary_sha256":binary_sha,"config_sha256":config_sha,"clock":shared.clock.metadata(),
        "windows":config.windows,"window_rule":"predefined [start,end); endpoints and boundary overlaps retained; gaps incomplete",
        "unavailable":{"actual_submission_index":null,"submit_source_ns":null,"gpu_completed_source_ns":null,"present_source_ns":null,"scanout_source_ns":null,
        "exact_dispatch_event_id":null,"reason":"framework submit/present are internal; RawInput lacks native dispatch ID/time; callback ID is encoding association only"},
        "duration_scope":"wall duration, may include preemption; not CPU utilization or GPU time",
        "viewport_scope":"App::update ROOT only; raw non-ROOT batches are not mapped to ROOT updates",
        "legacy_render_timing_enabled":std::env::var_os("RCAM_RENDER_TIMING").is_some(),
        "internal_evidence_compiled":cfg!(feature="internal-evidence"),
        "pid":std::process::id(), "observation":"passive, no screenshots, forced repaint, input injection or GPU waits", "queue":"bounded try_lock; drop new on contention/full",
        "supervision_required":"matching finalization_ack=ok requires footer/flush and completed writer thread join; late footer after timeout is insufficient",
        "writer_join_required":true,
        "metadata_limit":META_LIMIT,"output_limit":OUTPUT_LIMIT,
        "writer_batch_capacity":WRITER_BATCH_CAPACITY,
        "writer_batch_size_bytes":std::mem::size_of::<[Option<Record>; WRITER_BATCH_CAPACITY]>()});
    line(
        &mut output,
        &header,
        &mut bytes,
        OUTPUT_LIMIT - FOOTER_RESERVE,
    )?;
    output.flush().map_err(|_| "trace flush failed")?;
    line(
        &mut output,
        &serde_json::json!({"kind":"identity_ready","source_ns":shared.clock.now()}),
        &mut bytes,
        OUTPUT_LIMIT - FOOTER_RESERVE,
    )?;
    let mut last_flush = Instant::now();
    let mut flushed_bytes = bytes;
    let mut max_writer_lag = 0;
    let mut max_record_write_wall = 0;
    let mut flush_count = 1u64;
    loop {
        let (records, drained) = shared.writer_batch()?;
        if drained {
            break;
        }
        let dequeued_at = shared.clock.now();
        for record in records.into_iter().flatten() {
            max_writer_lag = max_writer_lag.max(dequeued_at.0.saturating_sub(record.source_ns.0));
            let write_start = Instant::now();
            if let Err(error) = line(
                &mut output,
                &record,
                &mut bytes,
                OUTPUT_LIMIT - FOOTER_RESERVE,
            ) {
                shared.failed.store(true, Ordering::Release);
                // Preserve failure status if the medium remains writable.
                let _ = line(
                    &mut output,
                    &serde_json::json!({"kind":"writer_failure","reason":error,"source_ns":shared.clock.now(),"counters":shared.counters()}),
                    &mut bytes,
                    OUTPUT_LIMIT,
                );
                let _ = output.flush();
                return Err(error);
            }
            max_record_write_wall = max_record_write_wall
                .max(write_start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
            shared.consumed.fetch_add(1, Ordering::Relaxed);
            if last_flush.elapsed() >= Duration::from_millis(250) || bytes - flushed_bytes >= 262144
            {
                output.flush().map_err(|_| "trace flush failed")?;
                flush_count += 1;
                last_flush = Instant::now();
                flushed_bytes = bytes;
            }
        }
        if last_flush.elapsed() >= Duration::from_millis(250) || bytes - flushed_bytes >= 262144 {
            output.flush().map_err(|_| "trace flush failed")?;
            flush_count += 1;
            last_flush = Instant::now();
            flushed_bytes = bytes;
        }
    }
    line(
        &mut output,
        &serde_json::json!({"kind":"footer","source_ns":shared.clock.now(),"cutoff_source_ns":Ns(shared.cutoff.load(Ordering::Relaxed)),
        "finalized":!shared.failed.load(Ordering::Acquire) && !shared.supervisor_timeout.load(Ordering::Acquire),
        "observation_complete":!shared.failed.load(Ordering::Acquire) && !shared.clock.failed.load(Ordering::Acquire) && shared.sequence.load(Ordering::Relaxed)==shared.consumed.load(Ordering::Relaxed),
        "bytes_before_footer":bytes,"max_dequeue_age_ns":Ns(max_writer_lag),"max_record_serialization_write_wall_ns":Ns(max_record_write_wall),"flush_count_before_footer":flush_count,"counters":shared.counters()}),
        &mut bytes,
        OUTPUT_LIMIT,
    )?;
    output
        .flush()
        .map_err(|_| "trace final flush failed".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shared(capacity: usize) -> Arc<Shared> {
        Shared::new(Clock::new().unwrap(), capacity)
    }
    #[test]
    fn raw_input_classification_excludes_event_contents_and_positions() {
        let mut recorder = Recorder::for_test();
        let raw = eframe::egui::RawInput {
            focused: false,
            events: vec![
                eframe::egui::Event::PointerMoved(eframe::egui::pos2(12345., 67890.)),
                eframe::egui::Event::PointerButton {
                    pos: eframe::egui::pos2(12345., 67890.),
                    button: eframe::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
                eframe::egui::Event::PointerGone,
                eframe::egui::Event::WindowFocused(true),
                eframe::egui::Event::WindowFocused(false),
                eframe::egui::Event::Text("private-content-marker".into()),
            ],
            ..Default::default()
        };
        drop(recorder.input(&raw));
        let rows = recorder.take_test_records();
        let counts = rows.iter().find(|r| r["kind"] == "input_counts").unwrap();
        assert_eq!(counts["counts"], serde_json::json!([3, 0, 0, 3]));
        assert_eq!(
            counts["detail"],
            serde_json::json!({
                "focused":false,"pointer_moved":1,"pointer_button":1,"pointer_gone":1,
                "window_focus_gained":1,"window_focus_lost":1
            })
        );
        let encoded = serde_json::to_string(&rows).unwrap();
        for forbidden in [
            "private-content-marker",
            "12345",
            "67890",
            "pressed",
            "modifiers",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }
    #[test]
    fn source_endpoints_survive_delayed_consumption_and_full_queue() {
        let shared = shared(1);
        shared.emit(
            Ns(10),
            Payload::Span {
                stage: Stage::Update,
                binding: Binding::default(),
                start_ns: Ns(2),
                end_ns: Ns(10),
                wall_duration_ns: Ns(8),
                outcome: "test",
            },
        );
        shared.emit(
            Ns(90),
            Payload::Span {
                stage: Stage::Update,
                binding: Binding::default(),
                start_ns: Ns(80),
                end_ns: Ns(90),
                wall_duration_ns: Ns(10),
                outcome: "test",
            },
        );
        let record = shared.queue.lock().unwrap().pop_front().unwrap();
        assert_eq!(record.source_ns, Ns(10));
        assert_eq!(shared.full.load(Ordering::Relaxed), 1);
        assert_eq!(
            shared.dropped[Stage::Update as usize].load(Ordering::Relaxed),
            1
        );
        assert_eq!(shared.first_drop.load(Ordering::Relaxed), 90);
        assert_eq!(shared.sequence.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn contention_and_close_do_not_wait_or_accept_after_cutoff() {
        let shared = shared(4);
        let lock = shared.queue.lock().unwrap();
        shared.emit(
            Ns(20),
            Payload::UpdateEnd {
                binding: Binding::default(),
                end_version_id: 1,
                snapshot: Snapshot::default(),
            },
        );
        assert_eq!(shared.contention.load(Ordering::Relaxed), 1);
        drop(lock);
        shared.close();
        shared.emit(
            Ns(30),
            Payload::UpdateEnd {
                binding: Binding::default(),
                end_version_id: 1,
                snapshot: Snapshot::default(),
            },
        );
        assert_eq!(shared.late.load(Ordering::Relaxed), 1);
        assert!(shared.queue.lock().unwrap().is_empty());
        assert_eq!(shared.inflight.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn staged_batch_releases_queue_during_blocked_io_and_close_preserves_fifo() {
        let shared = shared(64);
        for id in 0..33 {
            shared.emit(
                Ns(id),
                Payload::UpdateEnd {
                    binding: Binding::default(),
                    end_version_id: id,
                    snapshot: Snapshot::default(),
                },
            );
        }
        let consumer = shared.clone();
        let (started_tx, started) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            struct BlockedSink {
                started: Option<mpsc::SyncSender<()>>,
                released: mpsc::Receiver<()>,
                bytes: Vec<u8>,
            }
            impl Write for BlockedSink {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    if let Some(started) = self.started.take() {
                        started.send(()).unwrap();
                        self.released.recv().unwrap();
                    }
                    self.bytes.extend_from_slice(bytes);
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let mut sink = BlockedSink {
                started: Some(started_tx),
                released,
                bytes: Vec::new(),
            };
            let mut bytes = 0;
            loop {
                let (batch, drained) = consumer.writer_batch().unwrap();
                if drained {
                    break;
                }
                for record in batch.into_iter().flatten() {
                    line(&mut sink, &record, &mut bytes, OUTPUT_LIMIT).unwrap();
                    consumer.consumed.fetch_add(1, Ordering::Relaxed);
                }
            }
            sink.bytes
        });
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(shared.queue.lock().unwrap().len(), 17);
        // File I/O is blocked with a staged batch, but admission still succeeds.
        shared.emit(
            Ns(33),
            Payload::UpdateEnd {
                binding: Binding::default(),
                end_version_id: 33,
                snapshot: Snapshot::default(),
            },
        );
        shared.close();
        assert_eq!(shared.accepted.load(Ordering::Relaxed), 34);
        assert_eq!(shared.consumed.load(Ordering::Relaxed), 0);
        release.send(()).unwrap();
        let raw = thread.join().unwrap();
        let rows: Vec<serde_json::Value> = std::str::from_utf8(&raw)
            .unwrap()
            .lines()
            .map(|row| serde_json::from_str(row).unwrap())
            .collect();
        assert_eq!(rows.len(), 34);
        for (id, row) in rows.iter().enumerate() {
            assert_eq!(row["record_seq"], id as u64 + 1);
            assert_eq!(row["end_version_id"], id as u64);
        }
        assert_eq!(shared.consumed.load(Ordering::Relaxed), 34);
        assert_eq!(shared.contention.load(Ordering::Relaxed), 0);
        assert_eq!(shared.full.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn empty_staging_waits_for_admitted_span_after_close() {
        let shared = shared(64);
        let span = Span::new(shared.clone(), Binding::default(), Stage::CallbackPaint);
        shared.close();
        let (batch, drained) = shared.writer_batch().unwrap();
        assert!(batch.iter().all(Option::is_none));
        assert!(!drained);
        drop(span);
        let (batch, drained) = shared.writer_batch().unwrap();
        assert!(!drained);
        assert!(matches!(
            batch[0].as_ref().unwrap().payload,
            Payload::Span { .. }
        ));
        drop(batch);
        assert!(shared.writer_batch().unwrap().1);
    }
    #[test]
    fn admitted_span_crossing_close_is_retained_and_delays_finalization() {
        let shared = shared(8);
        let span = Span::new(shared.clone(), Binding::default(), Stage::CallbackPaint);
        let start = span.start;
        shared.close();
        assert!(shared.inflight.load(Ordering::SeqCst) > 0);
        assert!(
            !shared.queue.lock().unwrap().is_empty() || shared.inflight.load(Ordering::SeqCst) != 0
        );
        drop(span);
        assert_eq!(shared.inflight.load(Ordering::SeqCst), 0);
        assert_eq!(shared.accepted.load(Ordering::Relaxed), 1);
        let record = shared.queue.lock().unwrap().pop_front().unwrap();
        if let Payload::Span {
            start_ns, end_ns, ..
        } = record.payload
        {
            assert_eq!(start_ns, start);
            assert!(end_ns.0 >= shared.cutoff.load(Ordering::Relaxed));
        } else {
            panic!("expected retained crossing span");
        }
    }
    #[test]
    fn integer_json_and_budget_are_exact() {
        assert_eq!(convert_ticks(u64::MAX, 1, 1), Some(u64::MAX));
        assert_eq!(convert_ticks(u64::MAX, 2, 1), None);
        assert_eq!(convert_ticks(1, 1, 0), None);
        assert_eq!(convert_ticks(3, 125, 3), Some(125));
        assert_eq!(
            serde_json::to_string(&Ns(9007199254740993)).unwrap(),
            "\"9007199254740993\""
        );
        assert_eq!(
            serde_json::from_str::<Ns>("\"9007199254740993\"")
                .unwrap()
                .0,
            9007199254740993
        );
        assert!(std::mem::size_of::<Record>() <= 256);
        assert!(std::mem::size_of::<[Option<Record>; WRITER_BATCH_CAPACITY]>() <= 4096);
        let mut output = Vec::new();
        let mut bytes = 0;
        assert!(line(&mut output, &"long value", &mut bytes, 2).is_err());
        assert!(output.is_empty());
    }
    #[test]
    fn config_requires_matching_clock_unique_windows_and_integer_endpoints() {
        let mut config = Config {
            run_id: "run".into(),
            output: PathBuf::from("/tmp/example.trace"),
            expected_commit: "example".into(),
            expected_manifest_sha256: "a".repeat(64),
            expected_binary_sha256: "b".repeat(64),
            clock_id: "clock".into(),
            windows: vec![Window {
                id: "active".into(),
                start_ns: Ns(20),
                end_ns: Ns(40),
            }],
        };
        assert!(config.validate("clock").is_ok());
        assert!(config.validate("other_clock").is_err());
        config.windows.push(Window {
            id: "next".into(),
            start_ns: Ns(39),
            end_ns: Ns(80),
        });
        assert!(config.validate("clock").is_err());
        let mut encoded = serde_json::to_value(config).unwrap();
        encoded["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Config>(encoded).is_err());
    }
    #[test]
    fn early_return_multi_pass_and_fallback_keep_separate_source_versions() {
        let mut recorder = Recorder::for_test();
        let ctx = eframe::egui::Context::default();
        let mut view = crate::state::View::default();
        let mut raw = eframe::egui::RawInput::default();
        raw.events
            .push(eframe::egui::Event::PointerMoved(eframe::egui::pos2(
                1., 2.,
            )));
        drop(recorder.input(&raw));
        let mut passes = 0;
        let _ = ctx.run(raw, |ctx| {
            let start = recorder.source_now().unwrap();
            let update = recorder
                .update(start, ctx, &view, Snapshot::default())
                .unwrap();
            let old = recorder.version(&view);
            view.task_generation += 1;
            let new = recorder.version(&view);
            let callback = update.callback(
                new,
                Some(RenderSource {
                    update_id: 44,
                    version_id: old,
                    scene_serial: 91,
                    input_batch_id: 1,
                    canvas_physical: [10., 10.],
                    effective_ppp: 1.,
                    selection_epoch: 0,
                }),
            );
            drop(callback.span(Stage::CallbackPaint));
            if passes == 0 {
                ctx.request_discard("trace pass regression");
            }
            passes += 1;
            // Deliberately exercise RAII without normal_end.
        });
        let records = recorder.take_test_records();
        let starts: Vec<_> = records
            .iter()
            .filter(|r| r["kind"] == "update_start")
            .collect();
        assert!(starts.len() >= 2);
        assert_eq!(
            starts[0]["binding"]["input_batch_id"],
            starts[1]["binding"]["input_batch_id"]
        );
        assert_eq!(
            starts[0]["binding"]["egui_frame_nr"],
            starts[1]["binding"]["egui_frame_nr"]
        );
        assert_ne!(
            starts[0]["binding"]["pass_index"],
            starts[1]["binding"]["pass_index"]
        );
        let paint = records
            .iter()
            .find(|r| r["stage"] == "callback_paint")
            .unwrap();
        assert_ne!(
            paint["binding"]["version_id"],
            paint["binding"]["rendered_source_version_id"]
        );
        assert_eq!(paint["binding"]["rendered_scene_serial"], 91);
        assert!(
            records
                .iter()
                .filter(|r| r["stage"] == "update")
                .all(|r| r["outcome"] == "unfinished_or_unwind")
        );
        assert_eq!(starts[1]["previous"]["source_ns"], starts[0]["source_ns"]);
    }
    #[test]
    fn same_identity_reopen_gets_recorder_only_generation_and_early_reject_has_no_task() {
        let mut recorder = Recorder::for_test();
        let view = crate::state::View::default();
        let before = recorder.version(&view);
        let task = editor_service::task::TaskContext::new(42, Default::default());
        recorder.accepted(&task, true, None);
        recorder.installed(42, &view);
        assert_ne!(before, recorder.version(&view));
        assert_eq!(view.task_generation, 0);
        let mut attempt = recorder.request_attempt(&view).unwrap();
        attempt.outcome = "rejected_early";
        drop(attempt);
        let records = recorder.take_test_records();
        let rejected = records
            .iter()
            .find(|r| r["kind"] == "request_attempt")
            .unwrap();
        assert!(rejected["task_id"].is_null());
        assert!(rejected["attempt_id"].as_u64().unwrap() > 0);
        let version = records
            .iter()
            .filter(|r| r["metadata"]["kind"] == "version")
            .next_back()
            .unwrap();
        assert_eq!(
            version["metadata"]["version"]["installed_open_generation"],
            1
        );
    }
    #[test]
    fn metadata_gap_and_oversize_fail_closed_for_evidence_only() {
        let shared = shared(1);
        let mut view = crate::state::View::default();
        shared.metadata(Metadata::Version {
            version_id: 1,
            version: Version::capture(&view, 0),
        });
        shared.metadata(Metadata::Version {
            version_id: 2,
            version: Version::capture(&view, 0),
        });
        assert_eq!(shared.dropped_metadata.load(Ordering::Relaxed), 1);
        view.task_generation = 2;
        let mut version = Version::capture(&view, 0);
        version.document_id = Some("x".repeat(257));
        shared.metadata(Metadata::Version {
            version_id: 3,
            version,
        });
        assert!(shared.failed.load(Ordering::Acquire));
        assert_eq!(view.task_generation, 2);
    }
    #[test]
    fn close_race_never_leaves_accepted_records_outside_drained_queue() {
        let shared = shared(CAPACITY);
        let producer = shared.clone();
        let thread = std::thread::spawn(move || {
            for _ in 0..5000 {
                producer.emit(
                    producer.clock.now(),
                    Payload::UpdateEnd {
                        binding: Binding::default(),
                        end_version_id: 0,
                        snapshot: Snapshot::default(),
                    },
                );
            }
        });
        shared.close();
        thread.join().unwrap();
        let queue = shared.queue.lock().unwrap();
        assert_eq!(queue.len() as u64, shared.accepted.load(Ordering::Relaxed));
        assert_eq!(shared.inflight.load(Ordering::SeqCst), 0);
        assert!(
            queue
                .iter()
                .all(|record| record.source_ns.0 <= shared.cutoff.load(Ordering::Relaxed))
        );
    }
    #[test]
    fn real_writer_flushes_identity_records_and_footer_without_overwriting() {
        let directory = std::env::temp_dir().join(format!(
            "rcam-frame-trace-writer-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("trace.jsonl");
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        assert!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .is_err()
        );
        let shared = shared(CAPACITY);
        shared.metadata(Metadata::Version {
            version_id: 1,
            version: Version::capture(&crate::state::View::default(), 0),
        });
        let span = Span::new(
            shared.clone(),
            Binding {
                version_id: 1,
                ..Default::default()
            },
            Stage::CallbackPaint,
        );
        let config = Config {
            run_id: "writer-test".into(),
            output: path.clone(),
            expected_commit: "test-commit".into(),
            expected_manifest_sha256: "a".repeat(64),
            expected_binary_sha256: executable_sha256().unwrap(),
            clock_id: shared.clock.id().into(),
            windows: vec![],
        };
        let sink = shared.clone();
        let (done_tx, done) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let result = writer(
                output,
                &sink,
                config,
                "b".repeat(64),
                "test-commit",
                "auxiliary-test",
                "a".repeat(64),
            );
            let _ = done_tx.send(result);
        });
        // Header is flushed only after executable identity has been verified.
        let deadline = Instant::now() + Duration::from_secs(60);
        while std::fs::metadata(&path).unwrap().len() == 0 && Instant::now() < deadline {
            std::thread::park_timeout(Duration::from_millis(5));
        }
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        assert!(!thread.is_finished());
        assert_eq!(shared.inflight.load(Ordering::SeqCst), 1);
        assert!(matches!(done.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let closing = shared.clone();
        let producer = std::thread::spawn(move || {
            while !closing.closing.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            drop(span);
        });
        let mut recorder = Recorder::for_test();
        recorder.shared = shared.clone();
        recorder.done = Some(done);
        recorder.writer_thread = Some(thread);
        recorder.finish();
        producer.join().unwrap();
        assert!(recorder.done.is_none() && recorder.writer_thread.is_none());
        assert!(!shared.supervisor_timeout.load(Ordering::Acquire));
        assert!(!shared.failed.load(Ordering::Acquire));
        let raw = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<serde_json::Value> = raw
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[0]["kind"], "header");
        assert!(rows.iter().any(|row| row["kind"] == "identity_ready"));
        let footer = rows.last().unwrap();
        assert_eq!(footer["kind"], "footer");
        assert_eq!(footer["finalized"], true);
        assert_eq!(footer["observation_complete"], true);
        assert_eq!(footer["counters"]["accepted"], 2);
        assert_eq!(footer["counters"]["consumed"], 2);
        assert_eq!(footer["counters"]["producer_inflight"], 0);
        assert_eq!(
            rows[0]["unavailable"]["actual_submission_index"],
            serde_json::Value::Null
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn supervision_timeout_does_not_join_live_writer_or_accept_late_completion() {
        for notify_before_block in [false, true] {
            let shared = shared(CAPACITY);
            let (done_tx, done) = mpsc::sync_channel(1);
            let (release_tx, release) = mpsc::sync_channel(1);
            let (ended_tx, ended) = mpsc::sync_channel(1);
            let (ready_tx, ready) = mpsc::sync_channel(1);
            let thread = std::thread::spawn(move || {
                if notify_before_block {
                    done_tx.send(Ok(())).unwrap();
                    ready_tx.send(()).unwrap();
                }
                release.recv().unwrap(); // A deterministic stand-in for blocked I/O.
                if !notify_before_block {
                    let _ = done_tx.send(Ok(()));
                }
                let _ = ended_tx.send(());
            });
            if notify_before_block {
                ready.recv_timeout(Duration::from_secs(1)).unwrap();
            }
            let start = Instant::now();
            assert!(!supervise_writer(
                &shared,
                done,
                thread,
                Duration::from_millis(5)
            ));
            assert!(start.elapsed() < Duration::from_secs(1));
            assert!(shared.supervisor_timeout.load(Ordering::Acquire));
            release_tx.send(()).unwrap();
            ended.recv_timeout(Duration::from_secs(1)).unwrap();
            assert!(shared.supervisor_timeout.load(Ordering::Acquire));
        }
    }
    #[test]
    fn completion_notification_is_not_success_without_finished_thread_join() {
        let shared = shared(CAPACITY);
        let (done_tx, done) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            done_tx.send(Ok(())).unwrap();
            panic!("synthetic writer panic after notification");
        });
        assert!(!supervise_writer(
            &shared,
            done,
            thread,
            Duration::from_secs(1)
        ));
        assert!(shared.failed.load(Ordering::Acquire));
    }
    #[test]
    fn default_disabled_runs_never_read_config_or_start_writer() {
        for enabled in [None, Some("0")] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "frame_trace::tests::disabled_environment_child",
                    "--ignored",
                ])
                .env_remove("RCAM_FRAME_TRACE")
                .env(
                    "RCAM_FRAME_TRACE_CONFIG",
                    "/nonexistent/trace-disabled-probe.json",
                );
            if let Some(value) = enabled {
                child.env("RCAM_FRAME_TRACE", value);
            }
            let output = child.output().unwrap();
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
            assert!(!String::from_utf8_lossy(&output.stderr).contains("RCAM_FRAME_TRACE"));
        }
    }
    #[test]
    #[ignore = "owned subprocess only; parent verifies absent/0 environment"]
    fn disabled_environment_child() {
        assert!(std::env::var("RCAM_FRAME_TRACE").map_or(true, |value| value == "0"));
        assert!(Recorder::from_env().is_none());
    }
    #[test]
    fn render_source_serializes_only_measurement_sizes_and_opaque_identity() {
        let value = serde_json::to_value(RenderSource {
            update_id: 1,
            version_id: 2,
            scene_serial: 3,
            input_batch_id: 4,
            canvas_physical: [100., 200.],
            effective_ppp: 2.,
            selection_epoch: 5,
        })
        .unwrap();
        let keys: std::collections::BTreeSet<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "update_id",
                "version_id",
                "scene_serial",
                "input_batch_id",
                "canvas_physical",
                "effective_ppp",
                "selection_epoch"
            ]
            .into_iter()
            .collect()
        );
    }
    #[test]
    fn failed_io_is_not_a_successful_or_partial_record() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("broken test sink"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut bytes = 0;
        assert!(
            line(
                &mut Broken,
                &serde_json::json!({"record_seq":1}),
                &mut bytes,
                1024
            )
            .is_err()
        );
        assert_eq!(bytes, 0);
    }
}
