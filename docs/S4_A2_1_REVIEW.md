# S4-A2.1 — Mac-first review

Date: 2026-09-20. Scope and R/AT mapping: [plan](S4_A2_1_PLAN.md).
This is the complete UI/text-contour/floating-placement slice requested for review.
Windows remains deferred / not executed; this is not dual-platform V1 acceptance.
Global Units remains **BLOCKED UNTIL S4-A2.1 PASS and review**, and has not started.

## Implementation

- Text, Move, Rotate, Mirror, Flash Properties, Grid Settings and layer Rename use
  one exclusive centered modal with draft values, Apply/Cancel, background blocking,
  retained errors, scrolling and event-frame IME guards.
- Font material uses connected contour Regions with local retraced hole cut-ins,
  exact Line reduction and certified Arc refitting. Production vertical slabs are
  removed. Disconnected material components are separate objects within one text
  transaction/group; this avoids claiming unsupported multi-contour union metrics.
- Mouse placement freezes geometry and changes only its translation. Snapped anchor
  parity, one commit, group selection, Undo/Redo and cancellation are covered.
  Context changes cancel placement. Absolute/Relative modes remain available.
- The native IME run exposed a commit-frame race: preview scheduling could disable
  TextEdit before its candidate commit. Scheduling now also waits out `ime_event`.
  A regression test and native Chinese candidate re-run verify the correction.
- No dependency, global precision policy, core tolerance, resource limit or writer
  format was changed. See [geometry ADR](adr/0027-text-contour-region-geometry.md)
  and [interaction ADR](adr/0028-modal-interaction-and-floating-placement.md).

## Measured geometry comparison

Release, same Mac and system font files, face 0, 3 mm, 37 degrees, curve tolerance
0.00025 mm, zero offset. Thirty rows: 3 fonts × 10 fixed strings. Full raw metrics,
font hashes and outputs are in the evidence package. New implementation: 30/30
preview/create/export/reparse PASS. Old baseline: 29/30; Songti long Chinese input
hits its original resource limit. That failure is retained, not substituted.

| Font, `钢网测试口回` | Old objects / edges | New objects / edges / arcs | Old / new Gerber bytes |
|---|---:|---:|---:|
| Arial Unicode | 5709 / 22795 | 22 / 480 / 170 | 768680 / 16949 |
| STHeiti Light | 9679 / 38634 | 22 / 1011 / 446 | 1296193 / 36054 |
| Songti | ResourceLimit | 13 / 1699 / 781 | unavailable / 59169 |

Complexity and output size improve substantially; fitting is not universally
faster. For the same Heiti row preview is 361.1 → 577.0 ms and writer 380.1 →
535.1 ms; Songti's new preview/writer are 1341.8 / 2820.4 ms. These are individual
runs, not latency percentiles. The work stays off the GUI thread and placement
never repeats generation. Font bytes are excluded from every deliverable.

## Native evidence

`native/ime-fixed-launch.log` records the final production implementation on Apple
M1 Metal, Retina pixels-per-point 2. Computer Use operated the native app, with
screenshots inspected in the task conversation. Screenshots supplement the log;
no desktop/private file-picker screenshots are distributed.

- System Arial Unicode chosen in the modal; actual `zhong` preedit events followed
  by a one-scalar candidate commit produce `中`. Modal stays open; revision 0.
- Generation 12: at least five pointer positions, same generation/revision, then
  native window resize and Fit change camera scale 64.17 → 76.77. Preview still
  follows. This verifies native Fit-based zoom, not a physical pinch gesture.
- Text anchor snap ON, preview anchor (-2, -1.1) mm equals commit anchor. Exactly
  one create gives revision 1, undo 1, generated object selected. Undo gives revision
  2 / undo 0, Redo revision 3 / undo 1. Save As succeeds and marks content clean.
- Generation 17: O8 floating preview moves then Esc cancels. No create follows;
  revision remains 3 and the document remains clean. Reopening `ui-text-final.gbr`
  gives 4 objects (3 original plus the Chinese contour), with local holes visible.
- Grid modal NaN + Enter keeps the dialog and error visible; Esc cancels.

The system screenshot helper's Screen Recording permission request was rejected by
automatic approval. No permission was changed or bypassed. Existing app-window
Computer Use observations and application logs supply the native evidence instead.
Physical trackpad pinch/long soak and Windows are not claimed.

## Tests, provenance and packaging

Development full workspace tests passed; after the final context/IME changes,
app and modal regressions passed. The clean-commit final run uses:
`python3 scripts/run_s4a1_gates.py --stage S4-A2.1 --extra-service-test s4a2_text_preview --out <new-run>`.
It executes fmt/check/clippy/workspace tests, explicit service contracts and edit
workflows, dependency-tree audit, release build, source/package audits, and the
native Metal reference/production pixel test. Its `gates.json` is authoritative
for each command, exit status and final commit, including any unrun/failed gate.

The release matrix command is `RCAM_S4A21_MATRIX=<new-dir> cargo test --release
--locked -p editor-service --test s4a21_geometry_matrix -- --ignored --nocapture`.
Geometry certification tests include dense independent Bezier boundary checks,
collinear/near-circle/inflection cases and an analytic square-with-hole area 12,
perimeter 24 oracle. Existing offset, original manufacturing-error and writer
round-trip gates remain active. Rotation may rechoose cut-ins, so material-boundary
distance replaces invalid same-index vertex pairing; it retains the original total
manufacturing-error threshold. The legacy >700 renderer-load assertion is replaced
by <700 to verify the requested slab reduction; the renderer safety budget is unchanged.

Delivery contains the complete manifest-restricted Source ZIP and a Public Evidence
ZIP with raw logs, matrices, synthetic outputs, environment, clean status, HEAD,
tested source hashes and binary hashes. `EVIDENCE.sha256` covers every evidence file;
`SHA256SUMS.txt` covers both ZIPs. Fresh Python `zipfile.extractall()` must pass
source_manifest, test_package_source, and per-file tested/extracted hash parity.
The delivery's final acceptance JSON records those results; this review document
does not turn missing evidence into a pass.
