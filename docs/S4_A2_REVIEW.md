# S4-A2 Completion review and acceptance boundary

Base: 246f57e foundation. Its historical review is preserved in
S4_A2_FOUNDATION_REVIEW.md. Current scope is the full Completion brief plus
the mandatory Canvas UX addendum and the user-requested system-font list.
Requirements/cases/modules: S4_A2_PLAN.md. Numeric/architecture decisions:
ADR 0025. Windows deferred / not executed; no full V1 claim.

## Implemented closure

- Frozen Arial Unicode 口8 / 3 mm / tracking 0.1 / center-middle / 37° /
  (12.34567, -6.78901) / 0.0000625 mm succeeds in preview and create. The
  general Region validator is unchanged; sub-resolution slab omission still
  requires the retained-boundary certificate.
- Real material outline offset, bounded topology/resource rejection and
  total contour allocation below 0.001 mm. Dark geometry represents glyph
  holes without global Clear. Original default precision remains 0.00025 mm.
- Searchable macOS Core Text system-font list is primary, with a retained
  file-picker button, explicit face/hash and bounded recent choices. Named
  TTC faces are resolved from font tables, not guessed as index zero.
- Existing model worker handles font inspection and 200 ms debounced typed
  previews. Generation/document/revision/layer/font/parameters fence stale
  results. Preview and Cancel never mutate manufacturing state.
- Mouse, exact Absolute, explicit Relative/Pick Base placement; opt-in text
  snap. Apply is one create and one Undo transaction, with generated-group
  selection. Persisted Gerber contains ordinary manufacturing geometry.
- Grid visibility uses 180 ms transient opacity; 1/2/5 visual levels blend
  continuously in physical pixels. Manufacturing snap retains base spacing.
- Cached f64 viewport envelopes precede camera-relative f32 conversion;
  exposure order is preserved. Dynamic view precision clamps and retained
  last-good frames protect navigation. Pending display work blocks edits
  against a stale displayed frame; manufacturing invalidity still fails closed.
- Native testing exposed an inflated dense-text GPU work estimate. The
  correction integrates candidate work over visible cells, preserving the
  existing 2,000,000,000 work ceiling. A Retina Apply regression covers it.

## Evidence and limitations

The final run-specific evidence archive is authoritative for command exits,
source identity, native steps and unexecuted coverage. Code implementation,
automated checks, native observations and user-reported physical gestures are
separate evidence categories; none substitutes for the others.

Development native evidence includes actual macOS IME preedit/commit/edit,
preview/offset/rotation, and Retina ppp=2 grid fade. The user performed slow/
fast trackpad pinch, zoom toward bounds and Fit and reported no error or blank
canvas. Raw logs retain 973 camera events, scales 0.260343..27367.632, revision
0 and no display-prepare diagnostic for that run. This is evidence for the
observed run, not a proof of every mathematical zoom bound or every platform.
The old development window's unsaved user changes were preserved.

The three-font performance matrix intentionally records RESOURCE_LIMIT and
VALIDATION_FAILED cases as rejections, not successful geometry. Existing edge,
region and work budgets were not raised. Unsupported fonts/erosion topology,
variable fonts, complex shaping, multiline and automatic stencil bridges
remain fail-closed. Font data never enters source/evidence archives.

Full stage acceptance requires the run-specific native coverage table and all
final clean-commit gates. Do not infer PASS from this implementation review.
The next task is Global Units & Manufacturing Precision Foundation,
**BLOCKING BEFORE NEXT MAJOR FEATURE STAGE**, covering mm/inch/mil/µm and
0.1 µm default manufacturing precision. No S4-B/S5 work was started.
