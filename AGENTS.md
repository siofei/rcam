# Gerber 编辑器：Codex 开发规则

## 本项目子任务分工

仅对本 RCam 项目生效：执行任务时，将可独立拆分的简单子任务交给 Luna 子代理，使用 `model="gpt-6-luna"`、`reasoning_effort="max"`。主代理负责规划、复杂问题、整合与独立验收。指定模型时使用 `fork_turns="none"` 或正整数，并提供所需上下文及明确的允许修改范围。避免多个代理同时修改同一文件；仅在实际启动子代理后才能声称已委派。该规则不改变主代理模型，不适用于其他项目。

## 项目定位与必读文件

这是仅面向 Windows／macOS 的本地 Gerber 图形编辑器，技术基线为 Rust + gerber-parser／gerber-types + 自有语义模型 + egui／eframe + wgpu。

开始任何工作前，阅读 `docs/DESIGN_V1.md`，再阅读 `docs/ACCEPTANCE_V1.md` 中对应用例。机器可读规范在 `docs/acceptance_cases.json`；应用服务和脚本扩展契约见 `docs/AUTOMATION_API.md`。当前文档基线为 1.1，变更见 `docs/CHANGELOG.md`。尚未创建项目时，从 S0 开始；这些文件是设计要求，不表示已有代码或已通过验收。

不要直接 Fork MakerPnP 主程序。`lib_gerber_edit` 是可选适配器，不是本项目的对象模型、撤销引擎或中文文字引擎。

## 范围与实施方式

每个任务写明 S0–S6 阶段、R01–R22 需求编号、AT-xxx 用例编号和允许修改的模块。只实施一个可运行、可测试的小闭环，不一次生成整套空按钮或未接入的占位模块。

第一版必须完成：导入、图层、导航、选择、移动／复制／删除／旋转／镜像、数值编辑、Undo/Redo、网格／吸附／测距、中英文矢量文字、Gerber 安全导出（Export Gerber，自 S4-B1 起不再是 Save/Save As）与重新导入；GUI 共用的无界面应用服务及真实接口集成测试。

范围外功能按设计安全拒绝。不能为了兼容而跳过未知命令，也不能把“拒绝全部真实文件”当作 V1 可用；S0 需冻结 REAL30 与 CORE10，核心 10 份真实样本最终必须 10/10 编辑往返成功。

新增需求或调整门槛先更新设计、用例和决策记录。不得通过删失败用例、换简单样本、忽略错误或临时降低阈值来获得通过。

## 不可破坏的架构与几何约束

- `editor-core`／`editor-service` 不依赖 egui、wgpu、winit 或窗口 API。第三方 AST 通过 `gerber-io` 适配；UI 必须经 `ApplicationService` 提交，不能直接修改文档字段。
- 制造模型、命中、测距与导出使用 f64 毫米。保留真实圆弧和原始格式信息；GPU 局部 f32 与显示细分只用于显示，严禁从 Mesh／像素掩膜反推 Gerber。
- 每层按原曝光顺序组合 Dark／Clear；不同图层独立。标准光圈孔洞和字体孔洞是对象局部透明，不得用全局 Clear 擦除原有内容。
- Region 的填充和孔洞必须符合冻结的 Gerber 规范；不得直接套用 SVG 的填充规则。镜像圆弧时处理方向，输出量化后重新检查合法性。
- 使用稳定 LayerId／ObjectId。共享光圈采用写时复制；修改一个实例不得污染其他实例，源 DCode 不作为跨文件主键。
- 所有修改通过原子命令。一次拖动／粘贴／文字生成是一个 Undo 事务；Esc 取消不修改内容。不得靠反复逆旋转实现无损撤销。
- 图形复制为应用内几何剪贴板；输入框仍走系统文本剪贴板。快捷键必须尊重焦点和 Windows／macOS 差异。

## 多格式交换与 `VectorScene` 预留

当前 V1 仍是 Gerber 编辑器；DXF／SVG／HP-GL(PLT) 导入和 SVG／PDF／PNG／DXF 正式导出属于 Post-V1。长期架构按 `docs/DESIGN_V1.md` 第 5.3 节和 `docs/adr/0017-vector-scene-and-format-interchange.md` 执行。

