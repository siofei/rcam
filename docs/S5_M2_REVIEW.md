# S5-M2 kickoff / M2-A implementation checkpoint

阶段：S5，Mac-first。基线 `c6f9920cd25190cd9c45a6c55c0e8c31c7a57938`；工作区 `/Users/lxf/.codex/worktrees/s5m2-async-lifecycle/rcam`，分支 `codex/s5m2-async-lifecycle`。用户先前要求的主目录历史快照为 `1bd8bf7`，不作为本阶段基线，不合入其旧阶段差量。

## 本闭环

- 真实 GUI 有界串行 worker 的全部请求携带 task_id、输入文档/制造 revision、workspace revision、generation、rule revision 与制造精度 SHA-256；执行前查询服务真实状态校验，完成带 result_version 与终态。GUI 安装时再次核对请求/当前上下文和结果身份。旧回包不清较新 busy。
- 标准库原子取消令牌区分 Queued/Running/CancelRequested/Committing/Completed/Failed/Cancelled。取消反馈不是终止证明；worker 清理后才发布 Cancelled。cancel 与 begin_commit 互斥，提交后 TooLate，错误保留真实结果。未扩展 HTTP/脚本、JSON jobs API、持久化 job registry。
- Gerber 批量导入在文件读取/解析边界、准备批次以及 history.add_layers 前检查取消；成功仍一个 Undo，已有文档取消零修改。首次导入失败清除私有空工程准备状态。文件授权检查仍保留。
- 框选复用现有解析几何，在每个对象检查取消，整次不返回部分 ID。Scene full build 每对象检查，取消的读任务恢复旧 view/viewport/ppm，清理临时缓存。关闭工程释放 WorldIndex/BlockDisplayCache；退出通知待办令牌。
- 制造修改继续经 ApplicationService；精度、Dark/Clear、Gerber writer、schema、shortcut configuration 均未改。K1 冻结模块未重构，main/modal 仅适配 task envelope 和 UI 取消入口。

## 验证记录

版本化原始命令、stdout/stderr、退出码、耗时、环境、基线和 manifest 存于本工作区 `exports/S5M2_A_*/`。`commands.json` 是实际执行记录；每次失败和早期检查均保留。最后验证汇总将写入同一 evidence 的 RESULTS.json，不能将本文视为所有门禁已经通过。

针对性覆盖：真实服务取消/提交竞争、导入原子性和 Undo；真实 Model 文档变更、Undo/Redo、关闭/替换、工作区/policy/generation、直接服务 revision 与缓存 view 不同、排队取消、较新操作替代显示任务、精确框选循环内取消及输出身份变异拒绝。自动化不替代原生操作和取消时限测量。

## 状态与剩余门禁

**S5-M2 = IN_PROGRESS；M2-A = bounded implementation，完整 M2-A acceptance PARTIAL。**

- 初始取消检查与对象循环已接入，单次第三方解析/OS read、索引构建、一个复杂制造对象和 metrics 内部仍没有细粒度中断。OpenProject、文字提交和其他已有制造/I/O 动作在执行前进入不可取消段；不声称全部长任务满足 2s。取消反馈 500ms / 释放 2s、真实慢 I/O 与原生 UI 尚未验收。
- 串行 owner 防止执行中并发修改；排队旧身份和读结果安装分别校验。尚未建立多 worker analysis/pattern 调度，不宣称未来尚不存在的 Pattern/Area 功能已验收。
- M2-B 100/500/1000/5000 拖动及性能、PMIX、PPOL reference truth、30min/1h/2h PSTRESS、surface/device recovery 均未执行。此 checkpoint 不标完整 S5-M2 PASS。
- Windows、第二台 Mac、完整 V1/CORE10 与 S5-K1 native debt 保持未执行/延期。无 push；未提交此 implementation，未生成正式发布包。

范围/映射以 S5_M2_PLAN、ADR0051、原附件为准；96 cases/schema_version=2/required_platforms 与原始阈值均未降低。

A2 follow-up: this document records the earlier implementation checkpoint. The measured cancellation changes and subsequent native/clean-freeze protocol are in `S5_M2_A2_PLAN.md` and `S5_M2_A2_REVIEW.md`; the versioned A2 package review is authoritative for final gates/identity. Earlier NOT_EXECUTED statements above are historical, not claims that later A2 evidence is absent.
