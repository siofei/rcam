# INFRA1 Runtime Diagnostics Review

状态：实施中 / PARTIAL，**不是 INFRA1 PASS**。
平台：macOS arm64；Windows deferred / not executed。

已接入：后台滚动日志、bounded ring、服务 BEGIN/END、实际修订号与错误码、GUI 来源传递、Help 日志等级/目录/后台诊断 ZIP、独立 child-process panic、项目 codec 数值 timing、Gerber 导入汇总、Snap 限速观测。

开发证据：`evidence/infra1-development/`。首轮服务测试请求错误与编译 fixture 字段缺失保留原始日志，修复后的结果另存。最终门禁状态以该目录 gates.json 和原始日志为准；开发门禁不替代 final clean commit 门禁。

仍需完整收口：
- 完整 Gerber 导出内部 writer/reparse/compare 分段、Recovery prompt/retry 与 Renderer device-lost 等完整异常覆盖；
- 更完整项目/图层独立摘要和真实大型工程压缩比观测；
- 原生完整操作顺序、菜单导出和真实压缩工程计时（当前 Mac 锁屏，CUA 无法解锁）；
- 最终 clean commit 回归和可复现打包的结果以导出目录报告为准。

隐私限制：不记录 panic 任意字符串/回溯；保留消息哈希与源码 basename/行列；诊断包不含源文件/工程/字体/截图/用户文本/制造 geometry。不改变业务结果，不上传数据。

开发验证已通过：真实 Move / Rotate revision failure / Undo / Redo / Text Create / .rcam Save/Open 诊断测试；滚动/ring/隐私 ZIP；独立子进程 panic；服务正常依赖边界；工作区 check 和 Clippy；release build。

Mac 原生启动已确认 Apple M1 / Metal、2× scale，活动日志含 session.start、gpu.initialized、document.new begin/ok。原生 Metal coverage regression 通过。完整 UI 操作链未执行，不能用启动或 headless 测试替代。

release synthetic overhead（10000 次）：Off 254 µs，INFO operation BEGIN/END 20271 µs，DEBUG 限速 Snap 289 µs。三项工作负载不同，不能当作严格相对开销比或交互 SLA。ring_count=1000；原始输出见 overhead.log。

日志导出已改为 writer-thread snapshot barrier，避免活动日志轮转竞争。最终源码绑定验证使用 `scripts/run_infra1_gates.py`，早期验证目录保留并标注 superseded。
