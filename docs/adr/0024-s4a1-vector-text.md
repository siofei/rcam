# ADR 0024 — S4-A1 vector text manufacturing core

Status: accepted for this Mac-first closure, 2026-09-20.

Use the already locked ttf-parser 0.25.1 (MIT OR Apache-2.0) directly in the
window-free editor-text crate. Its local source OutlineBuilder/Face APIs were
reviewed. No shaping dependency is introduced: static horizontal ASCII, CJK
BMP and the documented punctuation ranges use explicit glyph advances and
tracking. Controls/newlines, complex scripts, variable fonts, missing glyphs,
bitmap-only outlines and ambiguous/unrepresentable geometry fail closed.

The service reads an explicit font path under FileAccessPolicy, bounded to
64 MiB independently of the unchanged 8 MiB Gerber budget, verifies SHA-256
and face index, and requires caller-supplied license status plus distribution
permission. These are provenance declarations, not legal determinations. No
font file is distributed. The response returns the identity and geometry bound.

Height is the visible total outline height, not em size. A fine first outline
pass establishes scale; a second pass flattens quadratic/cubic Beziers by
recursive de Casteljau with a convex-hull distance-to-chord bound <=0.00025 mm.
Font coordinates arrive as f32 from the parser and are promoted immediately.
A conservative parser conversion allowance <=0.00025 mm is required; scales
that exceed it are unsupported. First-pass height sampling contributes less
than 0.00008 mm at permitted scales. Adjacent points within 0.000004 mm are
collapsed before Region validation. A collapsed sub-quantum slab is omitted
only when its entire convex hull is certified within 4 nm of one retained
boundary segment; otherwise generation is rejected.
this contributes <=0.000004 mm. All are within the frozen 0.001 mm bound,
including ordinary output coordinate quantization. No display zoom participates.

Per glyph nonzero-winding contours become vertical slabs split at vertices and
edge intersections. Material intervals become convex solid Region objects.
This handles local holes without global Clear, without assuming Gerber has
SVG contour rules, and without GPU tessellation. Different glyphs can overlap
as ordered Dark objects. Metrics are exact for each resulting polygon; summed
polygon perimeter includes internal decomposition boundaries and is explicitly
not the outline perimeter of an entire glyph or final layer boolean result.

Layout is performed locally, with visible left/center/right and
baseline/bottom/middle/top anchors, then one rigid position/rotation transform.
Limits: 128 Unicode scalars, 4096 sampled points per glyph, 8 million slab work
units, 10,000 output objects, 24 subdivision levels; existing core/history
limits remain in force. Font input coordinates beyond the certified range are
rejected. No automatic fallback, kerning/shaping, live preview or snapping.

EditHistory::insert_generated validates/budgets everything before one append
transaction; IDs and Generated operation ID use the existing monotonic
allocator. Undo/Redo preserve identical object IDs and geometry. All geometry
uses the existing writer/reparse validation. Generated IDs constitute the
session group; normal Gerber loses original text/font/group editing metadata.
No automatic stencil bridges or manufacturing readiness claim.

Shared core correction: point_on_segment now compares cross product divided
by segment length against the existing 1e-6 mm epsilon. Previously it compared
mm^2 to mm, falsely rejecting small valid triangles. Regression covers short
and long edges, true overlap and a small valid Region; no threshold is raised.
