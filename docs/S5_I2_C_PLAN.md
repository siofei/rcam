# S5-I2-C — Interaction preferences, canvas cursor, stable status and flicker investigation

## Ordinary UI readability follow-up (2026-10-08)

Scope: S5-I2-C presentation only, R16/R17/R18, local AT-063/065/069/077/082/097 regressions. Baseline 354027c70a6c500e8b9d41feb2e507c8439645fd. Allowed modules: app status presentation, theme text tokens, existing panel warning call sites and focused UI regressions. No geometry, material statistics calculation, service, import/cancel, shader, dependency, acceptance identity or threshold changes. This task does not resume crash investigation or native experiments; earlier native failures remain in their original evidence.

The user explicitly replaces the earlier permanently fixed two-line status presentation with a compact default single line. Selection descriptions/counts appear only for a nonempty selection. Area/perimeter appear only from the current valid cache result; numeric 0 and ZeroArea are valid data. Pending/unavailable/stale results have no placeholder metric slots; their notice and detailed tooltip remain. Scale and right-aligned coordinates stay on the first line. Fixed maximum widths and truncation prevent long text from resizing the layout. Only present data that cannot fit the first line adds a second data line; shrinking to one line is immediate when that data disappears. Narrow data rows share the available width without overflow.

Panel warning/attention text uses an opaque dark amber/brown foreground in the light theme and a light amber foreground in the dark theme. Compatibility marks and existing warning text are retained. Fixed dark-canvas overlays and manufacturing rendering remain unchanged. Synthetic egui regressions cover visibility, 0, stale/pending/error states, row transitions, all field-presence combinations, clipping, theme contrast, logical-point scale and coordinate alignment. Mac ordinary Test/Release and visual confirmation remain the local executor's task; cloud CPU UI tests are supporting evidence only.

## Read-only click task follow-up (2026-10-08; native validation pending)

Scope: S5-I2-C, R05/R09/R16/R17/R21/R22; local AT-011–018/025–037/065–069/074–075/086–092 regressions. Allowed modules: editor-app canvas task routing, worker View selection caches, gesture ownership, status presentation, project transition guards, import/recovery isolation and focused tests. Existing service hit tests, f64 manufacturing geometry, Snap/Alt, shaders, resource budgets, trajectories, performance intervals and thresholds remain frozen.

The reproduced click path dispatches ProbeDrag and then CanvasSelect/SelectRect through the global write busy state, temporarily disabling toolbar/layer controls. Move these reads to a task-versioned serial canvas lane without global write busy. Probe cancellation/replacement must confirm only its owning gesture and avoid unrelated metrics/selection-epoch work. Selection changes worker state: serialize the next selection and other service actions until its terminal selection, click cycle, epoch, metrics and geometry cache are synchronized; Completed after late Cancel must still synchronize. Failed/Cancelled terminals synchronize their explicit worker state. Reject invalid identities without installing a snapshot and isolate edits until the document is reopened; abandon deferred project intent for unknown results. Preserve the first Open/Close/Quit intent until selection terminates and use the existing dirty confirmation. A slow canvas read displays its own status and Cancel after 150 ms. Normal writes retain busy, commit fences and cancellation behavior.

Show fixed-width scale text in logical screen points/mm with a stated 1 point/mm base, plus pixels-per-point detail. It is not physical-size magnification. Fixed canvas/status height and manufacturing coordinates do not depend on the text. Cloud synthetic App/worker regressions and macOS cross checks are supporting evidence only; native rapid click/blank/hit/box/drag/Cancel/close/dirty visual regressions and Metal/full12 remain the Mac executor's responsibility. Earlier static nonreproduction evidence does not close the reproduced user report.

2026-10-04. IMPLEMENTING / NOT ACCEPTED. Sole Mac executor. Baseline clean commit cf806b8f91360bf6547164f4b57f23d437ed77d4, B final independent review B_FINAL_PASS_MAC_FIRST_BOUNDED, manifest5c872f2b853d4257777608a8d7e7c2b0dbcc8767b1bbf696cbed10d0facc892a (622). Isolated codex/s5-i2-c-interaction-ux. Main dirty, A/B/I1/K1, PMIX and historical/tmp data protected. No deletion/upload/push. One reused target, Rust1.89.0 locked/offline. Git worktree creation required approved metadata-write escalation; completed.

## Requirements and allowed modules

S5 within S0–S6. R04/R05/R07–R14/R16–R19/R21/R22. Local AT-011–018/022/025–037/039–045/048–051/065–069/074–075/086–092, BP/B regressions and new C cases below; no case identities, required platforms, schemas or thresholds change. Read DESIGN_V1, ACCEPTANCE_V1 relevant selection/gesture/async/GPU cases, B plan/ADR0056, followup requirements and external B final independent review.

