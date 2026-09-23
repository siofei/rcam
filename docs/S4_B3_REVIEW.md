# S4-B3 — `.rcam` Project Lifecycle review

状态：**部分通过；最终原生 GUI/Recovery 复跑阻塞**。固定提交的自动化原始日志见 `evidence/s4b3-final-20260924-04/`；部分交付与 SHA-256 校验见 `exports/`。最终固定提交的原生界面复跑在 macOS 锁屏时中断，不能据此标记 S4-B3 PASS。前次提交 `45b26bd` 曾完成 New/Import/Save/Open/Block/Export 和受控重启 Recovery 实测，记录在 `evidence/s4b3-final-20260924-01/`；其二进制哈希与当前提交不同，不替代当前原生门禁。Windows 未执行；完整 V1、CORE10、P100K 和 S4-C 不在本结论内。

范围与 Rxx／AT-xxx 映射见 [S4_B3_PLAN](S4_B3_PLAN.md)，业务及恢复边界见 ADR 0033/0034。`ApplicationService` 受 `FileAccessPolicy` 约束，正式提供 Project New/Open/Save/Save As 与无界面 JSON 操作。Open 完整解码、迁移分派和验证后才切换会话；Save 使用同目录临时文件、fsync、持久字节重读解码和原子发布，成功后才更新路径、哈希与 dirty 基线。Gerber 仍仅 Import/Export。

| 门禁 | 结果 | 证据 |
|---|---|---|
| clean commit、fmt、check、clippy、workspace tests | PASS | `gates.json`、`00.log`–`03.log`、`clean-status-*` |
| project lifecycle、recovery、Block、多图层、codec 回归 | PASS | `04.log`–`09.log`、`native-probe/native_actions.log` |
| release build、依赖边界、source/package 脚本 | PASS | `09.log`–`14.log`、`binary-sha256.txt` |
| Mac Metal parity | PASS | `15.log`；沙箱首次适配器不可见的原始失败保留在前次运行 `evidence/s4b3-final-20260924-01/15.log` |
| 400×100 Block、两图层 encode/atomic save/open | PASS | `16.log`、`17.log`；固定 Apple M1 release 原始计时 |
| Mac 原生 Project GUI、Block、Gerber Export | 阻塞 | 最终固定提交尚未在解锁会话中执行；前次提交有原生记录 |
| 原生恢复：后台快照、受控重启、打开脏副本、另存为 | 阻塞 | 最终固定提交未执行；前次提交有原生记录 |
| 完整 Source/Public Evidence ZIP、fresh extract、双 ZIP SHA-256 | 部分 | 源码 fresh extract 已通过；部分证据包明确标记阻塞 |

独立测试覆盖：新工程跨会话 project id 不复用；失败 Open 保留原会话；外部修改、Save As 未确认覆盖和注入的校验/发布失败不破坏已有项目；无变化重复保存字节一致；图层视图/Grid/Snap、Block 与稳定 ID 往返；Solo 不持久化；恢复副本无项目路径且不能隐式覆盖原件。前次原生界面检查了 dirty 提示的取消／不保存／保存、Recent 列表和 Save As 文件选择器重复使用时的单个 `.rcam` 扩展名；当前提交仍需原生重跑。

运行环境与命令以 `environment.json` 和 `gates.json` 为准；未执行项为 Windows 真机、完整 V1/CORE10/P100K、Finder 文件关联及 S4-C。Recovery 为基础单快照机制，不提供版本历史。恢复的正常保存需显式选新 `.rcam` 路径；不会自动覆盖原项目。
