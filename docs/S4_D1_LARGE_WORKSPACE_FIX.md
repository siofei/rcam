# S4-D1 large-workspace maintenance

User-reported 2026-10-01: importing the complete 12-layer set exhausts display
items and repeatedly flashes disabled/enabled UI text; the large existing project
also stalls during interaction. This is maintenance of S4-D1, not a new stage.

Scope: R03/R05/R09/R11/R16/R17/R18/R19/R20/R21/R22. Regression references:
AT-003/011/030/039/040/068/073/074/075/082/083/084/088/089/091/094.
Allowed modules: editor-app display, viewport scheduling, state, spatial index
and their tests; editor-service canonical content hash buffering; editor-core
analytic point-selection and its regression tests (user-reported tangency error).
No parser support expansion, manufacturing simplification, new dependencies,
project schema changes or D2 capabilities.

User direction on 2026-10-01 explicitly removes fixed display admission limits
and permits slower performance for very large workspaces. Display item count,
per-cell density and estimated pixel work no longer reject otherwise valid
scenes. Index storage adapts to input size; acceleration storage remains an
optimization target. Exact index representation and finite geometry checks
remain correctness requirements. Import/codec/manufacturing/export safety
contracts are unchanged. This is a new product policy, not retroactive PASS
against the old performance gates.

Keep supplied files read-only and hash them before/after. Measure the complete
set and large project using the real Model and renderer. A failed display request
must settle without automatic identical retries, and a changed viewport must
still permit recovery. Preserve exposure order, local holes, hidden geometry
validation, rigid f64 manufacturing bounds and atomic service mutation.

Acceptance: targeted regression and native Metal parity; required locked
workspace gates and release build; real input timings and native interaction
observations recorded separately. Windows, full V1, CORE10 and P100K acceptance
are not inferred from this repair. Evidence uses a new run directory.

## Implementation and supplemental acceptance

Scene acceleration reserves exact raw contours first and shares remaining
storage by geometric workload. Per-contour conservative bounds skip outside
edge scans. Curved subpixel Regions retain an enclosed area. Spatial index
bounds reuse one aperture/block lookup; canonical content hashing uses 64 KiB
streaming buffering with identical digest. Active/name/lock state reuses the
scene. Viewport preparation accepts one queued user operation while keeping
controls enabled; failed identical camera attempts settle.

Development real-input profile (Retina 770x713 logical points, scale 2, release,
macOS arm64): the 12 layers contain 169482 objects and the 13-layer project
contains 527227. Both load and prepare all five 1/2/4/8/16x views successfully.
The set's 2x view estimates 3.46 billion work units and is accepted under the
new policy. Initial loads measured 6.7 s and 9.8 s; point selection 0.8 ms and
12.3 ms; active-layer changes 16-47 ms; Move/Undo 1.3-1.8 s. Undo exactly restores
all layers, apertures and block definitions. These are observations, not a
performance guarantee. Original SHA-256 checks remain equal for all 13 inputs.

New regression covers raw storage above the former 2M threshold, dense cells
above 16384 objects, high estimated pixel work, queued viewport/user commands,
scene reuse and failed-camera recovery, fair contour storage and curved
subpixel area. Frozen 96 cases and acceptance thresholds are unchanged.
Final gates, final binary identity, native observations and package verification
are recorded in the versioned S4D1_LARGE export review. Native/private sample
evidence stays local; no sample geometry is included in distribution.

Limits: Windows not executed; no full V1/CORE10/P100K or idle-CPU acceptance.
Very large scene edits still require rebuilding changed geometry and history
hashing; observed latency remains about 1-2 s. Hardware allocation and exact
index/finite-coordinate requirements cannot be replaced by approximate data.

## Point-selection tangency follow-up

The user's native point press exposed a rounded-pad aperture on a lower layer:
all seven primitives are Dark, but the previous code unnecessarily constructed
an intersection arrangement and rejected a floating-point tangent gap. Dark-only
macros now use their exact primitive union: inside any primitive is material;
outside the union, its nearest primitive edge is the distance witness. Internal
seams are allowed material witnesses, as in the existing boundary contract.
Ordered Dark/Clear macros still use the arrangement and retain numerical
ambiguity refusal. No tolerance is increased and no primitive is discarded.

Regressions compare a near-tangent rounded union with independently calculated
rectangle/circle distances, retain the mixed-exposure near-coincident refusal,
and replay private project press/select coordinates at zero, enlarged and
zoomed tolerances. The private regression is opt-in and does not distribute
sample geometry. Selection never changes manufacturing content or history.

## Subsequent edit-speed maintenance

The user requested faster modification/Undo after the 63db63d delivery.
The later ordered dirty-signature and incremental display/index work is scoped
in S4_D1_EDIT_SPEED and ADR 0045. Its final sample timings supersede the older
1–2 second observations for the covered edits; complex structural operations
continue to use the complete builder. Final identity and native/package proof
are recorded separately in the S4D1_EDIT versioned export review.
