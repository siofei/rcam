# ADR 0044: large-workspace display admission

Status: Accepted by explicit user direction, 2026-10-01. Scope: S4-D1 maintenance.

Fixed display performance budgets rejected valid full multilayer boards and
caused repeated viewport retries to toggle controls. The user explicitly
requested removal of resource admission limits and accepted slower performance
for very large workspaces.

Remove fixed object/item/per-cell/work-estimate refusal from the display path.
Use an adaptive reference budget and bounded acceleration targets to limit
redundant storage while retaining every primitive and contour. Estimated work
remains measurable. Exact finite/index representation is a correctness check,
not a performance threshold. Device allocation errors remain reportable.

Keep manufacture, import/codec/export safety contracts. Scene preparation stays
on the worker, background viewport updates keep controls enabled, and failed
identical camera requests settle. Per-contour bounds skip provably outside
polygon edge scans without changing exact exposure evaluation.

Acceptance is supplemental to frozen V1 cases: complete real 12-layer import,
527227-object project, Retina viewport, selection/move/Undo, Metal/reference
parity, original hashes, full locked gates and complete source packaging.
Performance observations are reported; no old-threshold or Windows/P100K PASS
is inferred. See S4_D1_LARGE_WORKSPACE_FIX.md.
