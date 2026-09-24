# S4-B3 `.rcam` 无损压缩局部验收

范围：R15/R16/R20/R21/R22；局部关联 AT-058/059/064/086/090/093。
允许修改：rcam-project ZIP codec、锁定依赖和许可记录、服务依赖门禁、相关测试与文档。
不进入 S4-C；不改变制造模型、JSON schema v1、坐标精度或 Gerber writer。

## 实现

固定 miniz_oxide 0.8.9 raw Deflate level 6，压缩无收益时使用 Store。新读取器兼容旧 Store。
解压前检查单项/总预算，固定输出缓冲；流结束、输入耗尽、实际输出长度、CRC 和 manifest SHA-256
均须通过。Service Save/Recovery 自动复用该 codec，原子文件替换和 dirty 行为保持。
旧程序仅支持 Store，不能打开包含 Deflate 的新工程；未替换旧源工程。

## 真实样本

输入：`tests/Untitled.rcam`，13 层，527,227 对象。
输出：`evidence/rcam-compression-20260925/Untitled-compressed.rcam`。
144,552,267 → 9,845,942 bytes，减少 93.1887%。
release 编码器 decode → encode → decode 完整模型相等；两次 encode 字节一致；
15 个 ZIP 条目解压后逐字节相等（包括 manifest），原文件 SHA-256 不变。
独立 Python zipfile CRC 及逐条目字节校验 PASS。

环境：macOS 26.5.1 arm64；锁定 Rust/Cargo 环境 `.tools/cargo`、`.tools/rustup`、`.tools/target`。
原始命令、退出码和耗时：`evidence/rcam-compression-20260925/results.json` 与 `recheck-results.json`。
逐条目哈希/大小：同目录 `size-and-integrity.json`。

初次全量测试暴露旧样本“Store 文件与压缩重存容器字节相等”的断言；保留旧 fixture，
改为语义完全相等、压缩变小、新编码确定性。未删样本、未降低制造几何门槛。

## 未执行与边界

未执行 Windows、原生 GUI 点击另存/打开、Metal parity 或完整 CORE10/V1；本轮未部署或重启运行中的应用。
压缩只降低磁盘存储，解码后的制造模型和内存规模不因此减少。
新 release 可执行文件：`.tools/target/release/editor-app`。

## 最终结果

PASS：fmt/check/clippy、workspace tests（611 passed，0 failed，26 ignored）、
automation_contract/headless_workflow、release editor-app build、真实样本 release 往返、独立 zipfile 核验。
忽略项保留原用例声明，不计为通过；本轮 opt-in 真实样本测试另行显式执行通过。
完整工作区复核退出码 0，首次失败日志保留用于追踪测试契约修正。
