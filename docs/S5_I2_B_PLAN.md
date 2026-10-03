# S5-I2-B — Unified world point and tool adapters

2026-10-03. IMPLEMENTING / NOT ACCEPTED. Parent approved A final Mac-first bounded. Baseline ca4f7ed1243b26f28abb65d3a5fadfa4bdc8352b, source 66c62b4ae0c6f5c6603b144193ff825f93ad68b093f5b1c28afd58aaca7a082e (603 files). Isolated codex/s5-i2-b-tool-adapters; A/I1/main/PMIX and all historical/tmp directories protected. No push/upload/deletion. One existing Cargo target, offline locked Rust 1.89.0.

S5 within S0–S6. R04/R05/R07/R08/R09/R10/R11/R12/R13/R14/R16/R17/R18/R19/R21/R22; inherited AT-011–018/022/025–037/039–045/048–051/065–067/086–092. Directed BP-01..05/15..18 from S5_I2_PLAN, B-01..09 below. No AT identity/platform or numerical threshold changes. ADR 0056 records point workflow; A area algorithm, uncertainty and resource limits remain frozen.

## Allowed changes and contract

editor-app common point/session UI, existing tools/modal/canvas integration and fixture-only internal native evidence; editor-core Snap standard-hole provider and a read-only transform preview adapter using existing edit math; directed core/app/service integration tests, public fixtures, portable scripts and docs. editor-service existing query/edit APIs reused; necessary read-only geometry_block_definition_centers_cancellable adapter added for unplaced local definitions using the unchanged A engine; no mutable model or GUI dependency added. renderer receives display overlays only; no manufacturing inferred from display. No dependencies, project schema, arbitrary axis/scale/clipboard, C preferences/cursor/status/flicker implementation or PMIX work.

Common point contains unrounded f64 world mm, provenance Numeric/BoundingCenter/AreaCentroid/Feature/Grid/Raw and a full TaskVersion+ordered selection+epoch context. Numeric length parsing follows DisplayUnit; display rounding never feeds back. Centers read only the A background result when context/DTO identity valid; area unavailable never falls back to bounds. Selected-only layer-composite semantics remain the user-approved scope. Explicit point input is transient, no revision/history/dirty.

Pick is a child operation that saves prior point and resumes its originating dialog/tool. While picking, ordinary I1 selection/cycling, box selection, drag and Grip startup are suspended. Visible selectable geometry including locked references and selected objects supplies B. Hidden/unselectable classes excluded. Escape/right-click/back restores saved point; blur/PointerGone/window/tool/project/selection/revision/permissions/precision change cancels transient work before a same-frame commit. Text/IME focus wins. Pan/zoom preserves confirmed world point and resets screen Snap hysteresis. Numeric/pick action never silently changes selection.

Dedicated point/target contour policy overrides a clone of global Snap settings: manufacturing boundary + Nearest and supported features, acquire8/retain11 physical px, default priority+0.35 penalty/stable ID; Alt bypasses Object/Grid. User global preferences remain untouched. Standard C/R/O/P holes included as analytic circles with local transform (scale/rotation/mirror); Macro/Region boundaries and resolved Block children reuse manufacturing providers. B includes selection; transform T and drag/Grip T exclude edited objects. No tangent/perpendicular capability claim.

Move/Copy resolve delta T−B once; Rotate stores angle+B; H/V Mirror stores axis through B. Preview and commit consume the same resolved SelectionEdit, with shared core transform math. Copy has no speculative original-location insertion. Frozen request context is checked again in worker before ApplicationService; full cross-layer SelectionEdit commits one transaction/revision/Undo, preserving LayerId/exposure/order/full text groups. Existing delta Move and in-place Duplicate commands retained. Large overlay bounded/simplified explicitly; display sampling never drives manufacturing.

## Applicable tool adapters

