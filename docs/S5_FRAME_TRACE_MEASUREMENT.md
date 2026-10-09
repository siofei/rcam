# S5-M2-C: passive source-clock measurement

This is a measurement-only candidate based on `81a1c81949e83e6e4857d6d909a2e3285427a753`.
Scope: R08/R16/R17/R18/R19 and bounded evidence supporting AT-001/023/024/025/032/063/074.
No manufacturing, save, input, geometry, shader, existing timing interval or acceptance threshold changes.
Multi-project implementation remains paused. Performance and true presentation loss remain OPEN.

## Explicit configuration and identity

The public application leaves the observer off unless `RCAM_FRAME_TRACE=1` and
`RCAM_FRAME_TRACE_CONFIG` names a local JSON configuration. `0` or absence disables it.
Invalid configuration reports an initialization error once and leaves product behavior intact.
No preference/UI setting is added. Evidence stays local; it must not enter a public source package.

Configuration uses `deny_unknown_fields`, at most 64 KiB and 16 sorted, nonoverlapping windows.
Choose a unique run ID and a new absolute output filename. Existing files are never overwritten.
Template values below must be supplied by the owned Mac executor:

```json
{
  "run_id": "mac-observer-unique-run",
  "output": "<new absolute private JSONL output path>",
  "expected_commit": "<new candidate full commit>",
  "expected_manifest_sha256": "<new candidate manifest SHA256>",
  "expected_binary_sha256": "<actual Release executable SHA256>",
  "clock_id": "mach_absolute_time_ns",
  "windows": [
    {"id": "move", "start_ns": "<predefined source ns>", "end_ns": "<predefined source ns>"}
  ]
}
```

Header schema 2 binds run/commit/build source, source manifest SHA, actual executable SHA,
configuration SHA, clock identity, origin and conversion. The writer hashes the executable
in 64 KiB blocks using the existing SHA-256 implementation. A mismatch fails measurement.
Wait for the flushed `identity_ready` record before an active window; initialization overlapping
a window makes its observation incomplete. Initialization and all queued earlier samples remain.
New source needs new Mac executable identity; prior native evidence is not reused.

Mac uses public `mach_absolute_time` and one `mach_timebase_info` conversion. Raw origin ticks,
converted origin ns and numerator/denominator are recorded. Conversion uses checked u128 arithmetic;
out-of-range clocks invalidate measurement instead of silently saturating a duration.
All timestamps/durations are decimal integer strings, preserving precision beyond 2^53.
Sleep/wake and external clock equivalence require Mac validation. CGEvent, SCK PTS and a script's
monotonic clock are not assumed equivalent. The non-Mac auxiliary clock is explicitly run-relative
`Instant`, without a cross-process equivalence claim.

## Source fields and their limits

- Raw-input hook: batch ID, actual viewport ID, source entry/exit, pointer/key/wheel/other counts.
  No text, key value, pointer coordinates, camera values, preview delta, object/manufacturing geometry,
  path, error payload or pixels are recorded. Local diagnostics include opaque project/document/task
  identifiers, counts, phases and physical viewport dimensions/PPP/zoom; do not publish raw evidence.
  The hook runs before a new egui pass: frame/pass mapping is established in `update`.
  Only ROOT batches map to the root App update; other viewport mapping is unavailable.
- Update: unique update ID, egui frame/pass numbers, pass index, viewport ID and input batch ID;
  source start/end and explicit previous-update source endpoint/ID/input/version. Multiple passes
  are preserved. An App update is not a physical display refresh.
- Start/end snapshots: selection count; project/document/revision/workspace, task/rule generation,
  selection epoch and scene serial through immutable version IDs; pending task IDs; drag status;
  inactive plus all six actual MovePlace phases: preparing/following/frozen/final_preview/ready/applying.
  Phase snapshots observe state, not an exact internal transition instant.
- Physical canvas size, effective pixels-per-point, native pixels-per-point and UI zoom. Start size
  is explicitly stored-before-layout; end uses this update's allocated canvas or `layout_not_reached`.
- Validation, real prepare and canvas-panel spans are separate wall durations. Prepare is nested
  inside canvas; validation is not added into the observer's real-prepare field. Do not sum nested
  spans as full update. Existing combined `cpu_prepare_ms` and legacy logging remain unchanged.
