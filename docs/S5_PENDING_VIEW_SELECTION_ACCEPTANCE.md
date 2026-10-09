# Pending view, selection and definition workflows

Status: PLANNED / NOT IMPLEMENTED / NOT TESTED. This documentation-only supplement records the user's 2026-10-09 requests. It adds no command, default factor, shortcut or capability to the current product. The platform-standard Select All correction is a separate source candidate; multi-project session isolation and tabs/batch-opening retain their phased order.

Scope: future S5 follow-ups, R08/R09/R16/R17/R18/R19/R22; existing AT-025/043/064/065/067/074/078/089 inform compatibility, but these supplemental scenarios do not claim those cases passed or reuse their IDs. Manufacturing geometry and history are protected; future commands use the shared command/keymap/macro boundary. Windows/native support and acceptance remain separate.

## PgUp/PgDn navigation

PgUp is the customizable default for zoom in and PgDn for zoom out. Each action starts from the current camera scale: multiply by the configured k, or divide by that same k. k=1.2 and two steps=1.44 is an example only. The user did not choose a default factor or zoom anchor; freeze those in the later contract consistently with existing navigation, finite k>1 validation (reject k=1, 0<k<1, nonpositive or nonfinite values), camera limits and boundary behavior. Store the factor as an app/view setting, with the fixed-width settings-window and stable control-rectangle rules.

| Case | Planned check |
|---|---|
| NAV01 | Current scale s, two in actions yield s·k² rather than s·k or original-scale k; corresponding out actions divide current scale |
| NAV02 | In/out use identical k and are inverse within frozen camera range/tolerance; clamping/boundaries explicitly tested |
| NAV03 | Nonfinite, zero, negative, k≤1, invalid text and extreme factors safely handled; current camera and manufacturing state unchanged on refusal |
| NAV04 | Setting persists and restarts; changing k takes effect on the next command; default factor remains an implementation design decision |
| NAV05 | Default/custom/cleared bindings, reserved-key and collision checks, both platform maps and real key lifecycle |
| NAV06 | Text inputs, IME, modal and scrollable-list ownership use PgUp/PgDn for their own navigation; no accidental canvas zoom |
| NAV07 | View commands preserve manufacturing bytes, dirty baseline and manufacturing Undo/Redo; pan/zoom/DPI/cursor-anchor compatibility |
| NAV08 | Macro invokes the typed view commands with explicit factor and scope; no simulated keys or implicit initial-scale dependency |
| NAV09 | Settings short/long/invalid/wait/error values and small viewport keep window and key controls stable and reachable |

## Same-definition selection

Seeds are the actual selected eligible Aperture/Block instances. Match the union of their real definition identities with explicit definition kind and document/definition namespace. Target layers are only layers containing those eligible seeds; they must be effectively visible and selectable. In those layers, select eligible instances matching any seed definition. All existing definition/type/layer/category/object locks and incoming selection rules remain. Same DCode text, display class or similar shape does not establish identity; Aperture and Block kinds never cross-match. Ordinary selected geometry contributes neither definitions nor target layers. No valid seed means disabled/explicit no-op, never fall back to All.

A single seed expands only its source layer. Seeds from top and bot expand only top/bot even if a third layer contains otherwise matching instances. Multiple definitions union and deduplicate in stable layer/exposure order. Hidden/unselectable/locked or deleted seeds/results follow the original eligibility policy without unlocking. Explicit Add may retain prior ordinary/inspection selection according to its existing contract, but those retained items do not contribute new scope; Replace installs only compliant matches. The entry's default Add/Replace mode is a later contract decision, not a user-confirmed choice.

Menu/right-click/customizable shortcut/macro route the same typed command. Macro records document/version, seed definition and target-layer sets and explicit mode; it does not depend on the current mouse or implicit active layer. Selection is transient, with owner/document/revision/workspace/selection fences and authoritative terminal synchronization. No manufacturing transaction or dirty change.

| Case | Planned check |
|---|---|
| SAME01 | Single top seed expands top only; single bot seed expands bot only |
| SAME02 | Top+bot selected seeds: union within those two target layers; third layer excluded; unmatched geometry stays excluded |
| SAME03 | Multiple Aperture definitions / multiple Block definitions union and deduplicate; deterministic layer/exposure order |
| SAME04 | Same DCode but different actual definitions do not merge; shared real identity may match across permitted target layers; equal shape with different identity does not |
| SAME05 | Mixed Aperture/Block kinds remain distinct; transformed appearance does not redefine the original definition/type lock |
| SAME06 | Effective visibility/Solo, selectable and original layer/category/definition/object locks gate seeds and results without unlocking |
| SAME07 | Mixed ordinary geometry adds no definition or target layer; empty/invalid/deleted-only seed set is explicit no-op/refusal |
| SAME08 | Explicit Add retains prior selection by its contract; Replace has only compliant matches; whole-text grouping and duplicate instance handling |
| SAME09 | Menu/shortcut/right-click/macro share typed scope and mode; input/IME/modal ownership and custom conflicts |
| SAME10 | Cancel/late terminal/version/selection changes and another project owner cannot install a foreign expansion |
| SAME11 | Full large selection is not truncated, no per-frame whole-ID clone/sort added; manufacturing bytes/dirty/Undo unchanged |

