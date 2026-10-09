# Pending keyboard navigation and same-definition selection

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
