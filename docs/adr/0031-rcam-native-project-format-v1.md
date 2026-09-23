# ADR 0031 — `.rcam` Native Project Format Schema v1

状态：Accepted（S4-B2，2026-09-22）。实现证据见 [S4_B2_REVIEW](../S4_B2_REVIEW.md)。`File → Open/Save` 的产品生命
周期（S4-B3）不在本 ADR 范围内——这里只冻结磁盘上的字节形状与 encode/decode 语义。

## 背景

`.rcam` 必须是 RCam 自己的工程真值，不是“把原始 `.gbr` 打包进 ZIP”。它需要从第一天认识 Block（ADR 0032），
否则未来把重复几何从物理复制改成引用共享会是一次破坏性迁移。S4-B1 结束时项目里已经有 Workspace/View
state、Manufacturing Precision、Import Provenance 等好几类必须持久化的状态分散在 `editor-service` 的运行时
记录里，从未序列化过。

## 决定

1. **`.rcam` = ZIP 容器**：`manifest.json` + `project.json` + `layers/<layer-id>.json`（每层一个文件，自包含
   `SemanticLayer` + `LayerWorkspaceState` + 可选 `ImportProvenance`）+ `blocks/<definition-id>.json`（每个
   Definition 一个文件）。`project.json` 只保留 `layer_order`（层 id 列表，决定 z-order）与
   `block_definition_ids`（Definition id 列表）作为索引，不重复存一份内容——避免同一份几何在
   `project.json` 与 `layers/*.json` 之间不一致。

2. **ZIP 编解码是手写的、只用 store（不压缩）**，不引入 `zip` crate 依赖。这与代码库现有的做法一致——
   `editor-service` 早就手写了一份不依赖 `sha2` crate 的 SHA-256（`sha256_hex`），现在把它搬到
   `editor_core::hash`（S4-B2 起 `editor-service`/`rcam-project` 共用同一份实现，不重复）。手写 store-only
   写入/读取给了完全可控的 fail-closed 读取策略（§8 要求“不能直接 `extractall()`”），比信任第三方解压器
   更省心；store（无压缩）让“两次 encode 同一逻辑内容得到相同字节”这件事没有任何压缩级别方差需要锁定。
   以后要加压缩，可以在同一个 manifest 形状下加一个新字段，不需要 schema 破坏性变更。

3. **`crates/rcam-project` 是纯模型 + 编解码 crate**，依赖只有 `editor-core`/`serde`/`serde_json`，没有
   `egui`/`eframe`/`wgpu`/`winit`（`dependency_boundary.rs` 测试核对，镜像 `editor-service` 自己的边界测试）。
   它与 `editor-service` 是兄弟关系，不是任何一方依赖另一方：`ApplicationService` 仍是唯一的制造修改边界，
   Project codec 只编解码 `ApplicationService` 已经拥有的状态，不取代它。

4. **`RCamProject`（内存模型）不持久化 `SemanticFormat`/`SourceMetadata`。** 这两个字段是 Gerber 坐标格式
   相关的概念（小数位数、零压缩等），`.rcam` 内部真值永远是 f64 mm，与来源文件格式完全解耦（§16）。
   为了复用 `SemanticDocument::validate()` 做语义校验，`RCamProject::to_semantic_document()` 会临时合成一个
   固定的、总是合法的 `SemanticFormat` 常量，这个值从不写入磁盘、也从不从磁盘读回。

5. **`apertures` 存在 project 根，跨层共享**，与 live 的 `SemanticDocument.apertures` 模型完全一致
   （S4-B1 的命名空间规则继续适用）；单层导出（`gerber.export_layer`）时会额外收集被该层引用的
   `BlockInstance` 所对应 Definition 内部使用的光圈，一起塞进单层导出文档，否则展平后的 Flash 会引用
   缺失的光圈。

6. **Grid/Snap 的 project 级设置是 `rcam-project::model` 里全新的小类型**（`GridSettings`/
   `SnapSettingsState`/`CameraState`），不是 `editor-app::tools::GridSettings`（纯 GUI 内部类型）。两者刻意
   不共享：`.rcam` 落盘的是“这个工程的网格/捕捉设置”，`editor-app` 的 GUI 状态是会话内部的操作细节；
   S4-B2 没有把两者接起来（没有 `File → Open/Save`），留给 S4-B3。

