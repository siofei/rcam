# ADR 0023：S2-C2 Rotate / Mirror GUI（Mac-first）

状态：已接受（Mac-first），2026-09-20。前置阶段为 S2-C1；S1-B2b 已实现制造变换，本轮不重写算法。

## 范围与决定

S2-C2；R10/R11/R14/R18/R19/R21/R22。
局部 AT-033/034/039/040/041/043/054/087/090/091。
允许修改 editor-core 的只读制造 bounds helper、editor-app 的属性面板/状态/测试、阶段文档与证据；
不改变 `objects.rotate` / `objects.mirror` DTO、制造变换、writer、Renderer、依赖或 96 个验收用例。

- 属性面板提供任意有限角、快捷 -90°/+90°、选择集中心/世界原点/自定义 Pivot。
  第一版采用 Apply 后提交，不实现 Transform preview、鼠标旋转手柄或 Angular Snap。
- 选择集中心来自所选对象材料 bounds 的 f64 union center。bounds 由 editor-core 从制造几何和光圈定义计算；
  不平均对象中心，不读取 renderer mesh、GPU bounds、screen pixel 或当前缩放。
- 镜像第一版固定为选择集中心的明确世界制造轴：Horizontal 为 `y=center_y`，Vertical 为
  `x=center_x`；按钮直接显示轴坐标。未增加自定义镜像轴 UI。
- 一次 GUI 操作只发送一个包含全部有序对象 ID 的服务请求；同层、显隐、锁定与跨层限制复用
  SelectionSet 编辑契约。失败整批不变，一个成功请求增加一个 revision/Undo。
- 角度、Pivot 和轴保持 f64 制造值；Grid Snap 不参与 Transform。0°/整周、非有限值、
  RectangularSweep 非整数 90°及数值/资源错误保留服务结构化拒绝，不制造假事务。
- GUI 只解析输入和分派请求；Flash/Arc/Region/RectangularSweep、Arc 方向、Undo、metrics cache、
  writer 与 reopen 全部复用既有 core/service 实现。
- Transform 输入使用普通 egui TextEdit，沿用事件时 text focus guard；焦点期间 Delete/Cmd+D/Undo/Escape
  不穿透到画布制造动作。没有 preview，因此 Escape 不承担 Transform 回滚。

## 验收边界

自动纵向测试覆盖制造 bounds center、单/多选、任意角/自定义 Pivot、双轴镜像、Arc/Region/
RectangularSweep、整批拒绝、Grid 分离、metrics、Undo/Redo、Save As/Reopen、焦点与非法输入。
Mac 原生控件、Metal 与最终 gates 证据见
`evidence/s2c2-20260920-run2-native/`；Windows deferred / not executed。
本阶段不包含 Scale/Skew/Group、文字、P100K 优化或任何 Post-V1 格式。
