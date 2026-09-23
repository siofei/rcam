# ADR 0034 — 本机 Project Recovery

状态：Accepted（S4-B3，Mac-first）。关联 R15/R16/R20；AT-058/064/065/082/094。

恢复副本存于用户 `Library/Caches/RCam/recovery`，绝不写在原工程旁边。dirty 后静置 30 秒、两次快照至少间隔 60 秒，GUI 将工作交给既有 model worker；该 worker 顺序处理操作，较旧任务不能晚于较新任务写回。快照复用 service 的 `.rcam` 编码与安全解码，配套有界 JSON 元数据包含 project id、原工程路径（仅本机）、时间、应用版本、来源工程哈希、快照哈希、修订号。元数据最后原子发布；不完整快照不会被发现。

启动时最多扫描 100 个元数据文件；损坏或过大的记录跳过。用户可将通过完整 codec 校验的副本作为**未保存**会话打开；原工程路径不自动成为 Save 目标。忽略前二次确认，并只删除该 project id 的恢复记录。正式 Save 成功后只清理匹配恢复记录。公共证据不得包含本机绝对路径。

本阶段只保留每个 project id 的最新快照，不做版本历史、云同步或自动覆盖正式工程。Windows 恢复路径和原生体验待后续验收。

S4-B3 closeout：只有 worker 回报 RecoveryWrite 成功，才把该 project/revision/workspace revision
记为已恢复。写入失败时清理 pending，保留 dirty；原有至少 60 秒间隔阻止忙循环，同一身份可在间隔后重试。
