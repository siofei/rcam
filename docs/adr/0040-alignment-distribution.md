# ADR 0040 — Alignment / Distribution (S4-C4)

Status: accepted for implementation, 2026-09-27. Acceptance is tracked separately.

Use existing editor-core manufacturing world AABBs in f64 mm, including aperture orientation and resolved rigid BlockInstance transforms. World Y points upward: Top=max_y, Bottom=min_y; HCenter aligns X, VCenter aligns Y. No screen bounds, tessellation, snapping or quantization enters these commands.

Align takes at least two unique IDs and an explicit member anchor. GUI uses existing ordered SelectionSet.primary(), the last added selected object; the anchor geometry and identity remain exact. Removing the primary reveals the preceding remaining selection. Bulk selection uses its deterministic existing selection order.

Distribute takes at least three unique IDs and spaces world AABB edges, independently of selection order. Sort by (min edge, max edge, stable layer object index). Fix both endpoints exactly. Gap=(last.max-first.min-sum(widths))/(count-1); negative gaps are allowed without clamping. Identical bounds remain deterministic.

Both operations require one visible, selectable, unlocked layer and visible/selectable/unlocked object categories. Cross-layer IDs fail CROSS_LAYER_EDIT_UNSUPPORTED; missing IDs, stale revisions and permission/resource failures leave everything unchanged. Service receives explicit IDs, layer, anchor/mode or axis and expected_revision; it never reads GUI selection.

Prepare every delta and validate translated geometry before one existing history transaction. Retain IDs, object exposure order, local topology/aperture identity and BlockDefinition contents/revision. Only BlockInstance translation changes. Exact zero deltas and unchanged translated geometry create no history or revision; no new epsilon. Successful changes increment revision once and dirty remains content-baseline derived. Undo/Redo restore exact stored geometry.

No schema change, new dependency, array/panelization, constraint solver, definition internal edit, PnP/RefDes or multi-object grip. Immediate execution is sufficient; hover preview and toolbar are deferred. Diagnostic events contain numeric mode/axis, hashed anchor, counts, revision, duration and result; no geometry, raw IDs or filenames.

GeneratedText is stored as several semantic objects sharing a text operation identity. Arrangement treats the complete text group as one logical unit: union its manufacturing bounds and translate every member by the same delta. An anchor ID belonging to text fixes the entire group. A request omitting any group member is rejected with INVALID_ARGUMENT, rather than moving/splitting glyphs. Minimum 2/3 counts refer to logical units; the 10,000-object budget still counts physical semantic objects. Ordinary non-text objects and BlockInstances each count as one unit.

Exact-policy numerical boundary: translating and recomputing analytic bounds can leave a representable f64 residual. The generated stroke-text regression observes min_x=4.49999999999999911 after a move to an anchor at 4.5; a subsequent +8.9e-16 mm correction is therefore a real transaction, and the following exact-aligned request is a no-op. Suppressing this residual with a new tolerance would violate the frozen policy; tiny nonzero edits are retained, and Undo restores each stored state exactly.

Signed zero is a numeric tie: -0.0 and +0.0 compare equally before the stable layer-order tie-break. All bounds must be finite before sorting.
