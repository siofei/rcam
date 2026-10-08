# S5 selection presentation cache — source candidate

Stage: S5-M2 follow-up, phase 1. Related requirements R09/R10/R11/R16/R17;
regression scope AT-022/030/032/039/040/064/065/067/069/074/075/078.
Allowed modules: editor-app selection/view snapshots, presentation command gating,
property metrics, Block eligibility membership and related CPU tests. Core,
service, manufacturing geometry, shader, Snap/Alt and existing performance gates
are unchanged.

## Problem and resulting behavior

Command enablement previously scanned the whole selection on every button query;
arrangement and property summaries repeated work on unchanged pointer frames.
Block creation eligibility additionally searched the selection linearly for every
layer object. A synthetic 80000-item selection exposes those repeated paths.

UI selection, metrics, layer policy and aperture metadata now use immutable shared
vector snapshots. Cloning a View shares those four collections; every write
performs copy-on-write, preserving exact order and values. Cached identities hold
the snapshots, so mutation or allocator reuse cannot masquerade as unchanged data.
JSON diagnostics still encode the same arrays. No selection IDs are removed.

Presentation queries retain the complete original predicates and ordered metric
sum. Keys cover project/document/revision/workspace, generation/rules/selection
epoch and exact relevant snapshot identities. Blocked and scene-unavailable views
refuse immediately; busy/focus/task guards stay in command routing. Metric errors,
pending/unsupported values, display unit and exact resolution bits remain distinct.
Block/array/arrangement gates also include the immutable manufacturing snapshot.
Accepted reply and per-frame synchronization retires obsolete cache owners before
UI guards can skip queries; Close/scene-none/blocked and actual stale/cancel
paths are covered with Weak ownership checks. Service and actual gesture commit
validators remain authoritative.

Block eligibility builds a membership set and retains its original layer exposure
positions and contiguous-selection rule, reducing membership to linear work.

## Evidence and remaining work

CPU regression checks include 80000 complete selected IDs, 240 repeated frames
with 41 button queries cycling five edit commands, one computation per distinct presentation query,
copy-on-write/JSON equivalence, policy mutations without epoch changes, rollback,
old selection replies, errors/pending and exact aggregation order. Auxiliary Linux
CPU harness is private validation only; product targets remain Windows/macOS.
Mac target checks do not certify native frame time or real-design behavior.

This phase does not resolve moving more than 10000 objects: old Move count and
64MiB history guards remain. The next independent change must share read-only
Move demand with the actual single/multi-layer commit routes, keep cancellation,
complete selection/one Undo and real resource protection, and keep persistent
capacity refusal separate from transient Snap errors. Ctrl+A/modifier box selection
and command-follow-pointer/click-placement are subsequent independent commits.
Restricted native crash/capture diagnostics are not resumed.
