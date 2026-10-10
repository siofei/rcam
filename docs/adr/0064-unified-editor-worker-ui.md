# ADR0064: accumulating session worker and UI

Status: authorized implementation on exact cb9 → 8a → 5b; candidate/native acceptance pending. S5/P3 UE-A; R05/R09/R10/R11/R12/R16/R17/R18/R21/R22; compatibility AT-030/031/034/035/036/037/038/039/040/041/043/064/065/066/067/078/086/089/090/093. Modules: editor-app session/display/Snap/modal/commands, service typed final result, synthetic regressions and documentation. One source writer and one Cargo, incremental cache disabled.

An independent fixed-width selected-object editor opens only on a confirmed editable selection. Existing numeric dialogs remain. Each session has a fresh Arc identity, actual worker owner, authoritative backend generation and serialized request identity. Immutable entry reference is decoration only. Move/Rotate/Mirror accumulate in a separate work snapshot; each step explicitly chooses its targets, including complete Text groups. Internal Undo/Redo retains exact backend checkpoints; Reset restores entry and clears parameters/history while staying open. Cancel discards, with confirmation for executed changes. Final Apply validates/calculates latest unexecuted input, prepares all CPU display/manufacturing Snap artifacts, crosses one barrier, commits one cross-layer transaction and closes. Exact NoChange closes without main revision/history/dirty changes. Postcommit terminal never becomes a retryable Apply error; TooLate waits for the authentic result. Unknown write replies fail closed.

Full working snapshots replace only authoritative source slots, preserving exposure, metadata, Aperture/Block definitions and layer composition. Working Snap uses the existing production resolver and matching f64 snapshot/index; entry ghosts cannot be hit, selected or snapped. Hysteresis/cache resets on session/work changes. Query resource refusal returns an error before feature/intersection allocation, rather than dropping candidates. Legacy Snap/Alt and shader paths are unchanged.

CPU admission reuses the existing point-preview temporary limit (128MiB) and 2-second calculation deadline. It explicitly charges backend reserved peak, live main/reference/work/preview snapshots and Scenes (unique Arc artifacts only), candidate full snapshot, Scene/index/Block/derived-contour/maps/selection/ghost scratch. Snapshot cost traverses actual owned nested capacities; raw display growth, source-resolution scratch and existing acceleration/index limits provide conservative pre-allocation reserves. Count-first polygon bins check reference demand before materialization, without dropping contours or changing exposure. Refusal keeps main manufacturing state, draft work/history and last valid artifacts. This engineering boundary is not a whole-process RSS or GPU-memory promise; actual worker/UI samples and native GPU/IME/resource stress remain separate gates.

Required regressions: real worker multi-layer sequence/one main Undo; prepared scene/index/budget cancellation refusal; newest unpreviewed/invalid input and exact NoChange; authoritative generation and namespace/draft/request fences; parked worker/UI fields and transition gates; working Snap/Alt and old-reference exclusion; full Dark/Clear/local-hole/Block/Text metadata; exact history/Reset; TooLate and malformed/disconnected terminal; actual fixed rectangles for all input/status/viewport states. Fresh complete candidate/source manifest and independent source/package audit precede Mac handoff. No threshold, measurement interval/sample policy, original native evidence or full-stage PASS is changed.

## First-slice operation and remaining gates

Open **对象 → 变换 → 选区编辑会话…** on a confirmed editable selection. The fixed 440-point window offers Move, Rotate, horizontal/vertical Mirror, numeric base/target and working-canvas point input. Each step explicitly selects ordinary objects or complete Text groups. Preview is optional; Execute accumulates; Apply includes the newest typed/picked input and closes only after an authentic Changed/NoChange terminal. Internal Undo/Redo and Reset stay in the session. Cancel confirmation owns its input; Escape from a child point picker returns to the session without discarding work. Navigation is held at the entry camera during this bounded slice; close/tab/drop/main commands and automatic recovery wait until the session ends. Preexisting ordinary terminal handling preserves a confirmed deferred Cancel. The renderer shows executed work while picking so the visible geometry and Snap snapshot/index agree.

The 2-second deadline is cooperative: inner existing geometry/index helpers have their original finite work bounds and object-boundary cancellation points, and the deadline is checked before publication. It is not a preemptive scheduling or worst-case wall-clock guarantee. The 128MiB admission is a conservative CPU reserve with actual owned-capacity checks, including retained LastGood Scene/index/flags, main selection, origin IDs, reference and matching work/preview. It excludes the existing main service document, runtime/framework allocations and GPU memory; it is not a process RSS promise. Draft Snap uses a fresh temporary definition cache per query and charges full layer/object/related identity copies before feature/intersection allocation; resource refusal drops no candidates.

Entering point input restores the executed work Scene together with its bounds, display scale and full coverage metadata. It cannot inherit scale metadata from an unexecuted Preview. If DPI changes make the executed Scene insufficient for the current display scale, point input returns to the session until Reset or Execute prepares matching work; manufacturing coordinates and selection are unchanged. A two-scale regression covers refusal and subsequent preparation.

