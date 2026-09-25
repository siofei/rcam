# INFRA1 Runtime Diagnostics Foundation

阶段：S4 基础设施（S4-C1 后，S4-C2 前），仅 observability。
关联 R16/R17/R18/R19/R20/R21/R22；AT-063/066/069/077/082/083/086/090/097；回归保留现有全部用例、schema_version=2 与平台门槛。
允许模块：独立 rcam-diagnostics、editor-service 入口、editor-app 诊断 UI/worker/观测、rcam-project / gerber-io 纯计时、验证脚本和文档。禁止改动制造模型、几何算法、Undo、Snap resolver、工程安全语义。

输入参考：用户提供的 RCam_INFRA1_RUNTIME_DIAGNOSTICS_NEXT_TASK.md；其中 PASS 前提须以本次证据验证，不将文字声明当实测结果。

实施顺序：
1. 本地有界后台写入、操作 DTO/ring、panic fixture、隐私白名单 ZIP。
2. 服务修改、工程/Gerber 数值观测，来源随后台请求传递，Help 日志等级/目录/诊断导出。
3. 分阶段计时、Snap 限速观测、Renderer 事件和错误归类。
4. 自动化/原生/性能回归，完整 Source/Evidence ZIP、fresh extract 与哈希验收。

退出必须符合附件的全部 INFRA1 硬门禁。部分实现不能标记 PASS；完成后停止，不启动 S4-C2。

## Final closeout（2026-09-25）

本轮参考 RCam_INFRA1_FINAL_CLOSEOUT_NEXT_TASK.md，只补齐 INFRA1。新增稳定 ProjectSummary / 最多 256 层 LayerSummary，来源内容 SHA 前 16 位，纯数值 Gerber export timing，Recovery 状态事件，Renderer 异常限速和大型工程实测。固定 performance prefix allowlist；最多各一段历史日志，ZIP 解压内容总预算 100 MiB，超限显式 truncated_logs。

新增 LayerInfo.compatibility_issue_count 为只读兼容问题条目数（不是几何错误数）；摘要从当前 info/layers/只读 snapshot 构造，不扫描对象 JSON，不从日志逆向还原。应用在后台生成 ZIP。最终 clean commit 与 release/source SHA 必须绑定原生证据；全部硬门禁通过前保持 PARTIAL。