- Existing wgpu callback prepare/paint entry/exit and draw-encoded/clip-no-resource outcome.
  A logical callback-enqueued ID connects these to the generating update/pass. It is not submission.
- Successful prepare preserves its source update/version/input batch, actual scene, physical canvas size,
  PPP and selection epoch. Last-good fallback retains that source and separately records the enqueue
  App version. Camera center/scale, canvas coordinates and preview delta are not serialized; source
  update/version IDs identify the old prepared frame without exposing those values.
- Request attempt IDs cover early rejection with `task_id=null`. Accepted requests retain the
  existing TaskVersion input and assigned task ID. Received receipts retain input/result versions
  and state, and are labelled received rather than universally installed. A recorder-only installed
  open/restore/new/close generation distinguishes reopening the same persisted identity.

The outer update wrapper observes both sides of the unchanged update body; an early body return
still reaches the end snapshot and one canonical end timestamp. Unwind/unfinished scopes are explicit
and cannot make complete evidence. The duration excludes raw input, egui tessellation and framework
render/submit/present/event waiting. Every duration is wall duration, possibly including preemption;
it does not measure actual CPU utilization.

`RawInput` has no native per-event creation/dispatch ID or timestamp. App batch to update/pass is
direct; external dispatch to batch is unavailable as an exact per-event link because OS delivery
can coalesce. Keep the external CGEvent timeline separately, including its clock mapping and SHA.
Do not use nearest timestamps to claim exact input latency or inject hidden input markers.

Framework-owned `Queue::submit` and surface `present` occur after custom callbacks, without an
App post-submit/present hook. Actual SubmissionIndex, submit/completion/present/scanout timestamps
are explicitly null/unavailable. Registering completion during callback prepare/paint would observe
earlier submitted work, not prove completion of this frame. No extra submit, poll Wait, screenshot,
forced repaint, third-party modification or additional system permission is introduced.

## Bounded output and shutdown

Input classification retains the original four category counts and adds separate PointerMoved,
PointerButton and PointerGone counts, RawInput.focused, and WindowFocused(true/false) counts.
Update snapshots also expose egui's focused state. No button contents, key contents, text,
pointer positions or world coordinates are captured. Focus events can coexist; the counts retain
each event even when the exit reason uses the branch's priority (unfocused, explicit window focus
loss, then PointerGone).

`move_exit` records are emitted immediately before existing placement clear/replacement paths.
They carry a fixed reason enum, prior phase, source-clock time and version binding. The reasons
distinguish input cancellation, context invalidation, modal/tool/transition replacement, task
identity/error/preview failures, request admission failures and terminal cleanup. `apply_terminal`
means an apply reply reached cleanup, not proof that manufacturing committed; task metadata and
the installed View must establish that result. A terminal View error takes reason priority as
`task_error`, including apply replies; the recorded prior phase retains the apply association.
`modal_task_completed` identifies the existing
successful modal-reply clear branch. Nested cleanup may emit another reason before the same clear;
the first source-ordered branch is the trigger, later records must not overwrite it. Calls outside
an active App update have `egui_identity_known=false`, never a borrowed previous update ID.
These are branch observations, not new cancellation rules or an exact native event mapping.
Any dropped record still makes the observation incomplete and prevents definitive attribution.
The offline report preserves complete input rows (including additive detail fields) in
`input_batches`, and every `move_exit` row under `move_exits[].raw` with a reason status.
Unknown reasons and malformed exit records remain visible and make classification and observation
incomplete. Supported schema2 traces without exit classifications return `NO_EXIT_RECORDS`, which
does not imply no cancellation; schema1 remains unsupported and is rejected explicitly.
Known labels identify source branches only. `native_exit_attribution_status` remains OPEN, as does
performance. Inspect original JSONL by update/input/version binding and source time; report output
does not grant native event identity, repair drops or overwrite an earlier trigger.

