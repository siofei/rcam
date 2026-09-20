# S4 — Global Units & Manufacturing Precision Foundation

Mac-first, 2026-09-20. User requested implementation of the supplied v2 task,
plus retention of the last text input state. Scope: R03/R08/R10–R16/R19/R21/R22;
AT-007/031/033/034/040/041/043–055/058/062–065/086–095/097 (existing identities).
Allowed modules: editor-text fitting lattice policy, editor-core units policy, gerber-io export normalization,
editor-service policy/query/export, editor-app input/display/modal/text state,
related tests, evidence/package scripts and documentation. No new dependency.

Gate 0: S4-A2.1 PASS (Mac-first), evidenced by
`evidence/s4-a2-1-c3c781d-final/gates.json` and native S4_A2_1_REVIEW;
S4-A2.2 PASS (Mac-first), evidenced by
`evidence/s4-a2-2-final-20260920/gates.json` (90bb706) and S4_A2_2_REVIEW.
These are bounded stage passes, not full V1 or Windows acceptance.

Implement four display units, centralized suffix parser/Auto formatter, session
text retention, exclusive settings modal, separate document export policy dirty
state and revision fencing, and quantization of a private export snapshot before
existing semantic/write/reparse/compare/no-clobber publication. Keep imported
and edited working coordinates f64 mm; never round after each edit.

Defaults: 0.0001 mm manufacturing resolution, 0.1 mm grid, Auto display digits.
Text curve tolerance derives from 2.5 × resolution within the existing supported
0.00001–0.00025 mm range. API explicit text tolerance remains compatible.
Text content, font, layout, position mode and values survive create/cancel/reopen
within the current app session; previews and pending submissions never survive.
App restart persistence is outside this slice (no native project format yet).

Exit requires full required checks, four-unit GUI and native evidence, Arc/Region
and text export safety, geometry/performance regression and verified source and
raw public evidence archives. Missing/failed gates remain explicit. Stop before S4-B1.