| Tool | Required integration and retained meaning |
|---|---|
| Move / Copy / Rotate / H,V Mirror | Numeric/Pick/bounds/area B, numeric or contour-picked T for Move/Copy, world preview and one commit; delta Move retained |
| Mouse drag | Dedicated target contour Snap+self exclusion, existing physical threshold and cancel strategy |
| Block create | Shared definition origin input, existing same-layer contiguous/full-text/no nesting rules |
| Block place | Definition-local reference Numeric/Pick/local bounds/local material center; map that reference to world T, no Definition mutation |
| Block instance transform | Absolute translation/angle/reflection retained; arbitrary world pivot via normal selection transform |
| Text reference | Common Numeric/Pick/selected centers for explicit relative reference; layout anchor/alignment untouched |
| Measure | Common point input for two endpoints, centers allowed without mutation |
| Grip | Common target point input; fixed parameter pivot remains fixed, same preview/service action |
| Array | Explicit B→T pitch helper, signed dx/dy; cell(0,0) unchanged, same-layer/full-text/row-major retained |
| Board registration | Existing World points use common input; Board-space numeric identity unchanged |
| Alignment/Distribution | Existing anchor-object semantics unchanged; no artificial base parameter |

No empty/inactive tool promises. Each adapter must be connected and tested before B candidate. If a tool requires a shared C interface, add only necessary point workflow interface and record boundary; C acceptance stays pending.

## Directed verification (not yet executed)

B-01: >=5 objects >=3 layers, bounds and unequal-area center independently checked; all four base sources Move/Copy/Rotate/H,V Mirror; exact preview params=commit, independent coordinates and arc direction.
B-02: selected/occluded/locked reference contours, standard holes under transform, nearest/end/mid/center, DPI1/2 and multiple zooms, acquire/retain, Alt/Grid; ordered selection/primary identical and no I1 request.
B-03: Pick/back/Esc/blur/PointerGone/IME/focus/tool/navigation/project/selection/precision/workspace/rule/revision/new-request matrix. Same-frame cancellation wins. No revision/dirty/history changes.
B-04: multi-layer Copy stable originals/new IDs/text groups/order; one Undo/Redo restores exact stored geometry; one locked layer/invalid point/stale request/resource failure zero partial mutation.
B-05: all applicable adapter rows, local Block reference versus selected-world centers, generated text holes, Array original-cell invariant, Board semantics, Grip fixed pivot.
B-06: headless service same query→resolved transform→Undo/Redo→export/reopen, independent geometry/exposure/layer oracle, service dependency boundary.
B-07: P100K nearby contour30 observations CPU p95<=20ms; visible feedback p95<=100ms is a mandatory B gate in three distinct native release rounds, using fixed machine/sample/display refresh/DPI2/zoom40. Ten predeclared warmups precede all thirty measured events. T0 precedes actual egui dispatch; T1 is the actual post-paint surface screenshot callback. Retain every input/update/paint/completion/callback timestamp and marker crop; independent fixture/coordinate/pixel/chronology and nearest-rank p95 checks are required. The controlled RawInput-to-surface-readback scope does not claim OS input or scanout latency. Navigation original thresholds retained. No CPU-as-end-to-end claim and no deferral of this gate to C.
B-08: existing A gates plus B directional regression, workspace fmt/check/clippy/tests/release, automation_contract/headless_workflow/tree, I1/A2/M2B/Metal regressions. Preserve failures/run IDs. No second Cargo writer.
B-09: frozen release native interaction evidence for B point/transform/adapters/cancel; full I2 three-round and C cursor/status/flicker acceptance still mandatory later. Physical OS input versus internal RawInput/helper observations labeled accurately.

Flow: plan→implement runnable loop→directed/full gates+native→frozen Source/Evidence/portable/tamper/fresh→parent independent review and defect fixes→authorized stage commit no push→clean same-commit final package/fresh→final audit. B is not complete I2. Windows deferred, K1 native deferred, PMIX paused, no full V1/CORE10/P100K claim.
