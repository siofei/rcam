# S5 Move capacity admission — source candidate

Stage S5-M2 follow-up phase 2; requirements R09/R10/R11/R16/R17,
regressions AT-022/030/032/039/040/064/065/067/069/074/075/078.
Allowed modules: dedicated core Move target/demand planning, service Move resource
admission, worker drag probe and point Move preview, UI gesture capacity and associated CPU regressions.
Base is frozen phase1 2cb6656, descended from Array 4aa5520/2049fba.

## Authorized policy change

Move alone may address up to the existing one-million document object boundary.
This is an upper bound, not an admission or success promise. Other edits, shared
batch target resolution and Array source retain their existing 10000 bound.
The actual configured history budget (default 64MiB/100 entries) stays unchanged.
A separate finite 256MiB conservative incremental Move work estimate charges
planning sets/indices, transaction/commit geometry copies, validation lookup sets
and returned IDs. It includes the largest temporary Block definition expansion
(with geometry/container copies) and memoizes only referenced definitions. It is not a process RSS bound and excludes pre-existing document,
UI/display/GPU snapshots and unrelated allocator/runtime overhead. A separate conservative Move validation
work limit of two million units charges object/edge work, layer/aperture lookup
and per-instance definition lookup/children/edges; it is not a time measurement.
The existing ten-thousand selection-group ceiling remains. Empty or repeated
layer groups are rejected before any per-layer permission scan.

Read-only demand uses the actual single-layer or multi-layer commit route and
checked arithmetic before any geometry clone. Final commit repeats target,
resource, permissions/revision and geometry/precision validation. A resource
preflight does not certify an arbitrary final delta. Failed commands are atomic;
successful commands produce one Undo. No trimming or partial selection is allowed.

Drag probe reports capacity independently of ordinary query errors. The terminal
TaskVersion, selection identity, task ID and owning gesture fence apply before
confirmation. Pending/refused/cancelled capacity cannot produce movement preview
or release submission; ordinary click/marquee selection remains available.
Capacity refusal survives transient Snap error clearing. Release before the reply
still waits for the matching terminal confirmation.

No Snap/Alt/shader/trajectory/performance threshold changes. No restricted native
crash/capture/performance experiment. Mac ordinary input and real-design testing
remain external; cloud uses synthetic CPU regressions and pinned Rust1.89 checks.

## Regression evidence boundary

Synthetic core and actual parser/service workflows cover complete 80000-item
single-layer Move, returned IDs, one revision/Undo and exact Undo/Redo; multi-layer
route budget refusal and an admitted 12000-item two-layer transaction; complex
Region and large Block work refusal; unchanged other-edit/Batch bounds;
configured history limits, invalid-delta error precedence, permission/revision
revalidation, checked arithmetic and deterministic demand-loop abort.
Point Move previews use the same resource preflight before geometry copies and
clear obsolete previews on failure. Post-submit point-tool selection rebind uses
the accepted manufacturing snapshot in one indexed pass, preserves service result
group/ID order and visibility filtering, and avoids repeated linear objects.get. UI regressions cover pending/refused capacity, Snap clearing, release before reply,
selection owner retirement and existing cancelled/stale probe fences.

Read-only permission/demand loops checkpoint cancellation in batches of 256;
existing terminal installation can roll back a late cancelled probe. This does not
claim the native two-second termination gate. Actual edits preserve the existing
Committing boundary: cancellation after that boundary can be TooLate.
The real Mac design is not present in this cloud workspace and its resource
admission/frame time must still be verified locally. No sample is discarded.
