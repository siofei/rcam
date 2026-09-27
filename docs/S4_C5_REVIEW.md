# S4-C5 Array / Panelization v1 review

Status: PASS (Mac-first bounded), 2026-09-28. This closes non-associative Rectangular Array only. Windows remains deferred / not executed; this is not full V1, CORE10 10/10 or P100K acceptance. Stop before associative arrays, PnP/RefDes, Drill or Legacy Wizard.

## Implementation and scope

`objects.array_rectangular` materializes ordinary objects or lightweight BlockInstance references through ApplicationService and one existing Insert transaction. Rows/columns, checked cell products, finite signed f64 mm pitch, same editable layer, contiguous exposure span and complete GeneratedText operations are enforced before mutation. Cell (0,0) remains unchanged; new cells follow the source span in row-major order, preserving per-cell exposure order. Each copied text group receives a distinct operation identity. Apertures and BlockDefinitions remain shared, and instance rotation/mirror remain unchanged.

Resource preflight covers 10,000 cells, 10,000 new objects, 1,000,000 total document objects including other layers, 2,000,000 Region edges, checked ID reservation and full history-byte estimates. Rejection preserves document/history/allocator; 1x1 preserves revision, dirty state and redo. Tests exercise exact new-object/document/history boundaries and the exact Region-edge preflight boundary plus one-edge rejection.

Menu and context commands use the same modal and worker/service path. Pitch uses the existing display-unit converter without Snap or quantization. Preview reuses shared source paths, checks source complexity before Block expansion, simplifies under display budgets, and rejects late worker results after cancellation. Apply preserves original selection. Project/recovery schema and expanded Gerber writer remain unchanged; no `%SR` or Array entity is introduced. Diagnostics contain counts, source category and pitch signs only.

Requirements and allowed modules remain those in S4_C5_PLAN; local cases C5-01 through C5-13 are specified in S4_C5_ACCEPTANCE_ADDENDUM. Frozen ACCEPTANCE_V1.md and acceptance_cases.json, dependency lockfile and product dependencies are unchanged.

## Acceptance evidence

The implementation checkpoint is `82329c872f011b8ad19f2668eca1ae329317dc7b`, preserved on `codex/s4c5-array-panelization` after baseline commit `1b0b4749175946cd2fb5227f8b24a999708ed8b1`. The baseline contains the supplied C3/C4 work; those changes were preserved, not attributed to C5. Main-workspace changes remain available.

Checkpoint automatic evidence is in `evidence/s4c5-clean-82329c8`: format/check/clippy, eight core tests, six service/diagnostic tests, three modal tests, 715 workspace tests (40 explicitly ignored), automation contract, headless workflow, dependency-tree boundary, release build, performance, Apple M1 Metal parity/invariance, source manifest and package tests. Raw commands, exits and immutable before/after source identities accompany every gate. Ignored private-fixture/platform/manual tests are not counted as executed.

Candidate native evidence in `evidence/s4c5-native-candidate-42ead99` uses real CUA input plus hash-gated read-only synthetic observations. It covers ordinary 3x4 positive pitch, 3x3 negative X/Y through Context, D/C/D order, source preservation, exact single Undo/Redo, visible preview/Escape zero mutation, one 400-object definition with 100 Block instances, ordinary/Block Save/Open, expanded 40,000-object Gerber export/native reimport and independent coordinate assertions, and Menu/Context diagnostic events. Historical local logs are excluded from public evidence; final native verification uses an isolated log directory.

The final tested clean commit, release-binary SHA-256, exact final native coverage, per-gate results and package identities are recorded in the versioned `exports/S4C5_*/REVIEW.md` and public evidence archive. Final acceptance requires matching clean source, final-build native verification and the four-file delivery audit; the versioned review is authoritative for delivery identity.

## Bounded performance

Apple M1 release checkpoint results: a 400-object shared definition with 100x100 cells commits 9,999 new references in about 298 ms; preflight is 9 µs and the project contains one definition and 10,000 instances. A one-object ordinary 100x100 array commits in about 31 ms. These are single-run measurements, not latency guarantees or P100K claims.

For the same 400-opening 5x5 panel, ordinary geometry has 10,000 objects and a 47,585-byte compressed project; Block form has 25 instances, one 400-object definition and a 3,129-byte project. No compression ratio is used as a pass threshold. At the recorded preview scale, 10x10 shared-Block preview build/update were 209/75 µs; 60-frame paint p50/p95 were 2,614/3,043 µs. Higher screen complexity can simplify to bounds. Final-run raw measurements are included separately.

## Delivery and remaining boundaries

Each versioned delivery contains `RCam_S4C5_<shortsha>_source.zip`, `RCam_S4C5_<shortsha>_public_evidence.zip`, `SHA256SUMS.txt` and `source_fresh_extract_report.json`. Source includes PACKAGE_INFO.json, PACKAGE_MANIFEST.sha256 and MANIFEST.sha256. A fresh zipfile extraction verifies every package/source hash, runs source_manifest.py --check and test_package_source.py, and compares the extracted source payload with the exact tested source set.

No private Gerber/project/font files are distributed. Existing recovery, preferences and logs temporarily isolated for native testing are restored and hash-checked at closeout. Windows, final dual-platform V1, full CORE10 10/10 and P100K remain deferred; no subsequent stage is started.
