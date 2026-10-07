# External guarded PMIX full12 coordinator, v3.6.3-bound candidate

This source-only candidate binds the independently reviewed product phase/helper
contract and independently measured fresh Mac build identities below. Final
independent bound review is still required before publication and execution.
No previous product/build hashes are reused as runtime pins.
The original set60 API timeout remains an unresolved diagnostic hypothesis;
this guard candidate does not claim that the display API has been repaired.

The five frozen identities are:

- Product commit: 42db040374b0c1643200b7f04e95b03d272ef792
- Source manifest: 0235f9c9bd89d76dd5b101dc2fd99ba226582f1431464467696377664088067b
- Internal app: 8223232d90072e7587cba09f6cf0963749f8e04a31f6ea89dce47b21ba02353f
- Capture producer: bf3a6b2f879d28b9fe1aa8a3a724639bbd1ff6d1191fbc8666f0ffbd1f863270
- Native runner: 20dba8c15a2ed547c98c00ba7d994ffac0dbc610c3f2f0178c046dd6b390a74c

Missing or malformed pins still fail before filesystem changes, compilation or
process launch. Every source entry and both runtime binaries are rechecked.
The exact original independently frozen external gate ledger remains an explicit
required input; do not derive its expected digest from a submitted bundle.
The old direct microcheck CLI is disabled.

This package is external to RCam. It changes no product source or manifest. All
paths are operator-supplied or derived from the package. Publish only these seven
source files: `interference.swift`, `guard_policy.py`, `supervise.py`, `matrix.py`,
`test_guard.py`, `test_matrix.py`, and this README. Never publish runtime controls,
raw process/display/input logs, captured media, evidence bundles or private paths.

## Fixed execution contract

The exact twelve cases, in order, are nav1, nav2, nav3, move1, move2, move3,
points1, escape1, new-project1, workflow1, workflow-reopen1, workflow-cross-layer1.
Each has its own fresh output directory, native UUID, owned runner and immutable
owned-app binding. No failed case is retried or skipped. The first failure stops
further GUI execution.

Every runner receives actual `--display-policy frozen-60hz`,
`--allow-display-mode-change`, and `--display-id 2`, with schema4 request evidence. The observer protocol remains version3.
The authorized original snapshot is target 2/mode 113/144 Hz, logical1920x1080,
physical3840x2160 and scale 2, never mirrored. There is no outer display lease.
Each runner enters60 Hz and restores its own original 144 Hz target in `finally`.
Five owned display operations are mandatory: AppKit display-probe.swift probe 2, Quartz-only display.swift set60 2, independent probe 2
after setter join, restore 2 113, independent probe 2 after restorer join. Both
independent probes and every complete target/geometry snapshot are checked.

The navigation protocol remains ten seconds warmup followed by the full sixty
seconds trajectory in each of three fresh navigation runs. All original product
thresholds, manufacturing fixtures, timing/ROI/MOV predicates and negative cases
remain the responsibility of the unchanged native/product verifiers.

Only workflow-reopen receives `--fixture`, and it is the immediately preceding
workflow1/workflow-output.rcam. The fresh runner's copied reopen-input.rcam and
request fixture digest must match those bytes. The original aggregate separately
verifies the same chain. A frozen synthetic checkout fixture is insufficient.

## Continuous protection and ownership

The Swift observer and original input/foreground predicates retain their
reviewed behavior. Formal startup adds the authenticated APP_LAUNCH deadline
origin described below.
A permanently unbound input-only sentinel is armed once before the first runner;
it retains the same raw baseline across round launch, display switches, native
verification, cleanup and inter-round gaps. Each round additionally has its own
original owned-process observer. No existing PID binding is reset or reused.

