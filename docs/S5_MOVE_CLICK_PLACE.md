# S5 click-placement Move follow-up

Independent source candidate after the selection-input batch. Scope: S5-M2 follow-up, R08/R09/R16/R17/R18; AT-022/030/032/039/040/064/065/067/069/074/075/078. Allowed changes: app point-transform/input/command and asynchronous project lifecycle, command catalogue, ordinary synthetic regressions and these design/acceptance records. No service mutation API, manufacturing schema, shader, dependency or toolchain change.

## User flow

The shared `object.move_place` command is available as **移动（点击放置）** alongside the existing numeric **移动…**. It has no new default key binding; users may assign one through the existing shortcut settings. Old configurations acquire the missing unbound command using existing schema v1 migration; the historical 41-command fixture remains unchanged.

Start with an editable selection. The serial worker checks existing Move capacity and the complete manufacturing geometry. Its exact f64 bounding-center supplies the common base B; unavailable/stale optional area statistics cannot substitute zero. After admission, the selection display follows the resolved pointer target T without holding a button. Click one canvas target to freeze it, validate that actual moved request, and commit one atomic Move through ApplicationService. Numeric Move and the existing drag interaction remain available.

The point policy reuses the existing analytic contour Object Snap, grid, selected-object exclusion and Alt suspension. The target click uses its release event's position and Alt state. Formatting does not round-trip the stored world point. The actual displacement is T−B in f64; display paths never supply manufacturing geometry. A click during initial admission freezes its target and scale and waits for the exact base before constructing the moved request. Once frozen, further pointer/Alt/camera changes cannot change the operation or trigger another commit.

## Display and task ownership

One zero-delta worker preview, including the existing complete validation and resource admission, provides only an immutable display template. Following translates that template for painting; no hover creates a transformed geometry vector or dispatches a PointPreview. Existing large-preview simplification and bounded contour generation remain. The actual moved final preview is required before PointApply and replaces/releases the zero template. This display optimization is not proof that the translated geometry is legal.

Point Context additionally holds independent ProjectId and a strong immutable selection snapshot. Explicit storage identity comparisons preserve O(1) validation and prevent reused-address identity errors; original document/revision/workspace/rule/precision/generation/selection-epoch checks remain. Request comparisons fast-path the shared group Arc. The worker still checks actual groups, permissions and geometry, and revalidates at its atomic service mutation boundary.

The placement task owns its ID, exact request, preview/apply phase, cancellation and the first deferred project transition. Replies must match task receipts, current source context, project/document and expected read-only result identity. Retired previews cannot install a full View, restart placement or authorize Apply. Unknown Apply results or disconnection block editing until reopening; deferred transitions are not executed against unknown terminal state. Ordinary Open/New/Close/Quit waits for a matching terminal point task and then follows existing dirty/save/discard policy.

Esc, right click and loss of focus retire placement and request cancellation. PointerGone pauses a focused, context-valid Preparing/Following placement under the later [safe re-entry amendment](S5_MOVE_POINTER_REENTRY.md); it still retires frozen/later phases and other point tools. Same-batch release cannot confirm, the re-entry batch discards button ownership, and a subsequent fresh press/release is required. Existing same-frame arbitration gives cancellation priority over Enter/target release. Text focus and IME own input and cannot confirm the target. After the service enters Committing, cancellation can be TooLate; a legitimate successful terminal result is installed and remains one Undo. No false claim of zero modification or compensating inverse edit is made.

## Verification boundary

Ordinary CPU/egui/raw-input tests cover following without a held button or per-hover worker, retained selection/template storage, accurate B without optional center statistics, click-before-admission, frozen target/scale, final preview then one exact commit/Undo/Redo, cancellation and IME conflicts, retired/duplicate/stale replies, resource/missing-preview refusal, legal late commit and unknown/disconnected state, deferred project operations and complete 80000-object synthetic Move. Inherited point-input/numeric/drag/I1/selection/shortcut regressions remain applicable.

Actual macOS pointer/modifier/IME delivery, ordinary native interaction and Windows execution remain external checks. This source batch adds no native performance acceptance requirement and makes no GPU/frame-time or complete platform acceptance claim. Restricted native crash/capture/performance experiments are not part of this work.
