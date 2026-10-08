//! Isolated S5-M2-C PMIX recorder; real input, worker and production Metal callback.
//! No public API and no alternate renderer.
use crate::{EditorApp, camera::Camera, gpu::PrepareStats, state::Action};
use editor_core::{BoundsMm, MmPoint, hash::sha256_hex, hit_test::SelectRectMode};
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
static WORKERS: OnceLock<std::sync::Mutex<Vec<Value>>> = OnceLock::new();
static FRAME: AtomicU64 = AtomicU64::new(0);

/// PMIX-only diagnostic spans. All endpoints use ORIGIN; no UI/producer clock subtraction.
/// Fixed stages, bounded storage and nonblocking append; no JSON or I/O in a hot probe.
pub mod spans {
    use serde::Serialize;
    use serde_json::{Value, json};
    use std::{
        cell::Cell,
        marker::PhantomData,
        rc::Rc,
        sync::{
            Mutex, OnceLock, TryLockError,
            atomic::{AtomicU64, Ordering},
        },
        time::Instant,
    };

    const CAPACITY: usize = 131_072;
    const CLOSED: u64 = 1 << 63;
    static RECORDER: OnceLock<Recorder> = OnceLock::new();
    static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);
    thread_local! { static THREAD: Cell<u64> = const { Cell::new(0) }; }

    #[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    pub enum Stage {
        RawInputHook,
        PmixInput,
        FencePoll,
        Tick,
        PhaseActions,
        SnapshotTotal,
        SnapshotSerialize,
        SnapshotWrite,
        SnapshotHash,
        SnapshotRecord,
        CpuCanvasPrepare,
        GpuResourcesNew,
        GpuCallbackPrepare,
        GpuCallbackPaint,
        RoiCallback,
        RoiRawInput,
        RoiFrame,
        RoiWriterSend,
    }
    #[derive(Serialize)]
    struct Row {
        stage: Stage,
        frame_at_entry: u64,
        thread: u64,
        start_ns: u64,
        end_ns: u64,
        thread_cpu_ns: Option<u64>,
        cpu_start_probe_ns: u64,
        cpu_end_probe_ns: u64,
        entry_probe_ns: u64,
    }
    #[derive(Serialize)]
    pub(super) struct Report {
        #[serde(flatten)]
        pub(super) metadata: Value,
        rows: Vec<Row>,
    }
    struct Recorder {
        origin: Instant,
        rows: Mutex<Vec<Row>>,
        lifecycle: AtomicU64,
        capacity: usize,
        started: AtomicU64,
        ended: AtomicU64,
        full: AtomicU64,
        contended: AtomicU64,
        poisoned: AtomicU64,
        late: AtomicU64,
        cpu_failures: AtomicU64,
        probe_ns: AtomicU64,
        max_probe_ns: AtomicU64,
    }
    impl Recorder {
        fn new(origin: Instant, capacity: usize) -> Self {
            Self {
                origin,
                rows: Mutex::new(Vec::with_capacity(capacity)),
                lifecycle: AtomicU64::new(0),
                capacity,
                started: AtomicU64::new(0),
                ended: AtomicU64::new(0),
                full: AtomicU64::new(0),
                contended: AtomicU64::new(0),
                poisoned: AtomicU64::new(0),
                late: AtomicU64::new(0),
                cpu_failures: AtomicU64::new(0),
                probe_ns: AtomicU64::new(0),
                max_probe_ns: AtomicU64::new(0),
            }
        }
        fn start(&self, stage: Stage, frame: u64) -> Option<Span<'_>> {
            let probe = Instant::now();
            // One atomic gate binds open spans to the terminal cutoff, including
            // starts racing finalization. No new span can pass a sealed gate.
            if self
                .lifecycle
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    (value & CLOSED == 0).then_some(value + 1)
                })
                .is_err()
            {
                self.late.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            self.started.fetch_add(1, Ordering::Relaxed);
            let thread = THREAD.with(|id| {
                if id.get() == 0 {
                    id.set(NEXT_THREAD.fetch_add(1, Ordering::Relaxed));
                }
                id.get()
            });
            let cpu_probe = Instant::now();
            let cpu = self.cpu();
            let start = Instant::now();
            Some(Span {
                recorder: self,
                stage,
                frame,
                thread,
                start,
                cpu,
                cpu_start_probe_ns: ns(start.duration_since(cpu_probe)),
                entry_probe_ns: ns(start.duration_since(probe)),
                _same_thread: PhantomData,
            })
        }
        fn cpu(&self) -> Option<u64> {
            let value = thread_cpu();
            if cfg!(target_os = "macos") && value.is_none() {
                self.cpu_failures.fetch_add(1, Ordering::Relaxed);
            }
            value
        }
        fn append(&self, row: Row) {
            if self.lifecycle.load(Ordering::Acquire) & CLOSED != 0 {
                self.late.fetch_add(1, Ordering::Relaxed);
                return;
            }
            match self.rows.try_lock() {
                Ok(mut rows) => {
                    if self.lifecycle.load(Ordering::Acquire) & CLOSED != 0 {
                        self.late.fetch_add(1, Ordering::Relaxed);
                    } else if rows.len() >= self.capacity {
                        self.full.fetch_add(1, Ordering::Relaxed);
                    } else {
                        rows.push(row);
                    }
                }
                Err(TryLockError::WouldBlock) => {
                    self.contended.fetch_add(1, Ordering::Relaxed);
                }
                Err(TryLockError::Poisoned(_)) => {
                    self.poisoned.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        fn finish(&self) -> Report {
            // Terminal UI hook: ROI writer joined and terminal poll returned. The
            // cutoff seals storage before serialization; an open span is INCOMPLETE.
            let (mut rows, finalization_lock_poisoned) = match self.rows.lock() {
                Ok(rows) => (rows, false),
                Err(poison) => (poison.into_inner(), true),
            };
            let lifecycle = self.lifecycle.fetch_or(CLOSED, Ordering::AcqRel);
            let cutoff_ns = ns(self.origin.elapsed());
            let frozen = std::mem::take(&mut *rows);
            drop(rows);
            let read = |a: &AtomicU64| a.load(Ordering::Acquire);
            // Read each counter once. With an open span these are a post-seal
            // snapshot, not an atomic cutoff snapshot; open_at_cutoff already
            // forces INCOMPLETE. Later rejected probes cannot alter this report.
            let full = read(&self.full);
            let contended = read(&self.contended);
            let poisoned = read(&self.poisoned);
            let late = read(&self.late);
            let cpu_failures = read(&self.cpu_failures);
            let probe_ns = read(&self.probe_ns);
            let max_probe_ns = read(&self.max_probe_ns);
            let active = lifecycle & !CLOSED;
            let started = read(&self.started);
            let ended = read(&self.ended);
            let complete = !finalization_lock_poisoned
                && active == 0
                && full == 0
                && contended == 0
                && poisoned == 0
                && late == 0
                && started == ended
                && ended == frozen.len() as u64;
            let metadata = json!({"schema_version":1,"diagnostic_status":if complete {"COMPLETE"} else {"INCOMPLETE"},
                "clock":"same PMIX Instant origin as observations.json; nanoseconds", "cutoff_ns":cutoff_ns,
                "frame_binding":"frame_at_entry is the atomic PMIX frame at entry; raw-input-hook/pmix-input/poll start before increment and bind the preceding frame; GPU callbacks bind their submitted painted id",
                "scope":"REPORT_ONLY; nested spans overlap and must not be summed; callback spans are CPU command recording, not GPU execution/submit/present/wakeup/scanout; old intervals remain intact",
                "thread_cpu_clock":if cfg!(target_os="macos") {"clock_gettime(CLOCK_THREAD_CPUTIME_ID)"} else {"UNSUPPORTED; null values"},
                "thread_cpu_scope":"same-thread CPU delta, start read before wall start and end read after wall end; boundary uncertainty is cpu_start_probe_ns + cpu_end_probe_ns; never process CPU or independently timed GPU",
                "cpu_diagnostic_status":if !cfg!(target_os="macos") {"UNSUPPORTED"} else if cpu_failures>0 {"INCOMPLETE"} else {"AVAILABLE"},
                "capacity":self.capacity,"row_size_bytes":std::mem::size_of::<Row>(),"row_count":frozen.len(),
                "started":started,"ended":ended,"open_at_cutoff":active,"finalization_lock_poisoned":finalization_lock_poisoned,
                "loss":{"full":full,"contended":contended,"poisoned":poisoned,"late_at_snapshot":late},
                "cpu_clock_failures":cpu_failures,
                "probe_wall_ns_total":probe_ns,"probe_wall_ns_max":max_probe_ns,
                "probe_scope":"estimated wall overhead of entry/exit probes and nonblocking append; excludes trailing counter updates and rejected-start probes; not subtracted from old statistics; aggregate excludes static dispatch, startup preallocation and final serialization; each counter is read once after sealing; open_at_cutoff binds the atomic cutoff and makes racing endings INCOMPLETE; later probes are outside this diagnostic observation",
                "roi_writer_queue_capacity":16,"roi_send_scope":"original blocking SyncSender::send plus sender access and Sample metadata construction; wall includes any queue wait, not pure send, queue wait or occupancy"});
            Report {
                metadata,
                rows: frozen,
            }
        }
        #[cfg(test)]
        fn finish_value(&self) -> Value {
            serde_json::to_value(self.finish()).unwrap()
        }
    }
    pub struct Span<'a> {
        recorder: &'a Recorder,
        stage: Stage,
        frame: u64,
        thread: u64,
        start: Instant,
        cpu: Option<u64>,
        cpu_start_probe_ns: u64,
        entry_probe_ns: u64,
        // A thread CPU delta must never cross threads.
        _same_thread: PhantomData<Rc<()>>,
    }
    impl Drop for Span<'_> {
        fn drop(&mut self) {
            let end = Instant::now();
            let cpu = self.recorder.cpu();
            let cpu_end_probe_ns = ns(end.elapsed());
            let thread_cpu_ns = self.cpu.zip(cpu).and_then(|(a, b)| b.checked_sub(a));
            if self.cpu.is_some() && cpu.is_some() && thread_cpu_ns.is_none() {
                self.recorder.cpu_failures.fetch_add(1, Ordering::Relaxed);
            }
            self.recorder.append(Row {
                stage: self.stage,
                frame_at_entry: self.frame,
                thread: self.thread,
                start_ns: ns(self.start.duration_since(self.recorder.origin)),
                end_ns: ns(end.duration_since(self.recorder.origin)),
                thread_cpu_ns,
                cpu_start_probe_ns: self.cpu_start_probe_ns,
                cpu_end_probe_ns,
                entry_probe_ns: self.entry_probe_ns,
            });
            self.recorder.ended.fetch_add(1, Ordering::Relaxed);
            let overhead = self.entry_probe_ns.saturating_add(ns(end.elapsed()));
            self.recorder
                .probe_ns
                .fetch_add(overhead, Ordering::Relaxed);
            self.recorder
                .max_probe_ns
                .fetch_max(overhead, Ordering::Relaxed);
            self.recorder.lifecycle.fetch_sub(1, Ordering::Release);
        }
    }
    fn ns(duration: std::time::Duration) -> u64 {
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
    }
    #[cfg(any(target_os = "macos", test))]
    fn cpu_ns(seconds: i64, nanos: i64) -> Option<u64> {
        if !(0..1_000_000_000).contains(&nanos) {
            return None;
        }
        u64::try_from(seconds)
            .ok()?
            .checked_mul(1_000_000_000)?
            .checked_add(nanos as u64)
    }
    #[cfg(target_os = "macos")]
    fn thread_cpu() -> Option<u64> {
        // Apple _time.h: clockid_t is unsigned, thread CPU clock is 16 (macOS 10.12+).
        #[repr(C)]
        struct Timespec {
            seconds: std::ffi::c_long,
            nanos: std::ffi::c_long,
        }
        unsafe extern "C" {
            fn clock_gettime(clock: std::ffi::c_uint, time: *mut Timespec) -> std::ffi::c_int;
        }
        let mut time = Timespec {
            seconds: 0,
            nanos: 0,
        };
        // SAFETY: writable timespec with the platform ABI; no borrowed pointers retained.
        if unsafe { clock_gettime(16, &mut time) } != 0 {
            return None;
        }
        cpu_ns(time.seconds, time.nanos)
    }
    #[cfg(not(target_os = "macos"))]
    fn thread_cpu() -> Option<u64> {
        None
    }
    pub(super) fn activate(origin: Instant) {
        RECORDER.get_or_init(|| Recorder::new(origin, CAPACITY));
    }
    pub fn enter(stage: Stage) -> Option<Span<'static>> {
        enter_frame(stage, super::FRAME.load(Ordering::Acquire))
    }
    pub fn enter_frame(stage: Stage, frame: u64) -> Option<Span<'static>> {
        RECORDER.get()?.start(stage, frame)
    }
    pub(super) fn finish() -> Report {
        RECORDER.get().expect("active PMIX spans").finish()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn cpu_clock_rejects_invalid_and_overflowing_values() {
            assert_eq!(cpu_ns(2, 3), Some(2_000_000_003));
            for (s, n) in [(-1, 0), (0, -1), (0, 1_000_000_000), (i64::MAX, 0)] {
                assert_eq!(cpu_ns(s, n), None);
            }
        }
        #[test]
        fn nested_spans_keep_endpoints_and_frames_instead_of_adding_durations() {
            let r = Recorder::new(Instant::now(), 8);
            let outer = r.start(Stage::SnapshotTotal, 7).unwrap();
            drop(r.start(Stage::SnapshotSerialize, 7));
            drop(outer);
            let report = r.finish_value();
            assert_eq!(report["diagnostic_status"], "COMPLETE");
            let rows = report["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0]["frame_at_entry"], 7);
            assert_eq!(rows[0]["thread"], rows[1]["thread"]);
            assert!(rows[1]["start_ns"].as_u64() <= rows[0]["start_ns"].as_u64());
            assert!(rows[1]["end_ns"].as_u64() >= rows[0]["end_ns"].as_u64());
        }
        #[test]
        fn full_storage_counts_loss_without_growing_or_blocking() {
            let r = Recorder::new(Instant::now(), 1);
            for frame in 0..3 {
                drop(r.start(Stage::Tick, frame));
            }
            let report = r.finish_value();
            assert_eq!(report["rows"].as_array().unwrap().len(), 1);
            assert_eq!(report["loss"]["full"], 2);
            assert_eq!(report["diagnostic_status"], "INCOMPLETE");
        }
        #[test]
        fn contended_storage_never_waits_and_reports_missing_sample() {
            let r = Recorder::new(Instant::now(), 2);
            let guard = r.rows.lock().unwrap();
            drop(r.start(Stage::RoiWriterSend, 11));
            drop(guard);
            let report = r.finish_value();
            assert_eq!(report["loss"]["contended"], 1);
            assert_eq!(report["diagnostic_status"], "INCOMPLETE");
        }
        #[test]
        fn early_return_and_unwinding_drop_spans() {
            let r = Recorder::new(Instant::now(), 4);
            let early = || {
                let _span = r.start(Stage::GpuCallbackPaint, 3);
                false
            };
            assert!(!early());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _span = r.start(Stage::SnapshotWrite, 4);
                panic!("write failure");
            }));
            assert!(result.is_err());
            let report = r.finish_value();
            assert_eq!(report["diagnostic_status"], "COMPLETE");
            assert_eq!(report["ended"], 2);
        }
        #[test]
        fn poisoned_storage_is_explicit_incomplete_and_can_be_finalized() {
            let r = Recorder::new(Instant::now(), 2);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = r.rows.lock().unwrap();
                panic!("poison");
            }));
            drop(r.start(Stage::Tick, 1));
            let report = r.finish_value();
            assert_eq!(report["loss"]["poisoned"], 1);
            assert_eq!(report["diagnostic_status"], "INCOMPLETE");
        }
        #[test]
        fn poisoned_finalization_without_append_is_incomplete() {
            let r = Recorder::new(Instant::now(), 2);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = r.rows.lock().unwrap();
                panic!("poison before any append");
            }));
            let report = r.finish_value();
            assert_eq!(report["diagnostic_status"], "INCOMPLETE");
            assert_eq!(report["finalization_lock_poisoned"], true);
            assert_eq!(report["started"], 0);
        }
        #[test]
        fn cutoff_with_an_open_span_is_incomplete_and_late_append_is_rejected() {
            let r = Recorder::new(Instant::now(), 2);
            let active = r.start(Stage::Tick, 1);
            let report = r.finish_value();
            assert_eq!(report["open_at_cutoff"], 1);
            assert_eq!(report["diagnostic_status"], "INCOMPLETE");
            drop(active);
            assert!(r.start(Stage::Tick, 2).is_none());
            assert_eq!(r.late.load(Ordering::Relaxed), 2);
            assert!(r.rows.lock().unwrap().is_empty());
        }
        #[test]
        fn inactive_recorder_does_not_create_or_probe_a_clock() {
            assert!(enter_frame(Stage::Tick, 0).is_none());
            assert!(RECORDER.get().is_none());
        }
        #[test]
        fn different_threads_have_distinct_ids_and_no_cross_thread_cpu_delta() {
            let r = Recorder::new(Instant::now(), 4);
            drop(r.start(Stage::Tick, 1));
            std::thread::scope(|scope| {
                scope
                    .spawn(|| drop(r.start(Stage::Tick, 1)))
                    .join()
                    .unwrap();
            });
            let report = r.finish_value();
            assert_eq!(report["diagnostic_status"], "COMPLETE");
            assert_ne!(report["rows"][0]["thread"], report["rows"][1]["thread"]);
        }
        #[test]
        fn report_is_self_consistent_when_rejected_starts_race_sealing() {
            let r = Recorder::new(Instant::now(), 4);
            drop(r.start(Stage::Tick, 1));
            std::thread::scope(|scope| {
                let thread = scope.spawn(|| {
                    while r.lifecycle.load(Ordering::Acquire) & CLOSED == 0 {
                        std::thread::yield_now();
                    }
                    for _ in 0..1000 {
                        assert!(r.start(Stage::Tick, 2).is_none());
                    }
                });
                let report = r.finish_value();
                thread.join().unwrap();
                let late = report["loss"]["late_at_snapshot"].as_u64().unwrap();
                assert_eq!(report["diagnostic_status"] == "COMPLETE", late == 0);
                assert_eq!(report["rows"].as_array().unwrap().len(), 1);
                assert_eq!(report["started"], report["ended"]);
                assert_eq!(report["open_at_cutoff"], 0);
                assert_eq!(r.late.load(Ordering::Relaxed), 1000);
            });
        }
        #[test]
        fn submitted_gpu_id_and_preincrement_raw_id_are_preserved() {
            let r = Recorder::new(Instant::now(), 4);
            drop(r.start(Stage::RawInputHook, 10));
            drop(r.start(Stage::GpuCallbackPaint, 11));
            let report = r.finish_value();
            assert_eq!(report["rows"][0]["frame_at_entry"], 10);
            assert_eq!(report["rows"][1]["frame_at_entry"], 11);
        }
    }
}

