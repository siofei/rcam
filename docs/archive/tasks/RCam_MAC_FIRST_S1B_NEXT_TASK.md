# RCam 下一阶段任务：Mac-first S1-B 基础编辑闭环

> 当前平台策略：macOS Apple Silicon 为唯一当前开发与阶段验收平台。Windows x64 保留为后续兼容目标，但不再阻塞 S1-B/S2 的开发推进；不得把未执行的 Windows 用例标记为通过。
> 基线：`rcam-s1a1-20260915-4c18470b`。

## 1. 阶段判断

S1-A.1 的圆弧语义修正已经具备继续开发的条件：G75 非零 arc deviation、G74 least-deviation 圆心选择、G74 零扫角、Writer round-trip 均已有真实实现与专项测试。

当前已知兼容性缺口继续保留：
- CORE-03：无效 CreationDate metadata，Strict 模式拒绝；
- CORE-07：坐标字段超出 FS 声明宽度；
- CORE-08：G74 已通过，后续 Region topology 仍被安全拒绝；
- large-deviation arc 与 gerbv 存在独立解释差异。

这些问题暂不阻塞 S1-B 的编辑基础设施，但不得宣称 CORE10、完整 V1 或生产兼容性通过。

## 2. 本轮目标

实现第一个真正的、无 GUI 依赖的编辑闭环：

```text
Open supported Gerber
  -> Query object
  -> Move object(s)
  -> Undo
  -> Redo
  -> Validate
  -> Export to new path
  -> Reopen
  -> Compare manufacturing geometry
```

本轮只要求在当前已支持且 S1-A.1 可安全往返的 Gerber 子集上工作。

## 3. 必须实现的业务能力

### 3.1 `objects.move`

新增 ApplicationService 操作：

```text
objects.move
```

参数至少包括：
- document_id
- expected_revision
- layer_id
- object_ids[]
- dx_mm: f64
- dy_mm: f64

要求：
- 坐标统一使用 f64 mm；
- 空集合、NaN/Inf、未知对象、锁定图层等明确拒绝；
- 同一调用内多个对象原子提交；
- 成功后 revision +1；
- 一次调用只增加一个 Undo 事务；
- 失败不得产生部分移动、revision 变化或 Undo 记录。

### 3.2 Undo / Redo

新增：

```text
history.undo
history.redo
```

要求：
- Undo 恢复内容，但 revision 继续单调增加，不倒退；
- Redo 恢复同一事务；
- 新编辑发生后清理 Redo 分支；
- 删除/关闭文档后不得错误复用历史；
- Undo/Redo 不依赖 GUI 当前选择。

### 3.3 可编辑几何

第一步至少支持当前安全子集中的：
- Flash；
- Line stroke；
- Arc stroke；
- Region 整体平移。

移动必须更新：
- 实际制造几何；
- bounds / spatial data；
- Writer 所使用的模型；
- 不得修改 aperture 定义本身。

Arc 平移时 start/end/center 必须同时平移，保持方向、deviation、full/zero sweep 身份。

Region 内所有 edge 作为一个对象整体平移，不改变轮廓顺序与拓扑。

## 4. 安全导出闭环

沿用现有 `gerber.export_layer`，但必须验证编辑后的 revision：

1. 打开源文件；
2. 记录源文件 SHA-256；
3. 查询明确对象 ID；
4. 移动 `dx=+5.0mm, dy=-3.0mm`；
5. 验证目标对象变化、非目标对象不变；
6. Undo，验证恢复；
7. Redo，验证再次移动；
8. `document.validate`；
9. 导出到全新路径；
10. 重新 `document.open` 导出文件；
11. 验证重新打开后的制造几何等于 Redo 后的模型；
12. 验证源文件 SHA-256 未改变。

禁止用“文件生成成功”代替制造几何核对。

## 5. 必须新增的自动测试

新增独立测试文件，例如：

```text
crates/editor-service/tests/s1b_edit_workflow.rs
```

至少覆盖：

1. `move_flash_roundtrip`
2. `move_line_roundtrip`
3. `move_arc_preserves_arc_semantics`
4. `move_region_preserves_topology`
5. `multi_object_move_is_one_transaction`
6. `failed_move_is_atomic`
7. `undo_restores_original_geometry`
8. `redo_restores_moved_geometry`
9. `new_edit_clears_redo_branch`
10. `stale_revision_returns_REVISION_CONFLICT`
11. `export_reopen_matches_edited_document`
12. `source_file_is_never_modified`

至少一个测试必须走完整 JSON `execute_json()`，不能全部直接调用内部 Rust 方法。

