# S5-M2-B Batch Drag — executable task book

Status: PLANNED / NOT YET ACCEPTED. Local-only deliverables. Started 2026-10-02 UTC.
User authorized implementation after this plan without a plan approval pause.

## 1. Identity and stage boundary

Stage S5, bounded M2-B. Clean parent commit:
`03576c5264dafe54712b84663237c18890af8f35` (A2 independently PASS).
Worktree: `/Volumes/外接硬盘/rcam/.worktrees/s5m2-b-batch-drag`.
Branch: `codex/s5m2-b-batch-drag`. Initial tracked/untracked status empty.
A2's worktree, packages and main-directory historical work are protected.
The managed-worktree tool cannot serve this task type; Git worktree add was
explicitly approved and used. No reset, cleanup, push or Library upload.

Requirements: R05/R09/R10/R11/R16/R17/R18/R19/R21/R22.
Local acceptance mapping: AT-022/030/032/039/040/041/043/044/065/067/075/083/
086/087/090/091/092. Existing 96 cases, required_platforms and severity unchanged.
AT-075's complete PMIX / dual-platform signature remains a later gate: this
stage supplies fixed P100K circles drag measurements plus bounded mixed
correctness, and cannot relabel them as PMIX or full P100K acceptance.

Sources read: DESIGN_V1 (especially 8.4, 9.5, 10, 14), corresponding ACCEPTANCE_V1,
AUTOMATION_API, RCam_S5M2_TASK sections 16/23, RCam_S5M2_A2_TASK sections 39–43,
S5_M2_PLAN, current AGENTS and A2 independent final audit. No .agents/skills
folder exists in this checkout. Ponytail minimal-change approach applies.

## 2. Scope

Make 100 / 500 / 1000 / 5000 selected objects interactively drag with a transient
common f64-mm delta, one service command on release, one transaction, one Undo,
and one Redo. Add evidence for cost, correct restoration and interruptions.
The initial selection, IDs, order and source transforms stay immutable during
preview. Keep the normal hit-confirmation step; do not bypass exact hit testing.

Do not start PMIX, PPOL, PSTRESS, Recovery, new editing features, associative
arrays, PnP changes, keymap work, Windows closeout or CircuitCAM compatibility.
K1 remaining native work is explicitly deferred. No new dependency or service
API, no schema/writer/manufacturing precision change is planned.
A2 cancellation/CAS/version-fencing code is frozen. Any real B0/B1 requiring an
A2 change must be isolated, explained and independently reviewed.

Allowed modules: editor-app drag/main/gpu/editor.wgsl/render_index and directly
related selection/refresh code only if measured; app/core/service tests; bounded
internal-evidence native driver; scripts, synthetic protocol, docs, MANIFEST.
Core/service product changes require a concrete measured bottleneck or defect
and a recorded rationale, not speculative redesign.

## 3. Actual baseline and bottlenecks

`Drag::arm` snapshots document/revision/layer/ordered IDs and camera/rect; a
Gesture confirms a worker ProbeDrag hit before preview. `update_snapped` changes
only delta. `Action::DragMove` verifies current editable selection then uses one
`ApplicationService::objects_move`, followed by display refresh. Existing tests
cover small multi-object undo, cancellation, locked selections and stale drag.

Current performance problems visible in source:
1. gpu::prepare_measured rebuilds RenderIndex over ALL scene objects for each
   nonzero delta, despite manufacturing geometry remaining unchanged.
2. Callback::prepare allocates a new index storage buffer/bind group and uploads
   the complete rebuilt index every preview frame.
3. main clones the entire selected-ID snap exclusion HashSet every frame.
4. Refresh/selection geometry lookups and end-to-end commit costs need measurement;
   no unmeasured service rewrite is authorized by this plan.

## 4. Minimal intended implementation

Reuse immutable scene geometry and its immutable bins throughout a drag.
For each material sample, query the static bin at p for unselected objects and
at p-delta for selected objects, merge candidates in original exposure order,
and evaluate selected geometry at p-delta. Do not append all selected objects
after unselected objects; that would change Dark/Clear and layer composition.
Selection halos query p-delta too. Preview outside original world bounds must
still work; empty static/shifted query branches must not early-return incorrectly.
CPU viewport diagnostics use the corresponding two conservative queries and
selected/unselected filtering, while retaining original scene index identity.
Only the common uniform delta changes on GPU frames. No whole document/index
rebuild, new geometry upload, or new per-frame GPU buffer is permitted.
Borrow the existing exclusion set instead of cloning it. Preserve Snap semantics.

