# External guarded PMIX full12 coordinator, v3.6.2-bound candidate

This source-only candidate binds the independently reviewed product phase/helper
contract and independently measured fresh Mac build identities below. Final
independent bound review is still required before publication and execution.
No previous product/build hashes are reused as runtime pins.
The original set60 API timeout remains an unresolved diagnostic hypothesis;
this guard candidate does not claim that the display API has been repaired.

The five frozen identities are:

- Product commit: d6e5fe8a542cca51a65a66100ffeed5a6abae1c8
- Source manifest: a1692b3da28df336b81b030c19c9f84c568e6b274c3850e2987aea05e180f88f
- Internal app: 3bea984b659cb15ae3b7e2b9be94ad68af1851b3b7504675c3901958431fca15
- Capture producer: c5f207d3ac37dccb69f8dd1d15dded18e95fdf5bf6c647d3fc3c3dbcb05ebf22
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
external guard interfaces retain their reviewed contract. The product's new
PMIX input records and native verifier remain internal product predicates.
This successor changes only product identity pins and their test/documentation.
All earlier product native rows retain their original identities and cannot
qualify this rebuilt candidate. No new native12, aggregate, actual48 attacks
or stage acceptance is granted by this bound source package.
