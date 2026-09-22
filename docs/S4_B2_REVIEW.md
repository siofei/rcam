# S4-B2 — Block Core + `.rcam` schema v1 review

2026-09-22。范围见 [S4_B2_PLAN](S4_B2_PLAN.md)，决策见 [ADR 0031](adr/0031-rcam-native-project-format-v1.md) /
[ADR 0032](adr/0032-block-core.md)。Windows deferred / not executed。

## 结论（必须先读）

**源码实现 + 自动化测试 = PASS（Mac-first，云端/无窗口环境）。** 本文件随源码提交，描述实现内容与验证方法。
**原生 Mac GUI 冒烟、native Metal parity 与固定 ZIP 发布（`RCam_S4B2_<shortsha>_{source,public_evidence}.zip` +
`SHA256SUMS.txt`）本轮未执行**——工作树在本次会话结束时仍未提交（无 clean commit 可绑定固定交付与 evidence），
且本轮没有对已构建的 `RCam.app` 做过 computer-use 驱动的原生 GUI 检查。因此**不得声称 “S4-B2 = PASS
（Mac-first）”**；按任务书 §70 的措辞，这是“完成后停止并提交复审”的中间状态，不是最终 PASS 声明。

## 已实现

### Block Core（`editor-core`）

- `crates/editor-core/src/block.rs`：`BlockDefinition`（新增 `revision: u64`）、`BlockObjectGeometry`
  （类型层面无法表示嵌套实例）、`BlockTransform`（rigid-only，`to_coordinate_transform()` 复用
  `board::CoordinateTransform2D`）、`resolve_geometry`/`resolve_instance`（flatten，复用既有
  `WorldTransform::apply` 与 `edit::translate` 的 RectangularSweep 90° 约束与 Arc 方向翻转）、
  `local_geometries`。
- `SemanticDocument.block_definitions: Vec<BlockDefinition>`（`#[serde(default)]`）；
  `SemanticGeometry::BlockInstance { definition_id, transform }` 新变体，穷尽匹配迫使编译器审出全部
  需要更新的调用点（bounds.rs、hit_test.rs + select_rect、metrics.rs、transform.rs、workspace.rs 的
  `classify_object`/`geometry_fingerprint`、gerber-io 的 writer/precision、editor-app 的渲染器/snap 点/
  属性面板）。
- Bounds：`geometries_bounds_with_blocks`，定义局部 bounds 每次查询内按 `(id, revision)` 缓存一次
  （跨查询缓存留给 service 层，见下）。Hit test：`SemanticDocument::hit_test`/`select_rect` 都在遇到
  `BlockInstance` 时 resolve 后取最近图元，返回值仍是 instance 的 `object_id`（§30，全靠既有循环结构，
  没有新代码路径）。Metrics：`metrics::calculate` 对 `BlockInstance` 求和其定义内每个对象的 metrics
  （rigid/mirror 变换不改变面积/周长，§31）。
- `DisplayClass::BlockInstance` 新分类（不是 `ApertureBlock`）。
- Gerber export：`gerber-io::flatten_block_instances`（`write_s1_with_budget` 内部）把 Instance 展平成
  普通对象再写出；`documents_semantically_equal`（round-trip 校验）现在也对 `expected` 侧先 flatten
  再逐对象比较——这是本轮发现并修复的一个真实 bug：修复前该函数直接 `zip` 两侧 `objects`（长度不等
  时必然失败），会让任何包含 Block 的 Layer 导出都报 `VALIDATION_FAILED`。

### Block Service API（`editor-service`）

- 新增 `blocks.list_definitions`/`blocks.get_definition`（只读）、
  `blocks.create_definition_from_objects`、`blocks.create_instance`、`blocks.update_instance_transform`、
  `blocks.rename_definition`、`blocks.explode_instance`、`blocks.delete_definition`（引用中拒绝，
  `BLOCK_DEFINITION_REFERENCED`）；均走 `expected_revision` 守卫、`check_workspace_edit`（层锁定/分类锁定）、
  `record.history.*`（一次 Transaction、一次 Undo）。JSON 自动化契约新增对应 8 个 op；`Capabilities.
  supported_operations` 已收入这 8 个 op（`unsupported_operations` 的 `blocks.define` 占位已移除，
  `project.open/save (.rcam)`、`drill.import` 继续保留）。