DragStart keeps stable IDs/version plus immutable view/scene geometry; no O(N)
geometry copying at mouse-move. DragUpdate has no service mutation. Release
commits the already frozen IDs/delta exactly once. Cancel discards all preview.
Zero delta or below threshold produces no history entry. Commit failure leaves
all geometry/history unchanged and reports the existing structured error.

## 5. Frozen measurement protocol (before implementation)

Machine: current macOS arm64 Apple M1 / Metal, 16 GiB; record live OS/build,
display, window/canvas physical dimensions, power context and binary hash per run.
Use pinned Rust 1.89.0/Cargo.lock, release, a single Cargo writer, offline cache.
Synthetic fixture: existing `fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr`;
SHA256 `8111ecada7a66defe30f614cd3851a328d861bbab4c595cb12ee3131fa465a31`.
100,000 manufacturing objects remain loaded; select exactly 100/500/1000/5000
via the normal selection command. Keep unchanged background objects observable.
Freeze selected IDs/region, camera, delta trajectory and protocol bytes with hash.
No substitutions by an empty document or smaller total-object fixture.

Native canvas target 1600×900 physical pixels, 60 Hz reference sampling. Record
actual refresh rate (144 Hz display is supplementary, no 144 Hz promise).
Each size: 10 s warm-up, 10 s deterministic drag, release/Undo/Redo, three
independent runs. Keep all frames, not only fastest samples. Frame intervals
are application input-frame intervals, not scanout or GPU execution time.
Input-to-visible upper bound uses the production callback + completed GPU fence
(or surface readback), with timing boundary disclosed. Synthetic egui input is
not claimed to be physical mouse latency. Readback screenshots supplement exact
manufacturing assertions; they do not replace them.

Baseline: run existing release preview benchmark before code change; save raw
output under a unique intake run. Then rerun same benchmark after change.
Baseline implementation failures are evidence, not a reason to weaken thresholds.

## 6. Pass/fail budgets frozen now

All four sizes: application-frame p95 <=50 ms, p99 <=100 ms, no >200 ms stall
in continuous confirmed preview; input→GPU-complete p95 <=100 ms. These extend
the existing 1000-object p95 and existing navigation stall budget to this slice.
100/500/1000: release→complete model/display <=300 ms; Undo/Redo each <=300 ms.
5000: release/Undo/Redo each <=1500 ms (linear 5× extension of the 1000 budget,
fixed before measurement; not a substitute for the original 1000 threshold).
Record worker-only service+refresh and end-to-end completion separately.
All sizes: RSS peak <=1 GiB, explicit custom GPU buffers <=512 MiB; read-only
Metal allocation observation also reported, never added blindly to unified RSS.
During steady preview: zero manufacturing commits, zero geometry/index rebuilds,
zero geometry/index/selection reuploads, zero GPU buffer allocations; uniform
uploads allowed. CPU allocations counted explicitly with scope; no invented zero
whole-process allocation requirement (egui and evidence recorder allocate).
Final geometric restoration is exact according to semantic snapshots. Revision
advances once per successful Move/Undo/Redo; history counts match one transaction.

A failure remains FAIL until repaired and all affected measurements rerun. No
post-hoc sample/camera/threshold/required-case change to turn it into PASS.

## 7. Correctness and interruption matrix

B01: each selection size, 600+ updates, preview leaves revision/dirty/history,
geometry, selected IDs and original scene/index identities unchanged.
B02: release one service move, all selected manufacturing coordinates delta,
unselected objects unchanged; one revision and Undo; exact Undo/Redo restoration.
B03: zero motion, threshold motion, out-and-back zero delta: no edit.
B04: Esc during confirmed preview: no edit; release after cancellation no edit.
B05: focus loss / PointerGone / lost capture, including same-frame release:
no edit. Re-enter and start another drag normally.
B06: project close/new/open switch during preview; old gesture cannot commit to
new document. Existing A2 full receipt fence remains unchanged.
B07: selected locked layer/category, hidden or nonselectable target, mixed layer
selection: existing safety policy refuses complete batch, never subset success.
B08: mixed Flash/Line/Arc/Region/BlockInstance and complete generated text group;
include ordered Dark/Clear and hole coverage assertions before/preview/after.
B09: revision or selection changes after arm; commit rejects unchanged batch;
invalid numeric delta or history-budget rejection leaves complete state intact.
B10: shifted objects outside original indexed world and overlapping stationary
objects; compare production shader to independently ordered brute-force renderer.
B11: repeated drag/Undo/Redo and cancel/restart; no stale preview or accumulation.
B12: existing grid/object Snap, Alt exclusion, text focus and modal guard preserve
behavior. No UI hotkey redesign. Shortcuts use existing Command dispatch.

