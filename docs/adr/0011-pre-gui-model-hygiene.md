# ADR 0011：S2 GUI 前的模型边界与结构历史债务

日期：2026-09-15；阶段 S1-B2b 记录，S2 前实施；R07/R11/R15/R17/R21。
相关 AT-022/040/041/042/060/061/072/075/086。

## 本轮决定

- 继续一个 Gerber 文件对应一个服务文档。S2 GUI Workspace 聚合多个独立 service documents；
  每份文档的文件路径、保存基线、revision 与历史独立。document.import 多层模式不开放。
- lock/name/visibility 是 editor/workspace state，不属于 Gerber manufacturing content。
  S2 开放 layer.update 之前先拆分纯 UI 元数据与制造 dirty hash；Undo/Redo 几何历史不应被当前
  workspace lock 阻断。当前没有公开此入口，本轮不改历史锁定语义或增加 layer.update。
- Duplicate/Delete 的 before_order/after_order 为 O(N) 字符串数组，现有预算也包括临时峰值。
  先保留安全检查。Production Renderer/大文件编辑之前，另一个小闭环改用结构版本号、
  固定大小顺序摘要与局部插入/删除锚点，并保留错序拒绝/完整身份恢复/预算拒绝回归。

## 可测量基线

`s1b2b_transform_workflow::structural_order_guard_scalability_baseline` 在 10k/100k/150k/500k
个有效合成 Flash 上实际 Duplicate 一个对象，记录耗时、通过或 RESOURCE_LIMIT 和 64 MiB 预算。
原始数据在本轮公共证据 `workflow/order-guard-baseline.json`。此项为 debug 测试构建的算法债务基线，
不是 release 性能验收，不据此宣布 AT-072/075 通过。当前服务可打开大文件但仍可能拒绝结构编辑。