- `editor-core::edit::EditHistory` 新增 `create_block_definition`、`explode_block_instance`、
  `rename_block_definition`、`delete_block_definition`、`create_block_instance`、
  `set_block_instance_transform`；`Operation` 新增 `ReplaceObjects`（一次事务内同时删除 N 个对象、插入
  M 个对象、可选插入/删除一个 `BlockDefinition`，复用既有 Insert/Delete 的 merge/retain 辅助函数）、
  `RenameBlockDefinition`、`RemoveBlockDefinition`。Move/Rotate/Mirror/Duplicate **没有新增方法**——
  既有 `objects.move/rotate/mirror/duplicate` 在 `translate()`/`WorldTransform::apply()` 里各加一个
  `BlockInstance` 分支后就已经正确处理实例（ADR 0032 §5）。
- 单层导出（`workspace.rs` 的 single-layer 提取）现在也收集被导出层引用的 Definition 内部使用的光圈，
  否则展平后的 Flash 会引用缺失光圈。

### `.rcam` schema v1（新 crate `rcam-project`）

- `crates/rcam-project`：`model.rs`（`RCamProject`/`LayerProjectState`/`WorkspaceProjectState`/
  `GridSettings`/`SnapSettingsState`/`CameraState`/`BoardProjectState`，复用
  `editor_core::workspace::{LayerWorkspaceState, ImportProvenance}`）、`zip_codec.rs`（手写确定性
  store-only ZIP，CRC-32、路径校验、entry 数/大小预算、local/central 双重一致性校验）、`manifest.rs`
  （`manifest.json` 形状 + 逐条目 SHA-256 校验 + 未被 manifest 描述的条目拒绝）、`codec.rs`（encode_v1/
  decode 全链路：ZIP 结构 → 路径 → 预算 → manifest → 哈希 → schema 解析 → `RCamProject::validate()`）、
  `migrate.rs`（`format_version != 1` 的调用点，目前总是 `UnknownFormatVersion`）、`error.rs`
  （`ProjectError` 全部变体 fail-closed）。
- `editor_core::hash`：把原来只存在于 `editor-service::lib.rs` 的手写 SHA-256 搬到 `editor-core`，
  `editor-service` 与 `rcam-project` 共用同一份实现，不重复。
- `fixtures/synthetic/s4b2/sample.rcam`（`examples/generate_sample_fixture.rs` 生成，11215 字节）：
  40 个开口的 `BlockDefinition` + 5 个实例（0°/90°/37°/Mirror X/Mirror-X+Rotate180≡Mirror Y），全部
  synthetic，无私有 Gerber；`tests/sample_fixture.rs` 核对它仍能 decode/validate/re-encode 字节相同。
- `dependency_boundary.rs`：`rcam-project` 的 normal 依赖树只有 `editor-core`/`serde`/`serde_json` 及其
  传递依赖，无 egui/eframe/wgpu/winit/zip。

## Mac 云端证据（Apple M1 / macOS / Rust 1.89.0 锁定工具链，无窗口环境）

| 项 | 命令 | 结果 |
|---|---|---|
| 格式 | `cargo fmt --all -- --check` | rc=0 |
| 检查 | `cargo check --workspace --all-targets --locked` | rc=0 |
| clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | rc=0 |
| 全量测试 | `cargo test --workspace --locked --no-fail-fast` | rc=0，49 个测试二进制、0 失败 |
| block core | `-p editor-core --test block_core` | rc=0，8/8 |
| block service workflow | `-p editor-service --test block_core_workflow` | rc=0，4/4 |
| rcam codec workflow | `-p rcam-project --test rcam_project_codec_workflow` | rc=0，14/14 |
| rcam 依赖边界 | `-p rcam-project --test dependency_boundary` | rc=0，2/2 |
| rcam sample fixture | `-p rcam-project --test sample_fixture` | rc=0，1/1 |
| rcam 性能（400×100） | `-p rcam-project --test performance_workflow` | rc=0，1/1 |
| release | `cargo build --release --locked -p editor-app` | rc=0 |
| 依赖树 | `cargo tree --locked -p editor-service -e normal` / `-p rcam-project -e normal` | rc=0，均无 GUI/GPU 依赖 |
| 清单 | `python3 scripts/source_manifest.py --check` | rc=0，336 files（已用 `source_manifest.py` 重新生成一次以纳入新增源文件） |
| CORE10 单测 | `python3 scripts/test_audit_core10.py` | rc=0，7/7 |
| 打包脚本单测 | `python3 scripts/test_package_source.py` / `test_package_release.py` | rc=0，2/2、5/5 |

