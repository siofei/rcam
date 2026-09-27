# ADR 0041: Non-associative rectangular arrays

Accepted for S4-C5 implementation, 2026-09-27.

Rows/columns are positive integers, checked product <= 10,000 cells. Finite signed f64 mm pitch is origin-to-origin: (c * X, r * Y), positive Y is world up. An axis with count > 1 requires nonzero pitch. Display units convert at UI input only; no grid, snap or precision quantization.

All explicit IDs belong to one effectively visible/selectable/unlocked layer and category. Source physical objects must form one contiguous exposure span, with every selected GeneratedText operation complete. Original cell (0,0) is unchanged. Copies follow that span, before subsequent original objects, in row-major cell order and original layer order within each cell. This is an ordered exposure copy, not an isolated image or boolean union.

New stable IDs are reserved with checked arithmetic and consumed only after commit. Each copied text operation gets a distinct per-cell operation ID (multiple source text groups remain distinct); ordinary imported/generated origins become Generated with a new per-cell operation. Aperture IDs and BlockDefinition IDs/revisions remain shared. Instance rotation/mirror are retained and only world translation changes. CompatibilitySolid provenance/warnings remain intact.

Use one existing Insert structural transaction with exact order guards. Preflight new objects <= MAX_MOVE_OBJECTS (10,000), document total <= MAX_EDIT_DOCUMENT_OBJECTS (1,000,000), region edges <= MAX_EDIT_REGION_EDGES and conservative complete history bytes before allocation/commit. Source shared Block geometry is not counted as millions of copied objects; coordinate validity must still include transformed definition bounds. Rejected commands consume no IDs/history/revisions. 1x1 is a validated no-op, preserving redo, revision and dirty state.

Preview reuses source display primitives/BlockDisplayCache with translated cells, never clones a manufacturing document per frame. Bound preview work by cells and primitive complexity, simplify to bounds for large arrays, and report simplification. Cancel/Escape remove transient state only. Apply runs the same service on the existing background worker and retains original selection.

No Array entity/metadata/schema change or automatic Block conversion. Project/recovery store ordinary materialized objects/instances; Gerber export uses existing expanded writer, no %SR.

Diagnostics whitelist rows, columns, cell/source/created counts, block presence and pitch sign categories (0 zero, 1 positive, 2 negative, 3 nonfinite); no pitch magnitude, geometry, full ID lists or customer paths. Existing command source context records Menu/Context/Automation. No per-frame INFO.

Windows and final V1/CORE10/P100K gates remain deferred.
