# S4-D1 edit / Undo speed maintenance

Requested 2026-10-01: reduce the previously observed 1–2 second modification
and Undo latency on the same 12-layer set and 527227-object private project.
This is S4-D1 maintenance, not D2. Scope R10/R11/R16/R17/R19/R21/R22;
regression references AT-030/039/040/068/074/075/088/089/090/091/092/094.
Allowed modules: editor-core transaction change hints; editor-service session
content baseline/cache; editor-app refresh profiling/display reuse and tests;
corresponding design/ADR/evidence scripts. No schema or public DTO change,
new dependency, manufacturing approximation or acceptance threshold change.

Baseline measurements split real worker Model actions into service submission
and refresh. Keep original inputs read-only; preserve canonical signed-zero,
exposure/order, metadata, aperture and Block definition distinctions in dirty
comparison. Failed/cancelled edits must not invalidate history or move the
saved baseline. Undo must restore exact geometry and clean state; revision
continues advancing. Cache misses, nonconsecutive edits and structural changes
must rebuild rather than reuse incomplete data. Confirm both single-object
and 1000-object editing, saved baseline changes, inverse edits, redo branches,
failed operations and multiple transaction types.

Use per-object-range canonical SHA-256 signatures only for session dirty
comparison. Saved/current signatures are compared in layer and chunk order;
unchanged chunks are reused only from explicit successful transaction hints.
The former whole-document canonical digest remains an independent test oracle.
File SHA-256, project validation, writer output and persistent schemas stay
unchanged. Benchmark exact release code on unchanged samples, with raw phase
measurements and native final-binary interaction evidence. Windows/full V1/
CORE10/P100K/physical gestures remain separate unexecuted gates.

Refinement within the same scope: workspace settings queries copy only settings;
metrics reconciliation visits only tracked targets; GUI selection refresh scans
one immutable service snapshot instead of repeating object lookups. f64 object
envelopes update only changed objects and preserve query order. Their union
provides visible bounds with layer/category visibility. The UI and worker share
one immutable index. An in-place display patch is allowed only with identical
ordered IDs, anchor/LOD and primitive/raw-contour layout; otherwise use the
original full builder. No growing append-only display storage is introduced.

Development release measurements (same read-only inputs, macOS arm64): the
original worker workload measured Move/Undo 1155/1164 ms for the 12-layer set
and 1301/1388 ms for the 527227-object project. Incremental refresh measured
87/81 ms and 209/209 ms respectively. Twenty repeated Move/Undo pairs with
1 and 1000 selected line objects restored exact geometry and initial dirty
state; development maxima were 107 ms for the set and 249 ms for the project.
The final b031c31 five-iteration-per-group run subsequently observed maxima
of 112 ms for the set and 516 ms for the project; retain those raw outliers
and do not interpret the development maxima as a latency guarantee.
These are bounded development observations, not formal AT-075/PMIX acceptance
or final native approval. Final clean-commit gates, supplementary full-builder
comparisons and native-binary identity are recorded in versioned delivery
evidence and its review. Structural changes and changed contour layouts retain
the full rebuild fallback and can take longer.