Producer uses a preallocated 4096-record queue, at most 256 bytes per numeric record (<=1 MiB).
`Mutex::try_lock` never waits for queue capacity or a mutex; contention/full drops the new record.
No producer JSON formatting, terminal or disk write, sleep or GPU synchronization is added.
Metadata generation happens only on changes/requests, with identifiers <=256 bytes and a conservative
4 MiB cumulative admission threshold covering clones/cache/scratch. The last bounded candidate's
attempted charge may cross that threshold; it immediately fails measurement and stops further capture.
The cumulative charge is distinct from live allocations and process RSS. Writer has a 64 KiB buffer,
64 KiB executable hash block and 512 KiB stack. Within that stack, a fixed 16-slot
`Option<Record>` staging batch is bounded to 4 KiB separately from the 4096-record queue;
the header reports its capacity and exact size. Metadata is moved into staging without cloning.
Output including header/metadata/numeric/footer is limited to 128 MiB, reserving 8 KiB for failure/footer.
Budgets describe this observer, not the existing renderer's memory.

Producer queue notifications occur after unlocking, only when enqueue changes an empty queue
to nonempty. Close and admitted span completion retain their independent notifications.
Writer moves at most 16 records per lock acquisition and formats/writes outside it; flush every <=250 ms of available
execution or 256 KiB. OS I/O can still block the writer and affect process scheduling. Counters expose
attempted/accepted/consumed, per-stage and metadata drops, full/contention, exact locked high-water,
inflight admitted scopes, first/last drop source time, bytes and flush count. Dequeue age and serialized
record write wall time are distinct fields. Record sequence is independent of physical row order.
Missing metadata or any missing record invalidates completeness; samples are not silently repaired.
Batching reduces lock acquisitions when records accumulate, but lengthens one drain section;
it does not guarantee zero contention drops. Native paired measurements are still required.

Closing stops new admissions. Spans admitted before closing remain counted until their retained end
has been attempted, including spans crossing the cutoff. Writer drains after admitted producers/scopes
finish, writes footer and flushes, then sends completion. The owned exit supervisor retains the JoinHandle,
uses one 2 s deadline for completion reception and finished-thread polling, then explicitly joins
only after is_finished. Deadline is checked again before/after join. Rust/OS join epilogue and process
scheduling are not a hard real-time wall bound; no live-writer join waits for blocked file I/O. Only successful
flush/completion/join yields an acknowledgement. A live thread is detached on timeout; that timeout
cannot interrupt a blocked file write. The live-writer path never joins on the GUI thread; flush is not fsync. `finalization_ack=ok` proves successful flush and writer join,
not zero loss. A late footer after timeout cannot establish successful supervision. A hard kill without
footer remains incomplete; raw prefix is recoverable. Default-disabled creates no clock, queue, thread
or file. Observer overhead target is p95 <=0.25 ms/update, p99 <=0.5 ms/update; these are unmeasured
targets requiring paired Mac Release tests, not a performance PASS.

## Offline endpoint classification

Run the report script only after owned-process supervision, using its matching local stderr log:

```text
python3 scripts/frame_trace_report.py trace.jsonl --output new-report.json --supervisor-log stderr.log
```

The raw file SHA is bound in the report. All endpoints/spans remain in its classification lists,
including before/after/cross-start/cross-end/cross-both and missing-neighbor gaps. For [F,L), a main
interval needs both endpoints inside the window and an observed contiguous previous update.
The first point has no invented interval. A 140–902 ms interval beginning before F is retained in
full as a boundary interval, not assigned to pure activity. Endpoints never use writer/stderr arrival.
Drops, dangling identities, unfinished spans, missing footer/ack or startup overlap make observation
INCOMPLETE. Nested stages and post-activity drain are retained. No application FPS is computed.

Optional `--actual-boundaries` and `--clock-calibration` read separately hashed executor JSON.
A boolean assertion does not establish calibration. Without supported structured evidence, the report
retains internal source-clock diagnostics and unassociated external window metadata; activity-window
association is INCOMPLETE. Predefined windows are explicitly internal source-clock classifications.
No external dispatch payload is copied into the report.

