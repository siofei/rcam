# S1-B1 Mac-first 编辑闭环审查

日期：2026-09-15；运行 ID：`s1b1-20260915-final`。
任务：[RCam_MAC_FIRST_S1B_NEXT_TASK.md](../RCam_MAC_FIRST_S1B_NEXT_TASK.md)。
开始前已提交 S1-A.1 与任务文档：`4bf84a0`；该提交前实际 workspace test 退出 0。
本轮依据 [ADR 0008](adr/0008-mac-first-s1b1.md)，只完成 S1-B1，不进入 S1-B2 或 GUI 编辑。

## 结论

**PASS：macOS arm64 的 S1-B1 实现门禁。**
工作区 105 通过、0 失败、2 忽略；独立 s1b_edit_workflow 专项 17/17 通过。
完整 AT、CORE10、生产兼容性及双平台 V1 **未通过/未完成**。
Windows 本轮 deferred / not executed，不作为当前阶段退出条件。

## 范围与改动

阶段 S1-B1；R01/R04/R10/R11/R14/R15/R19/R21/R22。
关联 AT-013/014/030/039/040/041/042/054/086/088/089/090/091/092/093/095/097 的局部检查。
允许修改并实际涉及：editor-core/editor-service、editor-app 服务生命周期、gerber-io 模型构造适配、
对应测试、设计/用例/API/能力/决策/审查文档及源码清单。无新增依赖、锁文件或工具链变更。

- editor-core 新增原子多对象平移及受限历史。候选全部验证后才提交；历史保存选中几何前后值，
  不复制整份文档，不通过逆运算撤销。保留对象/图层 ID、光圈定义、曝光及源顺序。
- Flash、Line、RectangularSweep、Arc、Region 整体平移。圆弧起终点/圆心共同移动，
  保留方向、full/zero sweep 与源分辨率；不改原有 arc interpretation。
- ApplicationService 和 execute_json 实际接入 objects.move、history.undo/history.redo。
  成功推进一次 revision；冲突、空集、重复/未知 ID、锁定层、非有限/超范围数值和预算错误整体拒绝。
  历史以文档会话隔离；新成功编辑清空 Redo，失败保留 Redo。
- 增加 document.close 的明确放弃策略。关闭后历史移除，新会话 ID 不复用。
- document.get 返回 dirty/undo_entries/redo_entries。脏状态使用流式 SHA-256 内容基线，
  新路径导出成功更新基线，失败和待确认不改变基线；Undo/Redo 保留保存状态的正确判断。
- 查询直接读取当前模型，原先不存在独立持久 bounds/空间索引缓存，故不会保留旧位置；
  移动后查询、撤销恢复和旧分页游标冲突均已检查。
- GUI 长期持有 ApplicationService，文档 ID/revision 仍由现有只读 snapshot 携带。
  窗口仍是 S0 演示，没有编辑按钮或绕过服务的模型写入。

## 验证内容

独立测试文件：[s1b_edit_workflow.rs](../crates/editor-service/tests/s1b_edit_workflow.rs)。
17 项测试覆盖任务指定的 12 项，并补充关闭隔离、历史数量预算、实时查询、保存失败、矩形扫掠/宏孔洞。
所有主要流程经 execute_json 使用真实打开/查询所得 ID，实际写新文件并重新 document.open。
完整混合样本走 (+5,-3)mm Move → Undo → Redo → Validate → Export → Reopen，
独立断言 Flash/Line/Arc/Region 的具体坐标/方向/宽度/轮廓边、非目标对象和曝光。
单弧专项包含 G75 exact/small deviation/full/near-full、G74 顺逆 quarter/zero；
零弧输出允许既有 Writer 的等价零长度 Line，但不会成为整圆。
Region 弧边与 cut-in 孔洞、局部宏孔洞用固定物理点和移动前后采样网格检查；不从像素写回制造数据。
源文件字节和重新打开时的 SHA-256 都与移动前相同。

