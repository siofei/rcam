# ADR 0029 — Multi-Gerber Workspace、Import/Export 生命周期与 Layer View State

状态：Accepted（S4-B1，2026-09-21）。实现证据见 [S4_B1_REVIEW](../S4_B1_REVIEW.md)。

## 背景

S1–S4-A2.2 的 Document 只有一个 Gerber 图层，GUI 的 “Save/Save As Gerber” 隐含了把磁盘文件当作文档本体。
钢网设计需要一个 Workspace 同时包含 Top/Bottom Paste、Outline、Mark 等多份 Gerber，并且为以后的 `.rcam`
工程、Block、Drill、Board Coordinate 留出不返工的模型。

## 决定

1. **Gerber 只 Import / Export。** 导入后与磁盘文件解耦：只保存 `original filename`、`imported SHA-256`、`import time`
   作为 provenance。不做 live source link、mtime reload、外部文件绑定或写回原 gbr。未来 Save/Save As 只属于 `.rcam`
   （S4-B2/B3）；S4-B1 中 Save/Save As 明确 disabled / “not available”，旧 “Save As Gerber” 改为 **Export Gerber…**。
   Export 不清 Workspace dirty、不建立 source link、不改变 LayerId；只更新该输出所用的 saved precision 基线。
2. **稳定身份。** `layer-{n}`（LayerId）与 `src-{n}`（import source）由 Workspace 单调计数器分配，删除后**不复用**。
   多文件导入后对象/光圈加命名空间前缀 `src-N::object-M` / `src-N::aperture-D`，源 DCode 不作跨文件主键。
   为兼容旧测试，旧 `document.open` 路径保持无前缀 ID（仅用于测试/单文件 headless）；GUI 走 NewWorkspace + ImportGerbers。
3. **原子批量导入。** `document.import_gerber_layers` 一次多个文件：全部成功或全部不加入；成功占一个 revision、一个 Undo 步；
   单文件坏文件不留下半个 Workspace。同一文件导入两次得到两个独立图层，显示名自动去重（不合并）。
4. **New Empty Layer** 是一个 Workspace 事务；LayerKind 默认 `Gerber`，为 `Drill` 预留枚举与 Tool namespace 占位。
5. **Layer View State 与制造内容分离。** 颜色、Visible、Selectable、Locked、Z-order、Active、Solo、Filled/Outline/ZeroWidth、
   分类样式、面板宽度均属于 View/Workspace state：不改变 writer bytes，不推进“制造 revision”语义（`workspace_revision`
   单独 fencing；Undo 历史条目按事务）。
6. **有效状态（不可绕过）：**
   ```text
   effective_visible    = layer.visible && class.visible      （Solo 只隐藏其他层，不显示已隐藏的层）
   effective_selectable = effective_visible && layer.selectable && class.selectable
   effective_locked     = layer.locked || class.locked
   ```
   Locked-but-selectable 仍可 select / measure / snap / 看属性，但拒绝制造编辑（服务层强制，不只是 GUI 灰掉）。
7. **DisplayClass**：Stroke / FlashCircle / FlashRectangle / FlashObround / FlashPolygon / ApertureMacro / ApertureBlock（保留，
   parser 目前拒绝 `%AB`，无对象产生）/ RegionFreeform / GeneratedText / Other；由对象几何与其光圈形状确定
   （`classify_object`），GeneratedText 由 `ObjectOrigin::GeneratedText` 标记；只用于显示、过滤、选择与锁定，不进入 writer。
8. **Display Mode（仅 View）**：Filled = 真实制造 composite；Outline = 制造边界 hairline（诊断显示，跳过 Clear 对象）；
   ZeroWidth = Stroke 中心线、Flash/Region 轮廓 hairline。禁止从 mesh/像素反推。
9. **顺序约定。** 面板 `display_order`（`layers_list`、`LayerInfo.z_index`）为 TOP-first；`RenderSnapshot.layers` 为
   BOTTOM-first（合成顺序）；命中测试沿面板 top-first，先命中的层胜出。Reorder 只改 z-order，不改对象和曝光顺序。
10. **Layer Delete。**
    - 空层：直接删除 + Undo；非空：强确认；含 dirty/generated：更强确认（复选框）。
    - Headless `document.remove_layer` 对非空层要求 `allow_non_empty=true`；`layer.summary` 提供确认所需的对象数、
      modified/generated 数、manufacturing_dirty，并给出 `DeleteRisk`。
    - 允许删除最后一层；结果是空 Workspace。Locked 图层同样可删（删除是 Workspace 结构事务，不是对象编辑）。
    - Undo 恢复**同一 LayerId、同一 z-order、同一样式**、对象与 source provenance；Redo 再删除。
11. **Dirty 定义。** dirty = 当前 Workspace 内容哈希 ≠ Workspace baseline 哈希，**不是文件**。哈希按制造 revision 缓存，
    图层显隐等 view 操作不重算（否则大文档每次切换耗时 ≈1 s）。
12. **Export 语义。** `gerber.export_layer` 对单个 LayerId 生成隔离快照，走原 writer 安全流水线（语义校验、重新解析、
    同目录临时文件、无覆盖发布），覆盖策略沿用既有 `overwrite`（默认 deny；`replace_if_unchanged` 需 expected_sha256）；一次导出一个图层，不合并。Solo/隐藏/Outline 不影响输出。
13. **不静默丢工作。** 关闭有未导出修改的 Workspace（`document.close`，GUI 的 New Workspace 同理）返回 `CONFIRMATION_REQUIRED`，
    必须显式 `discard_changes`（GUI 提供 “放弃修改”）。`document.new` 本身只创建空的干净 Workspace。

## 占位类型（S4-B1 只建类型与测试，不接功能 UI）

`LayerKind{Gerber,Drill}`、`DrillHit/DrillSlot/Route`（drill.rs）、`CoordinateTransform2D` +
`Source/Board/World`（board.rs）、`BlockDefinition/BlockInstance`（block.rs，无 nested）、
`SnapFeatureId/SnapQuery/SnapCandidate/SnapFeatureProvider`（snap.rs）、`CommandId/CommandRegistry/Keymap/ShortcutContext`
（command.rs）。它们没有产品入口，不写入 CAPABILITIES 的“已支持”列表，也不冻结 `.rcam` schema。

## 被拒绝的方案

- 把 Gerber 文件当作 Document 并支持 Save 回原文件：破坏 provenance 解耦，且 Export 不能清 dirty 的语义无法成立。
- 每个 Gerber 一个独立 Document：无法跨层选择/测距/统一 Undo，也无法承载未来 Workspace 级 Block。
- 用光圈 DCode 或对象序号做跨文件 key：不同文件冲突；改用命名空间前缀与稳定 ID。
- 把 view state 计入制造 revision：会污染 `expected_revision` 与 Undo 语义。

## 后果

- `capabilities` 必须与实际支持的 operation 一致（Gate 0.1，有测试 `capabilities_are_consistent_with_the_supported_operations`）。
- 打包 sidecar `SHA256SUMS.txt` 只含本阶段两个 ZIP（Gate 0.2，`test_package_release.py`）。
- `.rcam` 序列化、Block core 与 Project lifecycle 分别属于 S4-B2 / S4-B3，S4-B1 **不冻结**这些格式。
