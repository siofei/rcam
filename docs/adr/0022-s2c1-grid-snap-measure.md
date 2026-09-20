# ADR 0022：S2-C1 Grid / Snap / Measure（Mac-first）

状态：已接受并增补，2026-09-20。前置审查 9779a45；历史成绩不替代本轮回归。

## 范围与决定

S2-C1；R08/R10/R11/R13/R16/R17/R18/R19/R21/R22。
局部 AT-023/024/025/031/032/039/040/044/045/062/075。
允许修改 editor-core 的纯数值 helper、editor-app 的工具/拖动/Overlay 与测试、
阶段文档与证据脚本；不修改 Gerber 语义、writer、服务契约或 Renderer 算法。

- GridSettings、ActiveTool、MeasureState 只在 app，不能进入 SemanticDocument、
  manufacturing hash、dirty baseline、Undo 或 writer。默认网格隐藏、吸附关闭、步长 0.1 mm。
- 原点固定制造 (0,0)。显示网格按 10 倍抽稀，最小间隔 12 logical points；
  每轴最多 512 条，1 physical pixel 线宽。显示抽稀不改变 snap spacing。
- Snap 使用纯 f64 mm，halfway 相对原点远离零；有限正步长、有限参数，拒绝
  溢出和无法分辨相邻格点的尺度（格点索引绝对值 >= 2^52）。不从 mesh/pixel 求制造几何。
- Drag 冻结按下时的 snap 设置，以按下的制造坐标为共同 reference；
  snap(pointer target) - pointer start 得到全选择集唯一 delta。不是每对象独立吸附。
  UI 明示“吸附拖动抓取点”。preview 不提交，release 复用一次 DragMove/objects.move；
  非法 snap 取消整个手势并提示。数值 Move 保持用户精确输入，不经 snap。
- Measure 为两点直线测距：click A -> 动态 B -> click B 固定；第三次 click 开始下一条测量，
  已完成测量继续保留。显示 A/B、ΔX/ΔY/distance 与相对世界 +X 轴逆时针、归一化到
  `[0°, 360°)` 的 angle，全部由 f64 mm 坐标计算；Snap ON 明示并同样应用到测量点。
  每条线段中点使用不透明底色显示距离与角度，可连续保留多条。Esc 清除全部测量；切换工具、
  文档清除。输入框焦点/模态窗口不触发画布工具快捷键。
  在 raw_input_hook 保留事件到达时的输入焦点，避免 egui 的 Escape 提前释放焦点后穿透到测距清除。
- Grid 和测距使用独立 egui Overlay，不进入制造 scene/reference parity/hit test。
  光标坐标使用统一 Camera::world；离开 Canvas 不显示制造坐标。
- 保留 P1K native 与 production/reference 回归，不增加 P100K/Windows 门槛。

## 验收边界

这是局部 AT 验证；完整 AT-044 的 Object Snap/临时修饰键和 AT-045 英寸显示尚未实现，
不降低 V1 原要求。Windows deferred / not executed；最终双平台、CORE10 与 B0/B1 门槛不变。
Mac 原生证据、最终 gates 与源码身份齐全前不得宣布阶段通过。