These scenarios are requirements for future implementation and exact-candidate native checks. This document is not implementation evidence or a change to existing performance gates.

## Independent shape-equivalence mode

The user explicitly requires translation and arbitrary-angle rotation invariance, including30°; fixed-orientation bounding boxes and90° bins are insufficient. Geometry-equivalent but differently identified definitions may match in this mode, while the same-definition command above continues to require actual identity. Preserve real contours, arcs, holes and topology with a later frozen tolerance. Size normalization and mirror equivalence are separate suggested options, whose existence/default values were not separately confirmed by the user. Define target scope explicitly and retain seed-layer limits if reusing the same-definition entrance.

Candidate algorithm: conservative rotation-invariant summaries eliminate impossible candidates, then enumerate rigid-transform hypotheses and verify full f64 manufacturing contours/holes/topology under an arbitrary-angle transform. Area/perimeter/centroid summaries are insufficient acceptance tests. Freeze exposure/contour semantics and error bounds before implementation; never derive manufacturing shape from raster/tessellation. Treat symmetric multiple solutions and continuous circle symmetry deterministically; candidate budgets, cancellation and owner/version fences remain.

| Case | Planned check |
|---|---|
| SHAPE01 | Translation plus0°,30°,17.5°, negative and arbitrary angles match identical real shape; rotation is not limited to90° |
| SHAPE02 | Same area/perimeter/centroid or bounding box with different contour/holes/topology fails exact verification |
| SHAPE03 | Different definition IDs with equivalent geometry match only in shape mode; same-definition mode remains identity-based |
| SHAPE04 | Symmetric discrete/continuous angle solutions, cyclic contour anchors, hole correspondence and deterministic selection |
| SHAPE05 | Frozen tolerance inside/outside boundaries, small features and real arc/contour precision; no manufacturing rounding change |
| SHAPE06 | Size and mirror options independent, disabled/enabled behavior defined by later contract; no fabricated confirmed defaults |
| SHAPE07 | Large candidate sets, conservative coarse filtering, exact verification cost, memory/work budget, cancellation and late foreign-owner replies |
| SHAPE08 | Explicit scope/mode/tolerance/options in macros; source definitions/manufacturing bytes/dirty/Undo unchanged |

Shape matching remains PLANNED / NOT IMPLEMENTED / NOT TESTED. Algorithm selection and acceptance thresholds belong to its later implementation phase.


## Aperture creation and shared definition editor

User-confirmed additions: standard-parameter Aperture creation and selected-contour conversion to a custom Aperture, sharing the independent definition editor. Current imports/standard Flash size COW do not constitute either creation entrance. Existing definition editing defaults to changing the original definition/all references, with an explicit create-copy/selected-only alternative; creation retains a separately explicit source-retention/replacement choice whose default is not frozen. Ordinary Aperture, RCam reusable Block and unsupported Gerber %AB remain distinct.

| Case | Planned check |
|---|---|
| AP01 | Standard C/R/O/P dimensions, polygon orientation/count and local holes validate finite geometry; invalid parameters preserve original state |
| AP02 | Selected real contours/arcs/holes and Dark/Clear convert only when representable and geometrically proven equivalent; unsupported composition preserves sources with explanation |
| AP03 | Created definition has scoped identity, instances share the intended definition; equal DCode text never merges separate source identities |
| AP04 | Existing edit defaults to original definition/all references with impact count including hidden references; explicit copy/selected-only path remains distinct from creation |
| AP05 | Type/definition/category/layer/object locks and hidden references preserve permissions; no implicit unlock or visible-only partial edit |
| AP06 | Preview/cancel/apply/failure and exact Undo/Redo maintain source geometry, references, order, dirty baseline and cache invalidation |
| AP07 | AP/Macro/Block references, cycles/nesting and unsupported Gerber %AB safely reject or follow a separately admitted capability; no automatic scope expansion |
| AP08 | Export/reimport independent manufacturing contour/exposure truth, budgets, cancellation, owner/version fences and no private geometry upload |
| AP09 | Shared typed create/edit service and serializable macro parameters; no simulated screen coordinates or claimed current API |

## Match and replace with a newly created Aperture or Block

User-confirmed behavior: create a template definition, then replace existing geometry matching that complete template. In full-composition mode the two-circle Block example requires actual size/shape and pairwise offsets; arbitrary neighboring circles are not sufficient. Final-contour mode instead compares the complete effective Dark/Clear region and topology and permits different internal composition. Full arbitrary-angle rigid rotation and translation are admitted without implicitly allowing scaling/mirroring. Preview/highlights/counts, source-layer default with explicitly selected additional target layers, skip-ineligible results and conflict UX are proposed design details, not individually user-confirmed defaults. This explicit scope applies only to match/replace and cannot broaden seed-layer-only same-definition selection.

