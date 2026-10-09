# S5-I1 near-contour selection follow-up

Baseline: `969ccc36b249b9d30856b9a46b7f7b6d5b37d461`. This is the authorized source follow-up after read-only synthetic diagnosis. Native mouse experience is not yet accepted.

Scope: S5 interaction correction, R07/R09/R16/R17/R18/R21/R22; AT-022/024/025/026/027/028/029/030/031/032/039/043/064/065/067/074/078/089. Allowed modules: analytic hit query, internal service query/policy projection, app selection state, targeted tests and these design/acceptance amendments. No shader, manufacturing geometry, Snap/Alt, point-placement trajectory, dependency, project schema, radius UI, measurement interval or acceptance-threshold change.

## Frozen behavior

- The default selection radius stays **6 physical pixels**: `6 / (camera points_per_mm * pixels_per_point)`. Retina 2x therefore has a three-logical-point radius. Snap radii remain separate. No new preference or settings interface is added.
- Core keeps distance and uncertainty from **one analytic pass**. Legacy `hit_test` still returns IDs in source exposure order with the same point/tolerance validation, numerical ambiguity, unsupported geometry and work-budget failures. The service exposes scores only through an internal Rust query; the public automation operation, DTO and capabilities are unchanged.
- A direct candidate means the old zero-tolerance predicate `distance_mm <= uncertainty_mm`, including positive roundoff distances. Direct candidates keep original top-to-bottom layer and reverse-exposure priority. Exterior near candidates rank by f64 analytic distance; exact distance ties keep original order. No non-transitive epsilon comparator or AABB distance substitutes for geometry.
- GeneratedText is one logical candidate keyed by layer and operation ID, distinct from an ordinary object with the same string ID. Aggregate minimum distance and any direct hit across all eligible members **before** sorting. A direct group takes priority and representative from its first direct member, not an earlier exterior member; this matters for interleaved exposure members. Block remains one atomic candidate.
- Standard holes, Macro local Clear and true arcs keep existing analytic meanings. Points within tolerance of an inner hole wall may select the object, including sufficiently small-hole centers. Deep holes, concave voids and empty space inside a huge envelope do not become hits solely because of bounds. Standalone Clear remains geometrically inspectable. Block uses its existing minimum resolved-primitive geometry distance, including Clear; this is not final composed layer/Block material picking.
- Repeat clicks retain the original anchor's ranked logical candidates and valid representative IDs while within two physical pixels, with identical camera/viewport/DPI/navigation/radius and document/revision/workspace identity and the same eligible logical candidate set. Distance crossings and changes in hit glyphs alone do not reorder that cycle. Membership or context changes rebuild at a new anchor. No time timeout or incremental-anchor drift is added.
- Candidate arrays and logical membership are immutable shared snapshots; View cloning does not copy or scan their contents. Analytic queries, classification and ranking are confined to selection tasks, not frame/update loops.

## Inspection and editable incoming selections

The 969 baseline explicitly allowed locked objects into Ctrl+A and modifier selection, then refused the complete mixed edit. This batch's user instruction requires editable incoming Add/All, so it amends that behavior without changing the service's general `selectable_only` meaning:

- Replace preserves existing visible/selectable inspection, including locked layers/categories and standalone Clear.
- Ctrl Add, rectangle Add and All adds only objects that pass visibility/Solo, layer/category selectable and layer/category unlocked checks. Add eligibility is applied before choosing the first click candidate, so a locked upper/near object cannot conceal an eligible lower candidate.
- GeneratedText origin always classifies as GeneratedText; members of one operation share a layer and category policy. Eligibility therefore applies to the whole operation without per-glyph scans of the whole group.
- Add preserves the earlier selection and its order, including objects previously selected for inspection. It never silently purges old locked members. Remove may remove those existing read-only members and never adds objects. Shift wins over Ctrl and uses the existing empty-result semantics.
- Move/drag and other edits still reject an entire selection containing a locked, hidden or unselectable member. They must not filter a mixed selection and submit only its editable part.

## Gesture and failure boundaries

ProbeDrag keeps its old selected-hit membership and Move admission; it does not advance the click cycle or retarget dragging to the fresh nearest unselected object. The four-physical-pixel drag threshold and press-time click coordinate/mode remain. PointMove retains its launch/confirmation/back-release input ownership, final preview/application and exact Alt target behavior.

Queries and collection check cancellation at task, object/resolved-primitive, lookup/membership and final publication boundaries. Whole-query errors publish no partial selection/cycle. Block resolution, Macro arrangement preparation and existing category scans retain their previous internal work/cancellation granularity; this batch does not claim a hard cancellation-latency bound or a new allocation-peak guarantee.

## Required regression matrix

| Area | Synthetic CPU checks |
|---|---|
| Ranking | Near lower versus farther upper; direct versus exterior; direct overlap old layer priority; equal distance reverse-exposure ties; interleaved text direct priority and group minimum distance |
| Geometry | Standard holes/transforms, Macro local Clear/numerical ambiguity, Gerber Region concavity/cut-ins, true arc sweep/endcaps/full circle/deviation, long diagonal/huge envelope, rotated/mirrored atomic Block gaps/holes/Clear |
| Scale | Six DPI values and three zooms; below/at/above radius using actual screen-to-world input; no widening of six-pixel admission |
| Cycle | Distance crossing under anchor jitter, glyph representative change, candidate membership change, radius/camera/rect/DPI/navigation/doc/revision/workspace reset; all candidates/wraparound |
| Policy | Hidden/Solo/category filters; layer/category locks; eligible Add behind locked candidate; all-locked All; whole text; existing inspection preserved; Remove read-only; mixed Move atomically rejected |
| Input/async | Ctrl/Shift priority, no Probe advance, covered lower selected drag, delayed probe/release, cancelling/stale selection; PointMove launch/held-press/target/Alt/final request ownership |
| Resources | Legacy/scored ID/error equivalence; nonfinite/invalid parameters, uncertainty/unsupported and budget rejection; no partial state after cancellation; unchanged10k candidate cap (10k accepted,10k+1 atomically refused), shared candidate clones and80k selection View clones |

Run locked Rust 1.89 ordinary CPU tests, fmt/Clippy on supported cloud targets, headless automation compatibility and portable checks. Auxiliary Linux app tests require an isolated preferences adapter; they are not a native product build. Mac ordinary build/input/Metal validation remains with the local task after connection recovery. Keep every initial failure and corrective rerun under the new delivery identity; do not relabel prior native or performance evidence.
