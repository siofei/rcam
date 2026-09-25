# ADR 0037：本地运行诊断

- 状态：已接受 / INFRA1 PASS（Mac-first bounded）；Windows deferred。
- 日期：2026-09-25
- 关联：S4 基础设施；R16/R17/R18/R19/R20/R21/R22；AT-063/066/069/077/082/083/086/090/097。

独立 `rcam-diagnostics` 不依赖窗口/GPU；`editor-core` 不依赖该库。服务公共修改入口包装实际业务调用，记录 BEGIN、成功/失败、实际 revision 和稳定错误码；不序列化参数、错误详情、字体、用户文字或制造对象。来源用 enum 随 worker request 传递。

第一步复用 std bounded channel、BufWriter、现有 serde 和项目 ZIP 编码器，无新外部依赖。该实现尚未接入 tracing subscriber，不能宣称已统一捕获第三方 tracing。后续若接 tracing，仍须执行相同 typed allowlist，不能默认采集任意 Debug 字段。

运行日志 20 MiB × 5、操作日志 10 MiB × 5（均含当前文件）；2048 条非阻塞队列；单事件最多 4096 字节；最近操作最多 1000 条；250 ms 刷新，正常关闭 join/flush。队列满丢弃计数，I/O 错误计数，不传播到业务操作。崩溃报告最多 20 个，try_lock 获取最近操作，直接同步写；panic 自由文本仅保留有界哈希，位置只保留源码 basename/行/列，绝对源码路径与 backtrace 当前省略，避免泄漏用户数据。

默认日志目录 `~/Library/Logs/RCam`。INFO 默认，DEBUG 持久化在 AppPreferences，TRACE 仅本次（持久化为 INFO）。项目不保存日志偏好。Snap 普通查询不写 INFO；慢查询/高交点数 WARN 和状态切换 DEBUG 按调用点每秒最多一次。仅观察，不截断交点计算、不改变 Snap 结果。

诊断 ZIP 由用户显式选择新路径，后台生成、create_new，拒绝覆盖。白名单仅包含程序诊断文件、已知数值摘要、最近三个 crash；不扫描源目录，不包含 Gerber、工程、字体、截图或 geometry。日志仅本地，无自动上传、遥测或网络调用。ZIP 取活动日志和每类最近一段滚动日志；内容上限 100 MiB，超过预算省略日志并在 manifest 标记 truncated_logs。

项目 codec 仅暴露线程局部、固定键的数值 timing DTO；不依赖诊断 I/O。JSON/Deflate/inflate/验证计时不参与序列化与制造结果。

Windows、S4-C2、Grip/Block Editor、快捷键设置、Command Palette 均不进入此范围。INFRA1 原生证据、完整细分观测与可复现最终包未齐全时必须保持“部分”。

依赖审查：rcam-diagnostics 的正常闭包只新增本项目 crate（std + serde/serde_json + editor-core hash + rcam-project ZIP），无窗口、GPU、dialog、网络依赖。服务 dependency_boundary 仅将此明确审查的 crate 加入 allowlist，禁止包检测保持不变。

Final closeout 补充：typed ProjectSummary/LayerSummary 仅含计数、版本、单位、状态与哈希，最多 256 层并报告 actual_count/truncated。performance_summary 只纳入 project.open/save、gerber.import/export、render_index.build 和 snap.query 前缀；compatibility_summary 只含类别计数和来源哈希。Gerber writer 内部采用纯数值线程局部计时，不引入 diagnostics 依赖。Renderer 异常各类型每秒至多一次；device-lost 忽略正常 Destroyed，surface error 保留框架默认处理。Recovery 写失败不改变 recovered identity，后续重试成功才更新。

计时口径：stage 是单调时钟 elapsed µs，可能包含子阶段，不可简单相加（例如 container_build 包含 Deflate；atomic_publish 包含临时写入/同步/最终复读）。Gerber reparse/semantic_compare 汇总本次 writer 自检与磁盘复读自检；readback 包含两次必要读取。日志对象数为语义对象数，不是 renderer primitives。Layer compatibility_issue_count 是已有元数据条目数，不是每个原始命令出现次数。

来源哈希字段只接受 64 位十六进制 SHA-256，输出前 16 位；工程中畸形 provenance 不能将路径或任意文字送入摘要。
