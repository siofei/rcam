# S4-C4 Alignment / Distribution plan

Status: PASS (Mac-first bounded); final source/package identities are recorded in the versioned exports review. User started S4-C4 on 2026-09-27; this supersedes the preceding closeout's “next stage not started” boundary for this task only.

Scope: six world-AABB alignment commands, horizontal/vertical equal-edge-gap distribution, last-selected anchor, atomic same-layer service, shared menu/context commands, diagnostics, regression and bounded Mac native acceptance. ADR 0040 freezes semantics before implementation.

Requirements: R04/R05/R09/R10/R11/R12/R14/R15/R16/R17/R19/R20/R21/R22. Related local cases: AT-017/022/026/029/030/031/039/040/041/042/043/054/058/059/060/064/067/075/077/082/083/086/087/088/089/090/091/092/093/095/097. These are local coverage, not full case or platform completion.

Allowed modules: editor-core (alignment calculations, atomic translation history, command registry), editor-service (DTO/validation/dispatch/diagnostics), editor-app (shared commands/menu/anchor display/tests and observation-only probe), rcam-diagnostics (bounded numeric operation metadata), tests/synthetic fixtures/scripts and named docs. Existing writer/project codec/manufacturing precision semantics remain authoritative.

Baseline contains uncommitted C3 source/doc changes. Preserved snapshot: evidence/s4c4-start-20260927/baseline.zip, baseline.patch and status.txt. Do not revert or attribute those pre-existing changes to C4. Final source identity and clean-checkout evidence must be separately recorded.

Sequence: core/service/UI implementation; semantic/permission/undo/roundtrip/diagnostic/resource tests; required fmt/check/clippy/workspace tests and service contracts; release/performance/Metal; native interaction with synthetic data; review and deterministic source/evidence packages in exports. Do not label PASS until all C4 exit gates have evidence.

Windows deferred/not executed. No full V1, CORE10 10/10 or P100K claim. Stop after C4; no Array/Panelization.
