# ADR 0027 — Text manufacturing contour Regions

Date: 2026-09-20. Scope: S4-A2.1, R04/R05/R12/R14/R16/R21/R22;
AT-015/047–052/054/055/063–065/087–095. Supersedes the production slab
representation of ADR 0024/0025, not their safety or total error thresholds.

## Decision

Vertical slab text geometry is retired from production. The old slab helper is
compiled only for historical unit comparisons; a separate baseline checkout is
used for the old/new release benchmark. No slab writer or renderer reconstruction.

Static font outlines are flattened with the existing convex-hull distance bound
at 0.4 × requested curve tolerance, resolved using the existing integer NonZero
material union, optionally offset, rotated, then refitted with Line/Arc edges.
The refit tolerance is 0.5 × requested tolerance. Original line segments are kept;
exact collinear segments and duplicate vertices are reduced. Curvature sign changes,
sharp corners, degenerate circles, and unproven joins stop an arc fit.

A candidate circular arc is certified against every source polygon segment:
its radial maximum is at a segment endpoint; its radial minimum is the center's
projection onto the segment. Angular monotonicity and sweep <= pi establish
bidirectional coverage. This is an analytic bound, not dense sampling as proof.
Dense independent quadratic/cubic/near-line/inflection tests supplement the proof.
Greedy extension combines co-circular samples; a short original line separates
successive fits so independently rounded annuli do not introduce tangent ambiguity.
There is no claim of a globally minimum-node optimization.

Fitting occurs after the frozen rotation, on the existing writer's 1 nm coordinate
lattice. A bounded 7x7 lattice-center search minimizes radial deviation, requiring
<= 0.1 nm, followed by the full segment certificate and analytic join checks.
This is text representability preparation, not a new global precision setting.
Writer FS66, core EPSILON, parser limits, and the 0.001 mm total gate are unchanged.
The anchor is a final common f64 translation: changing only X/Y never changes
fitting, topology, edge count or font access. Writer reparse remains authoritative;
unrepresentable output is rejected before file replacement.

Maximum allocation at the default tolerance (mm): parser 0.00025, flatten 0.0001,
refit 0.000125, height normalization 0.00008, offset approximation 0.0001,
integer union rounding 0.00000003, lattice 0.000000708, radial envelope 0.0000001,
writer 0.000000708, floating reserve 0.00025; total 0.000906546 < 0.001.
The per-curve allocation does not include the already separate offset budget.
No legacy threshold, input/resource budget, or failing sample is removed.

## Local holes and material components

Existing Gerber semantics independently fill each contour. RegionRole::Hole must
not be used as implicit subtraction. Clipper's oriented material boundaries are
classified into solids/holes. Holes attach by horizontal visibility rays at an
interior edge point, avoiding vertex-level degeneracy. Connectors are exact retraced
pairs and remain local to the Region. There is no Clear exposure.

One connected material component becomes one Region object. This is the explicit
fallback allowed by the task: independent components can have overlapping bounds,
which current multi-contour GeometryMetrics cannot certify as a union. Text group
identity is still one generated operation and one Undo transaction.

Core envelope validation now recognizes an already-proven cut-in attachment as the
same endpoint contact permitted for adjacent edges, within the unchanged EPSILON.
Envelope membership is evaluated on the candidate edge parameter, not off-edge
near-contact points from the tolerant intersection helper. Interior intersections
and ambiguous topology still reject. A regression verifies both legal attachment
and an illegal crossing. Final validation runs after fitting and translation.

Metrics remove retraced seams before integrating real material boundaries. The
square-with-hole regression independently expects area 12 mm² and perimeter 24 mm.
The UI says object manufacturing boundary perimeter sum (including holes), not the
Boolean perimeter of overlapping text groups or final layer exposure.

Nodes mean boundary edge endpoints and arc center identity. Objects/Contours/Edges,
not GPU triangles or slab count, describe text complexity. The preview draws the
same manufacturing boundaries, with local holes visible and display-only arc
sampling; it is a wireframe preview and never an export source.