另外两个服务内部测试通过真实 JSON/服务入口验证锁定层拒绝、超过 2^53 的 revision 和 u64 溢出。
锁定状态只由测试在私有模型注入：本轮不宣称 layer.update/GUI 锁定控件已经交付。
历史公开上限：10000 对象/次、100 事务、64 MiB 保守计费（含 Redo 和提交峰值）；超限拒绝，不静默删历史。

首次专项 14/15 通过，失败是新测试误把 contains 当作“矩形包含对象”。
既有实现表示“对象包含矩形”，本轮未改关系语义；测试改用 intersects 验证位置更新。
首轮失败日志保留于 `evidence/s1b1-precommit-20260915/first-workflow.log`，未覆盖。
后续 workspace 和最终专项均通过。

## 环境、命令和原始证据

macOS 26.5.1（25F80），arm64，Apple M1；Rust/Cargo 1.89.0；使用既有 `.tools` 缓存。
CPU 型号读取首次被沙箱拒绝，随后只读提权实际取得 Apple M1。不是干净环境构建。
所有以下命令实际退出 **0**；证据根目录 `evidence/s1b1-20260915-final/`。

| 命令 | 原始日志 |
|---|---|
| `cargo fmt --all -- --check` | `gates/01.log` |
| `cargo check --workspace --all-targets --locked` | `gates/02.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | `gates/03.log` |
| `cargo test --workspace --locked` | `gates/04.log` |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | `gates/05.log` |
| `cargo test --locked -p editor-service --test automation_contract` | `gates/06.log` |
| `cargo test --locked -p editor-service --test headless_workflow` | `gates/07.log` |
| `cargo tree --locked -p editor-service --edges normal` | `gates/08.log` |
| `cargo build --release --locked -p editor-app` | `gates/09.log` |

正常服务依赖树经读取/匹配检查，无 egui/eframe/egui-wgpu/wgpu/winit。
两个默认忽略项为原生 GPU 和需显式授权私有路径的 CORE10；本轮未运行，不计成功。
旧 headless_workflow 继续通过；真正的编辑闭环在独立 s1b_edit_workflow，不冒用旧用例名称表示编辑已验。

- `environment.json`：系统、CPU、Rust/Cargo、父提交、平台说明。
- `tested-source-hashes.json`：本轮受测 crate 源码/Cargo.lock/工具链哈希；最终检查确认未再修改。
- `release-sha256.json`：实际 release 二进制哈希。
- `gates/commands.json`：每条命令的退出码、耗时和日志位置。
- `workflows/*/requests.json`：真实 JSON 请求/返回；同目录保存公开合成输入和导出文件。
- `workflow-file-hashes.json`：全部上述 Gerber 文件哈希。
- `acceptance-results.json`：schema_version=2，当前切片统计与 96 用例/185 平台槽。
  完整 AT 均未签署通过；局部证据另行关联。96 个 cases 及 required_platforms 与父提交逐项比较完全不变。

证据目录仍默认 Git 忽略；没有上传 Gerber 或私有文件。本次不制作发行 ZIP。
根目录 MANIFEST.sha256 已重新生成并使用 `python3 scripts/source_manifest.py --check` 核验。

## 剩余限制

- CORE-03/07/08、large-deviation 独立工具差异：保留历史失败，未扩大兼容范围，未重新授予通过。
- 当前导入为单层文档；未来多层导入必须扩展逐层保存基线，不能仅因某层导出就清除整文档 dirty。
- 服务仍同步；内容哈希每次读取制造模型，历史和查询未进行大文件性能验收。
  正式 GUI 接入需沿设计移至后台，不能在 UI 主线程直接运行重型操作。
- Duplicate/Delete/Rotate/Mirror、edit.batch、完整 GUI、中文文字、后台任务和取消尚未实现。
- 本轮 GUI/IME/Metal 交互、性能、Windows 原生测试未执行；无界面测试不能替代这些证据。

退出：S1-B1 完成，按任务停止；后续顺序为 S1-B2，未自动实施。