Each round preserves the 190-second execution / 40-second cleanup budgets,
including actual observed join times even on a first poll that finds an exited
process. Exception cleanup saves its original 40-second deadline at the first
interruption, immediately after SIGINT and before any journal I/O. After journal
I/O, it recalculates only the remaining wait against that same deadline. If that first clock read fails,
its bounded relative wait happens before journal I/O; the cleanup origin stays
unknown and cannot be restarted from a later clock or
join; a prior interrupt with an unknown deadline permits only a zero-time join
check. The 20 ms bracketed event-age tolerance and 250 ms raw freshness, gap
and post-join limits remain unchanged. HID changes halt. Combined-only changes also halt without human
attribution. Malformed, exited, stale or stopped monitors fail closed. The clock
is Darwin CLOCK_UPTIME_RAW nanoseconds, matching Swift Dispatch uptime; Darwin
Python3.9 process-relative monotonic time is never used for guard deadlines.

There is no external activation, event tap, key content, pointer position or
window-content inspection by the observers. RCam's existing focus request is
observed passively. After first owned foreground, focus loss halts. Only the
already-owned runner receives SIGINT; its cleanup owns its app/producer. Every
joined path requires an actual post-join sample, not a queued older sample.
Strict Swift6 concurrency checking and warnings-as-errors remain enabled.

Formal startup now distinguishes the authenticated runner preparation phase
from the app launch opportunity. The owned Popen receives one canonical UUID
through RCAM_PMIX_LAUNCH_NONCE. Its kernel credential must first be verified by
the unchanged observer. Only then may the guard follow the fresh output's
immutable runner-binding.json to the exact native/request/runID/source/app/
producer/target graph. Native/request bytes are complete before that output
commit is published. app-launch.json is immutable and published after the
original setter and independent60Hz postprobe have actually joined and passed
all original product diagnostic validators, immediately before app Popen.
Both phases use CLOCK_UPTIME_RAW; hashes and parsing use the same byte snapshot.

The original20s app-binding and30s readiness clocks start once at APP_LAUNCH's
actual opportunity, never at owned_bound, ready or a later file read. The
190s overall runner budget still begins at the original owned runner launch.
Every preparation child keeps its original budget. A preparation failure does
not create an APP marker: the failed runner may complete its bounded finally
restore without an app deadline demanding an app that was never started. Input,
identity and250ms raw protection continue throughout; no baseline is reset.
Late first binding/readiness observations cannot hide an expired opportunity.

The temporary native directory and the canonical --output directory are distinct.
Original runner argv and the immutable binding connect them. Round validation
binds its input to --output while supervisor native metadata binds to the temporary
native directory. Offline validation replays the same authenticated opportunity
at its recorded original raw sequence and checks the kernel/binding/marker bytes.
Emergency recovery still requires all five paired completed display stages;
a preapp failure does not qualify through a partial journal or later144Hz probe.

Quartz mutation receipts contain exactly eight real core fields; they do not
supply backing_scale. The three independent AppKit probes supply all nine fields
and the real scale. Core projections must agree across setter/restorer joins,
and the final complete nine-field probe must equal the original snapshot.
Successful phase stderr is validated by the pinned product's pure diagnostic
API. Restore and its independent probe use separate reviewed literals and files.

Trusted per-round native verification runs in a separately owned, file-logged
child while the sentinel continues sampling. Long verification or malformed
input cannot create an unobserved baseline reset. The sentinel has a final fresh
barrier and a checked tail drain after its intentional owned stop. Offline final
validation replays every raw continuous sample using the unchanged policy and
checks coverage of all twelve launch/join intervals. Both streams must start at
sample 1 and actually reach ARMED on sample 2 before launch. Each original owned
stream is also fully replayed against its nonce, immutable control/path/runID,
kernel credentials and actual foreground/capture state; a clean summary receipt
cannot replace that raw proof. A legitimate capture marker first observed by the
fresh post-join sample remains valid under the original policy.

The complete extra owned-round check also runs in a separately owned,
file-logged child through the same watcher. This includes every original raw
sample replay, display/identity checks and workflow project hashing. The parent
continues consuming the live sentinel throughout this work. It accepts the
check result only after exit0, observed join, a fresh post-join barrier and exact
case/runID/input/supervisor-receipt/workflow bindings. A failed or interrupted
check cannot advance the matrix. The internal `check-round` operation only
reads evidence and writes its own external result; it does not launch RCam.

