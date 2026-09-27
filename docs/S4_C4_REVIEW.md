# S4-C4 Alignment / Distribution review

Status: PASS (Mac-first bounded). Final clean-source gates, native binary binding and deterministic package identity are recorded in the versioned exports/S4C4_*/REVIEW.md. Windows deferred / not executed; no full V1, CORE10 10/10 or P100K claim.

## Behavior and boundary

Six align commands use analytic manufacturing world AABBs and the existing last-selected primary object. Top/Bottom use world max_y/min_y; HCenter/VCenter align X/Y centers. Horizontal/vertical distribution uses equal edge gaps, deterministic min/max/layer-index sorting, fixed endpoints and permitted negative gaps. Menu and canvas context menu dispatch the same registered commands.

Both service APIs require explicit IDs and expected_revision. Same-layer visibility/selectability/lock/category/Solo restrictions are checked at submission. A changed command is one atomic transaction and one revision; exact no-op changes neither history nor dirty state. IDs, exposure order, BlockDefinition content and instance orientation remain unchanged. Complete GeneratedText groups move as logical units; partial groups reject before mutation. Save/Open, recovery, Gerber writer and manufacturing precision policies are reused.

The shared Block bounds path was corrected to compute analytic bounds of transformed child geometry. Rotating a local AABB overestimated sparse/rotated geometry and is unsuitable as an alignment truth source. No renderer mesh or pixels enter this calculation; the cache is local to the query and keyed by definition revision and orientation.

The exact-f64 policy is retained: a representable residual is a real movement even when much smaller than the export grid. The stroke-text regression records a +8.9e-16 mm residual correction, then verifies the exact-zero repeat adds no transaction. No new tolerance or quantization was introduced (ADR 0040).

## Evidence matrix

| Local cases | Evidence | Status |
|---|---|---|
| C4-01/02 | Core/service tests: all six modes, two distribution axes, unequal sizes, negative gap, ties and order independence | PASS; final raw logs in delivery evidence |
| C4-03/04 | Atomic permission/revision/identity failures; exact Undo/Redo, no-op and sub-grid movement | PASS; final raw logs in delivery evidence |
| C4-05/06 | Mixed manufacturing geometry, complete text groups, ordered Dark/Clear, 100 BlockInstances and sparse rotated bounds | PASS; final raw logs in delivery evidence |
| C4-07 | Project/recovery/Gerber/source-hash service tests; native-produced files checked separately | PASS; final raw logs in delivery evidence |
| C4-08/09 | App command/anchor/menu tests; numeric diagnostics and exported ZIP privacy | PASS; final raw logs in delivery evidence |
| C4-10 | Release 1K bounds/delta/transaction timings; 10K accepted and 10001 rejected; native 1K context command | PASS; final raw logs in delivery evidence |
| C4-11 | Native CUA interaction, fixture-limited read-only observations and Metal baseline | PASS; see delivery evidence |
| C4-12 | Clean source identity, complete gates, deterministic archives and fresh-extract verification | PASS; see delivery evidence |

Related requirements/cases and allowed modules are recorded in S4_C4_PLAN.md. The 96 schema_version=2 acceptance cases and their required_platforms remain unchanged. These local results do not promote the global platform acceptance state.

## Source and delivery identity

The main workspace began at f1293f2 with uncommitted inherited C3 work. It was preserved in a baseline snapshot before C4 edits. A separate local checkout records inherited source at 9b8cb0d4d3dac82eda19f3045a80193371c9c84a and holds the clean tested C4 commit identified by the delivery review. Main-branch history is not rewritten and no source is pushed remotely.

Final source/evidence ZIP paths, tested-source hashes, binary identity and SHA256SUMS verification are recorded in the versioned exports review. The full Source ZIP includes PACKAGE_INFO.json, PACKAGE_MANIFEST.sha256 and MANIFEST.sha256; the Evidence ZIP includes raw logs and EVIDENCE.sha256.

## Remaining scope

Windows, full V1, CORE10 10/10 and P100K remain deferred/unproven. Preview/toolbar additions, Array/Panelization, internal BlockDefinition editing, PnP/RefDes and multi-object grips are outside C4. Existing C2 fixture-locked input-driver production hardening remains deferred; C4 adds only opt-in, fixture-limited read-only observations, with no new active input control. Stop after C4 and submit for review.

## Executed acceptance

The candidate workspace run passed 696 tests with 36 explicitly ignored tests; focused service checks passed 9, and app command checks passed 5. Two ignored release performance checks were executed separately. Native Metal C3 invariance and BlockInstance parity passed on Apple M1/macOS 26.5.1. A sandbox-only no-adapter failure is retained alongside the successful native rerun. The final clean commit repeats all required gates; its exact counts, commands, exit codes and binary hash are in the immutable delivery evidence.

Native CUA on public synthetic fixtures covered all six alignment modes, both axes, negative gaps (-3 mm horizontal and -3.25 mm vertical), last-selected marker, 3 BlockInstances plus 1 Flash, a 1000-object context command (995 moved), exact Align and Distribution Undo/Redo, and repeated distribution with unchanged revision/history. Sixteen independently checked native before/after cases passed on the initial frozen binary. Its identity is separate from the final clean-binary confirmation run; do not attribute the initial runtime log to a later binary.

Native Save/Open and Export/Reimport retained 3 objects, their Dark/Clear/Dark order, area 13 mm² and perimeter 26 mm. A separate read-only service verification of the actual native-produced .rcam/.gbr files passed. Diagnostics ZIP inventory verifies all six numeric modes, both axes, moved counts, revisions and no-op; no private geometry or full object lists are included. Screenshots labeled `native-blocks-distributed.ppm` show the earlier Left Align; the actual distribution screenshot is `native-blocks-distributed-horizontal.ppm`.

Candidate release measurements on synthetic data: core 1K bounds 260 µs, deltas 100 µs, transaction 250 µs (610 µs total); service 1K 3408 µs and 10K 34840 µs. These are CPU command timings, not GUI frame/input latency or P100K claims. Final-run measurements are retained verbatim in the delivery. 10,000 objects are accepted; 10,001 fails atomically at the unchanged resource limit.

Final review corrected signed-zero sorting: finite numeric comparison treats -0.0/+0.0 as equal before the layer-order tie-break. The added regression covers minimum and maximum edges on both axes, fixed endpoints and reversed selection order.

Closeout identity correction: the clean 90cdaa1 run found two inherited capability tests still expecting the C3 stage string. Only those test expectations were updated to C4; their functional assertions and thresholds are unchanged. The 12-case native confirmation uses clean product commit 90cdaa1. The subsequent final package commit changes tests/documentation only; a per-file production-source audit binds the native binary to that final payload, while the final package commit repeats all automated gates. Both failed and successful run logs are retained.
