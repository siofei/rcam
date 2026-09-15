# ADR 0010：S1-B2b 旋转／镜像的表示边界

日期：2026-09-15；决策状态：S1-B2a 冻结表示范围，S1-B2b 已实现，实际验收见 S1_B2B_REVIEW.md。
阶段 S1-B2b 前置设计；R04/R10/R11/R14，AT-016/033/034/039/054/055。

## 决策

保留当前制造模型，首先只开放各类型能够精确表示的变换集合；无法表示时整批 fail-closed。
S1-B2a 前置阶段不公布 objects.rotate / objects.mirror；S1-B2b 专项通过后仅公布受测集合，
不能以“部分对象可旋转”宣称 AT-033 任意角完整通过。

- Flash：中心绕显式 pivot 旋转／镜像。非圆光圈必须组合 LocalTransform，不能仅移动中心。
  用二维正交矩阵组合既有 mirror 与 rotation，再确定性分解回模型；保留 uniform scale。
  须按 gerber-io 当前局部变换顺序核对，测试 R/O/P、宏孔洞、已有镜像／旋转组合。
- 无孔圆形 Line：共同变换 start/end；宽度保持，可支持任意有限角度与轴镜像。
- Arc：共同变换 start/end/center；旋转保持 CW/CCW，反射翻转 CW↔CCW。
  保持 full_circle、zero_sweep 身份、source.resolution_mm/single_quadrant 和 deviation 语义。
  不通过重拟合中心或显示细分改变制造圆弧；输出量化后再次检查。
- Region：对所有 contour 的每条 edge 一起变换，弧方向在反射时翻转。
  保持 edge order 与轮廓拓扑，验证变换后闭合、孔洞和不确定环带关系。
- RectangularSweep：当前 start/end/width/height 表示轴对齐矩形光圈扫掠，缺少 aperture orientation。
  首阶段仅允许精确整数倍 90° 旋转，奇数次交换 width/height；中心和端点按整数正交映射计算，
  避免三角函数舍入造成“几乎水平”的假表示。镜像限定水平／垂直轴（显式轴位置）。
  非 90° 倍数、任意斜轴反射全部拒绝；不得将矩形线仅旋转端点后冒充同一矩形光圈。
  未来要完成完整 AT-033，另立模型迁移到 Path+Aperture+LocalTransform，并补 writer/验证/查询测试。

所有类型先生成候选、验证和计费后一次提交；混合选择中只要一个不能表达，整批不变。
Undo 保存原始前后制造状态，不能靠逆旋转恢复。
实际实现前补齐精度、溢出、中心／轴参数、四次90°／两次镜像、Clear 和导出量化回归。
本 ADR 不改变 CORE10、制造容差、96 个用例或双平台门槛。

## S1-B2b 实施与回归澄清（2026-09-15）

本轮实现范围：editor-core/edit 与刚性变换模块、editor-service DTO/分派、对应测试和文档证据；
复用原有 writer、安全文件发布和 Modify 历史，无新增依赖。关联 R04/R05/R10/R11/R14/R19/R21/R22；
AT-013/014/015/016/033/034/039/040/042/054/055/086/088/090/091/095/097 的局部步骤。

- Rotate 接收 angle_deg 和 pivot_mm，Mirror 接收 tagged horizontal/vertical 轴及 coordinate_mm。
  非有限、无效范围、零整周旋转或全部候选存储状态不变均 INVALID_ARGUMENT，失败不清 Redo。
- 正向 Flash 顺序明确为 R * M * scale；世界矩阵左乘，用保留 mirror 两位的确定性分解，
  重新计算矩阵验证分解；角度规范为 [0,360)，scale 保留。重复变换允许此角度规范化，
  一般 f64 坐标按原 1e-6 mm 比较，Undo/Redo 则恢复原始存储位，不依靠逆变换。
- 90° 倍数使用精确正交矩阵；一般旋转使用 f64 sin/cos。点运算的保守数值误差上界
  大于原 EPSILON_MM 时整体拒绝；圆弧保留 source/full_circle/zero_sweep，并检查半径及偏差变化。

### 两边闭合 Region 的共享端点修复

首次专项运行 23 通过、2 失败，原始日志 evidence/s1b2b-development/first/01.log。
37° 后 Region 的 writer 量化产生小于 1e-6 mm 的圆弧偏差；既有 envelope 校验仅豁免一个
相邻连接点，错误拒绝“半圆弧 + 回程直线”轮廓的另一个合法共享端点。
既有 intersection 校验已经对两边闭合轮廓识别两个连接点；envelope 同步采用这项拓扑规则。
只对恰好两条边且相邻的闭合轮廓允许两个共享端点的原 EPSILON_MM 邻域；其他点和内区交叠
仍拒绝，容差不增加。新增量化半圆回归和超过端点邻域的拒绝对照，不改 writer 的量化格式，
不改变 Gerber 支持范围或验收门槛。

### cut-in 方向不变性修复

第二次运行 24 通过、1 失败，region-fix/01.log：带孔 Region 37° 后被既有仅水平/垂直
cut-in 识别器判为自交。将合法反向重合线对的识别改为端点对应与共同方向检查，
保留“各 cut-in 必须平行”和所有交叉/重叠拒绝；方向比较按最长线的垂直偏差不超过
既有 EPSILON_MM，不增加几何容差。整体刚性旋转不改变 cut-in 拓扑，不引入 SVG 填充规则。
独立验证旋转前后的方环孔洞和覆盖点，并保留非平行 cut-in、自交与不安全环带拒绝回归。
