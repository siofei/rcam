# RCam Gerber editor — S1-A.1 arc validation

This workspace contains the S0 demonstration window and the S1-A semantic
parser, model, validation, and new-path writer. It is not EditableV1 or a
production Gerber editor. The S0-C Windows platform gate remains blocked.

The slice proves one deliberately small path: a real `gerber_parser` parser
reads a finite circle-flash subset, `gerber-io` rejects everything outside that
subset, `editor-core` evaluates f64 millimetre coverage with local aperture
holes and per-layer Dark/Clear order, and `editor-service` exposes the result
through a versioned JSON DTO boundary. `editor-app` displays the same service
snapshot through an eframe-managed wgpu callback. The demo also includes the
fixed line/ring and cross-layer Clear checks used by the S0 risk review.

The S1-A headless service opens explicitly authorized files, queries owned
geometry DTOs, validates them, and exports an unchanged layer to a new path.
Its supported subset is frozen by ADR 0005. The existing window still uses
the S0 demonstration path; full GUI integration, object editing, Undo/Redo,
and source overwrite remain unimplemented. See `docs/S1_A_REVIEW.md` for
historical evidence. The current arc/legacy review is `docs/S1_A1_REVIEW.md`;
redacted, distributable logs are in `evidence-public/s1-a1/`. CORE10 editing round trips are not claimed.

The S0 window uses English labels so the default eframe font remains legible
on a clean installation; no user font is bundled. Chinese UI and vector text
support are deferred to the planned S4 scope.

## Workspace

```text
crates/editor-core/       f64/mm geometry and layer coverage
crates/gerber-io/         strict finite parser adapter
crates/editor-service/    UI-free S0/S1-A service and JSON request envelope
crates/editor-app/        eframe + egui-wgpu S0 demonstration window
fixtures/synthetic/       immutable S0-C inputs and independent S1-A truth
docs/DEPENDENCIES.md      locked dependency and alternative record
THIRD_PARTY_NOTICES.md    direct dependency source and licence notices
evidence/                 local verification logs and review evidence
```

The toolchain is pinned by `rust-toolchain.toml`. Use an existing installation
of Rust 1.89.0 with rustfmt and clippy; do not switch RUSTUP_HOME to an empty
folder. The original machine has its existing toolchain and caches in
`.tools/`; those paths are local evidence, not prerequisites for other hosts.

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
cargo run --release --locked -p editor-app
```

The real parser/service contracts live in `automation_contract.rs` and
`headless_workflow.rs` under `crates/editor-service/tests/`. They exercise
open/query/validate/export/reopen without a window. This is an unchanged
document baseline; a V1 headless editing workflow is not yet implemented.

```text
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo test --locked -p editor-service --test s1a_semantic_truth
cargo test --locked -p editor-service --test s1a_independent
cargo test --locked -p editor-core --test s1a_independent_geometry
cargo tree --locked -p editor-service --edges normal
```

Read `AGENTS.md`, `docs/DESIGN_V1.md`, `docs/ACCEPTANCE_V1.md`, and
`docs/AUTOMATION_API.md` for the governing design and acceptance boundaries.

## S0-B regression and evidence

```
cargo test --locked -p gerber-io --test review_parser_regressions
cargo test --locked -p editor-core --test review_geometry_regressions
cargo test --locked -p editor-service --test review_service_regressions
cargo test --locked -p editor-service --test historical_audit -- --nocapture
cargo test --locked -p editor-service --test dependency_boundary -- --nocapture
cargo test --locked -p editor-app native_gpu_coverage_regressions -- --ignored --nocapture
python3 -m unittest discover -s scripts -p test_audit_core10.py
python3 scripts/source_manifest.py --check
```

The GPU check requires native Metal or DX12 hardware; it fails if no adapter
is available. It is explicitly ignored in ordinary workspace tests, not counted
as passed. Windows can use `python` for the dependency-free audit/hash scripts.
The original review attachment and its three regression files were absent;
new regressions are project-authored and do not claim to reproduce that attachment.
See `docs/S0_B_REVIEW.md` for actual runs, failures, platform gaps and file hashes.
Current source hashes are in `MANIFEST.sha256`; the old design-package manifest
is preserved under `docs/archive/`. Binaries have separate per-run hashes.

The S0 canvas release-frame drag fix and its regression/runtime evidence are
recorded in `docs/DRAG_FIX_REVIEW.md`.
