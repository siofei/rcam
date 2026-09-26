# ADR 0038：单对象制造 Grip Editing

- 状态：Accepted for implementation；S4-C2 验收未完成
- 日期：2026-09-25
- 关联：R04/R07/R08/R09/R10/R11/R13/R14/R15/R16/R17/R18/R19/R20/R21/R22；专项案例见 `../S4_C2_ACCEPTANCE_ADDENDUM.md`

## 背景

S4-C1 的 SnapFeature 标识制造几何上可吸附的位置；它不说明哪个制造参数能安全编辑。S4-C2 要在单个选中对象上给出稳定、可预览、可撤销的 Grip，且不把导入来源、共享光圈和 Region/Arc 拓扑误改。

## 决策

1. 独立定义稳定 `GripFeatureId` 与 `GripKind`，角色如 CircleDiameter、Side/Corner、Start/End、ArcRadius、`Vertex(contour, vertex)`；对象未修改时跨帧稳定。Grip 来自制造模型和光圈定义，不把所有 SnapFeature 当成可编辑点。只为单选生成；隐藏、不可选、锁定对象不提供可编辑 Grip。数目有预算，超限结构化拒绝。
2. Core 的 Grip 特征与目标计算是纯函数，预览仅克隆目标对象/光圈局部数据，返回合法或结构化无效原因；不得每帧复制文档或修改 revision、dirty、历史。GUI 冻结 session 身份和原始 revision；取消零修改，释放时通过 ApplicationService 最终权限/修订/几何校验，一次事务且一次 Undo。预览与提交使用同一几何计算。服务对外提供 `objects.grips` 与 `objects.grip_edit`，Automation 传世界毫米目标，不模拟鼠标。
3. Flash 尺寸编辑复用/抽取 `set_flash_size` 的 aperture Copy-On-Write 核心。R/O 的 side/corner 在 aperture 局部轴解析 target，逆 `LocalTransform` 后锚定对边/对角；中心位移和光圈尺寸在同一事务。禁止 world AABB resize。孔洞参数保持，原外形不容纳孔洞时拒绝。C 与 P 仅尺寸；Macro 不提供尺寸 Grip。
4. Line/RectangularSweep 仅可改 Start/End。Arc Start/End target 投影到当前圆，只改变角度；Radius 保持圆心、起止角、方向和 full_circle 身份，并重算端点。full circle 不显示角端点。零/非法 radius、退化 sweep 拒绝。
5. `ArcSource` 记录导入时的分辨率和 `single_quadrant` 来源声明，不是当前制造几何的通用约束。任一**实际改变** Arc 几何的 Grip 提交都令该 Arc 的 `source=None`，不延用旧 G74/G75 或分辨率声明；未改变的 target 视为 no-op，不新增历史且保留原来源。当前几何仍须通过完整 Arc 验证，保留 `direction` 和 `full_circle`，导出由 writer 重新选取合法模式。若现有验证/Writer 不能安全表达修改后的 Arc，拒绝提交，不伪造 `ArcSource`。
6. S4-C2 Region Grip v1 仅对 all-Solid、全 line-only Region 公开 Vertex Grip：每个 contour 的角色必须是 `Solid`，每条边必须是 `Line`。Solid outer + Hole inner 即使两条轮廓全为 Line，也返回空 Grip；Hole contour deferred，不在 closeout 中扩大拓扑范围。移动共享顶点时同时更新相邻边；闭合点同步首尾；提交需完整制造拓扑验证。含 Arc 边 Region、CompatibilitySolid、GeneratedText glyph、BlockDefinition 内部节点和多选不暴露节点 Grip。
7. 目标点调用 S4-C1 SnapResolver，Object > Grid、Alt 临时禁用、8 px acquire / 11 px retain；默认排除活动对象。标记与命中半径用 physical px。`grip.begin/cancel` 和 `objects.grip_edit` 只记录 INFRA1 白名单摘要，正常逐帧不写 INFO、诊断不含制造 payload/客户路径。

## 后果与边界

旧 `ArcSource` 可能在编辑后消失，这是来源真实性要求；项目和 Gerber 往返须检查制造几何，不要求保留失效的原始 G74/G75 声明。独立 Grip ID 与 SnapFeature ID 不强行合并。`.rcam` 保持现有 schema v1，除既有对象/光圈变更外不持久化 hover、session 或 snap candidate。Windows 与其他 S4-C2+ 功能 deferred；阶段 PASS 由原生门禁和版本化证据决定。