7. **Layer id / Object id / BlockDefinition id 继续是 `String`**，不是新引入的强类型 newtype
   （`BlockDefinitionId` 已存在，保留）。S4-B2 决定不做一次全代码库的 ID 类型系统重写：稳定性
   （§39，decode 后同一身份）由“解码后原样写回同一个字符串”保证，已经被
   `ids_view_styles_precision_grid_snap_round_trip` 测试覆盖，不需要类型系统额外背书。

8. **确定性编码（§38）不需要专门的“稳定排序”代码**：`RCamProject` 及其子结构全部是 struct/Vec，字段顺序
   就是 serde 派生的声明顺序，没有一处用 `HashMap`（唯一潜在风险点 `DisplayClassStyles` 早就是
   `BTreeMap`）；ZIP 内条目按路径排序后写入，时间戳固定为 ZIP 纪元（1980-01-01）。“encode 两次得到相同
   字节”与“encode → decode → encode 相同字节”都由测试核对。

9. **§48 非有限值检查在序列化之前完成，不是之后扫描 JSON 文本。** `serde_json` 把非有限 `f64`
   静默序列化成 `null`，而 `null` 也是很多 `Option<T>` 字段的合法值（比如未命中的光圈孔），区分不开；
   `RCamProject::validate()`（`encode_v1` 入口第一步调用）已经通过 `SemanticDocument::validate()` 传递性地
   检查了全部几何数值，外加 project 独有的浮点设置（精度、网格间距、捕捉半径），保证非有限值在触碰
   JSON 序列化之前就被拒绝。

10. **`format_version` 从第一版就存在两处**（`manifest.json` 与 `project.json`），任何非 1 的值都
    fail-closed（`migrate::migrate` 目前对任何版本号都返回 `UnknownFormatVersion`——S4-B2 不创造 v2，只
    留出未来 v2 读取器接管的一个调用点，不是在单个 serde struct 上堆 `Option<>`）。

11. **`LayerKind::Drill` 不可编码，也不可解码。** `RCamProject::validate()` 在遇到任何
    `LayerWorkspaceState.kind == Drill` 时返回 `UnsupportedFeature`——`encode_v1` 内部调用 `validate()`，
    因此编码器本身拒绝生成这样的文件；`decode` 在末尾也调用同一个 `validate()`，因此假设性地遇到一个
    （未来版本产生的）带 Drill 层的文件也会拒绝，不会假装能 roundtrip 一个不存在的 `DrillObject`。

12. **Solo / Selection / Undo 历史 / AppPreferences 不是运行时过滤掉的，是 schema 里根本没有这些字段**
    （§13/§14/§40/§44）。`LayerWorkspaceState` 没有 solo 字段；`WorkspaceProjectState` 没有 selection 字段；
    `RCamProject` 没有 history 字段；快捷键/面板宽度/最近颜色/主题不存在于本 crate 的任何类型里。

13. **reader 安全预算是显式格式契约。** `Budget.max_string_len` 统一验证全部 persisted、用户可控 String；
    `Budget.max_json_depth` 在 serde schema parse 前扫描 manifest/project/layer/block JSON。恰好等于上限允许，
    上限 + 1 返回 `RESOURCE_LIMIT`（`string_len` / `json_depth`）；被 v1 policy 忽略的可选字段也不能绕过
    entry、string 或 depth 预算。

14. **S4-B3 预冻结兼容修正：显示单位有四种。** `WorkspaceProjectState.display_unit` 的 v1 JSON 值为
    `millimeters`、`inches`、`mils`、`micrometers`；默认仍为 `millimeters`。此前三种值的文件继续读取，
    不提高 `format_version`。显示单位只影响 UI，制造几何仍为 f64 mm。

## 不做（本阶段）

`File → Open/Save/Save As .rcam`；Autosave；Crash Recovery；Recent Projects；v2 迁移的具体实现（只留调用点）。

## 不变量

`.rcam` 的逻辑内容（不含 ZIP 时间戳等元数据）在同一个 `RCamProject` 上两次 encode 必须字节相同；`decode`
的每一步失败都必须是命名清楚的 `ProjectError` 变体，绝不静默丢对象或静默截断；`manifest.json` 里的每个
条目都必须有对应的 SHA-256 校验，archive 里也不能有 manifest 未描述的条目。
