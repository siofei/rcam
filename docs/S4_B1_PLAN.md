# S4-B1 — Multi-Gerber Workspace + Layer/Class Display Control

Mac-first，2026-09-21。任务书：`RCam_MAC_FIRST_S4B1_MULTI_GERBER_WORKSPACE_NEXT_TASK_v6`。
决策见 [ADR 0029](adr/0029-multi-gerber-workspace.md)（Workspace/Import-Export/View State）与
[ADR 0030](adr/0030-reusable-blocks-and-forward-reservations.md)（Block/Snap/Command/Board 长期预留）。
证据与状态见 [S4_B1_REVIEW](S4_B1_REVIEW.md)。Windows deferred / not executed。

## GUI 顺序导入修复

阶段 S4-B1 导入局部修复，基于 S4-B3 单工程拖放分派修复；R02/R07/R11/R15/R16/R20/R21/R22，
AT-005/041/060/064/065/086/093/094。允许修改 `editor-app` 导入队列／进度／任务结果标识、
现有 `editor-service` 集成回归与本计划／DESIGN；服务原子批次 API、历史与制造预算不变。
GUI 多选和 Gerber-only 拖放逐文件提交，普通文件失败汇总后继续，取消保留成功；确认中工程拖放保护不变。
队列每次使用已接受回复的新 TaskVersion，提交结果绑定 task_id；显示刷新失败标为已导入并停止，不自动重试。
按输入顺序逐层导入，新层在顶部，等同手工逐层操作。合成 Flash 回归需在默认 64 MiB 预算下证明
原子批次失败无变更、相同文件单独成功并有独立 Undo，不使用用户设计或降低／提高限额。
Mac GUI／真实目录／闪烁仍需原生验证，此源码提交不复用旧原生证据或声明通过。

## 范围（允许修改：editor-core、editor-service、editor-app、scripts/package_release*、文档）

- **Gate 0**：`system.capabilities` 与实际 operation 一致（含 `document.set_manufacturing_precision`）；
  打包 `SHA256SUMS.txt` 只含本阶段两个 ZIP 并有 package test；Global Units 收口为 PASS（Mac-first bounded）。
- **Workspace**：LayerId/source identity（单调、不复用）、NewWorkspace、New Empty Layer、原子批量 Gerber 导入、
  命名空间对象/光圈、Active/Solo/Reorder/Rename、Layer 删除（分级确认 + Undo）、workspace revision fencing。
- **View State**：每层颜色 + 确定性自动配色 + 预设/最近色/完整拾色器；`DisplayClass` 分类的 Visible/Selectable/Locked 与分类色；
  Filled/Outline/ZeroWidth；有效状态规则；均不影响 writer bytes。
- **Layer-aware 渲染/索引/命中**：RenderSnapshot 带 view style；hidden layer/class 从 RenderIndex 与 hit 候选中过滤；
  Selection halo 用反色；Reference/Production WGSL 保持像素一致。
- **Export/Save 语义**：Export Gerber… 单层；不清 dirty、不建 source link；Save/Save As 为 `.rcam` 保留并 disabled。
- **Headless API**：`document.import_gerber_layers`、`document.import_gerber_layer`、`document.create_empty_layer`、
  `document.remove_layer`（`allow_non_empty`）、`layer.summary`、`layers.reorder/set_active/set_solo/update_many/reset_colors`、
  `layer.update`（含分类样式）、`render.snapshot`、`document.visible_bounds`；见 AUTOMATION_API。
- **GUI**：紧凑单列 Layer 面板（可调宽 240–480 px，名称 UTF-8 安全省略号 + tooltip + Rename/Settings 中显示全名）、
  右键与 `⋯` 同一菜单、面板头部“全显/全隐”（一个 workspace revision，全显同时结束 Solo）、双击图层名切换 Solo、整行空白处可点击激活、
  Settings 颜色含 4×4 紧凑色板 + 色盘取色器 + hex（不显示导入哈希，provenance 仍保存）、多选/拖放导入、Delete 分级确认对话框、Layer/Category Settings、New Workspace 的脏检查。
- **占位类型（无产品入口）**：LayerKind/Drill、Board/CoordinateTransform2D/ComponentPlacement、Block、Snap、Command/Keymap。
- **文档**：AGENTS 合并 Forward Architecture Reservations；DESIGN_V1 新增“长期架构方向”章节；IMPLEMENTATION_PLAN 阶段顺序；
  ADR 0029/0030；README/CAPABILITIES/AUTOMATION_API/CHANGELOG 更新。

## 明确不做

`.rcam` 最终格式与工程保存、autosave、字体嵌入、DXF/SVG/PLT、Final Layer Boolean Area、P100K、Windows、签名/公证、
跨层 Boolean、图层合并。S4-B1 完成后**停止并提交审查，不自动开始 S4-B2**。

## 关联需求/用例

本阶段由任务书 §59/§78/§85/§107 定义验收清单，不新增或改写 `acceptance_cases.json`（没有删除或放宽既有用例）。
既有 R09/R11/R12/R14 等的行为在多层语义下由更新后的旧测试继续覆盖；新增用例集中于
`crates/editor-service/tests/multi_layer_workflow.rs`、`crates/editor-core/tests/layer_transactions.rs`、
`crates/editor-app/src/layer_tests.rs`。

## 退出条件

任务书 §90 全部满足并有 Mac 原生证据后才可标记 “S4-B1 Multi-Gerber Workspace = PASS（Mac-first）”。
缺失项在 REVIEW 中逐项写“未执行/阻塞”，不用代码或测试推断代替原生证据。