- Gerber 继续由 `gerber-io` 语义解释后直接进入 Manufacturing Model；**禁止为了统一而强制经过 `VectorScene`**，不得丢失 Dark/Clear、Aperture、Region、曝光顺序和兼容语义。
- 非 Gerber 矢量导入统一采用 `source parser -> VectorScene -> explicit Manufacturing Conversion -> Manufacturing Model`。DXF/SVG/PLT importer 不得直接修改 `SemanticDocument` 或复用 Gerber parser AST 作为公共模型。
- `VectorScene` 只表达通用矢量几何/层/变换，不承诺可制造语义。单位、默认线宽、Layer 映射、ClosedPath stroke/fill/Region、Pen/颜色映射、Bezier/Spline 拟合误差必须由 Manufacturing Conversion 显式处理；歧义要拒绝或要求用户选择，不能静默猜测。
- **严禁从 renderer Mesh、GPU tessellation、当前缩放路径或屏幕像素反推 Gerber/SVG/PDF/DXF 矢量数据。** 矢量导出读取语义/几何模型；PNG 可走受控离屏 raster adapter。
- 在专门 F 阶段启动前，不新增 DXF/SVG/PLT 产品依赖、菜单空壳、公共 API 或“已支持”能力声明。开始实现时先更新需求/验收/ADR，并审查第三方 parser/writer 的许可证与输入安全。

## 脚本自动化扩展边界

V1 只预留并验收业务 API，不实现 Python／Lua／JavaScript 引擎、脚本控制台、正式 CLI、HTTP／JSON-RPC 服务或插件运行时。

- GUI 与无窗口测试共用打开、查询、编辑、文字、Undo／Redo、导出实现；未来适配器只做参数／结果转换，不能模拟点击或另写 writer。
- 公共 DTO 自有且可 JSON 编解码；API 版本、请求 ID、文档／图层／对象 ID、毫米／角度单位、错误码和能力表明确。不得暴露第三方 AST、可变模型引用或 GPU 资源。
- 修改携带 `expected_revision`；每文档串行提交；Undo／Redo 也推进 revision，脏状态另按内容基线判断。失败批次整体不变，成功只占一个 Undo；不把文件 I/O 混入内存原子批次。
- 服务不得弹窗／读 stdin，文件路径、字体和权限显式传入；需要覆盖或丢弃元数据时返回结构化待确认结果。与 GUI 复用文件安全、任务取消和资源预算。
- `editor-service` 的正常依赖树不含窗口／GPU；Windows 与 macOS 的真实无界面测试必须完成查询—编辑—导出—重开。仅有空 trait、mock 成功或 JSON 示例不算完成。

## 渲染与后台任务

复用 eframe 管理的 GPU 生命周期，采用兼容的 egui／eframe／egui-wgpu 版本，通过官方支持的自定义回调接入画布。不要在组件内部重复创建 Device／Queue。

缓存光圈、Mesh 与实例；导航不重解析文件，不全量重建制造几何。合批不能重排曝光，正确性优先于 draw call 数量。不要假定任意硬件都支持相同纹理格式和采样数。

读取、解析、大量细分、索引、字体转换和导出离开 UI 主线程。任务必须有版本标识、取消和资源预算；过期结果不能覆盖新文档。设备重建只影响 GPU 资源，不丢失编辑内容。

## 保存、隐私与发行

默认另存为。导出从同一版本快照生成，做语义与数值校验，重新解析核对，再写同目录临时文件并安全替换。失败不损坏已有文件、不错误清除脏标记；覆盖前检查外部修改。

对不能保证仍有效的元数据按设计报告并确认，不能暗中丢弃或照抄失效网络／元件关联。自身往返不是唯一真值，还需独立几何断言和独立工具核对。

不得自动上传 Gerber、字体或私有样本到在线查看器、遥测服务或远程仓库。`fixtures/private/` 与 `evidence/` 默认 Git 忽略。不得将用户字体文件纳入提交或分发包；实际依赖、复制代码与字体使用方式需有版本、来源和许可记录。

Windows 原生为主开发与验收环境；macOS 原生完成第二平台验收。Linux／WSL2 不支持，不要求核心兼容，不新增相应 CI、适配代码或安装包。个人辅助终端不算产品目标；无 GPU CI／无界面测试不能替代 Windows DX12 或 macOS Metal 真机测试。

## 依赖与构建

锁定工具链、Cargo.lock 与 Git 依赖 commit；先核对锁定版本的官方 API 和示例，不凭记忆编造第三方接口。新增依赖须说明用途、替代方案、平台条件和许可证。

