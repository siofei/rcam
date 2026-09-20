# S4-A2.2 — multiline text, stroke font and menu interaction

Mac-first follow-up, 2026-09-20. Scope / R / AT mapping: [plan](S4_A2_2_PLAN.md).
Windows is deferred / not executed; no dual-platform V1 claim or Global Units work.

## Implemented behavior

- Insert → Text opens the exclusive modal. Font dropdown defaults to an original
  RCam printable ASCII centerline font; Chinese requires a local outline font.
  No CircuitCAM or third-party glyph data were copied, and no font files ship.
- Multiline editor uses Enter for newline. Confirm in mouse mode freezes preview
  and hides the dialog. Esc restores that same draft; Cancel discards it. A canvas
  click creates the complete block once, through ApplicationService and one Undo.
  Read-only preview work no longer disables text input while typing.
- Baseline spacing 0 means 1.3 × height; explicit spacing, stroke width and existing
  block alignment/rotation are supported. ASCII produces finite-width Lines;
  outline fonts retain the existing contour/Line/Arc Region manufacturing path.
- Delete is in Edit alongside Duplicate/Undo/Redo and transform/property entries.
  Existing selection/measurement, layer controls and grid/view settings have menus.
- No dependency, writer precision, 128-character limit or offset threshold changed.

## Native defect found and corrected

The first native run produced Chinese geometry but retained an old canvas frame.
A 1 mm glyph contained a shallow fitted arc with a large radius; the renderer used
its full-circle envelope, expanding one glyph's bounds to roughly 149 × 149 mm.
Estimated sample work became 4,534,485,894, above the unchanged 2,000,000,000 limit.
The fallback path also cleared the display error, hiding the reason from the user.

Region display bounds now reuse manufacturing arc-sweep bounds, rounded outward
for local f32 display coordinates. They remain independent of tessellation/zoom.
The same regression estimates 540,288,928 and renders successfully; the resource
limit is unchanged. Fallback frames retain their error message. A regression covers
stroke create → Undo/Redo → multiline Chinese create and tight glyph bounds. Native
Metal reference/production parity includes an independently constructed shallow arc.

## Actual checks and evidence

Development evidence: `evidence/s4-a2-2-20260920/`. Preserve the original failure in
`display-repro.log` / `display-repro-detail.log`, correction in `display-fixed.log`,
and native logs `native/refined-launch.log` / `native/display-fixed-launch.log`.
`native/observations.json` binds the tested native binary and exported Gerber hashes.

Computer Use inspected the native macOS window: multiline ASCII typing, default
font selection, Confirm → Esc → same draft → placement, one Undo/Redo, and Edit/Insert
menus. After the display fix, Arial Unicode `中文\nAB` at 1 mm visibly renders,
Undo/Redo works, Save As succeeds, and the exported file reopens with seven objects
(three source objects plus four text Regions). Text overlapping existing Dark
material has the same layer color; its selection outline exposes the glyph boundary.
Current Chinese input used clipboard, not a new IME candidate test. Earlier IME
results are historical, not relabelled as current evidence.

The old test window was preserved: automatic approval rejected terminating its
process because of unsaved state. A separate `RCam S4-A2.2 Fixed.app` performed the
successful verification. No original sample was overwritten. Native screenshots
were inspected in the task conversation; no desktop/private picker captures ship.

Required clean-source command:

```text
python3 scripts/run_s4a1_gates.py --stage S4-A2.2 --extra-service-test s4a2_text_preview --out evidence/s4-a2-2-final-20260920
```

The resulting `gates.json` is authoritative for commit identity, commands, exit
codes and PASS/FAIL. It includes fmt/check/clippy, workspace and explicit service
contracts/workflows, dependency tree, release build, manifest/package audits and
native Metal parity. Multiline service tests independently check preview/create
identity, one transaction, Undo/Redo, export/reparse and failed-input atomicity;
text tests cover all 94 printable ASCII glyphs, Chinese rejection, width/resource
limits, baseline spacing, rotation and positive/negative outline offsets.

Physical trackpad/long-soak, Windows and full 96-case acceptance are not executed
by this follow-up. The original ASCII font is a compact engineering stroke font,
not CircuitCAM's proprietary font. Normal Gerber retains geometry, not editable
text/font intent; glyph groups are app-session data.
