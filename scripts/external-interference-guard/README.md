# External guarded PMIX full12 coordinator, v3.4-bound identity freeze

This bound candidate requires final independent review before publication and
Mac execution. It binds the reviewed clean product and the parent's actual fresh
Mac build/background-qualification receipts:

- commit: `1a7344a65f778544811762c0c6be3d157f5acf2b`
- manifest (671 entries): `8bd352392e3c50da44f5cd7c2108ef9c482ba27bf82dd91a1a37f5468d7510c3`
- internal app (23197632 bytes, arm64): `c795aaed3c528e71a8713661a002e7d671500aa4e54e0ee622542a30cec31db1`
- producer (280784 bytes, arm64): `90aedb747e65ab59ec030c15d223a95d9d17e0d15b648d22ff88d297abd157d6`
- native runner source: `e1369e039aa958921e54749fcf8442ae8cf3d407726f3cdeff86fc93a72838db`

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
`--allow-display-mode-change`, and `--display-id 2`, with schema3 request evidence.
The authorized original snapshot is target 2/mode 113/144 Hz, logical1920x1080,
physical3840x2160 and scale 2, never mirrored. There is no outer display lease.
Each runner enters60 Hz and restores its own original 144 Hz target in `finally`.
Five owned display operations are mandatory: probe 2, set60 2, independent probe 2
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

The Swift observer and pure input/foreground policy are unchanged from v2.5.
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

This bound freeze passed 132 Linux synthetic tests, including early background
qualification, strict JSON/identity rejection and preservation of the sealed seed.
This candidate also passed 26 independently authored legacy safety checks,
12 independent background-seed test groups, and safe replays of the prior
offline/deadline/budget attacks.
Those checks do not establish a Mac runtime or formal acceptance result.

The parent reports actual strict Swift6/display2 probe, background31 and sealed
background-only qualification for this same clean product identity. Those
preconditions preserve the original BLOCKED background capture result. Actual
guarded60Hz/full12/native capture and original48 attacks remain NOT_RUN.