// Only PMIX semantic snapshots use this fast path. Public/import/project hashes
// retain the dependency-free implementation and every evidence binding stays SHA-256.
fn snapshot_sha256_hex(bytes: &[u8]) -> String {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "System")]
        unsafe extern "C" {
            fn CC_SHA256(data: *const std::ffi::c_void, len: u32, digest: *mut u8) -> *mut u8;
        }
        snapshot_sha256_hex_with(bytes, |bytes, len, digest| {
            // SAFETY: CommonDigest.h defines CC_LONG as uint32_t and the output
            // as 32 writable bytes. The input remains borrowed for this synchronous
            // call; len was checked, output is distinct, and neither pointer escapes.
            unsafe { CC_SHA256(bytes.as_ptr().cast(), len, digest) }
        })
    }
    #[cfg(not(target_os = "macos"))]
    sha256_hex(bytes)
}

#[cfg(any(target_os = "macos", test))]
fn commoncrypto_length(length: usize) -> Option<u32> {
    u32::try_from(length).ok()
}

#[cfg(any(target_os = "macos", test))]
fn snapshot_sha256_hex_with(
    bytes: &[u8],
    digest_call: impl FnOnce(&[u8], u32, *mut u8) -> *mut u8,
) -> String {
    let Some(len) = commoncrypto_length(bytes.len()) else {
        return sha256_hex(bytes);
    };
    let mut digest = [0u8; 32];
    let output = digest.as_mut_ptr();
    if digest_call(bytes, len, output) != output {
        // No dereference of a returned pointer, no partial/unverified digest.
        return sha256_hex(bytes);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 15) as usize] as char);
    }
    hex
}