Native acceptance must bind this full UI candidate's new commit/MF/source package and fresh Release binary. Recheck the unchanged full 12 matrix with all measured/screenshot samples, 50ms frame and 1e-9 trajectory contracts; GPU completion remains a mixed schedule/draw/fence measurement. Also run actual modal rectangles, multi-layer Move→Rotate→Mirror subsets, Text/Block/holes/exposure context, working point Snap/Alt, invalid newest Apply, Reset/history, one main Undo, no-op dirty/Redo preservation, early cancel/TooLate, resource refusal, tab/lifecycle/IME and memory stress. Portable/auxiliary CPU results and the old cb9 native acceptance do not establish Metal/IME, Windows, full A2 or full-stage PASS.

## Point-picker HUD correction (2026-10-09)

S5/P3 UE-A follow-up; R07/R08/R09/R10/R12/R18; compatibility AT-021/022/025/030/043/078.
Scope is editor-app painter layout and synthetic regressions only. On the frozen
2f candidate, the picker text and fallback navigation text occupied the same
canvas corner. The picker now uses the existing fixed, wrapped, clipped HUD slot
and reports its ownership to the navigation fallback. Move keeps its existing
priority; reference geometry, input, Snap/Alt and manufacturing are unchanged.
Real painter regressions cover Base/Target, return to the editor, fallback states,
small canvases, native DPI/UI zoom and existing Move priority.

The parent cancelled the old full12/60Hz task: the historical matrix requirement
above is superseded for this handoff, without changing any numeric threshold.
2f automatic Mac checks passed per the parent report, but native acceptance
remains BLOCKED. This follow-up requires a fresh identity and native visual
verification; it does not inherit 2f's test or native evidence as a new PASS.

## Modal and terminal-read reliability correction (2026-10-10)

S5/P3 UE-A follow-up; R07/R08/R09/R10/R12/R18; AT-021/022/025/030/043/078.
Scope is editor-app modal painting, worker publication/read cancellation,
opt-in local trace diagnostics and synthetic regressions. A cancelled input
frame keeps a surviving unified dialog and its backdrop painted while disabling
all dialog controls and dismissal. The existing cancellation latch still spans
all egui passes; point-picker loss still returns to the editor.

Changed Apply establishes the new selection epoch with the existing optional
metrics refresh before publishing its terminal View. Metrics errors stay in
metrics_error after the committed edit; no new postcommit refusal or retry is
introduced. Read cancellation restores the metrics identity together with the
restored View/epoch, so a subsequent geometry query cannot advance an unpublished
epoch. NoChange, Cancel, manufacturing history and service mutation boundaries
retain their existing contracts.
The postcommit metrics refresh adds real CPU/cache work after the prepared-edit
deadline; the cooperative two-second preparation limit is not an end-to-end
Apply wall-clock promise.

Local opt-in traces record UI/worker epochs and equality decisions for geometry
replies, task publication epochs, session/task/barrier state and sticky metadata
budget/string failure flags. They record no selection identities, coordinates,
input text or filesystem paths. Queue, metadata and output limits stay unchanged.
Session state uses a separate fixed record on change, preserving the existing
256-byte Record and 4096-byte writer staging limits.
Native tab-disable and trace-stop causes remain unconfirmed until fresh evidence;
these diagnostics do not turn those failures into a PASS. Fresh candidate identity,
independent review and Mac native visual/task/resource verification remain required.

## Native panel and tab input ownership correction (2026-10-10)

S5/P3 UE-A follow-up; R09/R12/R18; AT-043/078. Scope is editor-app
window input ownership, fixed local trace diagnostics and regressions only.
A stale logical held key must not keep an otherwise idle tab locked. Window-owned
key quarantine survives document Memory exchange. All native pickers register
entry/return boundaries, including Cancel and returned errors. The return frame,
all its passes and the first backend return batch are consumed; an already
extracted shortcut queue stops at the boundary. Owned picker-result service
requests retain their existing route. Genuine modifier/button holds and pending
worker terminal receipts still prevent document input or exchange.

Every press of an isolated key is rejected regardless of backend repeat flags.
An observed release rearms that key in the next distinct backend batch, with no
extra blind drain. Release followed by press in the same batch remains consumed
and needs another release. Native Enter/Escape are isolated even if the panel
was their only prior receiver. Until a release is observed, a first new cycle of
an indistinguishable isolated key serves as rearming rather than an action; mouse
and unrelated fresh commands remain available once the whole-window guard ends.

Unowned Text/Paste/IME is consumed at the boundary and in batches containing an
isolated key. A subsequent complete pointer gesture or a key with an observed
post-boundary release establishes new window input intent in event order; it
never authorizes earlier payloads retroactively. A new Preedit (Enabled optional)
establishes composition before Commit. Whole-window barriers discard provisional
text/composition ownership, and native return ends the old app IME owner.
The backend supplies no timestamp/panel/composition provenance: this is a finite
ownership guarantee, not proof that arbitrarily delayed unowned native payloads
can be distinguished from fresh payloads after rearming. That ambiguity and the
release-rearm interaction cost remain explicit native acceptance limitations.

Opt-in SessionState diagnostics add bounded key counts, fixed control-key and
modifier/button bits, boundary serial and ownership state. No text, paths,
coordinates or arbitrary key stream is recorded. Existing fixed record, queue,
writer, metadata and output budgets remain unchanged. Prior native NOT PASS and
unknown root cause stay historical; fresh source/binary identity, independent
review and Mac input/IME/tab/save/cancel verification are required.
