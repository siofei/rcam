# ADR 0018：GeometryMetrics 派生数据与缓存

2026-09-20；S2-B2 仅冻结设计，S2-B3 才实施 GeometryMetrics + 属性面板。
此 ADR 不新增当前 API、依赖或 SemanticObject 字段，不表示已有面积/周长计算。

## 真值和边界

`SemanticObject -> GeometryMetricsProvider/Cache -> GeometryMetrics { area_mm2, perimeter_mm }`。
Metrics 是 f64 制造几何派生数据，不持久化为 SemanticObject 的制造源真值，不参与 writer。
只能从 Manufacturing Geometry、真实圆弧、canonical Region 和局部孔洞计算。
禁止 GPU Mesh、显示 tessellation 或 pixel mask 反推面积/周长。

Object Metrics 是独立对象局部材料的面积/边界长度：孔洞扣面积并计入孔边周长；
Clear 对象的数值也是其自身材料量，不是负面积。不同对象相互遮挡/重叠不改变各自指标。
多选面板显示“对象面积合计/对象周长合计”，绝不能标为实际开口面积或去重面积。
Final Layer Manufacturing Area 必须按 Dark/Clear 原曝光顺序布尔合成，是另一种查询和缓存，
等待 Canonical Boolean Geometry 成熟后单独实施，不以 Object Metrics 合计替代。

## Lazy 缓存与身份

首次查询 selected object 时 cache miss 才精确计算；不得导入50万对象时同步全量计算。
后台预计算留性能阶段。本阶段不实现空 provider/缓存壳。
S2-B3 使用文档会话范围的 GeometryKey/ShapeRevision：几何形状、光圈内容版本、孔洞和
局部尺度影响 key；平移、刚体方向不影响指标。不得只用 DCode、ObjectId 或 document revision。
相同对象ID的尺寸可能变化；Undo恢复旧形状必须找到对应旧key，不能拿当前key缓存冒充。
复杂规范化/同构检测不必要时采用显式 immutable shape revision 引用，命令携带前后身份。
跨文档不共享缓存，关闭文档释放引用；有上限的缓存按条目/字节计费并可淘汰，未命中可重算。
失败返回 NotAvailable/Unsupported/ResourceLimit，不能以0或近似值冒充 exact；不写制造事务。

| 操作 | 缓存规则 |
|---|---|
| Move | 保留 shape key 与指标 |
| Rotate 刚体 | 保留；不因坐标数值小差异全量清空 |
| Mirror | 保留面积及无向边界长度 |
| Duplicate | 可共享/复制 shape metrics，增加引用 |
| Delete | 清理活跃引用；历史保留的形状仍可复用或按需重算 |
| 尺寸/Aperture/孔洞变更 | 新shape revision，旧项不可命中 |
| Scale | 重算，或可证明的均匀尺度规则 A乘s²、P乘绝对s；须检查有限值/误差 |
| Region节点变化 | 新shape revision并失效 |
| Undo/Redo | 按恢复后的shape identity/revision取值；不可只按对象ID复用 |

## Macro 与下一轮

Macro 1/4/21 可包含有序 Dark/Clear 重叠，不允许 primitive area 简单加减。
简单可证明组合可提供 exact metrics；复杂组合返回 NotAvailable/Unsupported，
直到有经过独立验证的 Canonical Boolean Geometry，不复制不可靠布尔引擎追求覆盖率。

S2-B3 首轮：标准 C/R/O/P Flash、孔洞、Line/Arc stroke、Region 解析面积/周长，
lazy cache 和属性面板、多选对象指标合计；复杂 Macro 可以保持未支持。
仍须评审偏差圆弧径向接线/端帽重叠、Region cut-in 边界去重，不能直接套周长公式。
S2-B2 完成即停止，不提前实现上述计算。

## S2-B3 实施决策

core metrics 解析公式、Green积分；不使用macro命中边界（它包含内部距离见证线）。
Macro首轮全部明确unsupported。Arc deviation超过数值舍入量、r>=R的非全圆、major arc端帽相交/相切拒绝。
Region复用canonical contour和合法性判定，精确反向连接对移除后提取简单环；
按既有非零winding确定真正外/内边界，内部同向环不计材料周长。
多轮廓仅证明边界包围框不相交且不发生材料包含的组合；复杂组合拒绝，未新增布尔引擎。
Region拓扑工作按3*N²预收费，超限整次RESOURCE_LIMIT。近似闭合、模糊切入线拒绝指标，不改导入兼容性。
缓存identity按当前私有document mutation入口维护；现有操作仅刚性/结构，持久ID从不复用。
active/historical引用与bounded FIFO结果分开；Delete清活跃，Undo恢复；超身份预算清派生状态可重算。
未来非刚性命令须同时扩展history shape identity恢复，未发布尺寸编辑API。
