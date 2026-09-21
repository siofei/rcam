# Global Units & Manufacturing Precision — Mac-first review

2026-09-20. Scope/requirement/case mapping and allowed modules are frozen in
[PLAN](GLOBAL_UNITS_PRECISION_PLAN.md); policy decisions are in
[ADR 0026](adr/0026-global-units-manufacturing-precision.md).
S4-A2.1 and S4-A2.2 are bounded Mac-first PASS prerequisites. Windows deferred /
not executed; full V1, P100K and a complete CORE10 release are not claimed.

**Status (2026-09-21, S4-B1 Gate 0.3): Global Units & Manufacturing Precision = PASS (Mac-first bounded).** This closes the
foundation slice only; it does not extend to Windows, full V1, P100K or complete CORE10. S4-B1 work is tracked in
[S4_B1_REVIEW](S4_B1_REVIEW.md).

## Delivered behavior

- Four units share the core conversion/parser/Auto formatter, including squared
  area units. Bare input follows the current unit; explicit suffix wins. Working
  coordinates/API DTOs remain f64 mm. Unit changes never submit manufacturing edits.
- Units settings use the exclusive modal. Focused drafts cannot silently change
  unit meaning. Retained text fields cache canonical millimetres until edited;
  invalid unfinished fields block unit switching. Measurement annotations survive
  opening/closing Units and convert without changing their endpoints.
- Default manufacturing resolution is 0.0001 mm, independent of 0.1 mm grid and
  0.000001 mm FS encoding. Presets 1/0.5/0.1 µm and validated custom precision use
  ApplicationService revision fencing. Geometry dirty/history stay unchanged;
  export_policy_dirty is separate and participates in close/save handling.
- Export normalizes a private snapshot, including scaled aperture copy-on-write.
  Quantization is finite, symmetric, ties away from zero and idempotent. Arc/Region
  topology and original semantic/write/reparse/compare checks remain required.
  A coarse policy that cannot preserve a subgrid arc/Region safely rejects before
  touching the destination; imported working geometry remains exact. The finer
  1 nm policy explicitly covers the historical subgrid source-fidelity fixtures.
- Text retains content, font, alignment, spacing, dimensions and position state
  through create/cancel/reopen within the app session. Cancel clears transient
  preview/submission state only. Restart persistence is outside this slice.
- Text UI now exposes document precision; internal/API curve tolerance remains.
  Contours use the manufacturing lattice before the existing Line/Arc fitter.
  Bounded 64-vertex candidate lookahead tolerates lattice curvature jitter while
  preserving the original radius/error/topology checks and resource limits.
  No slabs or renderer-derived manufacturing geometry are introduced.

## Executed development and native evidence

Raw run records are in `evidence/global-units-dev-20260920/`; final immutable
command results and copies of the selected raw records are delivered under
`evidence/global-units-final-20260920/`. Development failures are retained separately
and are not represented as final successes.

- Full workspace no-fail-fast test run exited 0 before final minor display and
  measurement fixes; the final clean gate reruns all required commands below.
- Release real-font geometry/export/reopen matrix: **30/30 PASS**, Arial Unicode,
  STHeiti Light and Songti, ten fixed strings, 3 mm, 37 degrees, zero offset.
  `text-matrix-final.log` and `text-matrix-final/matrix.json` are the raw records.
- Release manufacturing-policy matrix: **8/8 PASS**, `钢网测试口回` and `中文ABC123`,
  default 0.1 / 0.5 / 1 / custom 0.2 µm. Default Arial has 22 objects/724 edges and
  8 objects/930 edges respectively, not hundreds/thousands of slab objects.
  See `policy-matrix-budget.log` and its `matrix.json`.
- Extended preview/performance matrix: **24 generated, 3 safely rejected** out of
  27. Existing negative-offset STHeiti/Songti topology rejections remain explicit;
  they are not counted as supported geometry. Prior 6ad0dfc evidence had 21 generated
  and 6 rejected. Raw durations/counts are in `preview-performance-final.json`.
- Native CUA observations on the final release binary: same Flash properties and
  numeric modal in all four units; grid 0.1 mm / 0.003937007874015749 inch /
  3.9370078740157486 mil / 100 µm; one fixed measurement 5.0647 mm / 0.199397 inch /
  199.397 mil / 5064.7 µm. Revision remained 0 and no dirty marker appeared.
- `RCAM\n123` retained its font, 1 mm height, 0.15 mm stroke and 40-object preview
  after cancel/reopen and four-unit roundtrip. Earlier native create was one
  revision and was saved/reopened as 43 objects including the original three.
- Native precision sequence default → 1 → 0.5 → custom 0.2 → default µm advanced
  revision 0→1→2→3→4, left geometry/history unchanged, set only policy dirty and
  cleared it when restored to the saved baseline.
- Final native Save As and Reopen of `钢网测试口回`, including Region Line/Arc and
  local holes, retained 25 objects and visible rotated contours. Synthetic inputs
  were not overwritten. Actual saved Gerbers are included with their hashes.
- `native-observations.json` is an explicitly labelled transcription of CUA visual
  observations, not stdout. `native-runtime-final.log` is a separate same-binary
  launch proving Apple M1 / Metal / 2 pixels per point. The CUA-launched session
  did not capture stdout. No human physical gesture/IME rerun is claimed.

## Clean commit gates and delivery authority

Run `python3 scripts/run_s4a1_gates.py --stage 'Global Units Manufacturing Precision'
--extra-service-test global_units_precision --extra-service-test s4a2_text_preview
--out evidence/global-units-final-20260920` from the committed manifest.
`gates.json` records the exact clean commit, command arrays, exits, platform and
source hashes and is the authority for the final command status. A pending or
failed gate must not be inferred to pass from this review text.

Required commands: cargo fmt --check; check/clippy --workspace --all-targets
--locked (-D warnings); test --workspace --locked; explicit automation_contract,
headless_workflow and stage regression suites; service normal dependency tree;
release editor-app; source_manifest --check; audit/source-package tests; release
native Metal reference/production pixel parity. Rust uses the pinned `.tools`
CARGO_HOME/RUSTUP_HOME/CARGO_TARGET_DIR; native Metal may require unsandboxed
macOS execution, which must retain its own actual log.

Delivery requires a clean tested commit, `RCam_UNITS_<sha>_source.zip`, verified
fresh extraction, `RCam_UNITS_<sha>_public_evidence.zip`, full `EVIDENCE.sha256`
verification and `SHA256SUMS.txt`. Final package verification records accompany
these archives. Only all actual successful gate/package records together with the
bounded native evidence authorize **Global Units Foundation PASS (Mac-first)**.

## Remaining boundaries

Coarsening precision after text creation can reject existing arcs rather than
refit them; regenerate under the new policy or select a finer resolution. Custom
precision outside the FS lattice is rejected. Negative offsets that invalidate
font topology remain rejected. Font data are system-local and not redistributed.
Display unit/text draft and document policy are session-local until the future
project format; Gerber reopen restores geometry and defaults editor policy.
Windows, complete V1, physical gestures/IME and P100K remain separate gates.
