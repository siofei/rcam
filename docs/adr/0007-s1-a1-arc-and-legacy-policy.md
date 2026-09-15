# ADR 0007：S1-A.1 圆弧输入语义与旧文件策略

日期：2026-09-15。阶段 S1-A.1；R03/R04/R06/R14/R19/R21/R22；
AT-006/013/014/019/053/055/057/080/086/088 的局部检查。
允许修改 editor-core、gerber-io、editor-service 能力和测试、公开小样、核验脚本、文档和分发清单。
依据 RCam_S1A1_NEXT_TASK.md 及 RCam_S1A_f6da9a4_Review.md；不开展 S1-B。

## 规范依据与选择

[Ucamco 2026.05 官方规范](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf)
第 4.7.2.2 节（印刷页 83–84）定义非零 arc deviation；第 8.2 的单象限章节（196–197 页）定义 G74 中心规则与零角度。
原始输入的 start/end/declared center/direction 留在 ArcGeometry；ArcSource 保存原 FS/MO 的毫米分辨率及象限模式。
start_radius、end_radius、arc_deviation 由原始字段计算，避免重复字段失配。

- G75 不以端点半径相等作为输入合法条件，不设置任意 deviation 上限。
- 圆心落在起终点直线的端点或线段外侧且距离该直线过近时拒绝；严格在线段内部允许半圆。
  规范未量化“close”，本项目保守冻结为一个源坐标分辨率加 f64 运算不确定度；这是本项目边界，不冒充规范数值。
- 浮点运算不确定度为 64 × f64::EPSILON × max(坐标绝对值、半径、1mm)。角度判断除以最小半径。
  它只处理在远离原点的小半径几何上的数值误差，不是放宽制造精度或把文件分辨率当作 G74 角度容差。
  当该不确定度超过原核心 1e-6mm 容差时拒绝该圆弧，不能以巨大坐标为由扩大制造误差或跳过 Region 环带检查。
- G74 I/J 非负，四候选去掉同一圆心，按指定方向与 ≤90°（加运算不确定度）筛选，按 deviation 升序。
  最小两个结果仅在运算不确定度内无法区分才安全拒绝；多个候选本身不是错误。
- G74 start=end 是零扫角的圆光圈 dot，允许 I=J=0；中心符号对该覆盖无影响，固定正号保留溯源。
  Writer 将零弧写成同一圆光圈的 G01 零长度 stroke（几何等价 dot）；其他弧显式写 G75。零弧绝不转成全圆。
  对照发现本地 gerbv 对 G74 零弧有漏画；规范化 G01 dot 的独立覆盖因此也单独检验。

## 曲线和导出

无孔圆光圈 stroke 的规范化覆盖路径是：沿起点径向连接到平均半径，按方向沿平均半径圆弧运动，再径向连接到终点。
整条曲线连续，极角与半径各自单调，始终位于两输入半径之间。起终点和连接段均按真实圆端帽扫掠。
这是明确的可重复覆盖解释；较大 deviation 下，其他查看器选择另一条规范允许曲线时可以存在差异，不能把它算作精确几何一致。
canonical_circle 仅提供中间圆弧，调用者还须使用原始端点与两条径向连接；不得将它直接当 Writer 数据。
零弧覆盖为当前位置的圆形 aperture image；G75 相同端点为 360°。

Region 的解析交点算法使用精确圆弧。将圆心正交投影到端点垂直平分线，
以解析极值检查整段曲线位于输入环带内且相对声明圆心角度单调，再做 Region 拓扑和点覆盖。
不是每个 fuzzy arc 都存在这样的圆形解释；不能证明时明确拒绝，不伪装成等半径输入。
浮点噪声只按上述运算不确定度归一，不使用源分辨率放宽核心精度。

对于超过运算噪声的 deviation，额外检查原始弧的整个不确定环扇区。
通过内/外圆弧与径向边的解析交点切分相邻边区间，验证其他边没有进入该扇区；
两个 fuzzy 边同时检查双方扇区边界，包含完全包含情况。相邻边只允许原始公共端点的核心容差邻域。
不能证明所有允许解释的拓扑安全就拒绝；不靠只验证选中的圆弧曲线放行。
该检查有 region_envelope_pairs 资源上限；较大/复杂模糊 Region 仍可能保守拒绝，完整 Region 兼容不宣称完成。

Writer 使用原始端点与声明圆心生成 I/J，保持方向、曝光顺序和输入环带；不输出覆盖辅助连接或 GPU 数据。
量化后重新解析核验端点、圆心、全圆/零弧身份和 deviation。输入分辨率是溯源数据，输出改为 FS6.6 不要求两者相同。

## 错误测试真值更正

原 g74_ambiguous_centers_and_full_circle_are_rejected 的两个原始输入保留，改为必须接受、可写出重开。
原 g75_invalid_radius.gbr 的字节与名称保留；其 1mm/2mm 端点半径是合法非零 deviation，manifest 改为正例。
新增实际 nonsensical center、负 I/J、无合法单象限候选等负例。保留更正前 manifest 与失败证据。
96 个有效 AT、AT-079 退役身份、required_platforms、REAL30/CORE10 身份和阈值不变。

## LegacyImportPolicy 冻结

本轮运行时策略为 Strict，沿用现有接口，不增加无实现的 policy 参数或空 enum。

- CORE-03：无效 CreationDate 继续严格拒绝。将来 MetadataWarning 必须显式选择，限定已知且不影响 image geometry 的字段，
  保留原始字段与 warning，独立证明图形成像不变；Writer 默认不复制无效属性，按既有元数据确认流程处理。
  本轮不实现或宣称已支持 MetadataWarning。
- CORE-07：FS 字段超宽继续拒绝。不全局自动扩位、截断或猜测小数点。
  只有明确模式的唯一独立几何真值、正负小样、legacy recovery 诊断与保持严格默认同时具备，才另立 ADR 实施。

CORE10 复扫成功只表示无编辑语义检查。CORE-08 如果圆心选择通过后暴露 Region 拓扑失败，保留该失败，不降门槛。
Windows 原生、双平台 V1 和生产文件发行仍需原有完整证据；本轮不自动进入 S1-B。
