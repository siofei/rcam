# S4-B3 — `.rcam` Project Lifecycle（Mac-first）

阶段：S4-B3，S4-B2 后、S4-C 前。关联 R02/R07/R11/R14/R15/R16/R18/R20/R21/R22；AT-005/021/041/058/059/060/061/062/064/065/086/087/090/093/094/095/096。此处为阶段局部映射，不改变 V1 验收基线或用例状态。

目标：将 S4-B2 `.rcam` v1 codec 接入 `ApplicationService` 和原生 GUI，完成 New/Open/Save/Save As、项目 dirty/关闭保护、最近工程、恢复快照、版本分派边界。Gerber 保持 Import/Export；Export 不改 project path/dirty。当前只验 macOS；Windows、S4-C、schema v2 不在本轮。

允许修改：`editor-service` 工程会话与受控文件 I/O、`editor-app` 文件菜单/弹窗/本机偏好/恢复、`editor-core` 文件 CommandId、`rcam-project` 现有 codec 的必要修正、对应测试和文档。制造编辑仍经 `ApplicationService`，GUI 不直接序列化工程。

门禁：从固定提交运行 fmt/check/clippy/workspace test、`project_lifecycle`/recovery/`rcam-project`/Block/Multi-layer workflow、release build、source/package gates，以及 Mac Metal parity、原生 New→Import→Save→Open→Export 和 Recovery GUI smoke。证据和未执行项按运行 ID 保存到 `evidence/`，交付副本到 `exports/`。只有所有 S4-B3 Exit Gate 实测通过才能标记 PASS（Mac-first）。

决策见 [ADR 0033](adr/0033-project-lifecycle.md)、[ADR 0034](adr/0034-project-recovery.md)。执行记录见 [S4_B3_REVIEW](S4_B3_REVIEW.md)。
