# S4-C2 Grip Editing v1 计划（Mac-first）

状态：已启动，实施与验收进行中；以 [S4_C2_REVIEW](S4_C2_REVIEW.md) 的实测记录为准。前置 S4-C1 Object Snap 为 Mac-first bounded PASS；INFRA1 panic hook closeout 的最终结论见 `exports/INFRA1_PANIC_c65bee0/REVIEW.md`。本阶段完成后停止并提交复审，不自动启动其他 S4-C2+ 功能。

## 范围与允许模块

- 阶段：S4-C2，单对象 Grip Editing v1。关联 R04/R07/R08/R09/R10/R11/R13/R14/R15/R16/R17/R18/R19/R20/R21/R22；局部映射 AT-025/030/032/037/039/040/041/043/044/054/055/058/062/074/075/086/087/088/090/091/092/093/095。专项案例见 [S4_C2_ACCEPTANCE_ADDENDUM](S4_C2_ACCEPTANCE_ADDENDUM.md)；原 96 个有效用例、schema_version=2、阈值与平台要求不变。
- 允许修改 `crates/editor-core` 的制造 Grip 特征、纯预览和原子编辑，`crates/editor-service` 的 `objects.grips` / `objects.grip_edit` DTO 与修订/权限校验，`crates/editor-app` 的选中对象 Grip、输入状态、标记/状态栏、现有 SnapResolver 接线，必要的 `rcam-project` 往返兼容修正及 `rcam-diagnostics` 白名单接线，以及对应测试、公开 synthetic fixture、文档和证据。`gerber-io` 只允许为合法导出/重开修正必要 writer 问题，不扩展输入语法。
- 不做 Macro primitive、含 Hole contour 或 Arc 边 Region、CompatibilitySolid、GeneratedText glyph 或 BlockDefinition 的节点编辑；Region Grip v1 仅支持 all-Solid、全 line-only contours。Solid outer + Hole inner 即使全为 Line 也没有 editable Grip；Hole contour deferred。不做多对象 Grip、Stroke width Grip、完整 Block Editor、Array/Panelization、Alignment、PnP/RefDes、Shortcut Settings、Command Palette、Tangent/Perpendicular Snap 或 Windows 实施。数值 Grip 输入若当前输入框不能安全复用，明确 deferred；鼠标 Grip 是硬门禁。

## 可运行闭环

`single selection → stable GripFeatureId → hover/active → raw pointer → S4-C1 Object/Grid SnapResolver → f64 mm 局部纯预览 → mouse release → ApplicationService 一次原子提交 → Undo/Redo → .rcam Save/Open → Gerber Export/Reopen`。

Grip 是可编辑制造参数/节点；SnapFeature 是目标候选，二者不能互换。Grip 只由 `SemanticGeometry`、标准 `ApertureDefinition` 或实例变换提供，不读取 mesh/GPU/显示细分。只在 effective visible/selectable 的单个对象上生成；locked selectable 可选择/吸附但不得编辑。稳定 ID 由对象 ID、几何种类及语义角色组成，不按帧内屏幕排序。仅处理选中对象，极端 Region 有明确 feature 预算和 `RESOURCE_LIMIT`，不扫描全工程。

开始拖动冻结文档/图层/对象/Grip ID、revision、原始几何和光圈身份。预览仅复制目标局部数据：每帧 revision、dirty、Undo 均不变；非法预览标红并显示原因，释放时拒绝提交并安全取消。Esc、失焦、PointerGone、模态取消、工具或选择变化、层隐藏/锁定/删除均取消且零制造修改。释放时对相同 resolved target 重新校验；旧 revision 返回现有 `REVISION_CONFLICT`，不存在对象返回 `NOT_FOUND`。成功仅一次 revision、一次 Undo；预览几何与提交几何一致。输入优先级为 Grip > Direct Drag > Selection > Pan，标记与命中半径按 physical px 和 `pixels_per_point` 验证。

Grip target 复用 S4-C1 Resolver：Object 优先于 Grid、Alt 临时禁用、8 px acquire / 11 px retain；默认排除编辑对象自身。Snap 坐标、预览与提交目标相同；状态栏给出 Grip 种类、目标坐标及 Snap 种类。`grip.cancel` 走既有 Tool/Canvas 快捷键上下文，不抢 IME/TextInput。正常移动帧不写 INFO；`grip.begin`、`grip.cancel`、`objects.grip_edit` 记录 INFRA1 白名单摘要，不记录客户路径或制造 payload。

## 制造几何约束

| 对象 | Grip v1 及约束 |
|---|---|
| 标准 C/R/O/P Flash | C 外径保持中心及孔；R/O 侧边/角以相对边/角锚定，同时原子更新中心和尺寸；P 仅直径，保持顶点数、旋转与孔。尺寸均走现有 aperture 写时复制语义，其他 Flash 不变。R/O 在 aperture 局部轴计算，world target 先逆旋转/镜像，禁止按 world AABB 改尺寸。孔与外形冲突时预览无效、提交拒绝。 |
| Line / RectangularSweep | Start/End 直接改路径端点；宽度、高度、曝光与 origin 不变。零长度 Line 可转非零，Undo 精确恢复。 |
| Arc | Start/End 投影到原圆，仅改对应角；Radius 保持圆心/角/方向并重算端点；full circle 仅 Radius（Center 仍可用 Direct Move）。拒绝零半径/非法 sweep。任一实际 Grip 改动清除旧 `ArcSource`，保留当前解析方向、圆心及 full_circle 身份；具体规则见 ADR 0038。 |
| all-Solid、全 line-only Region | `Vertex(contour, vertex)` 同时更新相邻两条 line 边；闭合点同步首尾，提交时完整拓扑验证。含 Hole contour 或 Arc 边 Region 无可编辑节点 Grip。 |

所有修改使用 f64 mm，预览帧不提前量化；导出仍用当前 ManufacturingPrecision 快照。显示单位 mm/in/mil/µm 仅影响格式。共享光圈只改目标实例；Undo 移除/恢复生成定义，Redo 保持同一生成 ID。导入 Gerber 源文件保持只读。

## 退出门禁与证据

专项自动门禁逐项列于 addendum：稳定 ID、形状矩阵、Arc/Region 约束、COW、权限/取消/修订、Snap parity、服务 DTO、Undo/Redo、项目和 Gerber 往返、诊断、有界性能。固定提交执行 fmt/check/clippy/workspace test、service automation/headless 测试、release app build 与相关回归（INFRA1、S4-C1、compatibility、compression、S4-B3、Block、multi-layer、text、Grid/Snap/Measure）。

Mac 原生 Apple Silicon/Metal 要有 Retina `ppp=2` 的真实鼠标操作、COW 两 Flash 对比、Object/Grid/Alt、取消、Undo/Redo、项目保存/重开、Gerber 导出/重开和诊断 ZIP。最终交付以同一受测 clean commit 生成完整 Source ZIP、Public Evidence ZIP、`SHA256SUMS.txt` 和 fresh-extract 检查报告，原始运行证据留在 `evidence/`，版本化副本放 `exports/`。缺一不得写 S4-C2 PASS。Windows deferred / not executed；不宣称双平台 V1、CORE10 10/10 或 P100K。