Allowed: editor-app preferences/gesture integration/canvas display/status/native evidence and focused tests; existing service DTOs queried read-only; evidence/scripts/public fixtures/docs. Frozen A engine and B service/model/point-transform semantics unchanged. No dependencies, project schema, renderer manufacturing semantics, PMIX or post-C features. GUI manufacturing remains ApplicationService-only.

## Frozen implementation decisions

1. Two independent persisted app preferences: mouse drag movement and mouse Grip shape editing. **Defaults enabled/enabled** preserve existing behavior and old preference files; this is an implementation migration decision, not a claimed user preference. Both switches are visible in View→Canvas interaction and can be turned off for prevention. Neither disables click/box selection, pan/zoom, focus-aware shortcuts or explicit menu/numeric point commands. Switch changes cancel only affected active mouse gesture before same-frame release; no manufacturing/selection transaction. Disabled Grip features are neither hittable nor painted. Disabled drag still creates a selection gesture with object-movement arming suppressed. Turning on during an existing press never retroactively arms a manufacturing gesture.
2. Persisted cursor Normal (default, existing behavior), SmallCross, LargeCross. Display-only overlay in physical pixels, clipped to usable canvas. System pointer hidden only while custom cross is actually shown. egui response/layer hit checks keep menu/panel/modal/context popup ordinary; pointer gone/blur/outside removes overlay. No world/GPU geometry-derived cross.
3. Status is fixed-height/non-wrapping, no manufacturing/workspace revision or internal object ID. Fixed-width selection description: no selection; one aperture type/dimensions or Block name; multiple count. Material area and perimeter use **the same selected-only per-layer ordered union/difference boundary result as A**, holes included, overlap internal edges removed, cross-layer independent perimeter sums. Ready displays formatted values with full tooltip and error bounds; ZeroArea displays 0 with explicit zero-material tooltip; pending/unavailable/stale never substitutes object sums or old numbers. Reuse existing worker SelectionCenters and identity/cache/cancel/fencing; do not add per-frame Boolean work. Hover only changes right-aligned mouse coordinates. Bounds: reserve coordinates first, then selection, area, perimeter; narrow widths progressively hide lower-priority fields while tooltips expose full values. Status messages/toasts/errors constrained to allocated separate fixed line; varying numeric text cannot resize canvas or push coordinates. Units mm/inch/mil/µm presentation only.
4. Flicker remains OPEN initially. Audit callback scissor/viewport, panel wrapping, cursor and redraw scheduling. Collect candidate release identities, owned PID/window, phase/action/frame clocks, actual canvas/clip/ppp, lossless noncanvas static-menu ROI captures. Run fixed P100K select5000 warmup10s/move10s/commit/Undo/Redo; I1 overlapping selection/modifiers/box/drag/Esc/pan/zoom/project/recovery; new preferences/cursor/status/center/Snap matrix including Region/Block/large selection. Separate missing/bright/dim/clipped/moved glyphs from changed toolbar/status and expected window close black frames. Unknown user's exact scene remains NOT_EXECUTED unless obtained. No speculative renderer fix or nonreproduction PASS; report measured scope and retained OPEN issue. Reproduced B1 blocks candidate until fixed and relevant regressions rerun.

## Directed cases and evidence

C-01 four independent switch combinations: click/box selection and navigation retained, correct drag/Grip arm and overlay, explicit five transforms remain available. Actual Model undo/revision/snapshots.
C-02 live switch-off for nonempty preview with same-frame release, repeat/Esc/blur/Gone/IME; zero mutation/selection/history/scene; unaffected gesture not cancelled, off→on midpress cannot arm.
C-03 preferences old-file defaults, new choices atomic disk save/reload/restart; no `.rcam` or Gerber changes; invalid/corrupt/oversized safe bounded fallback. Native evidence uses isolated preference directory.
C-04 three cursor styles at DPI1/2 and zoom levels, inside/outside/toolbar/menu/context/modal/blur/Gone. Physical endpoints clipped and hit-layer priority proved; no repaint loop at rest solely from cursor.
C-05 area/perimeter independent numeric checks: overlapping/clear/refill/hole, multilayer, Block/Region, empty/zero/unsupported/budget. Same selection material DTO consumed by both center and status. Uncertainty tooltip present; display-unit changes do not requery/round geometry.
C-06 status pending→ready/error and 0/1/many selection at wide/narrow/min window; stable line height/rects/right coordinate anchoring, full tooltip/truncation, no revision/ID leakage. Large/negative coordinates and long block labels never grow panel.
C-07 stale matrix: selection epoch/order, document/revision/Undo/Redo, permission, precision/rules, task-generation/project switch. Stale result no status or center overwrite. Repeated hover/navigation frames no new center computation for unchanged identity; cancellation worker retained.
C-08 B contour Nearest/analytic holes/8–11 physical px/Alt/self exclusion and bounds/area point centers regression across switches/nav/focus; five cross-layer transforms preview=commit oneUndo; original B cancel conflicts retained.
C-09 actual release P100K menu-ROI performance scene with public100000/selected5000 and recorded clock/phases. Lossless ROI coverage and comparisons; issue conclusion is reproduced/fixed or NOT_REPRODUCED_IN_MEASURED_SCOPE, user report remains OPEN.
C-10 actual release overlap/I1 and C UI matrices, complex Region/Block and large selections, menu ROI/cursor clipping/async ready state. Window exit analyzed separately.
C-11 three distinct release native C rounds with raw input/frame/action/snapshot/surface/ROI evidence, declared mandatory scenes; B native/B07 trajectory regression as applicable. No simulated/unit result presented as OS input or scanout proof.
C-12 portable evidence verifier binds external source manifest + binary + commit/candidate, gates/full mandatory trajectories, actual input/time/ROI dimensions/pixels and staleness; resealed omission/false ROI/identity negatives must reject. Fresh Source/Evidence extraction actual release build and strict replay.

