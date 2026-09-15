# ADR 0010：S1-B2b 旋转／镜像的表示边界

日期：2026-09-15；决策状态：本轮冻结后续实现范围，尚未实现或验收。
阶段 S1-B2b 前置设计；R04/R10/R11/R14，AT-016/033/034/039/054/055。

## 决策

保留当前制造模型，首先只开放各类型能够精确表示的变换集合；无法表示时整批 fail-closed。
本轮 capabilities 不公布 objects.rotate / objects.mirror。未来实现后才公布受测集合，
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