This addresses a reproduced consumption pause in the previous coordinator:
synchronous raw replay could exceed250ms while the observer kept sampling,
then the queued first sample correctly failed the unchanged freshness check.
Historical Mac timing supports that explanation; the original caught exception
was not retained, so that historical cause remains an inference. Runtime
receipts now preserve the first stream exception, its sample sequence and
acquisition timestamps, and the consumption clock when available. Input halt
details are preserved separately. Raw samples are never skipped or rebased.

The parent freezes the exact round-check input bytes before launch. The worker
returns the digest of the bytes it actually parsed, and its canonical evidence
directory must equal the original runner's --output directory. The immutable
phase binding separately connects that output to the supervisor's temporary
native directory. The returned
digest and current input file must both match the parent's frozen digest.
Supervisor receipt hashing and parsing use the same single byte snapshot in
both parent and worker. The worker returns the digest of its actual parsed
receipt, and the parent binds it to the original receipt bytes.
Interruption-journal write errors are retained as failures while the owned child
still receives its bounded join attempt within the original cleanup deadline.
This also applies to the existing recovery-child branch; logging cannot start a
new reserve, skip the join attempt or grant a successful result.

## Bounded exceptional display restoration

A failed case always remains failed, even if restoration succeeds. Emergency
restoration is limited to the same original target 2/mode 113 and an independent
post-join 144 Hz probe. It uses the reviewed PMIX helper and its existing isolated
owned-command cleanup, within the original remaining round cleanup reserve.
It never sets60 Hz or starts an app, and input halt does not cancel the obligation
to return the original display mode.

The inner runner must first be joined, and every recorded private child must
have matching launch/process records proving leader join and group release.
For every exit status, all five display stages must have completed paired join
records. The exact pinned product writes launch evidence after Popen; signal
death or compound launch-journal, cleanup and process-journal failures can omit
a whole pair. A nonnegative exit cannot exclude an unrecorded late setter. Missing join evidence,
exhausted cleanup budget, unavailable target, mirroring, failed restore or failed
independent probe means explicit failure and no next round. The receipt states
that60 Hz may remain. Hardware loss and mid-launch SIGKILL are not guaranteed
recoverable; do not report successful restoration from an intended command alone.

## Background tests

```sh
python3 -B -m unittest -v test_guard test_matrix
```

Tests use explicitly synthetic JSON, project bytes, process actors and raw input
samples. A complete simulated12-round execution exercises the actual coordinator
and both actual policy streams. Linux PASS is not macOS/full12/full48 acceptance.
No Mac/full12/full48 or stage acceptance is granted by these synthetic tests.

## After final independent bound review

Prepare a fresh sealed canonical product bundle seed containing exactly
Source.zip, gates/, REVIEW.json and Evidence_MANIFEST.sha256, with native=[] and
the original pending-review/platform fields. No native directory or top-level
seed symlink is accepted. The complete original background inventory must match
its seal. The gate ledger SHA must be independently frozen outside that bundle.
Keep all external guard evidence outside the bundle and outside the product
checkout.

Before Swift compilation, clock initialization, observers or runners, `run` calls
the exact pinned product verifier with `--background-only`. This bounded pure
read-only qualification must return exit0 and one strict JSON object with
BACKGROUND_QUALIFIED_FOREGROUND_PENDING, stage_PASS_claim=false, both cargo and
writer/refusal tests PASS, exactly31 gates and all five matching product/ledger
identities. Foreground initialization must remain PENDING_REAL_OWNED_NATIVE.
Only two clean fully verified original background branches qualify: the narrow
BLOCKED_CAPTURE_INITIALIZATION/BLOCKED/no-eligible-window branch, or genuine
GATES_PASS/INITIALIZATION_ONLY_PASS/null. Dirty, permission-denied, unsupported,
skipped, malformed or otherwise blocked evidence is rejected before GUI work.
The product qualifier checks the complete original raw/Source/toolchain/31-ledger
evidence; the external adapter cannot create qualification from summary fields.
Preparation qualification never upgrades the original background capture result
or grants native/stage PASS.

