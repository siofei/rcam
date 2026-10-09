# S5 click-placement Move: safe pointer pause and re-entry

Source candidate; ordinary native interaction validation pending. Scope: S5-M2
Move follow-up, R08/R09/R10/R11/R16/R18/R19; local coverage of
AT-022/030/032/039/040/064/065/069/074/075. Allowed changes: app Move placement,
the two point-input cancellation entrances, ordinary synthetic regressions and
these design/acceptance records. Manufacturing data, service mutation APIs,
project schema, shaders, dependencies, Snap resolution and Alt meaning stay
unchanged. This follows the halo source candidate as a separate change.

## Contract amendment

PointerGone by itself can pause an unfrozen click-placement Move. Only a
Preparing or Following session whose document/project/selection/rule/precision
context remains valid and whose window remains focused qualifies. Keep the
session, manufacturing base B, latest target and immutable preview template.
Keep the outstanding initial read-only admission task; its matching reply may
supply B and advance Preparing to Following while tracking remains paused.
Pause clears owned target-button state and Snap runtime state. It neither
freezes a target nor submits a final preview or manufacturing edit.

The entire input batch containing PointerGone is barred from confirmation,
regardless of event order. A later batch with an actual pointer-position event
inside the current canvas can resume tracking; cached hover alone cannot. All
presses/releases in that re-entry batch are discarded. A subsequent batch must
provide a fresh primary press inside the canvas, then its owned release inside
the canvas, before the target can freeze. An old release or held-button state
does not authorize confirmation. Repeated PointerGone restarts this barrier.
The barrier uses the existing egui frame identity, shared by all passes of one
input batch, and survives a discarded layout pass. The paused HUD instructs the
user to return to the canvas and click again; the canvas/HUD container remains
fixed and viewport constrained.

Actual focus loss, WindowFocused(false), explicit Esc/right click, an invalid
context, project transition, worker error/disconnection and existing modal/tool
cancellation paths still retire the session. Esc or a secondary-button event
in the Gone batch prevents pause. Frozen, FinalPreview, Ready and Applying do
not qualify for pause and retain their original pointer-loss cancellation and
authoritative terminal behavior. A legitimate TooLate commit still installs
its service result and creates one Undo; a retired read-only reply cannot
restart input. Numeric point tools, drag, Grip, Block and selection-read loss
policies are unchanged.

Both early point-input arbitration and the later update cancellation entrance
use the same pause qualification. Context is checked again after asynchronous
reply installation and before canvas input. Pause is input ownership, not a
replacement document version or a service task state.

## Verification and native handoff

Ordinary real App/raw-input/serial-worker regressions exercise both event orders
of Gone plus release/re-entry, discarded egui passes, old presses/releases,
late initial admission, repeated exits/re-entries, focus and explicit cancel,
version/project invalidation, clean/dirty project transitions and retired or
late terminal replies. Following/paused input cannot enqueue per-hover workers
or manufacture a transaction; a later fresh click still needs the actual final
preview and one authoritative Apply. Existing numeric/drag/point/input, task
identity, geometry, Undo/Redo and layout regressions remain applicable.

Mac normal interaction acceptance must use this candidate's exact source and
binary identities. Check: Preparing and Following leave/re-enter with and
without a held button; old release and first re-entry click do not commit;
the next fresh click commits the displayed resolved target once; Esc/right
click/real focus loss and project switching cancel safely; Snap/grid/Alt remain
correct; async admission/error and late Apply behave as documented; the paused
HUD and ghost fit small/high-DPI canvases. Record failures without redefining
input windows or reusing the previous candidate's evidence identity.

The inherited observer still records Preparing/Following as the task phase;
this amendment does not add pause fields or change its schema. A PointerGone
record no longer implies a Move exit in an eligible session. Such interrupted
pointer-tracking windows cannot silently become complete performance trials.
Actual present and overall performance remain OPEN, including the user's goal
of being as smooth as practical at 144 Hz. This source task introduces no forced
60 Hz/full12 gate or new frame-time threshold and claims no native acceptance.
