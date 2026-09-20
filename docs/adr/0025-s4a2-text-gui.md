# ADR 0025 — S4-A2 service foundation and remaining GUI boundary

Status: accepted for the foundation only, 2026-09-20; full S4-A2 pending.

`Layout.curve_tolerance_mm` defaults to the S4-A1 0.00025 mm value when absent
in JSON. Valid numeric interval: [0.00001, 0.00025] mm. NaN/infinity/nonpositive
or coarser values are INVALID_ARGUMENT; a positive value below the floor is
RESOURCE_LIMIT before font outline processing. The floor bounds requests below
useful writer/cleanup resolution; acceptance of a numeric tolerance does not
promise that every font/shape passes the unchanged geometry/work budgets.

Conservative contour error budget, in mm:

| Contribution | Upper allowance |
|---|---:|
| Flatten | 0.00025 |
| Font coordinate conversion | 0.00025 |
| First height pass | 0.00008 |
| Certified cleanup | 0.000004 |
| Writer quantization (Euclidean, 1e-6 per-axis grid) | 0.000000708 |
| Floating point reserve | 0.00001 |
| Total | 0.000594708 |

Total stays below DESIGN_V1's 0.001 mm manufacturing contour bound. We do not
allow coarser-than-S4-A1 flattening; the unspent allowance is not yet assigned
to offsetting. Offset error/topology must be reviewed before exposing that
parameter. Existing visible-height and writer/reopen regressions remain gates.
All MAX_EDGES/MAX_REGIONS/MAX_WORK/MAX_CHARACTERS limits are unchanged.

text.preview takes the same document, expected_revision and TextParams as
text.create. Although read-only, it requires revision to bind its result to a
specific manufacturing snapshot. Both call prepare_text: FileAccessPolicy,
64 MiB bounded read, hash, face/layout checks, generation, and the same
EditHistory::generated_plan preflight. Preflight validates geometry, available
IDs, document/edge/history budgets without consuming IDs or changing history.
The commit revalidates before insertion. Every create re-reads and hashes the
font; a preview is never a cached commit authorization.

TextPreviewResult contains owned geometry, document/revision, frozen params
(including font identity), and the existing error bound. It contains no
mutable model references, third-party AST or GPU objects. No font cache or new
dependency is introduced. Service callers own background scheduling. This
commit does not implement GUI jobs or claim stale GUI publication protection;
a future worker must tag draft generation and recheck tool/document/revision/
layer/current generation before publishing. Preview parameters include a local
font path and must be redacted before any public evidence publication.

A retained real-font regression, Arial Unicode face 0, text '口8', height 3 mm,
tracking 0.1 mm, center/middle, rotation 37 degrees, anchor
(12.34567, -6.78901), tolerance 0.0000625 mm, is rejected by the existing
semantic validator for overlapping adjacent edges. Both preview/create must
reject atomically. This is an open compatibility limitation, not a successful
ultra-precision acceptance and not grounds to relax geometry thresholds.

GUI, IME, material outline offset, placement modes, performance matrix and
session text groups remain unimplemented in this foundation closure.
