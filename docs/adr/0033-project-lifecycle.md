# ADR 0033 — `.rcam` Project Lifecycle

状态：Accepted（S4-B3，Mac-first）。关联 R07/R11/R14/R15/R21/R22；AT-041/058/059/060/061/086/087/090/093/095。

`ProjectSession` 随 `ApplicationService` 文档记录保存稳定 project id、独立 `.rcam` path、最近成功保存文件的 SHA-256、持久状态基线和 workspace 设置。新建工程生成 128-bit 非确定性 ID，避免不同应用会话都把 `doc-1` 当作同一个工程；`doc-N` 仅是本会话文档句柄，不能充当 Recovery 键。Gerber 导入来源只作为逐层 provenance；Gerber 导出目标不成为工程路径。`project_dirty` 比较制造内容哈希及 `.rcam` 会持久化的精度、图层样式/顺序、Grid/Snap 等状态。Active Layer 和 camera 会被保存，但普通切换/导航不触发 dirty；Solo/Selection/Undo 不持久化。AppPreferences（Recent、面板宽度、最近颜色和 keymap override 位置）不进入工程。

Open 由访问策略限定路径，先进行有预算的完整 ZIP/manifest/schema/语义解码，再构造候选 service 记录。GUI 在候选渲染验证后才交换当前会话；失败保留原文档、路径和 dirty。当前只接受 v1，其他版本经 `migrate` 分派后安全拒绝，不虚构 v0。

Save 使用 `rcam-project::encode_v1`，编码结果先解码比较；同目录临时文件完整写入并同步，重新读取解码后才发布。新文件用 hard-link no-clobber 发布，现有文件须经用户替换确认或已有项目路径，且已有项目路径保存前后复核磁盘 SHA-256。发布后再读取、校验，成功才移动 saved baseline/path/hash。Save As 失败保留原路径和 dirty。无路径的 headless `project.save` 返回验证错误，不弹文件选择器。

格式本身仍为 ADR 0031 的 schema v1；不新增 Gerber Save As、Drill、嵌套 Block 或云同步。
