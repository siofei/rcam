# S4-D2 RefDes-Assisted Stencil Candidate Selection v1 review

Status: **PASS (Mac-first bounded)**; closed 2026-10-02. Stop after D2. Candidate != Association, final Boolean opening or footprint ownership. Windows, full V1, CORE10 10/10 and product P100K are not claimed. Native evidence is controlled synthetic execution in the real EditorApp worker/ApplicationService/Apple Metal window; physical human input is not claimed.

## Scope and implementation

Started from clean S4-D1 `8b978cc6037de9a78731401d3b31b96738236b79` in isolated `codex/s4d2-stencil-candidates`. Unrelated main checkout changes remain preserved. Stage/R/AT mappings and allowed modules are in [S4_D2_PLAN](S4_D2_PLAN.md); bounded D2-01–D2-13 cases are in [the addendum](S4_D2_ACCEPTANCE_ADDENDUM.md). ADR 0047 freezes the query semantics; 0043 was already occupied by D1 input compatibility. Cargo.lock and the original 96 V1 cases/retired AT-079/platform requirements are byte-identical to D1.

`components.nearby_manufacturing` is a revision-fenced `&self` JSON/Rust query. It requires a registered Board, reuses D1 world_rotation_deg and performs no Bottom-side reflection of its own. Explicit Gerber layer IDs are mandatory; the API can query hidden/nonselectable layers, and reserved Drill layers reject before component lookup. Manufacturing f64 world bounds are queried through the shared WorldIndex, then classified against a rotated component-local rectangle. No renderer mesh/pixel data or footprint-size guesses enter the result.

The former app envelope index now has a window/GPU-free core implementation with a two-axis BVH. The app wrapper keeps display policy and source exposure order. A session cache retains its manufacturing generation, whole text groups, stable object lookup and component lookup; warm queries do not scan all manufacturing objects or the component table. Cold cache construction is reported separately. Ordering is fully-inside first, center distance, semantic layer order, lexical ObjectId. Page limit is 1–500. Nearby and expanded-member budgets are 10000, rejected without partial output.

BlockInstance is one atomic candidate. GeneratedText uses an existing member ObjectId as anchor and returns the complete logical group, with bounded member IDs instead of geometry JSON. Dark/Clear and geometry type stay explicit. Bounds intersection/distance is an envelope heuristic, not final material intersection or ownership.

The Components panel offers current-display-unit width/height, explicit visible layers, virtual candidate rows, Clear/type labels, bounds highlighting, object focus, region fit and Replace/Add selection. Search/Enter updates the component list only; explicit component selection focuses and refreshes candidates. Worker results fence sequence/document/revision/component/settings. Formal selection checks effective visible/selectable; a text group is selected wholly or skipped wholly. Locked selectable objects may be inspected but edits still reject through the existing service. Grip, Move, Create Block, Align and Array use existing edit paths, one transaction and exact Undo.

Query/overlay/focus/selection do not change manufacturing revision, dirty/history or Gerber bytes. Candidate state is absent from project/recovery serialization; Save/Open clears it and permits requery without a schema change. Internal native runs isolate preferences, recovery and diagnostic logs in their guarded synthetic scratch directory. Default public builds exclude the active drivers and capture-control entry point.

## Executed acceptance

The accepted pre-closeout source was `296776a3f31b900cf2eb75f3b0b03a0b67be7ebd`. Its immutable 19/19 gate results, native 21-observation verification and complete four-file/fresh-extract package passed. The first development run exposed a D1 test fixture that dropped its worker receiver; the fixture now retains and asserts the new focus-triggered candidate requests. The failure was preserved, not removed or reclassified as a pass.

Final closeout adds truthful stage/capability documentation, the reserved Drill guard test, hidden-layer selection proof and direct D2 native-state isolation. The final clean commit is tested again with all commands below, rerun natively, and packaged only when before/after tested-source hashes and clean status match. Final authoritative HEAD, binary hashes, command exits, native observations, archive hashes and fresh payload binding are in the versioned `exports/S4D2_*/REVIEW.md` and public evidence. Source cannot embed its own commit/archive hash without a circular identity; those independently recorded identities bind it exactly.

