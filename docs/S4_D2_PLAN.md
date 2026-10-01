# S4-D2 RefDes-Assisted Stencil Candidate Selection v1

Started 2026-10-01 by the user. Baseline: clean 8b978cc on isolated branch codex/s4d2-stencil-candidates; unrelated main-checkout changes preserved.

Scope: S4-D2, R05/R06/R07/R08/R09/R10/R11/R12/R16/R17/R18/R19/R20/R21/R22. Local regression mapping: AT-022/023/025/027/028/030/031/040/041/052/062/063/065/070/074/077/082/083/084/086/087/088/089/090/091/094/095/097. New bounded cases D2-01–D2-13 live in the addendum. Frozen ACCEPTANCE_V1.md and acceptance_cases.json are unchanged.

Allowed modules: editor-core manufacturing envelope index/window helpers; editor-service candidate DTO/read-only API/session caches; editor-app Components worker/panel/selection/overlay and internal-only synthetic native driver; diagnostics counters; synthetic tests/fixtures; stage documentation and evidence/packaging scripts. No new dependency or project schema change.

Candidate != Association. Require registered Board, reuse world_rotation_deg without automatic Bottom reflection, explicit Gerber layers and component-local rectangle (default 10×10 mm). Reuse shared WorldIndex, analytic manufacturing bounds and deterministic ranking: fully inside, center distance, semantic layer order, ObjectId. Query ignores temporary visibility/selectability; actual selection obeys both. Blocks are atomic; GeneratedText expands the entire logical group. Exposure remains Dark/Clear. Limits: pages 1–500, nearby/expanded members <=10000; reject instead of partial truncation.

Worker requests/results fence sequence, document, revision, component and query settings. D2 state is transient; no manufacturing revision, dirty flag, Undo, persistence, footprint-size guesses or special edit path. Native Apple Silicon, full regressions, clean final source and deterministic four-file package are required before PASS. Windows/full V1/CORE10/P100K deferred. Stop after D2.