Coarse invariants only reject candidates. Exact f64 contour/arc/hole/topology equivalence and ordered Dark/Clear effect with intervening untouched geometry must establish safe substitution. Full-composition mode additionally verifies all template members/relative transforms/overlap/exposure relationships; both modes completely account for every consumed source object. A Block's single instance/exposure position cannot represent every noncontiguous source group: refuse/preserve candidates when flattening fails equivalence, retaining unmatched relative order. Replace is one atomic Undo; whether creation and replacement share that transaction is a later design choice.

| Case | Planned check |
|---|---|
| REPLACE01 | 100 circles with a two-circle Block template: full-composition mode only replaces complete size/shape/relative-distance matches; unmatched single circles remain |
| REPLACE02 | Translation and 30°,17.5°,negative/arbitrary rigid rotation retain location/direction and full shape; near-pair or wrong-distance combinations fail |
| REPLACE03 | AP exact real contour/arc/hole/exposure matches; same summaries with different contour/topology fail; no unrequested scale/mirror substitution |
| REPLACE04 | Source-layer default and explicit other-layer proposal is frozen later; same-definition fast selection still strictly excludes third layers |
| REPLACE05 | Preview highlight/count/template/version and confirmation agree; no changes before confirmation; locked/unselectable/type-locked candidates are skipped and reported |
| REPLACE06 | Overlapping candidate groups report conflict; explicit deterministic strategy consumes each original object at most once; duplicate/partial-member matches refuse |
| REPLACE07 | Ordered Dark/Clear, local AP holes, Block member/outer exposure and intervening untouched objects preserve manufacturing effect; nonrepresentable noncontiguous groups remain unchanged |
| REPLACE08 | Unmatched objects retain relative exposure order; complete text-group and other structural admission rules remain; no selective destructive truncation |
| REPLACE09 | Replace is one Undo transaction with exact source objects/order/definition refs/dirty restored; creation transaction grouping is explicitly decided later |
| REPLACE10 | Template/reference version, deleted definitions, cross-project owner, changed source revision/selection and late preview cannot commit |
| REPLACE11 | Cross-layer groups, Macro/Block references, cycles/nesting and unsupported %AB are safely refused until separately admitted; permissions never bypassed |
| REPLACE12 | Atomic failure/precision/resource budget/cancellation preserve all sources; unproven equivalence explains refusal without partial consumption |
| REPLACE13 | Source vs replaced export/flatten/reimport independently agrees in f64 manufacturing contours/exposure; no raster/tessellation acceptance |
| REPLACE14 | Macro serializes template ID/kind/version, explicit final-contour/full-composition mode, target layers, tolerance, rigid rotation, conflict/failed-candidate policy; no mouse/screen recording or private upload |

Both workflows remain PLANNED / NOT IMPLEMENTED / NOT TESTED. Public command names and capability boundaries require their later implementation contract; this supplement changes no product code or acceptance threshold.


## User-confirmed final-contour versus full-composition modes

Mode A evaluates the effective solid region, holes and topology produced by original ordered Dark/Clear composition. Member count and segmentation need not agree. Mode B additionally requires equal member count, each real member shape/size and relative transforms, overlap and exposure relationships. Both admit translation/arbitrary-angle rotation, without comparing absolute position. A changes internal composition and must make this explicit in preview. No default mode has been chosen. In both modes isolated region equality is insufficient: prove substitution preserves surrounding ordered exposure effects, otherwise retain sources and report refusal. Keep identity selection, shape query and these template replacement modes distinct; serialize the exact mode in macro parameters.

| Case | Planned check |
|---|---|
| MODE01 | One large rectangle vs two overlapping template rectangles: A matches effective region, B rejects different composition |
| MODE02 | Multiple alternative segmentations with same contour/holes/topology match A; B verifies member count and individual size/shape/relative transforms |
| MODE03 | Same contour but different member overlap/exposure relationships: B rejects; A still requires full effective region and contextual manufacturing equivalence |
| MODE04 | Holes, positive/negative Dark/Clear, intervening exposures and external geometry: isolated equality never bypasses surrounding-effect verification |
| MODE05 | Both modes support 30°,17.5°,negative/arbitrary-angle rotation plus translation with frozen tolerance; absolute positions do not define shape identity |
| MODE06 | Multi-candidate shared members trigger explicit conflicts, deterministic handling and no duplicate consumption in either mode |
| MODE07 | A preview states internal-composition change; B preview states complete-composition matching; explicit mode survives typed macro serialization/replay |
| MODE08 | Nested/reference identity equivalence is separately frozen; scoped definition identity/DCode distinctions, unsupported nesting rejection and exact atomic Undo remain |

This user correction supersedes any universal member-count/structure requirement above: that requirement belongs to B only. Both modes remain PLANNED / NOT IMPLEMENTED / NOT TESTED.
