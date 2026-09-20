# S2-C2 Rotate / Mirror GUI — Mac-first

2026-09-20；范围决策见 [ADR 0023](adr/0023-s2c2-transform-gui.md)。
本轮承接用户提供的 S2C2 下一任务说明与 S2-C1 `577b670` 复审；附件约束不替代实际执行证据。

阶段 S2-C2；需求 R10/R11/R14/R18/R19/R21/R22。
局部 AT-033/034/039/040/041/043/054/087/090/091。
允许模块：editor-core 只读 bounds 复用、editor-app 状态/属性面板/测试、阶段文档、manifest 与证据。
保持 `objects.rotate` / `objects.mirror`、制造模型、writer、renderer、Grid/Measure 和依赖不变。

## 可执行闭环

- SelectionSet union manufacturing bounds center 作为默认 Pivot；可选世界原点或自定义 X/Y mm。
- 任意有限角输入与 -90°/+90°按钮共用一个 `objects.rotate` 服务入口。
- Horizontal/Vertical 按钮显示并提交明确的 `y=center_y` / `x=center_x` 世界制造轴。
- 多选一次请求/事务/Undo；锁定、隐藏、跨层、RectangularSweep 非 90°、0°和非法数字整批拒绝。
- Grid Snap 不读取或改写 angle/Pivot/axis；无 Transform preview，只有 Apply 才提交。
- 成功后保留选择，刷新制造 scene/metrics；Undo/Redo、Save As/Reopen 复用原实现。
- 属性字段沿用 text-focus 快捷键保护；错误显示服务 code/message/details。
- 用户追加的 Measure 回归同时保留多条两点标注；每条线段中点显示距离与相对世界 +X 轴的角度，
  Esc 清除全部。仍为 app-only Overlay，不进入制造事务。

## 验证

先执行 `cargo test --locked -p editor-app` 的 S2-C2 纵向测试，再执行：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1b2b_transform_workflow
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo tree --locked -p editor-service -e normal
cargo build --release --locked -p editor-app
cargo test --release --locked -p editor-app native_metal_reference_production_pixel_parity -- --ignored --nocapture
```

项目锁定工具链使用 `.tools/cargo/bin`、`.tools/cargo`、`.tools/rustup`、`.tools/target`。
原生 GUI 证据使用公开 synthetic 文件，记录 commit/dirty status、macOS/M1/Metal、二进制及输入输出哈希；
缺少的手势或 Windows 证据明确写 NOT TESTABLE / not executed。

## 保留边界

不做 Preview、Rotate Handle、Angular/Object Snap、Scale/Skew、Group、文字、Final Layer Area、
DXF/SVG/PLT、P100K 优化或 Windows。本轮完成后停止；S4-A 需复审后另行开始。