Environment: macOS 26.5.1, Apple M1, arm64, release optimization, Metal; native pixels_per_point=2. No Windows execution. Rust uses the pinned toolchain through `.tools/cargo`, `.tools/rustup` and `.tools/target`. Exact commands, exit codes, durations and stdout/stderr are preserved under `evidence/s4d2-<testedsha>-final`, with host paths redacted in public evidence.

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --locked -p editor-app --all-targets --features internal-evidence -- -D warnings
cargo test --locked -p editor-core --test pnp_foundation -p editor-service --test pnp_workflow --test pnp_diagnostics --test pnp_input_formats --test component_candidates --test candidate_diagnostics -p rcam-project --test pnp_project -- --nocapture
cargo test --locked -p editor-app candidates_ui -- --nocapture
cargo test --workspace --locked --no-fail-fast
cargo test --locked -p editor-service --test automation_contract --test headless_workflow
cargo tree --locked -p editor-service -e normal
cargo build --release --locked -p editor-app
python3 scripts/measure_s4d1_performance.py
python3 scripts/measure_s4d2_performance.py
cargo test --locked -p editor-core --lib candidate_window
cargo test --locked -p editor-core --lib world_index
cargo test --release --locked -p editor-app native_metal_c3_create_invariance -- --ignored --nocapture --test-threads=1
cargo test --release --locked -p editor-app native_metal_block_instance_parity -- --ignored --nocapture --test-threads=1
cargo build --release --locked -p editor-app --features internal-evidence
python3 scripts/source_manifest.py --check
python3 scripts/test_package_source.py
```

The runner copies/hash-binds both binaries, checks the public-driver marker boundary and service normal dependency tree, records HEAD/clean status/source hashes before and after, and stops on any failed gate. Native observations are independently checked with `scripts/verify_s4d2_native.py`. Delivery uses `scripts/package_s4d2_delivery.py`, which verifies both native binary identity and the exact tested-source payload, extracts with zipfile.extractall, runs source_manifest.py --check and test_package_source.py in the fresh source, checks every EVIDENCE.sha256 entry, reruns native assertions on freshly extracted evidence and checks SHA256SUMS.txt.

| Cases | Concrete evidence |
|---|---|
| D2-01/02 | service registration/0/37/90/reflection/Bottom/writer invariance; core rotated SAT/corner test; native unregistered warning and unchanged camera/state |
| D2-03/04 | explicit multi-layer and hidden query tests, reserved Drill guard, inside/intersect/Clear/equal-distance deterministic pages |
| D2-05/06 | invalid dimensions/limits/layers, 10001-overflow reject, atomic Block and full GeneratedText member test |
| D2-07 | service stale revision and Move/Undo cache tests; app settings/component/document fences; D1 replace/registration history regressions; native registration Undo/Redo |
| D2-08 | app Replace/Add/hidden/nonselectable/locked tests and existing Grip/Move/Block/Align/Array transactions with exact Undo |
| D2-09 | typed/JSON query information equality, before/after writer bytes; native selection purity, Save/Open empty candidate state and requery |
| D2-10 | standalone release 100k manufacturing and dense 5000 metrics; standalone D1 100k component regression; actual process RSS |
| D2-11 | native 100k PnP table, explicit C15 search/focus, Top/Bottom 74° window, reflected 180°, Replace/Add 4 candidates, Move and exact Undo, units/zoom; two native Metal invariance/parity tests |
| D2-12 | operation/capability/JSON and privacy-counter tests; native diagnostic ZIP independently scanned for prohibited payload/path strings |
| D2-13 | all 19 final gates, clean hashes, public/internal boundary, deterministic archives, full source/fresh payload and per-file SHA verification |

## Measurements and native limits

Pre-closeout release manufacturing query: cold build 34.083 ms; warm 100-sample wall P50 0.010875 ms/P95 0.016959 ms; nearby count 16, leaf-envelope tests 24. Sparse phase P95: WorldIndex <1 µs timer resolution, classify 2 µs, sort <1 µs, measured service phases 7 µs. Dense 5000 candidates: ten deterministic 500-item pages; nine warm phase P95 values were WorldIndex 42 µs, classify 757 µs, sort 612 µs, query phases 1382 µs. Actual standalone peak RSS 89,489,408 bytes, within 512 MiB. These are synthetic local query measurements, not a product P100K SLA. Final measurements are retained separately in public evidence.

D1 100k component regression: service search 954/795/948 µs, list 685 µs, import 148998 µs; actual process peak RSS 112,508,928 bytes. Document-info/dirty hashing was measured separately (103377 µs), not hidden inside a claimed query score.

Native proof uses generated 100000-row PnP and four-flash Gerber (including Clear). Both Top and Bottom have component angle 37° plus registration 37°, yielding 74° without automatic Bottom mirror. Registration reflection changes the angle to 180°. Candidate highlight precedes formal selection, Replace/Add yields four selected objects, Move changes geometry in one Undo entry, and Undo restores the exact geometry hash/clean manufacturing state. Before/after Gerber export bytes are identical. Project v2 retains the 100k Board but not candidates or selection. Diagnostic ZIP contains counts/timings/registration summaries and no RefDes, source, geometry or host paths.

The native controlled run recorded 911 app frames and a maximum observed frame gap of 224.065 ms across import/query/project work. This is bounded smoke evidence, not a latency/continuous-input SLA. Pixel-stable 1.5-physical-pixel outlines are asserted through real egui shape output across zoom and pixels_per_point 1/2/3; native drawing uses the same painter. Any CUA observation is identified separately; it does not establish human physical-device acceptance.

## Delivery and stop

Required four files: `RCam_S4D2_<shortsha>_source.zip`, `RCam_S4D2_<shortsha>_public_evidence.zip`, `SHA256SUMS.txt`, `source_fresh_extract_report.json`. Complete source includes PACKAGE_INFO.json, PACKAGE_MANIFEST.sha256 and MANIFEST.sha256. Deterministic ZIP order, epoch timestamps, permissions, UTF-8 flags, CRC and repeated byte identity are checked. Public evidence includes allowlisted synthetic artifacts and path-redacted raw gate logs; original logs and both binaries remain local. Private Gerber/PnP/projects/fonts are excluded.

Windows, physical human interaction, full V1, CORE10 10/10, manufacturing P100K, signatures/distribution and persistent associations remain deferred/not executed. No footprint library replacement, Drill implementation or assembly export was started. Stop after S4-D2.