Only `shared_mach_absolute_timebase` is currently supported: the owned executor directly captures
public Mach ticks around dispatch, rather than assuming CGEvent.timestamp or SCK PTS equivalence.
Calibration schema 1 binds `run_id`, `trace_identity` (commit, source_manifest_sha256, binary_sha256,
config_sha256), `source_clock` and `external_clock` (clock_id, origin_raw_ticks, timebase numer/denom),
`provenance` (source, producer_pid, producer_sha256, evidence_sha256), 2..32 ordered `sync_samples`
(before_raw_ticks, external_raw_ticks, after_raw_ticks, external_ns) and `max_error_ns`. Clock ID is
mach_absolute_time_ns; both timebases must equal the trace header. Every external sample must lie
inside its raw tick bracket and convert exactly; declared error must cover the largest converted
bracket width. Two samples must cover every external window. Each actual window adds
first_dispatch_raw_ticks/last_dispatch_raw_ticks and its ns endpoints must match checked conversion;
raw tick samples must also cover F/L before ns rounding. Window IDs match a nonempty predefined set.
All ticks/ns are decimal strings. Unknown calibration/window fields are rejected. Unassociated windows
are projected through a numeric/opaque-ID whitelist; provenance.source is an ASCII descriptor ID,
never a path or payload. Provenance hashes identify
external artifacts; their existence/truth needs the executor's separately preserved raw evidence.
The analyzer checks consistency, not physical instrumentation authenticity or universal clock equality.
A different clock domain is unsupported and remains INCOMPLETE; this cloud run fabricates no Mac
calibration. Parser fixtures are synthetic tests only.

The report records supported evidence with its declared error bound; it does not claim exact boundary
precision or a per-CGEvent receipt link. Both planned and actual-boundary reports are retained.
Use `--clock-calibration calibration.json` with the two existing evidence arguments. This script always
leaves `performance_status=OPEN`, including complete short observations.
480 events over 5.2–5.5 s and a 24-condition matrix are diagnostic, not the 60 s frozen acceptance.

## Checks and remaining native work

Meaningful regressions cover queue-full/lock-contention/drop/metadata gaps, integer conversion,
close crossing a live span, source endpoints, I/O/budget failure, multi-pass and unfinished scopes,
same-identity reopen, fallback lineage, six phases and a real App update, exact window endpoints,
supervision, mismatched clocks and missing records. Initial failures are retained in private evidence.
Locked Rust 1.89 strict product checks keep the known Linux GUI `ordered-float 5.5.0` MSRV blocker;
headless and App CPU harness checks are auxiliary and do not establish Mac/Metal PASS.

Mac must verify new source/binary/config identities, ordinary checks, phase and resize logging,
source-clock mapping, callback behavior, observer off/on overhead/drop/CPU/RSS, shutdown/flush and
sampler/logging conditions with all original samples retained. True GPU presentation needs a separate
supported platform observation and cannot be inferred from update or capture callback intervals.

## Mac candidate handoff

Fetch the independently published candidate branch, check out its exact verified commit in a clean
worktree, and verify its direct parent is the frozen `c41a6388e90c51926fccefd2db81b84e7dc4a04e`.
Do not treat the remote default branch as this candidate. Verify all701 manifest entries before building:

```text
git rev-parse HEAD
git rev-parse HEAD^
git status --porcelain
python3 scripts/source_manifest.py --check
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
```

Use the repository's locked toolchain and record its actual version. A blocked or failed command is
retained as such; the auxiliary Linux harness is not a native substitute. Build/config/trace/run IDs,
manifest/executable hashes and structured shared-Mach calibration must belong to this new candidate.
The header's expected identities and matching finalization acknowledgement follow the contract above.
Keep all planned/external windows, screenshot intervals, dropped records and inactive Move windows.
Any residual loss remains INCOMPLETE; neither full12 nor observer-off/on overhead is already passed.

For a new Following→inactive transition, retain the last Following and first inactive update IDs and
source times, associated input detail, all source-ordered move_exit rows (record_seq breaks time ties),
version and task metadata, footer and matching acknowledgement. Preserve simultaneous focus/Gone counts,
all nested cleanup records and unknown reasons. Existing source reasons can identify the App branch;
they do not establish exact external CGEvent→RawInput identity or identify an unrecorded external cause.
Do not retrospectively split the old aggregate pointer count or label the old unexplained exit as
input interference. The prior queue-contention result is still INCOMPLETE, and this repair has not
yet been shown to eliminate drops on Mac. Actual submit/GPU completion/present/scanout remain unavailable.

Clock conversion source: [Apple QA1398](https://developer.apple.com/library/archive/qa/qa1398/_index.html).
Framework boundaries were checked in the locked eframe/egui-wgpu 0.33.3 and wgpu 27.0.1 source.