## Gates and stage flow

Plan→implementation→focused meaningful tests→frozen candidate→parent independent audit. Fix any defect and re-freeze. Only after independent approval: stage Git commit (no push), clean sameSHA full gates/native, Source/Evidence bundle with fresh extraction/build/strict verifier→independent final audit. Never mark unexecuted cases PASS. C or full-I2 acceptance requires explicit audited scope; open flicker cannot be called fixed.

Required commands: cargo fmt --all -- --check; cargo check --workspace --all-targets --locked; cargo clippy --workspace --all-targets --locked -- -D warnings; cargo test --workspace --locked; cargo build --release --locked -p editor-app; editor-service automation_contract/headless_workflow and normal dependency boundary. Include internal-evidence check/clippy/release, C directed tests, inherited B16/service/Snap/verifier, A numeric/resource and I1/batch/actual Metal gates. Use frozen run_i2_b_gates contract plus explicit C additions; no weakening. Single Cargo target /tmp/rcam-target-s5-i1-dev-20261003; owned subprocess lifecycle only, no global process queries or unapproved cleanup.

## Current measured risks and remaining boundaries

Actual baseline status uses horizontal_wrapped and unbounded dynamic revision/ID/Snap/message text; this can change canvas size but is not yet established as the reported flicker cause. Existing metrics panel sums objects and is unsuitable for composited status; A SelectionCenters already carries material perimeter/error and can be reused without core changes. Gesture press/release and async Probe reply need joint switch fencing. Crosshair must use egui top-layer hit test, not merely canvas rectangle. No `.agents/skills` directory found in root or new checkout. Parent sole-writer authorization persists.

Windows and K1 native deferred, PMIX paused until C final approval; CircuitCAM4.4 full compatibility last. No full V1/CORE10/globalP100K claim. All initial C cases NOT_EXECUTED. Progress and raw attempts saved per-run under exports/S5I2C_intake_20261004; failed/historical runs retained.

### Development findings retained (2026-10-04)

The first complete development run used an ordinary New helper on a dirty synthetic session; the product correctly required discard confirmation, retaining old layers and refusing Block Create. C resets are now explicitly labelled c_new / DiscardNewWorkspace and never bypass user-document guards. Actual phase90/92 drag/Grip switch-off releases showed revision/Undo/selection unchanged; the overall failed run is not PASS. Menu-checkbox close behavior required reopening the real menu before each widget. Status display normalizes all line/control separators while tooltip retains raw text. Additional C native mouse conflicts cover switch-off+release with Esc/Enter/repeat/blur/Gone/IME; mouse previews now also cancel for text focus/IME rather than committing after focus changes. These are C scope changes, not changes to A composition/B manufacturing math.

Development full native02 completed189 steps /5555 continuous frames with runner exit0; this is a development run, not frozen C acceptance. Initial three-layer area14.89048623, Region/ordered composite area48 and live drag/Grip twelve conflicts had unchanged revision/Undo in raw records. Full independent oracle and candidate reruns remain required. Native batch screenshot observer now recognizes only its own typed requests so independent UI-ROI screenshots do not masquerade as batch completion evidence or duplicate full-window pixels. User-scene clarification requested asynchronously; investigation continues.

The B mouse-target contour policy deliberately differs from the global Snap
switch. The historical free-pointer batch oracle rejects an unmodified C
investigation when nearby unselected contours attract the target. C's fixed
P100K investigation explicitly injects and verifies the existing **Alt bypass**
throughout press/hold/release. This is declared test input, not a product/global
preference change. Sample100000/selected5000, camera/DPI, warmup10s/move10s,
three repeats, all existing frame/GPU/memory/commit thresholds and exact Undo/Redo
remain unchanged. B contour/B07 rounds remain without this bypass. The failed
development runner/trajectory records remain FAIL.

