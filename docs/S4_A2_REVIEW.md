# S4-A2 foundation review — full stage NOT COMPLETE

This delivery advances only Gate 0 and the service/precision foundation.
It is not the final Text GUI delivery. Scope and R/AT mapping: S4_A2_PLAN.md.
Decision and numeric budget: ADR 0025. Windows deferred / not executed.

Implemented:
- Direct font parser dependency/status documentation repaired.
- Backward-compatible precision field with bounded manufacturing tolerance.
- Owned read-only preview DTO and JSON operation, sharing full generated
  insertion preflight with text.create; preview never allocates object IDs.
- Tests for preview state preservation, accepted precision parity, one-Undo
  Apply, Undo/Redo preservation, stale/closed document, font/hash/face errors,
  layer locks, strict JSON and precision range/resource rejection.

Known failure retained: '口8' / Arial Unicode / 3 mm / 37 degrees at
0.0000625 mm returns VALIDATION_FAILED (overlapping adjacent edges). Regression
asserts atomic rejection in both paths; this does not count as precision GUI
acceptance. Default 0.00025 and 0.000125 parity remain tested. No failing sample,
resource limit, acceptance identity, platform requirement or threshold removed.

Not implemented / not executed: Text GUI; asynchronous preview publication and
stale-result fencing; font picker/recent fonts; outline offset; Mouse/Absolute/
Relative placement; group GUI interactions; native Chinese IME and all 29 native
steps; the multi-font/offset/precision performance matrix. These remain required
before S4-A2 PASS. Native renderer regression is not Text GUI/IME evidence.

Verification is bound to the clean commit in the foundation public evidence
archive: environment.json, git-head.txt, clean-status-before/after.txt,
tested-source-hashes.txt, gates.json and individual command logs. Test commands
include all workspace gates, S4-A1 text workflow, s4a2_text_preview,
automation_contract, headless_workflow, service normal dependency tree, release
editor-app build, source manifest/package tests and existing native Metal parity.
The gate result file supplies actual exits, not this planned command list.

Source/evidence packages are explicitly named S4A2_FOUNDATION to prevent
confusion with the still-open final S4-A2 delivery. Fresh extraction must verify
manifest/test results and equality with tested payload hashes. No font bytes,
private Gerber samples or private workspace paths belong in public evidence.
