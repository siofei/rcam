# ADR 0001：S0 技术验证与服务边界

状态：接受；日期：2026-09-14。关联 S0、R01/R05/R06/R21/R22，AT-003/011/017/018/085–097。

采用设计1.1的 Rust/gerber-parser/gerber-types、自有 f64 毫米模型、egui/eframe/wgpu 路线。S0 只验证有限小样，不提前承诺 V1 标准子集兼容性或生产输出。

editor-core 与 editor-service 的正常依赖不含 UI/GPU。第三方 AST 只在 gerber-io 解释；UI 只消费服务 DTO/不可变快照，正式编辑后续仍须通过 ApplicationService。渲染共享 eframe Device/Queue，自定义 Callback 使用锁定版本官方 API。

公共 DTO 使用 api_version=1，request_id 关联请求，自有不透明字符串 ID，距离 _mm、角度 _deg；revision 为十进制整数字符串。当前只读技术验证不能伪称已实现编辑、文件权限或导出操作。能力表只列真实操作；未知字段、操作、版本明确拒绝。

后续每文档串行提交，expected_revision 在准备前与提交前校验；Undo/Redo 递增 revision，脏状态按内容基线。批次内仅可逆内存编辑，失败整体不变，一次成功占一条 Undo。文件 I/O 不入批次。

后续路径由主机授权，服务无弹窗/stdin，覆盖默认拒绝、只允许明确目标身份授权，元数据丢弃逐类确认；所有导出从制造几何快照验证后同目录临时文件安全替换。尚未完成平台故障验证前禁止自动覆盖。

不实现脚本语言、正式 CLI、网络服务、插件运行时；不添加 Linux 支持或 CI。完整 S0 的样本、规范、许可和双平台缺口继续作为门禁，不随小样成功自动关闭。
