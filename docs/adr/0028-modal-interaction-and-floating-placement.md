# ADR 0028 — Exclusive parameter dialogs and floating text

Date: 2026-09-20. Scope: S4-A2.1, R09–R13/R16/R18/R21/R22;
AT-031–034/037/040/043–052/054/063–065/087–095.

One Option<ActiveModal> owns manufacturing parameter interaction: Text, Move,
Rotate, Mirror, Flash, Grid, and layer Rename. Native file chooser and the existing
save metadata/dirty-close confirmations are serialized with this owner. Opening a
close confirmation cancels text placement and closes parameter UI. Background
panels, dropped files and destructive keyboard actions are disabled during dialogs.
Draft values call the existing ApplicationService worker only on Apply. Invalid
input/service errors remain in the dialog. Successful mutation closes it. Cancel
and Escape discard the active draft/preview without manufacture changes. Cancellation
of an outstanding read-only preview invalidates its generation. Committed requests
are not falsely advertised as cancelable.

Centered egui::Modal provides dimming and focus ownership. The scroll area adapts to
small windows. Enter supports safe Apply; current and event-frame IME composition
block submission, including a composition commit in that same frame. Preview
scheduling also waits until that event reaches TextEdit, preventing worker busy
state from disabling the editor before its candidate commit. Text editors
retain their ordinary clipboard/selection/undo shortcuts. Native IME is a separate
acceptance item and cannot be inferred from the event-injection test.

Text entry is in the toolbar and Insert menu. Mouse mode's Start Placement freezes
font identity, content, size, offset, tracking, alignment, rotation and tolerance.
It hides the dialog, releases text focus, and uses the immutable preview with a
translated anchor. Pointer updates do not access fonts or invoke text generation.
Snap uses the independent text anchor switch and current validated grid spacing.
The displayed anchor is passed unchanged to text.create on a canvas left click.
One worker transaction creates/selects the group and returns to Select; busy state
blocks a second commit. Pan/zoom remain available. Escape/right click, tool switch,
layer/document/revision invalidation, or close cancel the floating preview.

Absolute and Relative apply inside the dialog. Relative reference picking briefly
returns to the canvas and reopens the same draft dialog after the click. Text preview
is an outlined manufacturing boundary overlay; it does not fill holes with background
color or replicate slab meshes.

Direct Select/Pan/Zoom/drag/marquee/Measure/Undo/Redo/Delete/Duplicate/Fit and grid
visibility remain direct operations. No Web UI framework or new dependency is added.