Frozen gates r1 passed20/28 and failed Metal adapter acquisition in the default
sandbox (NotFound/no adapter, exit101); later gates were not run. Source stayed
unchanged. Approved native validation uses escalation for the same hardware
tests, without backend/threshold substitution. Historical request-only ROI
outliers were enabled→disabled menu transitions during asynchronous readback;
the new verifier checks every intervening UI state and actual request clocks.

Frozen gates r2 passed28/28 in the approved native environment with unchanged
source/status. Its212-step development validation completed; strict surface
analysis bound970 callbacks, with each static menu keeping one pixel hash after
legitimate UI transitions were excluded. An oracle initially expected the bare
AreaCentroid enum instead of its Debug error bound; the parser now validates the
bound and independent fixture point. The P100K path also requires explicit typed
SelectionCenters read-only worker records, full TaskVersion receipts and exact
unchanged manufacturing/selection/history/scene state between each mutation and
query. It does not ignore generic extra workers or relax the original protocol.
These recorder/oracle changes require a new freeze, full gates and native rounds;
r2 development evidence is preserved, not reused as the final candidate.

Gates r3 passed28/28 with unchanged source/status. The first P100K round completed
and passed exact pointer/transaction/typed-query checks but lacked process metric
evidence, so remains FAIL. The old batch runner's ps sampling is prohibited in
this session. C reuses the project's existing Darwin wait4 completion accounting
for **only its direct RCam child**: kernel ru_maxrss in bytes and actual CPU
user/system seconds. This does not enumerate or inspect other processes. Kernel
lifetime peak replaces periodic RSS samples for C and retains the exact1GiB
limit; total/average CPU is explicitly named, without a CPU-maximum claim. Legacy
M2 verification remains unchanged. Missing usage/PID/method/invalid numbers fail
closed, and no historical missing result is reconstructed or relabelled PASS.

R4's first kernel-accounted round failed framep95=50.424125ms against50ms.
Read-only update spans identified synchronous eight-file ROI writing at roughly
10ms p95; profiling independently failed50.469666ms. The internal recorder now
uses a bounded16-job owned IO writer and joins/flushes at app exit; queue full
waits instead of dropping samples, and send/write/join failures fail the run.
Capture/request/callback clocks and100ms frequency are unchanged. Diagnostic
rerun p95=46.509041ms and all old batch/ROI gates passed, but is not a candidate
round. Keep failed R4/profile logs; re-freeze all gates and three native rounds.

The R5 real resealed negative relabelled the initial setup SelectionCenters
worker as other. The complete bundle verifier incorrectly accepted it: only the
six post-release labels were fixed, so that setup query bypassed receipt checks.
R5 is SELF-REVIEW REJECTED and retained, not handed off or accepted. C now emits
an explicit new-workspace setup label and freezes the complete12-worker path,
including initial query/probe and all three post-mutation queries. All four
SelectionCenters receipts use the same full TaskVersion/state check. Setup
unknown/omitted/inserted queries and a shifted release boundary fail closed.
Three additional directed tests (32 total) and new full frozen gates/native
rounds/package/actual negatives/fresh extraction are required.

R7 independent review is REQUEST_CHANGES / C-R1. A real eligible File-menu crop
shifted one physical pixel, with every RGB color/count and mean preserved and
crop/JSONL/BUNDLE resealed, was accepted by the complete frozen verifier. The
counterexample is a verifier defect, not a product flicker reproduction. Retain
the old R7 Source/Evidence and review attack; do not mark R7 accepted.

Correction adds exact threshold140 foreground occupancy comparison at physical
pixel coordinates within the same menu/physical-rect/DPI stable state. A modal
observed mask is a temporal reference; every different occupancy is an outlier,
including balanced shape changes with unchanged counts, bbox, centroid and
row/column projections. Existing minimum ink10, median0.75–1.25 and mean100
checks are preserved. RGB/antialias color variation with unchanged foreground
support is allowed; changed physical rect/DPI is a separate state, and actual
hover/popup/disabled transitions remain excluded over the entire readback
interval. A new hash alone is neither rejection nor exemption. The reference
does not establish absolute ideal font layout or catch all subthreshold/subpixel
or shorter-than100ms events. User flicker remains OPEN.

43 directed portable cases include physical horizontal/vertical1px translation,
same-count shape changes (including balanced projections), tied layouts and
color/state/rect/DPI positive controls. Reprocess existing real R7 pixels under
their original ec7ed07 capture/internal-binary identity and record the stricter
detector identity separately. A diagnostic replay must never be described as
native execution under the new Source identity. Re-freeze affected Source/gates;
current-source full native evidence remains NOT_EXECUTED until coordinated.