**未执行**：native Metal parity（`native_metal_*`，需要真实 GPU 会话）；computer-use 驱动的原生 GUI 冒烟
（本阶段没有新增 Block GUI，理论上没有新的界面路径需要冒烟，但按惯例仍应至少确认应用能正常启动/无回归）；
固定发布 ZIP（`RCam_S4B2_<shortsha>_source.zip`/`_public_evidence.zip`）与 `SHA256SUMS.txt`（需要先有
clean commit 作为锚点，本轮工作树未提交）；`scripts/verify_source_package.py --tested-source-hashes`
的 fresh-extract 核对（同样需要先打包）。

## 本轮发现并修复的真实缺陷

1. **Gerber writer round-trip 校验器不认识 Block**（`gerber-io::documents_semantically_equal`）：
   修复前对 `expected`/`actual` 两侧的 `objects` 直接按下标 `zip` 比较，只要 `expected` 里还留着
   `BlockInstance`（未展平）而 `actual`（重新解析导出字节得到）已经是展平后的多个普通对象，两侧长度
   必然不等，导致任何含 Block 的 Layer 导出都会在 `verify_roundtrip` 报 `VALIDATION_FAILED`。修复为
   在比较前对 `expected` 侧调用与 writer 相同的 `flatten_block_instances`。由
   `block_core_workflow.rs` 的端到端导出/重新导入测试发现。
2. **ZIP 读取器只校验 local/central 文件头的长度字段，不校验文件名字节本身**：本地文件头里镜像的
   文件名区域被破坏时，读取仍会用中心目录里的（未损坏）文件名，静默放过损坏。修复为额外比较两处
   文件名字节；由 `rcam_project_codec_workflow.rs` 的 manifest 篡改测试发现。
3. 单层 Gerber 导出此前只收集顶层 Flash 引用的光圈；含 Block 的层会因为 Definition 内部 Flash
   引用的光圈未被收进单层导出文档而导出失败（`missing aperture`）。已在提取单层文档时一并收集
   被引用 Definition 内部使用的光圈。

## 已知裁剪 / 设计决定

- `.rcam` v1 不持久化 `SemanticFormat`/`SourceMetadata`（Gerber 坐标格式概念，与 `.rcam` 内部真值 f64 mm
  完全解耦，见 ADR 0031 决定 4）；`RCamProject::to_semantic_document()` 用固定常量合成校验用文档。
- `.rcam` v1 的 JSON 嵌套深度没有专用限制器，依赖 `serde_json` 默认递归深度保护与 ZIP 条目大小预算的
  组合防护（ADR 0031「不做」一节明确记录，不是遗漏）。
- Block-local Flash 对象引用的光圈随项目 apertures 精度归一化（`gerber_io::normalize_manufacturing`）
  只处理 `BlockInstance.transform.translation`（像 Flash 的 `center` 一样量化），不重新量化
  `block_definitions` 内部几何——Definition 几何假设在被 `create_definition_from_objects` 捕获时已经是
  当时项目精度下的量化值；如果之后修改项目精度，旧 Definition 不会被追溯重新量化。这是一个已知、
  记录在案的范围裁剪（`gerber-io/src/precision.rs` 代码注释同步说明），不是静默 bug。
- Block 相关的渲染（`editor-app::display.rs`）与属性面板（`main.rs`）目前对 `BlockInstance` 分别返回
  `UNSUPPORTED_FEATURE`/展示只读信息：本阶段没有 GUI 能创建 Block 实例，因此这两条路径在真实使用中
  不可达，行为与既有“未支持显示情形”的惯例一致（参考 `oblique rectangular display sweep` 的先例）。

## 仍需完成

- 原生 Mac GUI 冒烟、native Metal parity（如果本阶段判定需要——Block 没有新渲染路径，可能只需确认
  既有 view-style parity 矩阵未回归）。
- 固定发布 ZIP + `SHA256SUMS.txt` + fresh extract（需要先有 clean commit）。
- `scripts/run_s4b1_final_gates.py` 风格的 `run_s4b2_final_gates.py`（如果需要复用同一套自动化门禁脚本
  框架，本轮未新增）。

## 下一阶段

S4-B3（`.rcam` Project Lifecycle：New/Open/Save/Save As、Migration、Recovery、Recent Projects）待本阶段
审查通过后另行启动；本阶段没有把 GUI 接到 `.rcam` 编解码上。
