# RCam Gerber editor — Mac-first S2-B3.2

S2-B3 增加独立对象解析面积/周长、会话lazy cache和后台属性面板。多选只显示对象合计；复杂几何明确不可计算，不代表图层最终开口面积。任务范围见 [S2-B3计划](docs/S2_B3_PLAN.md)。

默认应用现已接入真实 Gerber 文件：原生打开、Finder 拖放、图层工作区、f64 导航、
精确几何点选/框选、多选、属性、数值移动、整组拖动、原位复制、删除、撤销/重做与新路径另存为。
范围和限制见 [ADR 0016](docs/adr/0016-s2a3-gui.md)。本轮范围见 [S2-B3.1计划](docs/S2_B3_1_PLAN.md)，上一阶段证据边界见 [S2-B3报告](docs/S2_B3_REVIEW.md)。Windows与完整V1仍未通过。

Mac 启动：`cargo run --release --locked -p editor-app`。
测试应用包：先构建 release，再运行 `python3 scripts/package_macos.py --out target/gui-package`。
应用包未签名/公证，仅供本阶段本机验证。S0 演示保留为 `cargo run --release --locked -p editor-app --example s0-demo`。

单文件，Ctrl-click 加选，Shift-click 减选；空白拖框左→右 Window、右→左 Crossing。Clear 和锁定对象可查看，制造编辑仅同层且无锁定成员。中键拖动或双指滚动平移，捏合或 Cmd+滚动缩放，F 适合窗口。
移动只使用 ΔX/ΔY 毫米；Cmd+Z 撤销，Shift+Cmd+Z / Cmd+Y 重做。文本字段保留文本撤销。
另存为必须使用新路径；源文件不会被覆盖。元数据丢弃必须按提示确认。
显隐、锁定和显示名只在本次会话保留，不产生制造 dirty 或历史。
超出显示精度/预算时整幅拒绝并禁止移动与保存，可缩小视图或撤销恢复。

macOS 界面从系统字体加载中文，不打包用户字体；中英文制造矢量文字尚未实施。
Windows、完整 CORE10、性能与双平台 V1 验收仍未完成；不能作为生产发行声明。

Rotate preserves true manufacturing geometry; mirrored arcs reverse direction.
Rectangular sweeps support exact multiples of 90 degrees only (odd turns swap aperture dimensions);
mirror axes are world horizontal/vertical. Unsupported mixed selections reject atomically.
Workspace display name/visibility/lock now live in the service, independently of manufacturing dirty and history.
`layer.update` checks both manufacturing and workspace revisions. New edits respect locks; Undo/Redo remain available.
See ADR 0012 and `docs/S1_B2C_REVIEW.md` for scope and evidence.

## Workspace

```text
crates/editor-core/       f64/mm geometry and layer coverage
crates/gerber-io/         strict finite parser adapter
crates/editor-service/    UI-free S2-B3 service and JSON request envelope
crates/editor-app/        eframe + egui-wgpu editor and S0 regression example
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
document baseline; the S1-B1 Move workflow and S1-B2a Duplicate/Delete and S1-B2b Rotate/Mirror workflows are implemented; the full V1 headless automation workflow remains incomplete.

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
cargo test --locked -p editor-app --example s0-demo native_gpu_coverage_regressions -- --ignored --nocapture
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

S2-B3 GeometryMetrics implemented；完整 S2-B3 在 renderer 修复前受 1000 对象原生门禁阻塞。
S2-B3.1 active：生产显示改用有序 world-space bins，原 renderer 保留为 reference；见 [ADR0020](docs/adr/0020-scalable-render-index.md)。最终 native 门禁以本轮独立 evidence 报告为准，不由代码实现推断通过。

S2-B3.2：viewport-local preparation/budget 与 opt-in 原生 Metal benchmark，见 [计划](docs/S2_B3_2_PLAN.md) 和 [ADR0021](docs/adr/0021-viewport-native-gates.md)。最终收口结论由同一 clean commit 的独立 evidence 提供；P100K、PMIX 全 AT-075、Windows 与 V1 仍未通过。
