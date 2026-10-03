# ADR 0052 — immutable display bins for common-delta batch drag

2026-10-02. S5-M2-B accepted for implementation; acceptance is pending.

The existing Gesture snapshots IDs and revision and commits objects_move once.
Keep that service boundary and A2 lifecycle intact. Manufacturing remains f64 mm;
preview is a transient common delta, never a mutation or a source of export data.

Rebuilding all display bins on every pointer movement violates DESIGN §9.5.
Keep the original display index: merge the ordered static-cell candidates at p
(unselected) and p-delta (selected), then evaluate each in its own local position.
The merge preserves exposure/layer ordering and eliminates duplicates, including
when both samples land in the same bin. A sample outside the original world may
still have a nonempty shifted query. Selection halos use shifted sample points.
The CPU diagnostic candidate calculation mirrors only query conservatism, not
the independent manufacturing or brute-force renderer truth.

The six existing GPU buffers and bind layout stay unchanged. The preview only
uploads existing uniform bytes; selection membership remains stable for a drag.
No second GPU device, new dependencies, public DTO, geometry tolerances or
manufacturing writer changes. Existing independent pixel parity runs on Metal
before closeout, including Dark/Clear, holes, mixed selected subsets and layers.

This is a display optimization and a bounded performance closeout, not PMIX,
PPOL or full V1 acceptance. Frozen thresholds and all delivery gates live in
S5_M2_B_PLAN.md. New B evidence is separate from frozen A2 evidence.
