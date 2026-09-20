# S4-A2.2 — CircuitCAM-style text interaction

User-requested follow-up to S4-A2.1, Mac-first. R09/R11/R12/R14/R16/R18/R21/R22;
AT-031–034/040/046–052/063–065/087–095. Allowed: editor-text, editor-service text
preparation, editor-app text/modal/menu state, associated tests and documentation.
Native follow-up includes editor-app display Region envelopes and retained display
errors: shallow font arcs must use their actual sweep bounds, not full circles.
The existing 2,000,000,000 sample-work limit stays unchanged.

Add an original built-in printable ASCII centerline font (no CircuitCAM font/code
copied), default in the font dropdown. It produces real finite-width manufacturing
Lines; Chinese requires a local outline font. Add multiline input/layout, explicit
baseline spacing and stroke width. Keep the 128-character resource limit and
existing outline tolerance/offset safety checks. No new dependency or global units.

Confirm freezes preview and hides the dialog in mouse placement. Escape returns to
the same draft dialog, with no manufacturing change; Cancel discards placement.
One canvas click creates the entire text once, with one Undo. Enter in the multiline
editor inserts a newline; it must not apply. Insert owns Text; Edit owns Delete and
existing object operations; View/Tools/Layer expose their existing functions.

System-font multiline height is the shared visible glyph height before arranging
baselines, not the total block height. Alignment and rotation apply to the whole
block. Zero baseline spacing means automatic 1.3 × height. Built-in stroke width
must be positive and less than height; zero-width manufacture is rejected.
