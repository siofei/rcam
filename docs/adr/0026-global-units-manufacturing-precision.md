# ADR 0026 — Display units and document export precision

Accepted for the requested S4 Global Units implementation, 2026-09-20.

Manufacturing data and API length DTOs remain f64 mm; areas remain mm². DisplayUnit
(mm, inch, mil, µm) is a session workspace preference. Auto digits are derived from
ceil(-log10(resolution_mm / mm_per_unit)), clamped to 0–15. Area scales by the square
of the conversion factor. Suffixes override the current input unit. Numeric edits
are converted exactly once before ApplicationService submission.

Only the exclusive Units modal changes unit preference. Other parameter modals
and their focused fields cannot change units underneath an edit. Retained text
fields display the new unit, with canonical mm cached until their text changes,
preventing round trip drift or reinterpretation; invalid unfinished drafts block the change with an
error. Display preferences do not change revision, dirty, history or writer bytes.

ManufacturingPrecision defaults to 0.0001 mm. Its policy belongs to the service
record, not the UI. Changing it advances revision to reject stale previews/exports,
leaves content dirty and undo/redo unchanged, and independently changes
export_policy_dirty. A successful export saves both baselines; failure saves neither.
Close checks both dirty states. This policy is session-local until a project format
exists; Gerber reopen defaults the policy (geometry persists, editor policy does not).

FS 6.6 remains the encoding grid (0.000001 mm), independent of the requested
manufacturing grid. Custom resolution is 0.000001–1 mm in integral FS ticks;
unsupported finer/nonrepresentable grids are rejected, never silently rounded.
Quantization uses nearest, ties away from zero, symmetric signs, finite arithmetic
and a grid index below 2^52. Working geometry is not repeatedly rounded; export
normalizes a clone including aperture dimensions, macro primitives and coordinates.
Region edge collapse and arc topology changes fail before publication. Existing
semantic validation and writer/reparse comparison remain mandatory and unchanged.
Coarser precision does not license deleting small features or weakening topology.

Precision is not the floating point calculation precision, display digits, FS digits,
or Bezier tolerance. GUI text derives its internal tolerance from 2.5 × resolution,
clamped to the existing text range; explicit API text tolerance remains supported.
Contour/Line/Arc geometry, local holes, multiline, stroke font and one-transaction
floating placement remain production paths. No slabs, mesh reconstruction or new
format dependencies are introduced. Windows remains deferred; V1 gates unchanged.


Text generation uses the document's relative manufacturing lattice before the
existing contour Line/Arc fitter. Its bounded arc candidate search and 1e-7 mm
radius agreement bound remain unchanged; a candidate that cannot certify a safe
arc keeps the source boundary Lines. No existing manufacturing arc is flattened.
A common placement translation stays f64 and is normalized only on export.
Changing policy to a coarser lattice after text creation can safely reject an
otherwise valid high-precision contour; regenerate text under the new policy or
select a finer precision. Imported arcs are never refitted or silently flattened.