创建工作区后按任务实际执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
```

接口包创建后，额外执行 `cargo test --locked -p editor-service --test automation_contract` 和 `cargo test --locked -p editor-service --test headless_workflow`，并审查服务正常依赖树。尚未实现测试入口时明确未执行，不伪造日志。

不要无条件组合互斥 feature；CI 矩阵只包含 Windows MSVC 与 macOS arm64。文档中 `xtask` 是待实施工具，不得在尚不存在时声称已运行。

## 每次提交与验收证据

交付说明包含：改动摘要、关联 Rxx／AT-xxx、实际执行命令、退出结果、测试环境、证据路径、未完成项及剩余风险。

没有执行就写“未执行”；缺真实硬件、样本或依赖写“阻塞”。截图不替代制造几何测试，单元测试不替代 GUI／IME／GPU 实测。性能保存 release 构建、固定样本与机器的原始数据，不把设计门槛当成实测成绩。

验收数据使用 schema_version=2；96 个有效用例在 `cases`，AT-079 仅在退役记录中且编号不得复用。按逐项 `required_platforms` 检查证据，不能混用 1.0 基线的用例含义。

验收结果按运行 ID 另存，不覆盖历史。B0 几何／数据安全失败立即阻止输出生产文件。只有全部适用必测通过、双平台证据齐全、CORE10 达到 10/10 且 B0/B1 清零，才可标记“双平台 V1 通过”。

**S4-B1 Multi-Gerber Workspace = PASS（Mac-first）**（见 docs/S4_B1_REVIEW.md）；**S4-B2 Block Core + `.rcam` schema v1 = PASS（Mac-first）**（见 docs/S4_B2_REVIEW.md）；**S4-B3 `.rcam` Project Lifecycle = PASS（Mac-first）**（见 docs/S4_B3_PLAN.md、docs/S4_B3_REVIEW.md、ADR 0033 / 0034）。**S4-C1 Full Object Snap = PASS（Mac-first bounded）**（见 docs/S4_C1_REVIEW.md）；INFRA1 panic hook closeout = PASS（Mac-first bounded，见 exports/INFRA1_PANIC_c65bee0/REVIEW.md）。
Global Units & Manufacturing Precision Foundation 按 Mac-first 范围收口（见 GLOBAL_UNITS_PRECISION_REVIEW）；
不声称 Windows、完整 V1、P100K 或完整 CORE10。
S4-C1 已按 `docs/S4_C1_REVIEW.md` 完成 Mac-first bounded 复审。用户现已明确授权启动 **S4-C2 Grip Editing v1**；范围、允许模块和退出门禁见 `docs/S4_C2_PLAN.md`、`docs/S4_C2_ACCEPTANCE_ADDENDUM.md`、ADR 0038，实际状态见 `docs/S4_C2_REVIEW.md`。S4-C2 未经验收不得声称 PASS；完成后停止并提交复审，不自动开始 Block Editor、Alignment、Array 或 PnP/RefDes。
Windows deferred / not executed，最终双平台 V1 门槛保持不变。

## Forward Architecture Reservations（长期约束，S4-B1 合并）

以下架构必须保留，不得因为当前阶段尚未实现完整 UI 而写死模型。完整背景见 `docs/DESIGN_V1.md`
“长期架构方向（钢网设计）”一节与 `docs/IMPLEMENTATION_PLAN.md` 的阶段顺序。

1. **Gerber 只 Import / Export**：导入后与磁盘源文件解耦（仅保存 filename、imported SHA-256、import time 作为 provenance）；
   不做 live source link、mtime reload、写回原 gbr。未来 Save/Save As 只针对 `.rcam`。Export 不清 dirty、不建立 source link、不改变 layer identity。
2. **LayerKind 可扩展**：不能写死 `Layer == Gerber`；必须允许未来 Drill/Excellon（DrillHit/DrillSlot/Route，独立 Tool namespace）。
3. **Board Coordinates**：Manufacturing World 继续 f64 mm；预留 Source→Board→World，`CoordinateTransform2D` 仅 translation/rotation/reflection，禁止默认 scale/shear。
4. **Component Placement**：ComponentPlacement/RefDes/PnP 独立于普通 Gerber SemanticObject。
5. **Reusable Blocks**（S4-B2 已实现，见 ADR 0032）：`BlockDefinition`（`SemanticDocument.block_definitions`，
   项目级）+ `BlockInstance`（`SemanticGeometry::BlockInstance`，属于某 Layer）；实例只允许
   translation/rotation/mirror（`BlockTransform`），第一版禁止 nested block（`BlockObjectGeometry` 类型层面
   不可表示实例，不是运行时检查）；Definition 修改（含 revision）更新全部 instance，Instance 编辑只改自己的
   transform；Gerber Export flatten；RCam Block ≠ Gerber `%AB`。完整 GUI Block Editor 仍属 S4-C。
6. **Object Snap / Grip**：统一 SnapFeatureProvider / SnapQuery / SnapCandidate / SnapFeatureId / SnapResolver；以 Manufacturing Boundary 为真值，
   不得从 GPU/tessellation/像素反推。S4-C2 Grip v1 另用稳定 GripFeatureId 表示可编辑制造参数/节点，单对象纯预览，释放时经 ApplicationService 一次事务；尺寸 Grip 对共享光圈写时复制，目标捕捉复用 S4-C1 Resolver。
7. **Shortcut Architecture**：CommandId/Registry + Keymap + ShortcutContext（IME/TextInput > Modal > Tool > Canvas > Global）；
   Menu/Toolbar/Context Menu/Shortcut 共用 Command；逻辑修饰键 Primary/Secondary/Shift/Alt；用户 keymap 属于 AppPreferences；Automation 仍调用 ApplicationService，不模拟快捷键。
8. **Layer View State**：颜色、Visible、Selectable、Locked、Z-order、Filled/Outline/ZeroWidth、category styles、面板宽度均不得改变 Gerber Writer 输出，也不产生制造 revision。
9. **Layer Delete**：空层可低风险直接删除；非空强确认；dirty/generated 更强确认；必须 one transaction + Undo（恢复同一 LayerId、z-order、样式）；允许删除最后一层；headless `remove_layer` 非空需 `allow_non_empty`。
10. **No global snap-point database**：Snap 使用屏幕半径 → 空间索引 → 附近对象 lazy features。
11. **No nested block in first block version**：避免循环引用和迁移复杂度（S4-B2 起由 `BlockObjectGeometry` 的类型形状强制，而不是留给运行时检查）。
12. **ApplicationService remains mutation boundary**：GUI、Command、Shortcut、Automation 不得绕过。

### Accepted Layer UI

LayerPanel 单列 compact list、可拖拽调宽、有最小宽度：

```text
●  ≡  ■  Layer Name      👁  🔒  ▣/□/─  ⋯
```

`●/○` 是 Active 指示，`▣ □ ─` 分别是 Filled / Outline / ZeroWidth 的快捷菜单；不再有 inline 展开行，详情放 tooltip / Settings / Categories。

右键与 `⋯` 使用同一个 context menu。颜色同时支持确定性 auto palette、presets、recent colors（session-only，8 个）、full picker；分类色继承图层色或覆盖。
手动列表顺序 = display Z-order。必须支持：Active Layer、Solo（双击图层名切换，再次双击取消）、Show/Hide All Layers、Fit Layer、Visible、Selectable、Locked、Filled/Outline/ZeroWidth、
Category color/filter/lock、New Empty Layer、Import Gerber、Delete + Undo。Selectable 放 Layer Settings，不长期占 Layer row。

宽度不足时：保持 Active/Color/Visible/Locked/DisplayMode/More 可用；图层名用 UTF-8 安全省略号截断；完整名称在 tooltip / Settings / Rename modal 可见；
到最小宽度后不再缩小，绝不允许控件重叠。面板宽度只是 UI preference，不改制造状态或 Gerber 输出。

### 阶段顺序

```text
S4-B1 Multi-Layer Workspace + reservations — PASS（Mac-first）
→ S4-B2 Block Core + .rcam schema v1 — PASS（Mac-first）
→ S4-B3 Project lifecycle — PASS（Mac-first）
→ S4-C1 Full Object Snap — PASS（Mac-first bounded）
→ INFRA1 Runtime Diagnostics — PASS（Mac-first bounded，panic hook closeout）
→ S4-C2 Grip Editing v1 — 当前实施切片，未验收
→ S4-C2+ Block/PnP/RefDes 等 — 必须另行立项
```


### INFRA1 Runtime Diagnostics（Mac-first bounded closeout）

INFRA1 panic hook closeout 最终为 PASS（Mac-first bounded），证据见 `exports/INFRA1_PANIC_c65bee0/REVIEW.md`；Windows deferred。S4-C2 已另获明确授权启动，但还没有实现/原生/打包验收结论。
