# S4-A2 Text GUI — Mac-first implementation plan

Task source: RCam_MAC_FIRST_S4A2_TEXT_GUI_NEXT_TASK_v2.md (2026-09-20).
Baseline: S4-A1 b0c027c9710373c1f02aba76eee60ecb8c326e2d.
The full task remains open. This commit is the service foundation closure only,
not the S4-A2 exit gate. Windows deferred / not executed.

Requirements: R05/R11/R12/R14/R16/R20/R21/R22.
Local coverage: AT-047–052, AT-054, AT-086, AT-088–092, AT-095, AT-097.
Future GUI/IME coverage: AT-043/046 and the native portions of AT-047–052.
All 96 case identities, required_platforms and V1 thresholds remain unchanged.

Allowed modules for this closure: editor-text precision validation;
editor-core read-only generated-insertion preflight; editor-service preview
DTO/dispatch; corresponding tests; dependency/status/decision documentation;
existing gate runner and source manifest. No app/renderer behavior changes.

Completed implementation scope:
- Gate 0 README and direct ttf-parser dependency registration.
- Backward-compatible curve_tolerance_mm JSON default (0.00025 mm).
- Read-only text.preview, sharing font identity, generation, semantic validation
  and history/document resource preflight with text.create.
- Snapshot identity and frozen parameters in preview results; no ID allocation,
  revision, dirty, history or filesystem writes.
- Precision bounds, parity, rejection atomicity and regression tests.

Remaining task closures, in dependency order:
1. Resolve the retained thin-slab precision rejection (see review), and implement
   material outline offset with reviewed dependency/topology/error budget.
2. Font identity/picker; draft/placement state; bounded debounced worker,
   stale-result protection and display-only overlay; Apply and group selection.
3. Native macOS IME/focus/clipboard, placement/edit/Undo/Save/Reopen and the full
   multi-font/precision/offset performance/evidence matrix.
4. Clean final S4-A2 source/evidence archives and independent stage review.

Do not add shaping, multiline, persistent editable text, automatic bridging,
font redistribution, post-V1 formats, P100K, Windows implementation or signing.

## Completion continuation

User requested continuation using the Foundation review and S4-A2 Completion
brief. Full S4-A2 is now the target, stopping before S4-B/S5. Additional allowed
modules: editor-app text draft/UI/worker/display overlay and tests; editor-text
material offset/canonicalization; service font inspection and timing; locked
geometry dependency and notices; native evidence and packaging. Requirements
R05/R09–R12/R14/R16/R18/R20–R22; AT-043/046–052/054/086/088–092/095/097 and
Grid/Measure/S3 renderer regression. Existing acceptance identities/platforms
and total 0.001 mm threshold are unchanged.

## Canvas UX mandatory hotfix (in progress)

The user-added 2026-09-20 addendum is part of S4-A2 Completion, not a new
stage. Allowed modules additionally include camera, display, render index,
Grid overlay and their app tests. R07/R08/R13/R14/R18 and the existing navigation,
Grid/Snap and renderer acceptance cases remain applicable; no acceptance
threshold or platform is relaxed.

Required closure: 180 ms view-only visibility fade; continuous physical-pixel
1/2/5 density LOD with separate style opacity; unchanged base snapping;
f64 viewport culling before camera-relative f32 conversion; safe zoom clamps;
last-good display protection; native Retina grid and extreme-zoom evidence.
These items remain IN PROGRESS until code, automated gates and native evidence
are recorded. Text GUI/IME/preview/font/placement continue in the same closure.

## Next mandatory foundation

**Global Units & Manufacturing Precision Foundation — BLOCKING BEFORE NEXT
MAJOR FEATURE STAGE.** After S4-A2, before S4-B/S5 or any other major feature,
freeze and implement global mm/inch/mil/µm display units and the requested
0.1 µm default manufacturing precision with explicit conversion, rounding,
error budgets and acceptance cases. This does not silently change current
text tolerances or writer quantization during S4-A2.

### User refinement: system fonts first

The latest user instruction changes the primary font interaction to a searchable
system-font list. macOS Core Text enumeration runs in the existing worker;
selecting a named TTC face resolves its actual index and verifies bytes/hash
through ApplicationService. The explicit font-file picker remains available.
This is part of the current S4-A2 scope, with no change to installed-font
redistribution policy or the post-S4-A2 global-units blocker.
