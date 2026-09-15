# ADR 0009：S1-B1.1 收口与 S1-B2a 原子复制／删除

日期：2026-09-15。依据根目录 S1-B2 任务和 S1-B1 复审；起点 9675664。
阶段：S1-B1.1 → S1-B2a；不自动进入 S1-B2b 或 S2。
需求：R01/R04/R05/R09/R10/R11/R14/R15/R16/R19/R21/R22。
局部用例：AT-030/031/035/036/038/039/040/041/042/054/086/088/089/090/091/093/095/097。
允许修改：editor-core 模型与历史、gerber-io 来源适配、editor-service DTO/公共入口、
相关测试、docs/README、任务归档、源码清单脚本和脱敏 evidence-public。
不修改生产渲染、字体、GUI 编辑、解析能力范围或私有样本。无新增依赖。

## 契约

- 精确零 Move 或所有目标制造几何在 f64 中完全不变：INVALID_ARGUMENT，内容、revision、历史、Redo 不变。
- NOT_FOUND.details 统一为 `{entity,id}`，明确 document/layer/object/aperture。
- Export 保留 source_path/source_sha256；document.get 的 last_saved_path 初始 null，
  仅成功发布新目标后更新。dirty 仍比较最后成功导出的内容基线；导出不回滚文件。
  当前仅单层文档，未来 GUI 必须分别呈现来源与最后保存位置。
- SemanticObject.origin 为 Imported{command_index} 或 Generated{operation_id}。
  layer.objects 是当前曝光顺序；源 command_index 仅作 provenance，不再参与排序验证。
  Writer 按数组顺序输出。API 仍为开发期 v1，自有 ObjectInfo DTO 同步迁移；没有持久工程格式兼容承诺。
- Duplicate 仅当前显式图层内复制，偏移单位毫米，零偏移有效。
  忽略请求 ID 排列，按当前源曝光顺序处理，各副本插入对应源对象之后。
  如 A(Dark),B(Clear),C(Dark) → A,A',B,B',C,C'；这是一组有序绘图操作，不是隔离图像。
  原相对顺序和副本相对顺序均保持。跨图层剪贴板后续实现。
- Generated ID 使用文档会话命名空间和历史内单调计数。仅成功 Duplicate 消耗 ID；
  拒绝不消耗，Undo/Delete 不回收，Redo 恢复原 ID。与已有 ID 冲突安全拒绝。
- 历史明确区分 ModifyObjects、InsertObjects、DeleteObjects。删除保存完整对象和索引；
  结构变更保存前后 ID 顺序守卫，Undo/Redo 恢复 exact ID、origin、geometry、exposure 和 order。
  新成功编辑清 Redo；任一预检失败均零修改。
- 共用每请求 10000 对象、100 条历史、64 MiB 保守历史/暂存预算；不淘汰旧历史。
  计费包含选中几何、origin 字符串、前后顺序守卫、临时合并数组及 Redo 提交峰值。
  Duplicate 不得突破 500000 文档对象或 2000000 Region 边界边预算。
  超预算整体拒绝，未实现完整 AT-042 的可配置预算和历史淘汰交互。

## 验收边界

macOS 原生执行 workspace 与真实 JSON workflow 门禁；API 查询直接读当前模型。
采用合成几何独立坐标/圆覆盖公式和 Writer/reopen 核验，源文件哈希保持。
Windows、原生 GUI 编辑、CORE10 完整流程和独立外部查看器复核仍需后续证据；
不将本轮局部通过写成完整 AT、生产输出或双平台 V1 通过。

## 删除至空图层的实施补充

新增真实回归发现旧解释器在全命令检查之后仍拒绝 object_count=0，导致 Delete All 无法保存。
S1-B2a 接受具备合法 FS/MO/M02 且每条命令均通过检查的零图元图层，用于删除全部后的导出／重开。
移除的仅是末尾“至少有一个图元”业务限制；空字节、缺 FS/MO/结束、非法引用及未知命令仍拒绝。
不引入新 Gerber 命令类别、不扩展 CORE-03/07/08，也不伪造可见图形。允许修改 gerber-io 的此项守卫。
