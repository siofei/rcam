# S4-B2 — Block Core + `.rcam` Native Project Model / schema v1

Mac-first，2026-09-22。任务书：`RCam_MAC_FIRST_S4B2_BLOCK_RCAM_PROJECT_MODEL_NEXT_TASK`。
决策见 [ADR 0031](adr/0031-rcam-native-project-format-v1.md)（`.rcam` schema v1）与
[ADR 0032](adr/0032-block-core.md)（Block Core）。证据与状态见 [S4_B2_REVIEW](S4_B2_REVIEW.md)。
前置：[S4-B1 = PASS（Mac-first）](S4_B1_REVIEW.md)。Windows deferred / not executed。

## 范围（允许修改：editor-core、editor-service、gerber-io、新 crate `rcam-project`、文档）

- **Block Core**（`editor-core::block`）：`BlockDefinition`（项目级，`SemanticDocument.block_definitions`）、
  `BlockObjectGeometry`（无 Block 变体，类型层面禁止 nested）、`BlockTransform`（rigid-only）、
  `SemanticGeometry::BlockInstance` 新变体；resolve/flatten、bounds/hit-test/metrics 解析、
  Gerber export flatten（RectangularSweep 非 90° fail-closed）、`DisplayClass::BlockInstance` 分类。
- **Block Service API**（`editor-service`）：`blocks.list_definitions`、`blocks.get_definition`、
  `blocks.create_definition_from_objects`、`blocks.create_instance`、`blocks.update_instance_transform`、
  `blocks.rename_definition`、`blocks.explode_instance`、`blocks.delete_definition`（被引用时拒绝）；
  Move/Rotate/Mirror/Duplicate 复用既有 `objects.*`；Layer 锁定拒绝全部 Block 结构性修改。
- **`.rcam` schema v1**（新 crate `rcam-project`）：`RCamProject` 模型、手写确定性 store-only ZIP
  编解码、`manifest.json` + 每条目 SHA-256、fail-closed 读取（路径穿越/重复路径/超预算/哈希不符/未知
  `format_version`/未知 mandatory 类型/非有限数值）、`decode_v1` 与未来 `migrate()` 的调用点分离。
  只做内存 encode/decode 与测试专用文件往返；不做 `File → Open/Save`。
- **Final Closeout**：resolved Block geometry 使用当前 ManufacturingPrecision；BlockInstance renderer/cache、
  整实例选择与 native parity；显式 `max_string_len`/`max_json_depth`；behavior-preserving UI Component Foundation；
  同一 release binary 的原生 GUI synthetic fixture；400×100 bounded performance；fixed ZIP/fresh extract。
- **同轮显示缺陷**（R12/R16/R17；AT-024/AT-050/AT-062/AT-063）：复杂系统字体文字的 Region 轮廓只在显示
  路径建立精确水平边分箱，禁止从该缓存反推制造/导出几何；以完整轮廓 reference renderer 做零差异核对。
- **文档**：Gate 0 收口 S4-B1 状态（见下）；ADR 0031/0032；README/CAPABILITIES/IMPLEMENTATION_PLAN/
  AUTOMATION_API/AGENTS 更新；本文件与 S4_B2_REVIEW。

## Gate 0 — 收口 S4-B1

`docs/S4_B1_REVIEW.md` 结论区已更新为 **S4-B1 = PASS（Mac-first）**（引用 `f6eed93` 的
`S4_B1_FINAL_RESULTS.md` 与已存在的 `RCam_S4B1_f6eed93_{source,public_evidence}.zip` + `SHA256SUMS.txt`
sidecar，哈希与任务书 §0.1 列出的两行完全一致，未重新生成）。README/CAPABILITIES/IMPLEMENTATION_PLAN 同步更新。
仍不宣称：Windows、Full V1、P100K、Drill Import、完整 Block UI、`.rcam` File Lifecycle。

## 明确不做

Nested blocks；任意角 scale/shear；完整 GUI Block Editor；block library 云同步；Drill 解析器；PnP 导入；
RefDes 搜索；完整 Object Snap UI；Grip Editing；`File → Open/Save .rcam`；Autosave；Recovery；
Recent Projects；Windows；P100K。完成后**停止并提交审查，不自动开始 S4-B3**。

## 关联测试

`crates/editor-core/tests/block_core.rs`（core 级 create/duplicate/rotate/mirror/move/explode/delete/
undo-redo/hit-test/mirror-arc 8 例）、`crates/editor-service/tests/block_core_workflow.rs`（service 级完整
headless workflow + 共享 Definition fixture + 锁定层拒绝 + capabilities 4 例）、
`crates/rcam-project/tests/{rcam_project_codec_workflow,dependency_boundary,sample_fixture,
performance_workflow}.rs`（codec/security + 边界 + fixture + 400×100 性能）、
`crates/rcam-project/src/zip_codec.rs` 内嵌单元测试（8 例）。
