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

## Completion continuation (implementation in progress)

Gate A retains the exact frozen real-font case; fix text decomposition/cleanup,
never the general Region validator. Gate B evaluates clipper2-rust = 1.1.0,
BSL-1.0, pure Rust / Rust >=1.70, with existing num-traits as its only normal
dependency. Source/API: https://docs.rs/crate/clipper2-rust/1.1.0/source/ .
Offset policy reference:
https://www.angusj.com/clipper2/Docs/Units/Clipper.Offset/Classes/ClipperOffset/_Body.htm .
Use normalized nonzero material union before closed-polygon round offset,
explicit integer scaling and arc tolerance. Do not offset intersecting raw
contours, assume hole winding, or allow unverified collapse. Compared with a
new handwritten offset engine or C++ FFI, the pure Rust library avoids both
an unaudited geometric implementation and a new native build toolchain.
Cross-platform source is portable Rust; only macOS is tested this round.
Exact limits/error allocation and topology regressions must pass before UI use.

## Completion implementation and Canvas addendum

The paragraphs above record the historical foundation limitation. The exact
Arial Unicode 口8 regression now passes all three presets after text-local
thin-slab detection plus the existing retained-boundary certificate; the
shared Region validator and 0.001 mm manufacturing limit are unchanged.

Offset is real material geometry, after nonzero contour union and before
rotation: positive expands outer boundaries and contracts holes; negative does
the reverse. Integer grid is 1e-8 mm, round-join error <=0.0001 mm. Negative
offset rejects changed component/hole counts, overlapping glyph intervals and
unverified thin-feature loss; work/edge budgets are unchanged. Visible-height
normalization uses height-2*offset before material offset and rejects a final
height discrepancy greater than the existing 0.00008 mm height allowance.

Updated worst-case error allocation in mm: parser 0.00025, flatten 0.00025,
height normalization 0.00008, offset round approximation 0.0001, integer
rounding 0.000000015, certified cleanup 0.000004, writer 0.000000708,
floating reserve 0.00025; total 0.000934723 < 0.001. Rotation is applied in
f64, and shared insertion/writer validation remains mandatory.

The app uses a 200 ms debounced typed worker, immutable preview parameters,
font hash/face identity and document/revision/layer/generation fences. A changed
font is read and hashed again at Apply. Cancellation and preview remain
view-only. Mouse, exact absolute and explicit relative-reference placement
are separate; text grid snap is opt-in. Apply selects one generated operation
and is one Undo transaction. Save/reopen yields ordinary geometry.

Canvas addendum: f64 manufacturing envelopes are cached by geometry revision,
queried before f32 conversion, and restored to source exposure order. Render
origin follows the requested camera center with a viewport margin. No global
geometry is inferred from this display cache. Grid uses a 180 ms view-only
fade and continuously weighted 1/2/5 levels in physical pixels, independent
of base snapping. Camera limits include world-f64 and local-f32 precision;
transient display precision failures retain the last valid frame.

The sample-work limit remains 2,000,000,000. Candidate visits are bounded per
visible spatial cell plus AA/selection halo, rather than multiplying the
single densest cell over the whole screen. This fixes a native text Apply
regression without increasing the resource budget or omitting geometry.

Native and automated completion evidence is still being collected. These
implementation notes are not a declaration of full S4-A2 acceptance.

The user's final font-picker refinement makes the searchable installed-font
catalog primary, retaining a separate explicit file picker. macOS Core Text
supplies catalog metadata on the existing worker (bounded to 10,000 descriptors).
Selection resolves the PostScript name against TTF/TTC name tables (including
ASCII Macintosh Roman names), then validates the exact face, static-outline
support and file hash. Catalog discovery itself grants no arbitrary file access
and does not read font bytes into the UI. Current-host catalog/face tests found
528 entries and verified distinct TTC indices 0 and 1 without document mutation.
