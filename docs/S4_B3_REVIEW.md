# S4-B3 — `.rcam` Project Lifecycle review

状态：**PASS（Mac-first，含 µm DisplayUnit 预冻结修正）**。本轮修正的固定提交门禁、原生 µm/Recovery 及完整交付证据见 `evidence/s4b3-displayunit-final-20260924-02/`、`exports/S4B3_displayunit_closeout/`。上轮 `f4aa57e` 的主体生命周期证据见 `evidence/s4b3-final-20260924-05/`；其原生二进制 SHA-256 为 `6b050f7d012446f02ad7d2db926db8d842ed00dcebb74ae1438cc4a13f859a04`，但尚未覆盖 µm roundtrip，不能代替本轮证据。Windows 未执行；完整 V1、CORE10、P100K 和 S4-C 不在本结论内。

范围与 Rxx／AT-xxx 映射见 [S4_B3_PLAN](S4_B3_PLAN.md)，业务及恢复边界见 ADR 0033/0034。`ApplicationService` 受 `FileAccessPolicy` 约束，正式提供 Project New/Open/Save/Save As 与无界面 JSON 操作。Open 完整解码、迁移分派和验证后才切换会话；Save 使用同目录临时文件、fsync、持久字节重读解码和原子发布，成功后才更新路径、哈希与 dirty 基线。Gerber 仍仅 Import/Export。

## 2026-09-24 DisplayUnit / Recovery closeout

`rcam-project::DisplayUnit` 在 schema v1 中新增 `micrometers`，与既有 `millimeters/inches/mils` 一起逐项编码、解码并验证二次编码字节一致；默认仍为 mm，S4-B2 `sample.rcam` 与上轮 S4-B3 `project.rcam` 两份旧文件保持可读。GUI 的四单位一一映射到 Project，Open 后同步 Text 草稿显示单位。Service 四单位 Save As/Open 测试确认显示单位精确相等、制造 f64 mm 文档及 ManufacturingPrecision 未变、制造 revision 未推进，四份 Gerber 导出字节一致。关联 R08/R14/R15/R16；AT-045/062 与 S4-B3 局部工程门禁，不改变 V1 用例总体状态。

RecoveryWrite 只在后台 worker 成功回报且当前脏身份匹配后记录为已恢复。失败清除 pending；原 60 秒最小间隔继续限制重试。原生复验另发现：已备份的工程保存后重开，新改动可能复用旧 revision 组合，造成恢复写入被错误去重。因此干净状态同时清除脏身份和已备份身份。测试覆盖首次写入失败、同一 revision 再写成功及保存后去重状态清除，原始项目不被覆盖。原生流程再次执行 dirty → snapshot → 受控重启 → 恢复为 Untitled dirty → Save As，并核对源文件哈希与缓存清理。

本轮原始命令退出码、release 二进制哈希、原生操作/截图、四单位断言、恢复前后哈希、源码 fresh extract 和双 ZIP 逐文件校验均保存在上述新运行 ID。完整范围只授予 Mac-first S4-B3；Windows 与完整 V1 仍未执行。

## 上轮主体生命周期证据（f4aa57e）

| 门禁 | 结果 | 证据 |
|---|---|---|
| clean commit、fmt、check、clippy、workspace tests | PASS | `gates.json`、`00.log`–`03.log`、`clean-status-*` |
| project lifecycle、recovery、Block、多图层、codec 回归 | PASS | `04.log`–`09.log`；`native-probe/native_actions.log` |
| release build、依赖边界、source/package 脚本 | PASS | `09.log`–`14.log`、`binary-sha256.txt` |
| Mac Metal parity | PASS | `15.log` |
| 400×100 Block、两图层 encode/atomic save/open | PASS | `16.log`、`17.log`；固定 Apple M1 release 原始计时 |
| Mac 原生 Project GUI、Block、Gerber Export | PASS | `native-gui-smoke.json`、`screens/01`–`07`、`screens/11`–`14` |
| 原生恢复：后台快照、受控重启、打开脏副本、另存为 | PASS | `recovery-native-before-restart.json`、`recovery-native-after-save.json`、`screens/08`–`10` |
| 完整 Source/Public Evidence ZIP、fresh extract、双 ZIP SHA-256 | PASS | `source_fresh_extract_report.json`、`SHA256SUMS.txt`、`EVIDENCE.sha256` |

原生过程：New → Import `import.gbr` → Save As `project.rcam` → 隐藏图层 → 未保存提示 Cancel/Save → New → Open，确认隐藏状态恢复；随后显示图层并导出 `export.gbr`，工程仍 dirty。打开 `blocks.rcam` 时对未保存更改选择 Don't Save；Block 在 Filled/Outline/ZeroWidth 三种图层模式下实测，并导出展平 `block-export.gbr`。Recent 菜单只显示 basename；缺失文件给出错误，可移除失效路径，仍可打开有效路径。恢复验收在该 release 应用中产生脏项目后台快照，以受控 SIGTERM 中断并重启；启动提示出现，恢复为 `Untitled *`，另存为 `recovered.rcam` 后变干净。原 `blocks.rcam` 的 SHA-256 前后均为 `02e644032942954e78bb1433240590088008d07bd2878ed078aa5f8befd63dd1`，Recovery 缓存随成功保存清除。probe 无 service error 或渲染 blocked；截图均来自原生窗口。

独立自动化测试覆盖：新工程跨会话 project id 不复用；失败 Open 保留原会话；外部修改、Save As 未确认覆盖和注入的校验/发布失败不破坏已有项目；无变化重复保存字节一致；图层视图/Grid/Snap、Block 与稳定 ID 往返；Solo 不持久化；恢复副本无项目路径且不能隐式覆盖原件。原生截图不替代这些制造语义和文件安全断言。

运行环境与实际命令以 `environment.json` 和 `gates.json` 为准。未执行项：Windows 真机、完整 V1/CORE10/P100K、Finder 文件关联及 S4-C。Recovery 是基础单快照机制，不提供版本历史；恢复副本必须显式选择新的 `.rcam` 保存路径。