The original background seal and REVIEW bytes, qualification command/stdout/
stderr and exact result are retained in external runtime evidence. A failed run
keeps the original background seal. After all twelve successful runs, resealing
first proves that every original Source/gates byte and inventory is unchanged.
REVIEW may only replace its empty native list with the exact twelve-case list;
the original seal remains preserved outside the bundle before the root seal is
replaced with the full inventory. Offline validation rechecks those bindings.

```sh
python3 -B matrix.py run --root /locked/product --binary /locked/internal-app \
  --producer /locked/capture-producer --bundle /fresh/product-bundle \
  --guard-evidence /fresh/external-evidence --gate-ledger-sha256 EXTERNALLY_FROZEN_SHA
```

`run` stops after guardedGUI12 and each original native verifier. It seals only a
complete successful matrix, and explicitly records native_aggregate/full48 as
NOT_RUN. It cannot grant stage PASS. Once all GUI/display operations and monitors
have joined, run the original offline aggregation and actual48 resealed attacks:

```sh
python3 -B matrix.py validate --root /locked/product --bundle /fresh/product-bundle \
  --guard-evidence /fresh/external-evidence --gate-ledger-sha256 EXTERNALLY_FROZEN_SHA \
  --attacks-output /fresh/actual48-output
```

No capture-precheck flag or plan-only flag is passed. The trusted original
aggregate must require every real owned-window initialization/capture chain and
pass; the original attack runner must accept the authentic
baseline before and after every isolated resealed mutation and reject all 48.
Offline aggregate is bounded by 190 seconds and the48-case job by 48 * 190 seconds;
a timeout is a failure, not permission to omit predicates or cases. User flicker
remains OPEN, Windows/K1 remain DEFERRED, and independent stage acceptance remains
separate from guard, native aggregate and negative-test success.

The candidate has160 portable synthetic tests, including the new authenticated
opportunity, preapp failure, exact deadline/input boundaries, source-byte and
distinct native/output directory contracts. Independent ordinary phase tests
cover46 boundary cases using an explicitly mocked preparation validator.
These checks do not establish real kernel/CG behavior or Mac/native acceptance.
The current product literals compile under explicit Swift6, but the initial
unflagged interpreter path failed before runtime. Every actual PMIX display
operation now requires the exact prefix /usr/bin/swift -swift-version 6
-warnings-as-errors before its reviewed helper and operation arguments. Window
query and the strict observer compiler remain separate unchanged contracts.

The new pinned product has passed fresh clean source/build qualification,
all31 background commands, the workspace and targeted Rust checks, and its
portable tests. Both sealed and freshly extracted background qualification
remain stage_PASS_claim=false with foreground initialization pending;
no-eligible-window is preserved as the original background capture result.
The source manifest now has672 entries. Runner, display/marker/qualifier and
external guard interfaces retain their reviewed contract. The product's
uniform-cache, PMIX input and native verifier checks remain internal product
predicates.
This successor changes only product identity pins and their test/documentation.
All earlier product native rows retain their original identities and cannot
qualify this rebuilt candidate. No new native12, aggregate, actual48 attacks
or stage acceptance is granted by this bound source package.

## Isolated display preparation diagnostics (source candidate, no acceptance)

Base: `01c5ed6fabe104833e9242c008e14e087b490d39`; product remains exactly
`42db040374b0c1643200b7f04e95b03d272ef792` / manifest `0235f9c9…` with the five
original pins above. This diagnostic changes only this observer and adds
`diagnose_display_prep.py`, `test_diagnostics.py` and this documentation. The three
production Python guard modules, product, four helper/runner APIs, literals,
manifest, twelve cases and48 gates are unchanged. Scope S5-M2-C bounded
infrastructure diagnosis, R16/R17/R18/R19/R20, AT-064/073/077/082/083; no AT closure.

The observed full12 remains **0/12 FAIL before APP launch**: continuous35→36 gap
296.303833ms; owned32→33 gap304.769500ms; owned32 also has a56.216417ms inter-field
interval. The measured98 CG brackets are short. These facts do not identify
WindowServer, runtime, scheduling, JSON/raw IO or control reads as the cause.
No full12 retry, threshold relaxation or production optimization is part of this
candidate. Raw packets remain private; no Library download/upload is needed.

