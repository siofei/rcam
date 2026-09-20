# RCam Gerber editor — Mac-first S4-A2（实施中）

S3-FINAL 正式收口基础编辑：GUI 制造修改统一走 `ApplicationService`；补齐标准 C/R/O/P
Flash 尺寸写时复制、单事务 `edit.batch`、完整事务边界的 Undo 预算淘汰、对象端点/中心优先
Snap、mm/in 显示以及同一 release build 的 Mac 原生 P1K/编辑/另存重开门禁。范围、证据映射和
边界审计见 [S3 coverage](docs/S3_FINAL_COVERAGE.md)、[service audit](docs/S3_SERVICE_BOUNDARY_AUDIT.md)
与 [final review](docs/S3_FINAL_REVIEW.md)。S4-A1 headless vector text core 已实现；S4-A2 正在实现正式 Text GUI / IME / Preview。Windows、完整 V1 与双平台验收仍未完成。

历史 `S2-B1/S2-B2/S2-B3/S2-C1/S2-C2` 名称和文件继续保留；其中编辑、Undo/Redo、
Grid/Snap/Measure 实际覆盖正式 S3 范围。自 S3-FINAL 起恢复 DESIGN_V1 的正式阶段编号。

S2-C2 将已有 `objects.rotate` / `objects.mirror` 接入属性面板：任意有限角、快捷 ±90°、
选择集制造边界中心/世界原点/自定义 Pivot，以及明确显示坐标的水平/垂直世界轴镜像。
多选只提交一个服务事务/Undo；Grid Snap 不量化 Transform 参数。见
[本轮计划](docs/S2_C2_PLAN.md) 与 [ADR0023](docs/adr/0023-s2c2-transform-gui.md)。

S2-C1 增加网格、显式 Grid Snap、光标毫米坐标与两点测距，见 [本轮计划](docs/S2_C1_PLAN.md)。
测距现可同时保留多条，并在线中显示距离与相对世界 +X 轴的逆时针角度；Esc 清除全部。
Grid/Measure 仅为视图状态；鼠标抓取点吸附，数值 Move 保持精确输入。

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

macOS 界面从系统字体加载中文，不打包用户字体；S4-A1 已支持中英文制造矢量文字核心；S4-A2 已接入 Text GUI、系统字体搜索列表（保留文件选择）、异步 Preview 和显式定位；最终验收状态见 S4_A2_REVIEW。
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
