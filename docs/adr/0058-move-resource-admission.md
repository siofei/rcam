# ADR0058 — dedicated Move resource admission

2026-10-08. User explicitly authorized selection performance and Move capacity
consistency after Array 4aa5520; implementation is a separate source stage.

Keep other edits and batch/source target sets at 10000. Give Move only the existing
1000000 document-object ceiling, while preserving the configured history budget
(default 64MiB/100 entries). Share read-only checked resource demand with the
actual single-layer Modify or multi-layer Selection commit formula. Add finite
256MiB conservative incremental work and 2000000 validation units; charge referenced
Block expansion peak and per-instance lookup/children/edges without cloning them.
Retain the original 10000-group limit and reject duplicate/empty groups before
permission scans. These limits are explicit refusal ceilings, not success/RSS/time
promises. Service failures report actual route and configured estimates.

Probe resource refusal is separate from ordinary query/Snap errors. Pending,
refused, cancelled or retired results cannot create movement preview/commit.
The actual release still validates revision, permissions, target completeness,
resource demand and final f64 geometry, with one atomic Undo. A preflight cannot
certify zero/nonfinite or arbitrary delta geometry. Cancellation respects the
existing Committing/TooLate contract.

See S5_MOVE_CAPACITY_ADMISSION.md for Rxx/AT scope and evidence limits. Native
50ms/1e-9 thresholds, samples, shaders, Snap/Alt and old evidence remain unchanged.