A fourth observer argument explicitly enables its separate diagnostic sidecar.
The original three-argument path retains original raw JSON,50ms main/common Timer,
main-thread CG/AppKit sampling, sample begin/end and fields; it reads no extra
clock and installs no signal handler. Diagnosis requires macOS13+ for the checked
Sendable queue protected by
[OSAllocatedUnfairLock](https://developer.apple.com/documentation/os/osallocatedunfairlock).
There are no unsafe concurrency opt-outs or raw-output workers. Mac must compile
this exact source with the original strict Swift6 command before any run.

Each observer sidecar is schema1, with nonce/role/PID/sourceSHA/executableSHA,
`darwin_uptime_raw_ns`, explicit queue/memory/byte/line limits and sequential rows.
Typed payloads are encoded by a separate strict FIFO worker. Each raw event,
including ready/binding/sample/fatal, has its own ordinal and independent
`emit_start`, `emit_progress` and `emit_return` facts; no intermediate emit is
replaced by a later sample. A cycle contains the previous returned emit reference,
control entry/return, the exact original sample endpoints,34 dictionary build
brackets and timer entry/exit. The raw sample end remains before encoding/writing.
`emit_progress` records only observed pre-write facts. Its write bracket includes
the bounded diagnostic submission hook; instrumentation overhead cannot be treated
as an uninstrumented performance measurement. A missing return never gains a time
from file length or a later sample. Partial cycle/emit traces remain incomplete.

Observer queue<=256; actual queue buffer and every retained field Array capacity
are charged against768KiB, leaving256KiB for the live cycle/encoder/line. Additional
queue/encoder budget<1MiB, sidecar<=16MiB, line<=8KiB and raw<=64MiB. The parent
queue additionally accounts encoded bytes<=768KiB and count<=256, with sidecar<=8MiB
and line<=8KiB; parent diagnostic memory budget is2MiB. Admission contention,
capacity exhaustion or encode/write errors fail explicitly; no old row is dropped
or replaced. Queue locks never encompass encoding/IO. There is no stdout worker,
Timer replacement, new sampler or optimized snapshot algorithm.

Only diagnosis installs a SIGTERM dispatch source with a MainActor bridge. It
stops the Timer, submits the last observed cycle and terminal, waits at most200ms
for FIFO flush, restores default SIGTERM and re-sends it to the same PID. The seal
is written only after all submitted records have returned from write. Parent
streaming validation independently checks nonce/role/PID/sequential rows, seal
counts/bytes/exact EOF, emit ordinals and all actual raw expected bytes/offsets/
RETURNED facts, sample endpoints and raw EOF. SIGKILL, worker error, a blocked main
callback, a missing seal or partial raw/sidecar is always `trace_incomplete`/BLOCKED.
The200ms belongs to the existing monitor-stop budget.

The parent uses thin diagnostic FileTail/ObservationStream adapters with the
original whole-batch parsing, ordered rows, continuous-before-owned priority,
validation clock call point, first error and unchanged original raw bytes.
Additional spans record pump, continuous callback, phase poll, tail read and batch
parse return. Per-row seq/byte offsets preserve the original actual validation
clock/error. A gap rejected before that clock leaves it null. The original250ms
freshness/gap/postjoin and20ms event-age limits never change. Input integrity never
re-arms after failure. No keys, pointer coordinates, window content or unrelated
process identities are collected.

After parent/Mac independent review and strict compile, the single controlled CLI
is (operator paths must be supplied; evidence is a fresh absolute nonsymlink path
outside product source):

```sh
python3 -B diagnose_display_prep.py run --root PRODUCT42 --binary APP42 \
  --producer CAP42 --evidence NEW_DIAG --display-id 2
```

All original source and runtime pins are verified before evidence creation,
compilation or observers. Two original streams arm on their first two continuous
samples before the sole owned internal child is launched. Parent retains its exact
Popen/argv/source SHA/nonce/kernel runner credential and sends GO only after that
credential is observed. Only runnerPID is bound; appPID remains0. The owned policy
requires an authenticated APP launch opportunity but none is supplied, so it stays
`RUNNER_PREPARING_APP`, never native ACTIVE/owned-ready. There is no FullRun, APP,
capture, window query, environment recheck, navigation, native verifier or
APP_LAUNCH marker. APP and CAP pins are checked but their executables are not run.

The child reuses reviewed literals, exact `DISPLAY_SWIFT_PREFIX`, original
`owned_command` and pure receipt/phase/command validators. The only five stages
are probe2 → set60 2 → independent probe2 → finally restore2 113 → independent
probe2. Each uses timeout10s and GRACE2 (TERM2s then KILL2s, worst4s total), private
PID=PGID, launch/process/raw/phase receipts. Nine-field probes and eight-field
mutators must match the original2/113/144Hz/scale2/nonmirror snapshot, independent
60Hz core snapshot and final exact original nine-field snapshot.

Every helper receives an immutable prelaunch attempt-intent binding nonce, child
PID, ordinal, exact argv, helper SHA and shared uptime. Failed intent publication
starts no helper. Before any subsequent helper, every issued intent must have
paired leader-join/group-release proof; a FAIL helper can permit restoration only
with those actual joins. Missing whole pairs or unjoined groups block recovery;
144Hz readback and known-PID lists cannot create an exemption. Parent independently
checks the complete attempt/launch/process inventory even on a failed child.

The190s execution and single40s cleanup budgets include this child. Parent sends
exactly one SIGINT to its owned child and immediately freezes the original cleanup
deadline before journal IO. A nonblocking dedicated datagram channel sends that
nonce/deadline or UNKNOWN; child must actually receive and ACK it before starting
new recovery. Parent checks the exact ACK independently, never equating send with
receive. No late clock restarts40s or reuses190s for cancelled cleanup. SIGINT
masking closes the intent-publication/Popen signal checkpoint boundary; original
owned-command drain handles any helper that actually launched. Each helper launch
rechecks remaining budget after intent IO, accounting for worst14s helper lifetime
and shared monitor4s plus postjoin250ms; two final helpers require28s plus that
reserve. UNKNOWN/missing ACK/insufficient reserve means no new recovery helper.
Logging failure does not skip the exact child join. A clock-fault path preserves
only the original immediate relative bounded join, with no new absolute recovery
deadline. Both monitors stop concurrently in one shared4s window, with continued
pumping and final tail drain. Both real postjoin samples must begin between actual
child join and join+250ms. No later good sample can clear the original failure.

Only all five natural successful stages, original stream integrity, actual
child/helper/monitor joins, both postjoin barriers and fully verified raw/sidecar
seals yield `DIAGNOSTIC_COMPLETE`. Cancellation, input/gap, partial evidence or
any error yields `BLOCKED`, even if display API error0 or physical144Hz returns.
Every result has `scope=DISPLAY_PREPARATION_DIAGNOSTIC`, `stage_PASS_claim=false`
and native/full12/full48=`NOT_RUN`. No cancelled collection is PASS.

Kernel hangs, SIGKILL, missing intent/process journals or an unknown cleanup clock
can prevent proven joins/restoration. That path records joined/unknown facts and
`possible_unrestored_display`, stops new helpers/recovery and remains BLOCKED;
it never grants a next run or invents successful restoration. It cannot promise
hardware recovery or actual joins that were not observed. A live unjoined child
at the original deadline is an explicit runtime blocker requiring the Mac owner,
not permission to extend cleanup or signal unrelated processes.

Portable verification:

```sh
python3 -B -m unittest -v test_guard test_matrix test_diagnostics
```

The original160 and38 new synthetic tests pass on cloud Python3.12/Linux, including
capacity/seal/raw EOF, unknown write return, original stream parity/null gap clock,
no-APP policy, no late restore after intent, cancellation never complete,
original deadline/ACK/UNKNOWN, paired failed-helper joins, journal failure retaining
actual join and concurrent monitor stop. These are ordinary/static tests; the
cloud has no Swift compiler, AppKit or real display. Strict Swift6 and one real
controlled Mac diagnostic are **NOT_EXECUTED**. This source candidate must be
independently reviewed and frozen before that one diagnostic; the historical
full12 failure, native/full48 NOT_RUN and stage_PASS_claim=false remain unchanged.
