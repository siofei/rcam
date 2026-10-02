# ADR 0051 — S5-M2 task identity and cooperative cancellation

2026-10-02，M2-A accepted for implementation。

继续单 worker、有界请求/结果队列与 ApplicationService 制造修改边界，不引入另一套业务流水线。任务身份是输入文档会话与单调 revision、workspace revision、generation 和 policy generation 的值快照；结果必须记录输出 revision。序号只负责请求顺序，不能替代文档/策略校验。

CancellationToken 使用标准库共享原子状态：可取消 → cancel requested → worker清理后cancelled，或可取消 → commit started；两者互斥。提交后的取消报告 TooLate，不假装回滚。读任务在安装结果前也进行同一裁决；取消/过期的临时结果释放，之前的可用视图保留。服务的同步入口仍可调用；带任务的入口显式传入 token，不使用全局隐式 cancellation，也不增加 HTTP/脚本/runtime。

多版本 policy 数据是内部值上下文，不新增 Pattern/Rule 产品 API 或能力宣称。长任务在受控检查点退出；第三方整段解析/OS read 等不可中断段必须列为剩余限制，直到实测满足 2s 门禁。规则与几何策略不允许靠制造 revision 推断。

## A2 measured cancellation closeout

The REAL021 semantic interpreter/final validation exceeded the bounded cancellation window. Add checkpoints to those owned loops and preserve the locked third-party parser. Its observed maximum combined callback gap is recorded with fixture hashes in the A2 evidence; no universal slow-device guarantee is inferred. Ordinary callers retain the original synchronous APIs. Cancellation cannot be downgraded into compatibility fallback or a partial import.

Native race synchronization is internal-evidence only: it exposes Committing and delayed reply delivery without adding a second service writer. The A2 task book and review explain the measured input/readback/worker clocks and distinguish injected waits from production latency. Terminal state follows rollback snapshot release. Committing Cancel returns TooLate with explicit UI feedback. Source/Evidence require the same clean commit and fresh extraction; a dirty implementation checkpoint is never acceptance.
