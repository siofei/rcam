# INFRA1 Runtime Diagnostics Review

状态：**PASS（Mac-first bounded）**。Windows deferred / not executed；不是双平台 V1、完整 CORE10 或 P100K 性能通过。不启动 S4-C2。

范围：S4 基础设施；R16/R17/R18/R19/R20/R21/R22；AT-063/066/069/077/082/083/086/090/097。96 个正式用例及平台要求不变。仅修改诊断、只读摘要、数值计时、UI 观测与验收支持，不修改制造几何、Undo 或 Snap resolver。

## 实现与核验

- 2048 条非阻塞队列，单事件 4096 字节，ring 1000 条；运行日志 20 MiB × 5、操作日志 10 MiB × 5；普通事件没有同步 fsync。崩溃报告最多 20 个，任意 panic 文本/绝对路径不入包。
- 服务操作记录实际 BEGIN/END、成功/失败、修订号与来源。打开/创建记录真实返回修订号；失败保留实际当前修订。F3 为 shortcut、测距点为 canvas、数值移动为 modal、撤销/导出为 toolbar、工程保存/重开与诊断导出为 menu、恢复为 recovery。
- ProjectSummary 从当前只读状态生成：工程/文档哈希、修订、脏状态、层/对象/Block 数、制造精度、显示单位与兼容层数。LayerSummary 最多 256 层，报告 actual_count/truncated；名称哈希，合法内容 SHA-256 前 16 位。畸形来源哈希省略。无对象 JSON、顶点、字体、用户文本或源路径。
- Gerber import 记录可关联的内容哈希。Export 记录 flatten/normalize/writer/reparse/semantic_compare/readback/publish、输出字节、精度、兼容覆盖/警告数、Block flatten 数与结果。gerber-io 仅输出纯数值 timing，不依赖 diagnostics。
- Recovery 覆盖发现、提示、调度、失败、重试、成功、请求恢复、恢复结果与保存后清理。回归证明失败不标记 recovered，后续重试成功才标记。原生受控快照/重启/恢复证明原工程哈希不变，恢复工作区仍 dirty。
- Renderer display_prepare_failed / last_good_frame_fallback / resource_limit / surface_error / device_lost 有结构化 producer，每类型每秒最多一次；1000 次 fallback 不产生 1000 条日志。保留框架 surface error 行为。device-lost 以 producer 测试 + 正常原生 Metal 验证，不人为破坏 GPU。
- ZIP 白名单包括明确 schema 的工程/图层/兼容/性能摘要、当前日志、各一段最近滚动日志、最多三个 crash。100 MiB 解压内容预算，省略日志时标记 truncated_logs。performance 有明确 prefix allowlist，不吸收任意未来 metrics。

## 原生与大型工程证据

Apple M1 / macOS arm64 / Metal / Retina：真实 release 完成 Import → F3 → 两点 Measure → Move → Undo → Save .rcam → Open .rcam → Export Gerber → Help 导出诊断包。通过日志和 ZIP 重建顺序、operation_id、source、修订及结果。Debug 重启保留，Trace 重启回到 Info；Help 打开本机 `~/Library/Logs/RCam`。

13 层、527,227 对象真实工程另存/重开：144,550,605 bytes 解压内容约压缩为 9,845,976 bytes（约 6.81%）；原文件不变。精确最终数值与各次时间以交付中的 large-summary.json / performance_summary.json 为准。公开包仅含数值、计时与哈希，不包含私有工程/源 Gerber/制造截图。

计时为阶段 elapsed µs，部分包含子阶段，不可简单相加；性能是本机固定样本观测，不是跨机器 SLA。INFO 操作链未观察到应用卡死；文件选择器/自动化工具等待不作为应用性能成绩。

诊断 ZIP 自身包含导出 BEGIN；durable 成功 END 在安全发布后写入活动日志，随原始操作日志交付，避免提前伪造成功。

## 交付与证据绑定

最终版本以 `exports/INFRA1_FINAL_<shortsha>/` 内 REVIEW、SHA256SUMS、source_fresh_extract_report 和完整 Source/Public Evidence ZIP 为准。绑定完整 commit、release SHA-256、Source ZIP SHA-256、clean before/after、tested-source 哈希；fresh extract 核验 source_manifest/test_package_source 和全部 payload 一致。

预验收原始记录：`evidence/infra1-closeout-bea0a32/`（原生链、Recovery、日志等级、大型工程、Metal），`evidence/infra1-closeout-51e16f9/`（干净完整门禁、第二次原生链及 fresh source 验证）。开发中因新增 layer_summary 导致旧 8 文件断言失败，已改为完整 9 文件白名单；失败日志保留，不冒充最终通过。最终提交再次执行完整门禁和原生链，证据另存，不覆盖这些记录。

剩余边界：Windows 未执行；没有硬件故障注入/长时间 soak 结论；未统一捕获任意第三方 tracing；不自动上传。S4-C2 必须另行立项。
