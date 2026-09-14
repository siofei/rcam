# Gerber editor S0 technology validation

This workspace contains the first runnable S0 slice of the Gerber editor. It
is a read-only technology validation, not EditableV1 and not a production
Gerber editor or exporter.

The slice proves one deliberately small path: a real `gerber_parser` parser
reads a finite circle-flash subset, `gerber-io` rejects everything outside that
subset, `editor-core` evaluates f64 millimetre coverage with local aperture
holes and per-layer Dark/Clear order, and `editor-service` exposes the result
through a versioned JSON DTO boundary. `editor-app` displays the same service
snapshot through an eframe-managed wgpu callback. The demo also includes the
fixed line/ring and cross-layer Clear checks used by the S0 risk review.

The current implementation intentionally provides no object editing, undo,
save/export, or general-file compatibility. Those remain V1 requirements and
are reported as unsupported by the capability DTO.

The S0 window uses English labels so the default eframe font remains legible
on a clean installation; no user font is bundled. Chinese UI and vector text
support are deferred to the planned S4 scope.

## Workspace

```text
crates/editor-core/       f64/mm geometry and layer coverage
crates/gerber-io/         strict finite parser adapter
crates/editor-service/    UI-free S0 service and JSON request envelope
crates/editor-app/        eframe + egui-wgpu S0 demonstration window
fixtures/synthetic/       deterministic S0 parser fixture
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

The S0 automation contract is the real parser/service contract test at
`crates/editor-service/tests/automation_contract.rs`. A V1 headless editing
workflow is not claimed because editing and export are not implemented.

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