#[cfg(test)]
mod snapshot_hash_tests {
    use super::*;

    fn patterned(length: usize) -> Vec<u8> {
        (0..length).map(|i| ((i * 131 + 17) & 255) as u8).collect()
    }
    fn equivalent(bytes: &[u8]) {
        assert_eq!(snapshot_sha256_hex(bytes), sha256_hex(bytes));
        #[cfg(target_os = "macos")]
        {
            // The fallback must not hide a failed system call in Mac regressions.
            unsafe extern "C" {
                fn CC_SHA256(data: *const std::ffi::c_void, len: u32, digest: *mut u8) -> *mut u8;
            }
            let mut digest = [0u8; 32];
            let output = digest.as_mut_ptr();
            // SAFETY: same checked one-shot ABI and live disjoint buffers as above.
            let returned = unsafe {
                CC_SHA256(
                    bytes.as_ptr().cast(),
                    commoncrypto_length(bytes.len()).unwrap(),
                    output,
                )
            };
            assert_eq!(returned, output, "Mac CommonCrypto call must succeed");
        }
    }
    #[test]
    fn snapshot_sha256_standard_vectors_match() {
        for (bytes, expected) in [
            (
                b"".as_slice(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".as_slice(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            assert_eq!(snapshot_sha256_hex(bytes), expected);
            equivalent(bytes);
        }
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            snapshot_sha256_hex(&million),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        equivalent(&million);
    }
    #[test]
    fn snapshot_sha256_padding_and_binary_boundaries_match() {
        for len in [
            1, 2, 54, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 255, 256, 257, 1023, 1024,
            1025, 4095, 4096, 4097,
        ] {
            equivalent(&patterned(len));
        }
    }
    #[test]
    fn snapshot_sha256_large_buffers_match() {
        for len in [1_048_583, 37_954_766, 38_081_126] {
            equivalent(&patterned(len));
        }
    }
    #[test]
    fn snapshot_sha256_rejects_null_and_unexpected_return_pointers() {
        let bytes = patterned(129);
        let expected = sha256_hex(&bytes);
        assert_eq!(
            snapshot_sha256_hex_with(&bytes, |input, len, _| {
                assert_eq!(input, bytes);
                assert_eq!(len, 129);
                std::ptr::null_mut()
            }),
            expected
        );
        let mut other = [0u8; 32];
        assert_eq!(
            snapshot_sha256_hex_with(&bytes, |_, _, _| other.as_mut_ptr()),
            expected
        );
    }
    #[test]
    fn snapshot_sha256_hex_preserves_all_byte_values_and_leading_zeroes() {
        for block in 0..8 {
            let digest: [u8; 32] = std::array::from_fn(|i| (block * 32 + i) as u8);
            let expected: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
            let actual = snapshot_sha256_hex_with(b"abc", |_, _, output| {
                // SAFETY: helper supplies a distinct 32-byte output for the call.
                unsafe {
                    std::ptr::copy_nonoverlapping(digest.as_ptr(), output, 32);
                }
                output
            });
            assert_eq!(actual, expected);
            assert_eq!(actual.len(), 64);
        }
    }
    #[test]
    fn snapshot_sha256_length_limit_never_truncates() {
        assert_eq!(commoncrypto_length(0), Some(0));
        assert_eq!(commoncrypto_length(u32::MAX as usize), Some(u32::MAX));
        if let Ok(over) = usize::try_from(u64::from(u32::MAX) + 1) {
            assert_eq!(commoncrypto_length(over), None);
            assert_eq!(commoncrypto_length(usize::MAX), None);
        }
    }
    #[test]
    #[ignore = "REPORT_ONLY release hash timing; run explicitly with --release --ignored --nocapture"]
    fn snapshot_sha256_release_timing_probe() {
        assert!(!cfg!(debug_assertions), "release timing requires --release");
        use std::{hint::black_box, time::Instant};
        for len in [37_954_766, 38_081_126] {
            let bytes = patterned(len);
            equivalent(&bytes);
            for round in 0..5 {
                let start = Instant::now();
                let old = sha256_hex(black_box(&bytes));
                let old_ms = start.elapsed().as_secs_f64() * 1000.;
                let start = Instant::now();
                let new = snapshot_sha256_hex(black_box(&bytes));
                let new_ms = start.elapsed().as_secs_f64() * 1000.;
                assert_eq!(old, new);
                println!(
                    "REPORT_ONLY_SNAPSHOT_SHA bytes={len} round={round} mac_fast_path={} rust_ms={old_ms:.6} candidate_ms={new_ms:.6} digest={new}",
                    cfg!(target_os = "macos")
                );
            }
        }
    }
}

static REQUESTS: OnceLock<std::sync::Mutex<Vec<Value>>> = OnceLock::new();
pub fn request_input(
    task: &editor_service::task::TaskContext,
    view: &crate::state::View,
    action: &Action,
) -> Option<Value> {
    let origin = ORIGIN.get()?;
    Some(
        json!({"sequence":task.task_id,"input":task.input,"view_version":editor_service::task::TaskVersion::capture(view.info.as_ref(),view.task_generation,view.rule_revision),"action":action_label(action),"frame_id":FRAME.load(Ordering::Acquire),"at_ns":origin.elapsed().as_nanos() as u64}),
    )
}

pub fn accepted_request(input: Option<Value>) {
    if let Some(input) = input {
        REQUESTS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .push(input);
    }
}

pub fn action_label(action: &Action) -> &'static str {
    match action {
        Action::OpenProject(..) => "project-open",
        Action::SaveProject(..) => "project-save",
        Action::BlockSelect(..) => "block-select",
        Action::Rotate(..) => "rotate",
        Action::SetAllLayersVisible(..) => "visibility",
        Action::SetSoloLayer(..) => "solo",
        Action::Layer(..) => "layer-update",
        Action::FitLayer(..) => "fit-layer",
        Action::SelectionCenters(..) => "selection-centers",
        _ => crate::native_s5m1::action_label(action),
    }
}
pub fn worker_result(id: u64, action: &str, start: Instant, view: &crate::state::View) {
    let Some(origin) = ORIGIN.get() else {
        return;
    };
    let Some(started) = start.checked_duration_since(*origin) else {
        return;
    };
    let d = view.info.as_ref();
    WORKERS.get_or_init(Default::default).lock().unwrap().push(json!({"sequence":id,"action":action,"receipt":view.task_receipt,"started_ns":started.as_nanos() as u64,"finished_ns":origin.elapsed().as_nanos() as u64,"error":view.error,"blocked":view.blocked,"state":{"document_id":d.map(|d|&d.document_id),"revision":d.map(|d|&d.revision),"workspace_revision":d.map(|d|&d.workspace_revision),"undo":d.map(|d|d.undo_entries),"redo":d.map(|d|d.redo_entries),"selected":view.selected.ordered.len(),"scene_serial":view.scene.as_ref().map(|s|s.serial),"version":editor_service::task::TaskVersion::capture(view.info.as_ref(),view.task_generation,view.rule_revision)}}));
}
fn worker_snapshot() -> Value {
    json!(*WORKERS.get_or_init(Default::default).lock().unwrap())
}
static HITS: OnceLock<std::sync::Mutex<Vec<Value>>> = OnceLock::new();
pub struct HitTimer(Option<(Instant, MmPoint, f64)>);
impl HitTimer {
    pub fn start(point: MmPoint, tolerance: f64) -> Self {
        Self(ORIGIN.get().map(|_| (Instant::now(), point, tolerance)))
    }
}
impl Drop for HitTimer {
    fn drop(&mut self) {
        if let Some((start, point, tolerance)) = self.0 {
            let origin = *ORIGIN.get().unwrap();
            HITS.get_or_init(Default::default).lock().unwrap().push(json!({"start_ns":start.duration_since(origin).as_nanos() as u64,"end_ns":origin.elapsed().as_nanos() as u64,"point":point,"tolerance_mm":tolerance}));
        }
    }
}
fn view_parameters(app: &EditorApp) -> Value {
    json!({"center_mm":[app.camera.center.x_mm,app.camera.center.y_mm],"scale":app.camera.scale,"rect":[app.canvas_rect.min.x,app.canvas_rect.min.y,app.canvas_rect.max.x,app.canvas_rect.max.y],"ppp":app.reported_ppp,"grid_snap":app.grid.snap_enabled,"object_snap":app.object_snap.enabled,"threshold_physical_px":crate::drag::THRESHOLD_PX})
}

pub fn directory() -> Option<PathBuf> {
    let dir = std::fs::canonicalize(std::env::var_os("RCAM_PMIX_NATIVE_DIR")?).ok()?;
    (dir.parent() == Some(Path::new("/private/tmp"))
        && dir.file_name()?.to_str()?.starts_with("rcam-pmix-"))
    .then_some(dir)
}

const DRAG_INPUT_POLICY_VERSION: u64 = 1;

fn input_modifiers(mode: &str, phase: u32) -> egui::Modifiers {
    let mut modifiers = egui::Modifiers::NONE;
    // Dedicated contour Snap is independent of the global Snap.enabled flag.
    // The frozen free-trajectory benchmark uses its ordinary Alt bypass.
    modifiers.alt = ["move", "escape", "new-project"].contains(&mode) && (4..=7).contains(&phase);
    modifiers
}

fn modifier_value(modifiers: egui::Modifiers) -> Value {
    json!({"alt":modifiers.alt,"ctrl":modifiers.ctrl,"shift":modifiers.shift,"command":modifiers.command,"mac_cmd":modifiers.mac_cmd})
}

fn button_inputs(raw: &egui::RawInput) -> Value {
    json!(raw.events.iter().filter_map(|event| {
        if let egui::Event::PointerButton { pos, button, pressed, modifiers } = event {
            Some(json!({"button":format!("{button:?}"),"pressed":pressed,"position":[pos.x,pos.y],"modifiers":modifier_value(*modifiers)}))
        } else { None }
    }).collect::<Vec<_>>())
}

fn key_inputs(raw: &egui::RawInput) -> Value {
    json!(raw.events.iter().filter_map(|event| {
        if let egui::Event::Key { key, physical_key, pressed, repeat, modifiers } = event {
            Some(json!({"key":format!("{key:?}"),"physical_key":physical_key.map(|key|format!("{key:?}")),"pressed":pressed,"repeat":repeat,"modifiers":modifier_value(*modifiers)}))
        } else { None }
    }).collect::<Vec<_>>())
}
fn state(app: &EditorApp) -> Value {
    let d = app.view.info.as_ref();
    json!({"document_id":d.map(|d|&d.document_id),"revision":d.map(|d|&d.revision),"workspace_revision":d.map(|d|&d.workspace_revision),"version":editor_service::task::TaskVersion::capture(app.view.info.as_ref(),app.view.task_generation,app.view.rule_revision),"dirty":d.map(|d|d.dirty),"project_dirty":d.map(|d|d.project_dirty),"undo":d.map(|d|d.undo_entries),"redo":d.map(|d|d.redo_entries),"selected":app.view.selected.ordered.len(),"primary":app.view.selected.primary().map(|o|&o.object.object_id),"scene_serial":app.view.scene.as_ref().map(|s|s.serial),"scene_objects":app.view.scene.as_ref().map_or(0,|s|s.objects.len()),"busy":app.busy,"display_pending":app.display_pending,"delta":app.drag.as_ref().map(|g|g.delta),"canvas_physical":[app.canvas_rect.width()*app.reported_ppp,app.canvas_rect.height()*app.reported_ppp]})
}
struct Pending {
    id: u64,
    input: Instant,
    observed: Value,
}
#[derive(Default)]
struct FrameLog {
    pending: Option<Pending>,
    frames: Vec<Value>,
}
impl FrameLog {
    fn observe(&mut self, pending: Pending) {
        // A second update without another raw-input hook is an unmodelled egui
        // discard pass, not another input frame. Never overwrite its evidence.
        assert!(self.pending.is_none(), "PMIX duplicate update pass");
        self.pending = Some(pending);
    }
    fn complete_pending(
        &mut self,
        painted_id: u64,
        wait: impl FnOnce() -> bool,
        origin: Instant,
        counters: impl FnOnce() -> Value,
    ) -> Option<Value> {
        let p = self.pending.take()?;
        let painted = painted_id == p.id;
        let mut frame = p.observed;
        frame["painted"] = json!(painted);
        // Also used at on_exit, while eframe's production device is still live.
        // An unpainted update gets no fabricated GPU completion observation.
        if painted {
            let okay = wait();
            frame["gpu_completed"] = json!(okay);
            frame["input_gpu_complete_ms"] = json!(p.input.elapsed().as_secs_f64() * 1000.);
            frame["completed_ns"] = json!(origin.elapsed().as_nanos() as u64);
        }
        frame["counters"] = counters();
        Some(frame)
    }
}
pub struct Run {
    dir: PathBuf,
    request: Value,
    count: usize,
    point_index: usize,
    workflow_step: usize,
    point_sequence: u64,
    protocol: Value,
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
    log: FrameLog,
    complete: Option<Value>,
    prepared: Option<Value>,
    events: Vec<Value>,
    failures: Vec<String>,
    press: Pos2,
    baseline: Value,
    baseline_snapshot: Option<Arc<editor_service::RenderSnapshot>>,
    baseline_scene: Option<Arc<crate::display::Scene>>,
    pub focused: bool,
    close_requested: Option<(u64, u64)>,
    surface_pending: Vec<Value>,
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
        assert_eq!(count, 1000);
        assert!(!cfg!(debug_assertions));
        let mode = request["mode"].as_str()?.to_owned();
        assert!(
            [
                "nav",
                "points",
                "move",
                "escape",
                "new-project",
                "workflow",
                "workflow-reopen",
                "workflow-cross-layer"
            ]
            .contains(&mode.as_str())
        );
        crate::native_s5m1::activate_counters();
        let started = *ORIGIN.get_or_init(Instant::now);
        spans::activate(started);
        Some(Self {
            dir,
            request,
            count,
            point_index: 0,
            workflow_step: 0,
            point_sequence: 0,
            protocol: serde_json::from_slice(include_bytes!(
                "../../../fixtures/synthetic/s5m2c/protocol.json"
            ))
            .unwrap(),
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
            log: FrameLog::default(),
            complete: None,
            prepared: None,
            events: vec![],
            failures: vec![],
            press: Pos2::ZERO,
            baseline: Value::Null,
            baseline_snapshot: None,
            baseline_scene: None,
            focused: true,
            close_requested: None,
            surface_pending: vec![],
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
                modifiers: raw.modifiers,
            });
        }
    }
    fn key(raw: &mut egui::RawInput, key: egui::Key, command: bool, shift: bool) {
        let modifiers = egui::Modifiers {
            alt: raw.modifiers.alt,
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
        let _span = spans::enter(spans::Stage::PmixInput);
        self.complete = None;
        if let Some(frame) = self.complete_pending() {
            if frame["painted"] == true {
                self.complete = Some(frame.clone());
            }
            self.log.frames.push(frame);
        }
        for e in &raw.events {
            if let egui::Event::Screenshot {
                image, user_data, ..
            } = e
                && user_data
                    .data
                    .as_ref()
                    .is_some_and(|d| d.downcast_ref::<Value>().is_some())
            {
                // The accepted UI collector owns its Tag/readback crops. Only
                // this recorder's explicit Value requests are full surfaces.
                let name = format!("surface-{}.ppm", self.frame_id);
                let mut bytes =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                bytes.extend(image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]));
                std::fs::write(self.dir.join(&name), &bytes).unwrap();
                let request = user_data
                    .data
                    .as_ref()
                    .unwrap()
                    .downcast_ref::<Value>()
                    .unwrap();
                if let Some(index) = self.surface_pending.iter().position(|r| r == request) {
                    self.surface_pending.remove(index);
                } else {
                    self.failures
                        .push("unknown or duplicate full-surface readback".into());
                }
                self.event("screenshot", json!({"path":name,"width":image.size[0],"height":image.size[1],"sha256":sha256_hex(&bytes),"request":request}));
            }
        }
        self.frame_id += 1;
        FRAME.store(self.frame_id, Ordering::Release);
        self.input_at = Instant::now();
        self.focused = raw.focused;
        let input_phase = self.phase;
        raw.modifiers = input_modifiers(&self.mode, input_phase);
        match self.phase {
            20 => {
                let t = self
                    .input_at
                    .duration_since(self.since)
                    .as_secs_f64()
                    .min(60.);
                let tau = std::f64::consts::TAU;
                let center = MmPoint::new(
                    200.5 + 80. * (tau * t / 11.).sin(),
                    125.5 + 8. * (tau * t / 7.).sin(),
                );
                let scale = 1.656 * (1.05 + 0.35 * (tau * t / 17.).sin());
                raw.events
                    .push(egui::Event::PointerMoved(app.canvas_rect.center()));
                raw.events.push(egui::Event::Zoom(
                    (scale / app.camera.scale).powf(1.25) as f32
                ));
                raw.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(
                        ((app.camera.center.x_mm - center.x_mm) * app.camera.scale) as f32,
                        ((center.y_mm - app.camera.center.y_mm) * app.camera.scale) as f32,
                    ),
                    modifiers: egui::Modifiers::NONE,
                });
                if t >= 60. {
                    self.enter(21);
                    self.event("navigation-end-input", Value::Null);
                }
            }
            75 => {
                if self.since.elapsed() < Duration::from_millis(300) {
                    raw.events
                        .push(egui::Event::PointerMoved(app.canvas_rect.center()));
                    raw.events.push(egui::Event::Zoom(1.01));
                    raw.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(1., -0.5),
                        modifiers: egui::Modifiers::NONE,
                    });
                } else {
                    self.enter(76);
                }
            }
            32 if !app.busy && !app.display_pending => {
                let pos = app.canvas_rect.center();
                Self::pointer(raw, pos, Some(true));
                Self::pointer(raw, pos, Some(false));
                self.operation_at = Some(self.input_at);
                self.point_sequence = app.sequence;
                self.event("point-input",json!({"index":self.point_index,"world":app.camera.center,"position":[pos.x,pos.y]}));
                self.enter(33);
            }
            35 if !app.busy && !app.display_pending => {
                let pos = app.camera.screen(MmPoint::new(-3., -3.), app.canvas_rect);
                Self::pointer(raw, pos, Some(true));
                self.event("box-press", json!({"position":[pos.x,pos.y]}));
                self.enter(36);
            }
            36 if !app.busy => {
                Self::pointer(
                    raw,
                    app.camera.screen(MmPoint::new(403., 253.), app.canvas_rect),
                    None,
                );
                self.enter(37);
            }
            37 if !app.busy => {
                let pos = app.camera.screen(MmPoint::new(403., 253.), app.canvas_rect);
                Self::pointer(raw, pos, Some(false));
                self.operation_at = Some(self.input_at);
                self.point_sequence = app.sequence;
                self.event("box-release", json!({"position":[pos.x,pos.y]}));
                self.enter(38);
            }
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
        self.injected = json!({"input_policy_version":DRAG_INPUT_POLICY_VERSION,"input_phase":input_phase,"modifiers":modifier_value(raw.modifiers),"buttons":button_inputs(raw),"keys":key_inputs(raw),"view":view_parameters(app),"pointer":raw.events.iter().rev().find_map(|e|if let egui::Event::PointerMoved(p)=e {Some([p.x,p.y])} else {None}),"pressed":raw.events.iter().any(|e|matches!(e,egui::Event::PointerButton {pressed:true,..})),"released":raw.events.iter().any(|e|matches!(e,egui::Event::PointerButton {pressed:false,..})),"escape":raw.events.iter().any(|e|matches!(e,egui::Event::Key {key:egui::Key::Escape,pressed:true,..})),"pointer_gone":raw.events.iter().any(|e|matches!(e,egui::Event::PointerGone)),"zoom":raw.events.iter().filter_map(|e|if let egui::Event::Zoom(z)=e {Some(*z)} else {None}).collect::<Vec<_>>(),"wheel":raw.events.iter().filter_map(|e|if let egui::Event::MouseWheel{delta,..}=e {Some([delta.x,delta.y])} else {None}).collect::<Vec<_>>(),"focused":raw.focused,"trajectory_origin_ns":self.trajectory_origin.map(|t|t.duration_since(self.started).as_nanos() as u64)});
        // Frame interval includes recorder overhead, scheduling and preceding
        // GPU completion. CPU update excludes this PMIX recorder but includes
        // the separate accepted UI ROI collector; all frame intervals include both.
        self.frame_interval = self
            .previous
            .replace(self.input_at)
            .map(|t| self.input_at.duration_since(t).as_secs_f64() * 1000.);
        if self.mode == "blur" && self.phase == 7 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
    fn complete_pending(&mut self) -> Option<Value> {
        let device = &self.device;
        let frame = self.log.complete_pending(
            self.painted.load(Ordering::Acquire),
            || {
                let _span = spans::enter(spans::Stage::FencePoll);
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(5)),
                    })
                    .is_ok()
            },
            self.started,
            crate::native_s5m1::counter_snapshot,
        )?;
        if frame["gpu_completed"] == false {
            self.failures.push("GPU fence failed".into());
        }
        Some(frame)
    }
    fn complete_current(&self, app: &EditorApp) -> bool {
        let current = state(app);
        !app.busy
            && !app.display_pending
            && self.complete.as_ref().is_some_and(|v| {
                v["painted"] == true
                    && v["gpu_completed"] == true
                    && v["state"] == current
                    && v["view"] == view_parameters(app)
            })
    }
    fn completion(&mut self, app: &EditorApp, label: &str, action: Option<&str>) {
        let frame = self.complete.as_ref().unwrap();
        let workers = worker_snapshot();
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
        self.surface_pending.push(request.clone());
        self.event("screenshot-request", request.clone());
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
            request,
        )));
    }

    fn snapshot(&mut self, app: &EditorApp, label: &str) {
        let Some(s) = &app.view.snap_snapshot else {
            return;
        };
        let _total = spans::enter(spans::Stage::SnapshotTotal);
        let bytes = {
            let _span = spans::enter(spans::Stage::SnapshotSerialize);
            serde_json::to_vec(&**s).unwrap()
        };
        let name = format!("{label}-semantic.json");
        {
            let _span = spans::enter(spans::Stage::SnapshotWrite);
            std::fs::write(self.dir.join(&name), &bytes).unwrap();
        }
        let hash = {
            let _span = spans::enter(spans::Stage::SnapshotHash);
            snapshot_sha256_hex(&bytes)
        };
        let _span = spans::enter(spans::Stage::SnapshotRecord);
        self.snapshot_files.push(json!({"label":label,"path":name,"sha256":hash,"frame_id":self.frame_id,"at_ns":self.started.elapsed().as_nanos() as u64,"count":s.layers.iter().map(|l|l.objects.len()).sum::<usize>(),"selected_ids":app.view.selected.ids(),"state":state(app)}));
    }
    fn finish(&mut self, _app: &mut EditorApp, _ctx: &egui::Context) {
        let done_path = self.dir.join("protocol-done.json");
        if !done_path.is_file() {
            // Quiesce only new readbacks. Frame/paint observation continues
            // while every existing ROI and full-surface request is delivered.
            crate::native_ui::quiesce();
            if !crate::native_ui::readbacks_drained() || !self.surface_pending.is_empty() {
                return;
            }
            let done = json!({"app_pid":std::process::id(),"run_id":self.request["run_id"],"frame_id":self.frame_id,"at_ns":self.started.elapsed().as_nanos() as u64});
            self.event("protocol-end", done.clone());
            std::fs::write(&done_path, serde_json::to_vec_pretty(&done).unwrap()).unwrap();
            self.enter(13);
            return;
        }
        let Ok(bytes) = std::fs::read(self.dir.join("capture-complete.json")) else {
            return;
        };
        let Ok(complete) = serde_json::from_slice::<Value>(&bytes) else {
            return;
        };
        if complete["app_pid"].as_u64() != Some(std::process::id() as u64)
            || complete["run_id"] != self.request["run_id"]
        {
            return;
        }
        if complete["success"].as_bool() != Some(true) {
            self.failures
                .push("capture producer failed before app release".into());
        }
        self.event("capture-complete", complete);
        // Keep the completion event's UI/GPU frame in the continuous raw record.
        self.enter(14);
    }
    fn close_after_capture(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        assert_eq!(self.phase, 14, "PMIX Close before capture completion");
        assert!(
            self.close_requested.is_none(),
            "duplicate PMIX close request"
        );
        if !crate::native_ui::readbacks_drained() || !self.surface_pending.is_empty() {
            self.failures
                .push("readbacks not drained before Close".into());
        }
        self.close_requested = Some((self.frame_id, self.started.elapsed().as_nanos() as u64));
        app.allow_quit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn on_exit(&mut self, app: &EditorApp, roi_finalization: Option<Value>) {
        // eframe can draw further genuine updates after Close. Only on_exit is
        // terminal, and it runs before the production painter/device is destroyed.
        let terminal_frame = self.complete_pending();
        if self.close_requested.is_none() {
            self.failures
                .push("application exited before PMIX Close".into());
        }
        if terminal_frame.is_none() {
            self.failures.push("missing PMIX terminal frame".into());
        }
        if !self.surface_pending.is_empty() {
            self.failures
                .push("undelivered full-surface requests at exit".into());
        }
        if roi_finalization.as_ref().is_none_or(|r| {
            r["quiesced"] != true
                || r["pending"] != false
                || r["writer_joined"] != true
                || r["requests"]
                    .as_u64()
                    .is_none_or(|n| n == 0 || r["samples"].as_u64() != Some(n))
        }) {
            self.failures
                .push("ROI readbacks/writer not finalized at exit".into());
        }
        let exit = json!({"close_requested_frame_id":self.close_requested.map(|c|c.0),"close_requested_ns":self.close_requested.map(|c|c.1),"exited_ns":self.started.elapsed().as_nanos() as u64,"roi_finalization":roi_finalization,"full_surface_requests_drained":self.surface_pending.is_empty()});
        let mut timing = spans::finish();
        timing.metadata["commit"] = json!(option_env!("RCAM_BUILD_COMMIT"));
        timing.metadata["source_manifest_sha256"] =
            json!(sha256_hex(include_bytes!("../../../MANIFEST.sha256")));
        let timing_bytes = serde_json::to_vec_pretty(&timing).unwrap();
        let timing_path = self.dir.join("pmix-stage-timing.json");
        assert!(!timing_path.exists(), "PMIX stage timing already finalized");
        std::fs::write(timing_path, &timing_bytes).unwrap();
        let timing_binding = json!({"path":"pmix-stage-timing.json","sha256":sha256_hex(&timing_bytes),"diagnostic_status":timing.metadata["diagnostic_status"]});
        // Release the fixed records and encoded sidecar before the existing
        // binary/report allocations; rows serialize directly, without per-row Value trees.
        drop(timing_bytes);
        drop(timing);
        let binary = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let fixture = Path::new(self.request["fixture"].as_str().unwrap());
        let report = json!({"schema_version":2,"stage":"S5-M2-C","profile":"release","observation_version":3,"stage_timing":timing_binding,"last_observed_frame_id":self.log.frames.len(),"request":self.request,"commit":option_env!("RCAM_BUILD_COMMIT"),"build_source":option_env!("RCAM_BUILD_SOURCE"),"binary_sha256":sha256_hex(&binary),"source_manifest_sha256":sha256_hex(include_bytes!("../../../MANIFEST.sha256")),"pid":std::process::id(),"fixture_sha256":sha256_hex(&std::fs::read(fixture).unwrap()),"protocol_sha256":sha256_hex(include_bytes!("../../../fixtures/synthetic/s5m2c/protocol.json")),"native_inputs_sha256":sha256_hex(include_bytes!("../../../fixtures/synthetic/s5m2c/native-inputs.json")),"adapter":app.adapter,"frames":self.log.frames,"terminal_frame":terminal_frame,"exit":exit,"events":self.events,"snapshots":self.snapshot_files,"worker":worker_snapshot(),"requests":*REQUESTS.get_or_init(Default::default).lock().unwrap(),"hits":*HITS.get_or_init(Default::default).lock().unwrap(),"failures":self.failures,"counters":crate::native_s5m1::counter_snapshot(),"measurement_scope":"synthetic egui input; production Metal callback completion upper bound including the terminal update, no physical input/scanout claim; peak RSS and cumulative CPU from owned-child wait4"});
        let report_path = self.dir.join("observations.json");
        assert!(!report_path.exists(), "PMIX report already finalized");
        std::fs::write(report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    fn workflow_action(&self, app: &EditorApp) -> Option<(&'static str, Action, Value)> {
        use crate::{selection::SelectionMode, state::PivotInput};
        if self.mode == "workflow-cross-layer" {
            let patch = |locked| editor_service::LayerUpdateParams {
                layer_id: "layer-2".into(),
                expected_workspace_revision: app
                    .view
                    .info
                    .as_ref()
                    .unwrap()
                    .workspace_revision
                    .clone(),
                locked: Some(locked),
                ..Default::default()
            };
            return Some(match self.workflow_step {
                0 => (
                    "cross-layer-select",
                    Action::SelectRect(
                        BoundsMm {
                            min_x_mm: -200.,
                            min_y_mm: -200.,
                            max_x_mm: 300.,
                            max_y_mm: 300.,
                        },
                        SelectRectMode::Window,
                    ),
                    json!({"count":51}),
                ),
                1 => (
                    "cross-layer-move",
                    Action::Move("1.25".into(), "-0.75".into()),
                    json!({"delta_mm":[1.25,-0.75]}),
                ),
                2 => ("cross-layer-undo", Action::History(false), Value::Null),
                3 => ("cross-layer-redo", Action::History(true), Value::Null),
                4 => (
                    "cross-layer-lock",
                    Action::Layer(patch(true)),
                    json!({"layer":"layer-2"}),
                ),
                5 => (
                    "cross-layer-locked-move",
                    Action::Move("10".into(), "0".into()),
                    json!({"expected_error":"LAYER_LOCKED"}),
                ),
                6 => (
                    "cross-layer-unlock",
                    Action::Layer(patch(false)),
                    json!({"layer":"layer-2"}),
                ),
                7 => (
                    "cross-layer-save",
                    Action::SaveProject(
                        Some(self.dir.join("workflow-cross-layer.rcam")),
                        false,
                        None,
                    ),
                    json!({"path":"workflow-cross-layer.rcam"}),
                ),
                8 => (
                    "cross-layer-reopen",
                    Action::OpenProject(self.dir.join("workflow-cross-layer.rcam"), true),
                    json!({"path":"workflow-cross-layer.rcam"}),
                ),
                9 => (
                    "cross-layer-export-base",
                    Action::Save(
                        self.dir.join("cross-layer-base.gbr"),
                        "layer-1".into(),
                        None,
                    ),
                    json!({"path":"cross-layer-base.gbr"}),
                ),
                10 => (
                    "cross-layer-export-upper",
                    Action::Save(
                        self.dir.join("cross-layer-upper.gbr"),
                        "layer-2".into(),
                        None,
                    ),
                    json!({"path":"cross-layer-upper.gbr"}),
                ),
                _ => return None,
            });
        }
        let layer = "layer-1".to_string();
        let patch = |locked| editor_service::LayerUpdateParams {
            layer_id: layer.clone(),
            expected_workspace_revision: app.view.info.as_ref().unwrap().workspace_revision.clone(),
            locked: Some(locked),
            ..Default::default()
        };
        let block = || app.view.block_definitions[0].id.0.clone();
        Some(match self.workflow_step {
            0 => (
                "select-blocks",
                Action::BlockSelect(block()),
                json!({"count":4}),
            ),
            1 => (
                "move",
                Action::Move("2".into(), "-1".into()),
                json!({"delta_mm":[2.,-1.]}),
            ),
            2 => (
                "rotate",
                Action::Rotate("37".into(), PivotInput::WorldOrigin),
                json!({"angle_deg":37.,"pivot_mm":[0.,0.]}),
            ),
            3 => ("undo", Action::History(false), Value::Null),
            4 => ("redo", Action::History(true), Value::Null),
            5 => ("hide-all", Action::SetAllLayersVisible(false), Value::Null),
            6 => ("show-all", Action::SetAllLayersVisible(true), Value::Null),
            7 => (
                "solo",
                Action::SetSoloLayer(Some(layer.clone())),
                json!({"layer":layer}),
            ),
            8 => ("clear-solo", Action::SetSoloLayer(None), Value::Null),
            9 => (
                "reselect-blocks",
                Action::BlockSelect(block()),
                json!({"count":4}),
            ),
            10 => ("lock", Action::Layer(patch(true)), json!({"layer":layer})),
            11 => (
                "locked-move",
                Action::Move("10".into(), "0".into()),
                json!({"delta_mm":[10.,0.],"expected_error":"LAYER_LOCKED"}),
            ),
            12 => (
                "unlock",
                Action::Layer(patch(false)),
                json!({"layer":layer}),
            ),
            13 => (
                "fit-layer",
                Action::FitLayer(layer.clone()),
                json!({"layer":layer}),
            ),
            14 => (
                "window-all",
                Action::SelectRect(
                    BoundsMm {
                        min_x_mm: -200.,
                        min_y_mm: -200.,
                        max_x_mm: 300.,
                        max_y_mm: 300.,
                    },
                    SelectRectMode::Window,
                ),
                json!({"rect_mm":[-200.,-200.,300.,300.]}),
            ),
            15 => (
                "crossing-all",
                Action::SelectRect(
                    BoundsMm {
                        min_x_mm: -200.,
                        min_y_mm: -200.,
                        max_x_mm: 300.,
                        max_y_mm: 300.,
                    },
                    SelectRectMode::Crossing,
                ),
                json!({"rect_mm":[-200.,-200.,300.,300.]}),
            ),
            16 => {
                let snap = app.view.snap_snapshot.as_ref().unwrap();
                let point = snap
                    .layers
                    .iter()
                    .flat_map(|l| &l.objects)
                    .find_map(|o| {
                        if matches!(o.origin, editor_core::ObjectOrigin::GeneratedText { .. })
                            && let editor_core::SemanticGeometry::Line { start, end, .. } =
                                &o.geometry
                        {
                            Some(MmPoint::new(
                                (start.x_mm + end.x_mm) / 2.,
                                (start.y_mm + end.y_mm) / 2.,
                            ))
                        } else {
                            None
                        }
                    })
                    .unwrap();
                (
                    "select-text-group",
                    Action::Select(point, 0., SelectionMode::Replace),
                    json!({"point":point,"expected_count":40}),
                )
            }
            17 => (
                "project-save",
                Action::SaveProject(Some(self.dir.join("workflow-output.rcam")), false, None),
                json!({"path":"workflow-output.rcam"}),
            ),
            18 => (
                "project-reopen",
                Action::OpenProject(self.dir.join("workflow-output.rcam"), true),
                json!({"path":"workflow-output.rcam"}),
            ),
            19 => (
                "export-base",
                Action::Save(self.dir.join("workflow-layer-1.gbr"), layer, None),
                json!({"path":"workflow-layer-1.gbr"}),
            ),
            20 => (
                "export-upper",
                Action::Save(
                    self.dir.join("workflow-layer-2.gbr"),
                    "layer-2".into(),
                    None,
                ),
                json!({"path":"workflow-layer-2.gbr"}),
            ),
            _ => return None,
        })
    }
    pub fn tick(&mut self, app: &mut EditorApp, ctx: &egui::Context) {
        let cpu_ms = self.input_at.elapsed().as_secs_f64() * 1000.;
        let _tick = spans::enter(spans::Stage::Tick);
        let locked_step = if self.mode == "workflow-cross-layer" {
            5
        } else {
            11
        };
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
        self.log.observe(Pending {
            id: self.frame_id,
            input: self.input_at,
            observed: json!({"id":self.frame_id,"phase":self.phase,"pass_index":ctx.current_pass_index(),"processed_modifiers":ctx.input(|i|modifier_value(i.modifiers)),"processed_buttons":ctx.input(|i|button_inputs(&i.raw)),"processed_keys":ctx.input(|i|key_inputs(&i.raw)),"processed_navigation":ctx.input(|i|json!({"scroll":[i.smooth_scroll_delta.x,i.smooth_scroll_delta.y],"zoom":i.zoom_delta()})),"injected":self.injected,"view":view_parameters(app),"gesture":app.drag.as_ref().map(|d|json!({"last":[d.last.x,d.last.y],"confirmed":d.confirmed,"dragging":d.evidence_dragging(),"error":d.error()})),"input_ns":self.input_at.duration_since(self.started).as_nanos() as u64,"input_seconds":interval,"observed_ns":self.started.elapsed().as_nanos() as u64,"state":state(app),"cpu_update_ms":cpu_ms,"frame_interval_ms":self.frame_interval,"focused":self.focused,"prepare":self.prepared.take(),"snapshot_identity_unchanged":stable,"scene_identity_unchanged":scene_stable,"preview_index_identity_unchanged":app.last_good.as_ref().is_none_or(|l|Arc::ptr_eq(&l.scene.index,&l.index)),"paint_delta":app.last_good.as_ref().map(|l|l.uniforms.preview),"paint":app.last_good.as_ref().map(|l|json!({"scene_serial":l.scene.serial,"scene_anchor":l.scene.anchor,"uniform_view":l.uniforms.view,"uniform_camera":l.uniforms.camera,"uniform_counts":l.uniforms.counts,"objects":l.scene.objects.len(),"primitives":l.scene.primitives.len(),"points":l.scene.points.len(),"index_words":l.index.data.len()})),"display_message":app.display_error}),
        });
        if self.close_requested.is_some() {
            return;
        }
        if self.phase != 14 {
            if self.started.elapsed() > Duration::from_secs(240) {
                self.failures.push(format!("timeout phase {}", self.phase));
                self.finish(app, ctx);
                return;
            }
            if (app.view.error.is_some()
                && !(self.mode.starts_with("workflow")
                    && (self.workflow_step == locked_step
                        || self.workflow_step == locked_step + 1)
                    && app
                        .view
                        .error
                        .as_ref()
                        .is_some_and(|e| e.code == "LAYER_LOCKED")))
                || app.display_error.as_deref().is_some_and(|message| {
                    !(app.display_pending && message == "正在准备当前缩放的完整图形")
                })
            {
                self.failures.push(format!(
                    "app error {:?} {:?}",
                    app.view.error, app.display_error
                ));
                self.finish(app, ctx);
                return;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        if app.busy {
            return;
        }
        let _phase = spans::enter(spans::Stage::PhaseActions);
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
                // Internal observation protocol: do not run a short workflow before
                // the runner has started its real owned-window movie producer.
                let window_ready = self.dir.join("window-ready.json");
                if !window_ready.is_file() {
                    let marker = json!({"app_pid":std::process::id(),"run_id":self.request["run_id"],"frame_id":self.frame_id,"canvas":view_parameters(app)["rect"],"ppp":ctx.pixels_per_point()});
                    self.event("window-ready", marker.clone());
                    std::fs::write(window_ready, serde_json::to_vec_pretty(&marker).unwrap())
                        .unwrap();
                }
                let Ok(bytes) = std::fs::read(self.dir.join("capture-ready.json")) else {
                    return;
                };
                let Ok(ready) = serde_json::from_slice::<Value>(&bytes) else {
                    return;
                };
                if ready["app_pid"].as_u64() != Some(std::process::id() as u64)
                    || ready["run_id"] != self.request["run_id"]
                {
                    return;
                }
                self.event("capture-ready", ready);
                let path = PathBuf::from(self.request["fixture"].as_str().unwrap());
                app.send(if self.mode.starts_with("workflow") {
                    Action::OpenProject(path, true)
                } else {
                    Action::Open(path)
                });
                self.enter(1);
            }
            1 if self.complete_current(app) => {
                if self.mode.starts_with("workflow") {
                    self.snapshot(app, "workflow-opened");
                    self.event("workflow-opened",json!({"state":state(app),"visible_frame_id":self.complete.as_ref().unwrap()["id"],"visible_completed_ns":self.complete.as_ref().unwrap()["completed_ns"]}));
                    app.grid.snap_enabled = false;
                    app.object_snap.enabled = false;
                    self.enter(if self.mode == "workflow-reopen" {
                        12
                    } else {
                        70
                    });
                    return;
                }
                if ["nav", "points"].contains(&self.mode.as_str()) {
                    self.enter(2);
                    return;
                }
                app.send(Action::SelectRect(
                    BoundsMm {
                        min_x_mm: 0.5,
                        min_y_mm: 0.5,
                        max_x_mm: 20.5,
                        max_y_mm: 50.5,
                    },
                    SelectRectMode::Window,
                ));
                self.enter(2);
            }
            2 if self.complete_current(app) => {
                if !["nav", "points"].contains(&self.mode.as_str())
                    && app.view.selected.ordered.len() != self.count
                {
                    self.failures.push("selection count mismatch".into());
                    self.finish(app, ctx);
                    return;
                }
                app.camera = Camera {
                    center: MmPoint::new(200.5, 125.5),
                    scale: 1.656,
                };
                app.fit = false;
                app.grid.snap_enabled = false;
                app.object_snap.enabled = false;
                self.snapshot(app, "before");
                self.event("warmup-begin", state(app));
                self.enter(3);
            }
            3 if self.since.elapsed() >= Duration::from_secs(10) && self.complete_current(app) => {
                self.baseline = state(app);
                self.baseline_snapshot = app.view.snap_snapshot.clone();
                self.baseline_scene = app.view.scene.clone();
                self.press = app.camera.screen(MmPoint::new(1., 1.), app.canvas_rect);
                self.event("baseline",json!({"state":self.baseline,"counters":crate::native_s5m1::counter_snapshot()}));
                self.screenshot(ctx, "pmix-baseline");
                if self.mode == "nav" {
                    self.enter(20);
                    self.trajectory_origin = Some(self.since);
                    self.event("navigation-begin", json!({"origin_ns":self.since.duration_since(self.started).as_nanos() as u64}));
                } else if self.mode == "points" {
                    self.enter(30);
                } else {
                    self.enter(4);
                }
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
            70 => {
                if let Some((label, action, params)) = self.workflow_action(app) {
                    self.operation_at = Some(Instant::now());
                    self.point_sequence = app.sequence;
                    self.event("workflow-input",json!({"step":self.workflow_step,"label":label,"params":params,"state":state(app)}));
                    app.send(action);
                    self.enter(71);
                } else {
                    self.screenshot(ctx, "pmix-workflow");
                    self.enter(12);
                }
            }
            71 if app.sequence > self.point_sequence
                && self.complete_current(app)
                && self.complete.as_ref().is_some_and(|f| {
                    f["input_ns"].as_u64().unwrap()
                        >= self
                            .operation_at
                            .unwrap()
                            .duration_since(self.started)
                            .as_nanos() as u64
                }) =>
            {
                let label = self.workflow_action(app).map(|v| v.0).unwrap();
                if self.workflow_step == locked_step
                    && app
                        .view
                        .error
                        .as_ref()
                        .is_none_or(|e| e.code != "LAYER_LOCKED")
                {
                    self.failures
                        .push("locked move did not reject with LAYER_LOCKED".into());
                }
                let frame = self.complete.as_ref().unwrap();
                let workers = worker_snapshot();
                self.event("workflow-complete",json!({"step":self.workflow_step,"label":label,"state":state(app),"error":app.view.error,"duration_ms":self.operation_at.unwrap().elapsed().as_secs_f64()*1000.,"visible_frame_id":frame["id"],"visible_completed_ns":frame["completed_ns"],"worker_sequence":workers.as_array().unwrap().iter().find(|v|v["sequence"]==self.point_sequence+1).map(|v|&v["sequence"])}));
                self.snapshot(app, &format!("workflow-step-{:02}", self.workflow_step));
                self.workflow_step += 1;
                if self.workflow_step == 14 {
                    self.enter(75);
                    self.event("workflow-navigation-begin", state(app));
                } else {
                    self.enter(70);
                }
            }
            76 if self.complete_current(app) => {
                self.event("workflow-navigation-complete",json!({"state":state(app),"visible_frame_id":self.complete.as_ref().unwrap()["id"],"visible_completed_ns":self.complete.as_ref().unwrap()["completed_ns"]}));
                self.snapshot(app, "workflow-navigation");
                self.enter(70);
            }
            30 => {
                if self.point_index == 200 {
                    app.camera = Camera {
                        center: MmPoint::new(200.5, 125.5),
                        scale: 1.656,
                    };
                    self.enter(34);
                } else {
                    let p = &self.protocol["points"][self.point_index]["position_mm"];
                    app.camera = Camera {
                        center: MmPoint::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap()),
                        scale: 20.,
                    };
                    self.enter(31);
                }
            }
            31 if self.complete_current(app)
                && self.since.elapsed() > Duration::from_millis(100) =>
            {
                self.enter(32);
            }
            33 if app.sequence > self.point_sequence
                && app.drag.is_none()
                && self.complete_current(app) =>
            {
                let expected = &self.protocol["points"][self.point_index]["expected_id"];
                let actual = json!(app.view.selected.primary().map(|o| &o.object.object_id));
                if &actual != expected {
                    self.failures.push(format!(
                        "point {} expected {} got {}",
                        self.point_index, expected, actual
                    ));
                }
                self.completion(app, "point-complete", Some("select"));
                self.point_index += 1;
                self.enter(30);
            }
            34 if self.complete_current(app)
                && self.since.elapsed() > Duration::from_millis(100) =>
            {
                self.enter(35);
            }
            38 if app.sequence > self.point_sequence && self.complete_current(app) => {
                self.completion(app, "box-complete", Some("select-rect"));
                if app.view.selected.ordered.len() != 100000 {
                    self.failures.push("box selection count".into());
                }
                self.snapshot(app, "full-box");
                self.enter(12);
            }
            21 if self.complete_current(app) => {
                self.event("navigation-complete",json!({"state":state(app),"visible_frame_id":self.complete.as_ref().unwrap()["id"],"visible_completed_ns":self.complete.as_ref().unwrap()["completed_ns"]}));
                self.snapshot(app, "after-navigation");
                self.screenshot(ctx, "pmix-navigation");
                self.enter(12);
            }
            12 if self.since.elapsed() > Duration::from_millis(300)
                && app.geometry_task.is_none()
                && worker_snapshot().as_array().unwrap().len()
                    == REQUESTS.get_or_init(Default::default).lock().unwrap().len() =>
            {
                self.finish(app, ctx)
            }
            13 => self.finish(app, ctx),
            14 => self.close_after_capture(app, ctx),
            _ => {}
        }
    }
}

#[cfg(test)]
mod frame_log_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn free_drag_alt_is_scoped_to_press_through_release_confirmation() {
        for mode in [
            "move",
            "escape",
            "new-project",
            "nav",
            "points",
            "workflow",
            "workflow-reopen",
            "workflow-cross-layer",
        ] {
            for phase in 0..=76 {
                let modifiers = input_modifiers(mode, phase);
                assert_eq!(
                    modifiers.alt,
                    ["move", "escape", "new-project"].contains(&mode) && (4..=7).contains(&phase)
                );
                assert!(
                    !modifiers.ctrl && !modifiers.shift && !modifiers.command && !modifiers.mac_cmd
                );
                assert_eq!(
                    crate::selection::SelectionMode::from_modifiers(modifiers),
                    crate::selection::SelectionMode::Replace
                );
            }
        }
    }

    #[test]
    fn egui_receives_alt_on_drag_buttons_and_escape_then_clears_it_for_history() {
        let ctx = egui::Context::default();
        let position = Pos2::new(20., 30.);
        let mut press = egui::RawInput {
            modifiers: input_modifiers("escape", 4),
            ..Default::default()
        };
        Run::pointer(&mut press, position, Some(true));
        let press_buttons = button_inputs(&press);
        let _ = ctx.run(press, |ctx| {
            ctx.input(|input| {
                assert!(input.modifiers.alt && input.pointer.primary_pressed());
                assert_eq!(button_inputs(&input.raw), press_buttons);
            });
        });
        let mut release = egui::RawInput {
            modifiers: input_modifiers("escape", 6),
            ..Default::default()
        };
        Run::pointer(&mut release, position, Some(false));
        Run::key(&mut release, egui::Key::Escape, false, false);
        let release_buttons = button_inputs(&release);
        let release_keys = key_inputs(&release);
        let _ = ctx.run(release, |ctx| {
            ctx.input(|input| {
                assert!(
                    input.modifiers.alt
                        && input.pointer.primary_released()
                        && input.key_pressed(egui::Key::Escape)
                );
                assert!(crate::drag::cancelled(
                    input.key_pressed(egui::Key::Escape),
                    true,
                    false,
                    input.pointer.primary_down(),
                    input.pointer.primary_released()
                ));
                assert_eq!(button_inputs(&input.raw), release_buttons);
                assert_eq!(key_inputs(&input.raw), release_keys);
            });
        });
        for (phase, shift) in [(8, false), (10, true)] {
            let mut history = egui::RawInput {
                modifiers: input_modifiers("move", phase),
                ..Default::default()
            };
            Run::key(&mut history, egui::Key::Z, true, shift);
            let history_keys = key_inputs(&history);
            let _ = ctx.run(history, |ctx| {
                ctx.input(|input| {
                    assert!(
                        !input.modifiers.alt && input.modifiers.command && input.modifiers.mac_cmd
                    );
                    assert_eq!(input.modifiers.shift, shift);
                    assert_eq!(key_inputs(&input.raw), history_keys);
                });
            });
        }
    }

    fn pending(id: u64, origin: Instant) -> Pending {
        Pending {
            id,
            input: Instant::now(),
            observed: json!({"id":id,"phase":14,"input_ns":origin.elapsed().as_nanos() as u64}),
        }
    }
    #[test]
    fn zero_one_and_multiple_close_redraws_retain_a_real_terminal_frame() {
        for redraws in [0, 1, 3] {
            let origin = Instant::now();
            let mut log = FrameLog::default();
            let mut fences = 0;
            // Closing changes protocol control only. Each actual subsequent
            // input still settles its preceding observed production callback.
            for id in 1..=redraws + 1 {
                if let Some(frame) = log.complete_pending(
                    id - 1,
                    || {
                        fences += 1;
                        true
                    },
                    origin,
                    || json!({"draw":id-1,"uniform-upload":(id-1)*112}),
                ) {
                    log.frames.push(frame);
                }
                log.observe(pending(id, origin));
            }
            let terminal = log
                .complete_pending(
                    redraws + 1,
                    || {
                        fences += 1;
                        true
                    },
                    origin,
                    || json!({"draw":redraws+1,"uniform-upload":(redraws+1)*112}),
                )
                .unwrap();
            assert_eq!(log.frames.len(), redraws as usize);
            assert_eq!(terminal["id"], redraws + 1);
            assert_eq!(terminal["painted"], true);
            assert_eq!(terminal["gpu_completed"], true);
            assert_eq!(fences, redraws + 1);
            assert!(log.pending.is_none());
        }
    }
    #[test]
    fn unpainted_terminal_update_never_invents_a_gpu_fence() {
        let origin = Instant::now();
        let mut log = FrameLog::default();
        log.observe(pending(1, origin));
        let frame = log
            .complete_pending(
                0,
                || panic!("unsubmitted callback polled"),
                origin,
                || json!({}),
            )
            .unwrap();
        assert_eq!(frame["painted"], false);
        assert!(frame.get("gpu_completed").is_none());
        assert!(frame.get("completed_ns").is_none());
        assert!(frame.get("input_gpu_complete_ms").is_none());
    }
    #[test]
    fn failed_terminal_fence_is_recorded_as_failure_not_completion_success() {
        let origin = Instant::now();
        let mut log = FrameLog::default();
        log.observe(pending(1, origin));
        let frame = log
            .complete_pending(1, || false, origin, || json!({"draw":1}))
            .unwrap();
        assert_eq!(frame["painted"], true);
        assert_eq!(frame["gpu_completed"], false);
    }
    #[test]
    fn final_counter_snapshot_follows_the_real_fence_and_is_not_reused() {
        let origin = Instant::now();
        let mut log = FrameLog::default();
        let fenced = Cell::new(false);
        log.observe(pending(1, origin));
        let frame = log
            .complete_pending(
                1,
                || {
                    fenced.set(true);
                    true
                },
                origin,
                || {
                    assert!(fenced.get());
                    json!({"draw":1})
                },
            )
            .unwrap();
        assert_eq!(frame["counters"]["draw"], 1);
        assert!(
            log.complete_pending(
                1,
                || panic!("duplicate fence"),
                origin,
                || panic!("duplicate counters")
            )
            .is_none()
        );
    }
    #[test]
    #[should_panic(expected = "PMIX duplicate update pass")]
    fn a_discard_pass_cannot_overwrite_an_unsettled_raw_frame() {
        let origin = Instant::now();
        let mut log = FrameLog::default();
        log.observe(pending(1, origin));
        log.observe(pending(1, origin));
    }
}