## 6. GUI 暂时只做最小接线准备

本轮不要大规模开发 GUI，但应确保 `editor-app` 以后可以长期持有 ApplicationService，而不是只取一次 snapshot 后绕过服务修改文档。

允许做：
- 服务生命周期接线；
- 文档 ID / revision 保存；
- 只读当前对象 DTO 的最小适配。

暂不要求：
- 框选；
- 鼠标拖动；
- 属性面板；
- 文本编辑；
- 大规模 renderer 重写。

## 7. Mac-only 当前门禁

当前阶段只要求 macOS Apple Silicon：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo build --release --locked -p editor-app
```

并保留：
- macOS 版本；
- Apple Silicon 型号；
- Rust/Cargo 版本；
- build SHA / 源码哈希；
- 原始测试日志；
- headless workflow 日志。

Windows：
- 当前记为 `deferred` / `not executed`；
- 不作为本轮退出条件；
- 不得删除既有 Windows 验收用例；
- 不得为了 Mac 快速实现而把 OS API 写进 editor-core/editor-service；
- 平台专用代码继续隔离在 `editor-app/platform` 或等价边界。

## 8. 当前已知兼容性问题的处理

本轮不要顺带扩大 Gerber 兼容范围。

对 CORE-03/07/08：
- 保持当前 fail-closed；
- 保留诊断；
- 不修改 CORE10 身份、输入或真值；
- 不将其失败算作 S1-B 编辑功能失败；
- 后续另立兼容性任务处理。

对于 large-deviation arc 独立参考差异：
- 保留现有证据；
- S1-B 测试优先使用已验证稳定的 Arc fixture / CORE-06 / CORE-09 可控对象；
- 不以编辑功能实现为由改变 arc interpretation。

## 9. S1-B1 退出条件

满足以下条件才进入 Mac GUI 基础编辑：

- `objects.move` 已实际实现并进入 capabilities；
- Undo/Redo 已实际实现并进入 capabilities；
- Flash/Line/Arc/Region 的 Move 测试通过；
- revision 与冲突语义通过；
- 失败事务原子性通过；
- 编辑后 Writer → Reopen → geometry compare 通过；
- 原文件保护通过；
- editor-service 正常依赖闭包仍无 egui/eframe/wgpu/winit；
- macOS 上 workspace 全部测试通过；
- 没有把 CORE-03/07/08 或 Windows 记为通过。

## 10. S1-B1 之后的顺序

```text
S1-B1  Move + Undo/Redo + Export/Reopen
  -> S1-B2 Duplicate/Delete/Rotate/Mirror
  -> S2-Mac 文件打开 + 图层 + 点选 + 数值编辑
  -> S3-Mac 鼠标拖动/框选/Grid/Snap/属性面板
  -> S4-Mac 中文矢量文字 + 完整保存流程
  -> S5-Mac Production Renderer + 大文件性能
  -> S6 兼容性补齐 / CORE10 10/10
  -> Windows x64 适配与原生验收
```

Windows 适配前不得主动引入 Linux/WSL2 产品支持。

## 11. 给 Codex 的直接任务

```text
先阅读 AGENTS.md、docs/DESIGN_V1.md、docs/AUTOMATION_API.md、docs/S1_A_REVIEW.md、docs/S1_A1_REVIEW.md 和 ADR 0007。

平台策略已调整：当前只以 macOS Apple Silicon 作为开发与阶段验收平台。Windows x64 延后到 Mac 基础功能完成后适配；现有 Windows 用例保留但本轮不作为门禁，不得标记为通过。不要引入 Linux/WSL2 产品支持。

本轮实施 S1-B1，只做第一个真实编辑闭环：objects.move + history.undo/history.redo + 编辑后 validate/export/reopen/geometry compare。

不要扩大 Gerber 兼容范围，不处理 CORE-03/07/08，不实现脚本解释器/HTTP/RPC，不大规模重写 GUI 或 renderer。

所有编辑必须通过 ApplicationService 和原子命令进入 editor-core，GUI 不得直接修改文档。移动 Arc 时同时移动 start/end/center；Region 整体移动保持拓扑。成功编辑推进 revision，Undo/Redo 也推进 revision；失败不产生部分修改或历史项。

新增独立 s1b_edit_workflow 集成测试，覆盖 Flash/Line/Arc/Region、批量移动、失败原子性、Undo/Redo 分支、revision conflict、编辑后导出重开和源文件不变。

在 macOS arm64 实际执行 fmt/check/clippy/test/release build，保留原始日志与源码哈希。完成 S1-B1 退出条件后停止，不自动扩张到完整 GUI。
```