Automated tests run the real Model/ApplicationService/geometry path. Native
release run confirms actual input→gesture→worker→callback wiring, cancellation,
completion and timings. Tests that only set Drag fields are insufficient native
evidence. Sample snapshots must include full data needed to independently check
geometry and history, not just an implementation-reported success boolean.

## 8. GPU correctness methodology

Production WGSL changes require native Metal comparisons at translated and
untranslated positions, selected subsets, zero delta, out-of-world delta,
interleaved Dark/Clear, multiple independent layers, local holes and selection
halos. Reference walks scene order without bins; known analytic points supplement
image comparison. Source and reference must not share the new candidate-merge code.
Retain readback raw bytes / mismatch coordinates. No relaxed tolerance to hide
missing geometry. Existing manufacturing tests are still mandatory.

## 9. Evidence and gates

Each run has schema_version=2, unique run ID, start/end UTC, exact command,
exit code, raw log hash, base/full source identity, binary hash, fixture/protocol
hash and machine data. Preserve failed/pilot runs. Native frames link input,
preview delta, selection/model state, worker request/reply and completed paint.
External runner records RSS/CPU samples and process exit; instrumentation records
GPU bytes/allocation counts and preparation CPU duration. CPU allocator counts
must state whether measured on UI thread, process or custom renderer only.

Required gates:
- cargo fmt --all -- --check
- cargo check --workspace --all-targets --locked
- cargo clippy --workspace --all-targets --locked -- -D warnings
- cargo clippy --locked -p editor-app --features internal-evidence --all-targets -- -D warnings
- cargo test --workspace --locked
- cargo test --locked -p editor-service --test automation_contract
- cargo test --locked -p editor-service --test headless_workflow
- cargo tree --locked -p editor-service -e normal (audit no window/GPU)
- targeted release batch-drag measurements and native Metal rendering checks
- A2 verifier/package unit regressions; source-manifest check
- cargo build --release --locked -p editor-app
- cargo build --release --locked -p editor-app --features internal-evidence
- native four-size three-round matrix + interruption matrix
- portable B evidence verifier tests (missing/altered semantic evidence rejects)

Historical A2 final evidence is a frozen prerequisite, not a rerun claim. Any B
integration regression touching queue wiring also runs relevant A2 task tests.
Windows/native IME/full V1/CORE10/PMIX remain NOT EXECUTED/deferred.

## 10. Execution order / freeze

1. Complete baseline inventory and immutable evidence intake; write this task book.
2. Record baseline measurements; add focused failing invariants for index reuse
   and selected/static exposure merge.
3. Implement minimal app/render changes; exercise correctness first.
4. Measure real commit/Undo/Redo; optimize only demonstrated bottlenecks, recording
   exact scope and preserving service transaction semantics.
5. Complete automated gates and native repetitions, fix failures and rerun affected
   measurements; retain raw failures.
6. Freeze reviewable dirty candidate Source/Evidence paths to parent. Parent runs
   independent review; no stage commit until reviewed. Resolve findings.
7. After source review PASS, stage only intended files and commit (no push).
8. Build in a clean independent target from exact clean commit. Rerun required
   gates/native; package Source and Evidence with same full commit, manifests,
   binary/fixture/protocol identities and local REVIEW/SHA256SUMS.
9. Fresh extract and full portable verifier; parent final package audit.
Only then PASS (Mac-first bounded). Do not automatically start M2-C here.

## 11. Current result

Implementation, baseline measurements, native matrix and gates: NOT EXECUTED.
No product change at plan creation. Known risks: shader order correctness,
shifted index precision near bin boundaries, large selection refresh cost,
GPU timing perturbation by evidence recorder, OS I/O and machine contention.
Actual outcomes will be recorded in S5_M2_B_REVIEW and versioned exports.
