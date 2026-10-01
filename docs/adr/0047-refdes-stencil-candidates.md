# ADR 0047: RefDes-assisted manufacturing bounds candidates

Accepted for implementation 2026-10-01. ADR0043 is already occupied by the D1 input compatibility follow-up, so this stage uses 0047.

Candidates are not associations, final Boolean openings, persisted links or footprint ownership proof. Registered Board coordinates and the existing D1 world orientation define a component-local rectangular search window. Bottom itself never mirrors it. Explicit layers must be Gerber; hidden/class-nonselectable objects may appear in API results. Actual GUI selection separately respects effective visibility/selectability; locks permit inspection and the existing service rejects editing.

Extract the existing pure f64 WorldIndex from app to core and reuse it for render/snap/candidate neighborhoods. Derived caches are session-only, keyed by manufacturing history generation and immutable Board identity. Cold index/table construction is separately measured; warm queries resolve one ComponentId without scanning the table. Intersect each nearby analytic world AABB against the rotated rectangle with a separating-axis test. No geometry from GPU or display meshes.

BlockInstance is one object. Any hit text glyph produces one logical candidate anchored by an existing ObjectId, with all member ObjectIds and union bounds; membership expansion is bounded. No CandidateId/AssociationId. Distance means distance to the envelope. Sort by full containment, center distance, semantic layer order, lexical ObjectId. Reject oversized neighborhoods/member groups; never truncate before pagination.

Pure &self query with content revision fence; Board edits and Undo/Redo share that revision. Selection/highlight/focus/window are transient app state. Worker sequence/document/revision/component/settings fences discard old results. No project schema bump or dependency; no footprint guess, persistent association or replacement.
