# 能力与实施状态

当前阶段状态：S4-C1 Full Object Snap、INFRA1 Runtime Diagnostics、S4-C2 Grip Editing v1、S4-C3 Block Editor v1 均为 **PASS（Mac-first bounded）**。S4-C4 Alignment / Distribution 已按 Mac-first bounded 完成验收。本项目仍不是双平台 V1 编辑器；Windows deferred / not executed，完整 V1、CORE10 10/10 与 P100K 均未通过。完整产品边界继续以 DESIGN_V1 第4节为准。
以下段落按阶段保留历史实施状态；本节末列出当前阶段能力与限制。

## S0 历史技术验证

| 能力 | 本阶段边界 |
|---|---|
| 最小 parser/毫米模型/曝光测试 | 已实现有限圆 Flash 子集；主代理35项输入/状态/契约检查通过，完整AT未通过 |
| 自有只读 DTO/ApplicationService | 已实现只读源文本打开、快照、固定点分析、能力查询与严格JSON请求；不等于完整自动化接口 |
| eframe/wgpu 最小画布 | 已实现快照驱动的自定义Callback，本机Metal已原生静态验证，交互因权限阻塞；不得宣称双平台GPU通过 |
| 正式打开/多图层编辑/选择/撤销/文字/保存 | 未实现，S1–S4 |
| EditableV1 与 ReadOnlyExact | 当前不授予文件此能力等级 |
| 真实样本兼容率/CORE10 往返 | 未执行 |
| Windows/macOS完整验收 | 未执行 |
| AM/AB/SR/G74/旧图像命令等 | 当前运行时仍安全拒绝；有限目标扩展已在 ADR 0005 冻结，未实现不放行 |
| Linux、脚本引擎、正式 CLI/HTTP、拼板等 | 不属于本次交付范围 |

S0 可以拒绝尚未实现的语义，但其失败不作为 V1 可用性证据。禁止输出生产文件；不能使用 GPU 网格或像素反推 Gerber。正式能力表只能在功能实现并经验证后更新。

实际S0上限：输入2 MiB、最多100,000个圆Flash；GPU演示最多4层/16对象，超限拒绝预览。此限制仅适用S0原型，不调整V1性能与能力门槛。普通open_s0保留输入几何，open_demo_s0才添加独立构造的线/圆环/跨层风险图形。当前无文件路径读写、无writer。

## S0-B 修补（2026-09-15）

- 已建立坐标后接受纯 `D03*` 重复 Flash；首条仍要求完整 X/Y，顺序/光圈/极性保留。
- 四个只读操作统一成功/失败响应和请求关联；`error.details` 提供结构化诊断。
  能力查询公开实际 source_bytes/objects 预算；close/revision 写入/任务/进程总预算仍未实现。
- 极小孔不再被核心比较容差填掉。GPU 数值无法保真时明确拒绝整个预览，制造模型保留 f64。
- 生产 WGSL 短线投影已修正，原生 Metal 离屏覆盖点验证通过；窗口交互、Windows DX12 尚无本轮证据。
- 服务无窗口/GPU正常依赖已成为 CI 失败门禁。历史 35 项独立断言迁入仓库测试。
- 冻结 CORE10 全命令使用审计已执行，但 IO/IC/增量格式等风险仍未解决；见 ADR 0004。
  S0 整体仍阻塞，当前不授予 EditableV1/ReadOnlyExact，不输出生产文件。

具体数值/服务规则见 `adr/0003-s0-b-service-and-numeric-boundaries.md`，本轮证据见 `S0_B_REVIEW.md`。

## S0-C 目标冻结（2026-09-15）

[ADR 0005](adr/0005-s0-c-editable-core.md) 冻结新增目标，DESIGN_V1 第4.4节同步。
CORE10 使用审计 10/10 完成，不能读作兼容或往返 10/10。
当前圆 Flash 的 FS 支持只覆盖原型已有范围，4.3 格式仍拒绝；18 份公开小样分别登记当前行为。
IO/ICAS/FSDI 有4组本机独立工具合成对照；没有产品语义/writer/真实业务往返通过声明。
没有增加服务 operation、产品依赖、GPU容量或脚本/网络功能。Windows 原生证据仍阻塞。

## S1-A 语义与规范化写出（2026-09-15）

实现与验收边界以 [本轮任务](../RCam_S1A_NEXT_TASK.md)、ADR 0005 和
[实际审查报告](S1_A_REVIEW.md) 为准。原 S0 API 与窗口保留；不授予 EditableV1/ReadOnlyExact，
S0-C Windows platform gate 仍 blocked，不作为生产加工文件发行。

- 自有 f64 毫米模型增加标准光圈、局部宏、Flash、Line、真实 Arc、Region 和轴向矩形扫掠，
  保留对象 ID、曝光顺序、来源格式和元数据；第三方 AST 限于 gerber-io。
- S1 parser 实施 ADR 0005 的 FS、增量、单位、IO、ICAS、限定旧命令和 AM 1/4/21 子集。
  具体正反例、遗留缺陷与 CORE10 结果必须查看本轮报告，不能从类型定义推断支持成功。
- Writer 以已校验快照生成毫米绝对 FS6.6，保留圆弧和局部孔洞；内存往返核对后写临时文件，
  再重读解析核对，最后以无覆盖发布操作写入显式新路径。不能覆盖源文件。
- 经宿主文件目录授权后提供 document.open/get、layers.list、objects.query/get、
  document.validate、gerber.export_layer；默认构造器仍只公开 S0 操作。
  查询的精确物理矩形过滤目前限圆 Flash、圆线段和轴向矩形扫掠，其余明确拒绝。
- 独立预算：输入 8 MiB、命令 2,000,000、对象 500,000、AM 展开 1,000,000、
  Region 边 2,000,000、Writer 和临时验证副本各 32 MiB；这些是实现上限，不是性能实测。
- Move/Undo/Redo、完整 GUI 文件流程、后台任务/取消、文字、批次编辑、覆盖保存、
  脚本运行时和网络服务未实现。本轮不增加依赖或更改锁定版本。

原 96 个验收身份、185 个逐平台槽、CORE10 编辑往返门槛均未变更。
公开小样与本机无窗口测试不能替代 Windows 原生、GUI/IME/GPU 或完整 V1 验收。

## S1-A.1 圆弧与旧文件门禁（2026-09-15）

见 [ADR 0007](adr/0007-s1-a1-arc-and-legacy-policy.md) 与 [本轮审查](S1_A1_REVIEW.md)。

- G75 stroke 接受合法非零 deviation，保留端点、声明圆心、方向、源分辨率/象限；拒绝 nonsensical center。
- G74 按方向/≤90°/最小 deviation 选择中心；0° 为圆形 dot，Writer 输出等价 G01 零长度 stroke。
- stroke 覆盖采用明确的平均半径圆弧与径向连接，含真实线宽和端帽；Writer 保留输入环带，不从该辅助曲线写回。
- Region 仅接受可证实在输入环带内、角度单调的圆形解释；非零 deviation 还检查完整不确定环扇区与其他边的拓扑。
  没有可验证圆形解释、扇区冲突或超过比较预算时拒绝，不能笼统宣称完整 Region/G74/G75 支持。
- CORE-03 metadata、CORE-07 FS 超宽继续 Strict；MetadataWarning 或 legacy recovery 尚未实现。
- 实际服务 capabilities 与这些边界由 s1a1_arcs 专项同步断言。
- 无新增产品依赖、编辑操作、GUI 文件流程、运行时或平台。Windows 和完整 V1 门槛保持原状。

公开专项通过不能替代独立工具一致性；大 deviation 和原始 G74 零弧的本地 gerbv 差异单独保留。
CORE10 当前状态及未完成项以本轮脱敏证据为准，不授予生产输出或 S1-B 编辑验收通过。

## S1-B1 基础编辑（2026-09-15）

当前实现切片为 S1-B1；前文 S1-A/A.1 段落保留历史状态。依据 ADR 0008 和 S1_B1_REVIEW.md。
宿主授权服务实现 objects.move、history.undo/history.redo、document.close。
支持 Flash/Line/RectangularSweep/Arc/Region 整体平移；版本单调递增，历史恢复原几何，
失败原子拒绝，保存后基于内容哈希判脏。新路径导出使用原有 Writer 和重新解析核验。
当前限制为 10000 对象/命令、100 事务、64 MiB 保守计费，查询实时读制造模型。
GUI 长期持有服务，画面仍为 S0 演示；没有编辑控件/拖动/完整文件打开流程。
Windows deferred / not executed；CORE-03/07/08、独立参考差异继续保留，不能宣称完整 AT/CORE10/V1/生产通过。

## S1-B1.1 / S1-B2a（2026-09-15）

当前新增同层 objects.duplicate / objects.delete 与原子插入／删除 Undo/Redo；详细顺序、ID、预算和保存身份
见 ADR 0009 与 AUTOMATION_API.md。Move 精确零／舍入不变时拒绝，NOT_FOUND 带 entity/id。
对象 provenance 改为 Imported/Generated，与当前曝光顺序分离；成功导出报告 last_saved_path。
旋转／镜像仅有 ADR 0010，仍为 unsupported；矩形扫掠缺少方向表示时不假称任意旋转。
无新增依赖或兼容范围；GUI 编辑、Windows、CORE10 完整流程及双平台 V1 仍未完成。

## S1-B2b 变换能力

当前无窗口服务支持 objects.rotate/objects.mirror，严格显式 ID 与 revision，一个请求一个 Modify 历史。
Flash/Circular Line/Arc/Region 支持数值可靠的有限角旋转；Flash 组合局部方向，Arc 反射翻转方向。
RectangularSweep 仅精确整数90°旋转（奇数交换宽高），所有对象镜像仅水平/垂直世界轴。
非支持角度的混合集合整批 UNSUPPORTED_FEATURE，不宣传所有对象任意角支持。
GUI/文字/edit.batch/完整 CORE10/Windows 未完成，production export 仍不授权。

## S2-B1 Mac GUI 局部能力

已选对象支持 4 physical px 阈值的直接拖动预览，释放通过服务提交一个 Move；Esc/失焦/PointerGone 取消。
Cmd+D 原位复制并选中新对象；Delete/Backspace 删除，菜单和按钮共用同一服务，文本焦点不触发制造快捷键。
单选、原位复制、同层曝光顺序边界不变；不含多选/框选/Grid/Snap/测距/Rotate-Mirror GUI。
Mac 原生及证据限制见 S2_B1_REVIEW.md；不声明 Windows、完整 V1 或任何 Post-V1 格式支持。

## S2-B2 精确框选与多选

objects.select_rect 支持当前语义子集的精确 Window/Crossing，错误/超预算整次拒绝。
GUI Ctrl-click 加选/Shift-click 减选、双向框选、整组同层拖动/原位复制/删除使用确定顺序的 SelectionSet 和服务原子事务。
多对象选择轮廓与预览使用显示选中标志缓冲，不改变制造数据或曝光顺序。
斜 RectangularSweep 仅算法测试，导入/编辑范围不变；无生产性能声明。
GeometryMetrics 仅 ADR 0018 设计；Rotate/Mirror GUI、Grid/Snap/测距/文字/Windows仍不在本轮。
实际阶段通过状态以 S2_B2_REVIEW.md 和对应运行ID证据为准，不扩大完整AT/V1能力声明。

S2-B3 GeometryMetrics implemented：objects.metrics 独立只读查询单对象解析面积/周长，标准C/R/O/P孔洞、Line、
RectangularSweep、安全Arc子集与可证明Region；Macro/不确定union返回unsupported，不以0冒充。
多选属性只显示对象指标合计（部分支持时明确已精确项），无最终Layer Boolean Area能力。
Windows deferred / not executed；具体门禁与原生证据以本阶段报告为准。

## S2-B3.1 active
生产 renderer 使用有界有序 world-space bins，选择边缘也查候选，reference renderer 保留。
S2-B3 full stage blocked before renderer fix；本次是否关闭由原生1000 selection/drag证据决定。
不新增 Grid/Snap、文字、Final Layer Area、多格式；Windows deferred / not executed。

## 2026-09-20 S2-B3.2

当前范围：视口候选去重/曝光排序、局部预算、事件时 selection flags、原生窗口 benchmark。详见 S2_B3_2_PLAN.md 与 ADR0021；冻结 50ms/300ms 门槛不变。P100K preview 成本采集保留预算拒绝，不能声明 P100K 性能通过。最终结果在独立 public evidence 中绑定受测 clean commit；不启动后续功能。

## 2026-09-20 S2-C1 / S2-C2

Mac GUI 已有 app-only Grid/Grid Snap/光标毫米坐标/两点测距；测距可保留多条，
在线中显示毫米距离与相对世界 +X 轴的逆时针角度，Esc 清除全部。Grid/Measure
都不进入制造模型、Undo 或 writer。
当前 S2-C2 将无窗口已验证的 objects.rotate/objects.mirror 接入同一 GUI ApplicationService 路径：
任意有限角、±90°、选择集制造 bounds 中心/世界原点/自定义 Pivot，及选择集中心的明确水平/垂直世界轴。
多选一次事务/Undo；RectangularSweep 非整数90°、锁定/跨层/非法值整批拒绝。
阶段通过状态以 S2_C2_REVIEW.md 的实际 gates/native 证据为准；Windows、完整 AT/V1、文字与 Post-V1 格式未完成。

## 2026-09-20 S3-FINAL

Mac-first S3 基础编辑正式范围由 `S3_FINAL_COVERAGE.md` 逐项签署。新增能力为：标准 C/R/O/P
Flash 尺寸 COW、`edit.batch` 的 Move/Rotate/Mirror/SetProperties 原子子集、历史完整事务淘汰
与 truncation 计数、对象端点/中心优先 Snap、Alt 临时关闭、mm/in 纯显示切换。
`objects.set_properties` 与 batch shape edit 会正确失效 GeometryMetrics shape cache。

没有公布 batch Duplicate/Delete、异步 jobs、文字、脚本运行时、覆盖保存、Final Layer Boolean Area、
P100K 或非 Gerber 格式。Mac 原生结果绑定 clean commit；Windows、双平台 V1、CORE10 10/10 和
96 个 AT 全量通过均未声明。

## 2026-09-20 S4-A1 and S4-A2 foundation

S4-A1 headless vector text core is implemented. S4-A2 foundation adds configurable
manufacturing precision and read-only text.preview sharing create validation.
Text GUI/IME/Preview worker, material offset and floating placement passed the bounded S4-A2.1/A2.2 Mac-first gates.
The retained high-precision thin-slab limitation is recorded in S4_A2_REVIEW.md.
S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）。Full S4/V1 and Windows are not claimed.


### S4-A2.1 — 参数弹窗、文字轮廓和浮动放置

当前实现与验收边界见 docs/S4_A2_1_PLAN.md、docs/S4_A2_1_REVIEW.md、
ADR 0027 / 0028（docs 内路径去掉 docs/ 前缀）。
参数型功能使用独占 Modal；连续画布操作保持直接交互。
Vertical slab text geometry = retired；contour/Line/Arc Region = production path，
每个材料连通组件一个对象，字洞使用局部 retraced cut-in，writer 不经过 slab。
Mouse 文字先生成再浮动，仅平移预览，左键提交一个事务；取消不改制造内容。
GeometryMetrics 周长排除 cut-in 接缝，但对象合计不是图层最终布尔周长。

S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded，见 GLOBAL_UNITS_PRECISION_REVIEW）。
S4-B1 Multi-Gerber Workspace 已实现并通过云端自动测试，原生 Mac 验收待执行，**尚未标记 PASS**（见 S4_B1_REVIEW）；DXF/SVG/PLT、Final Layer Boolean Area、Windows、`.rcam`（S4-B2）仍未启动。
阶段实现不等于全部原生验收；实际状态以 S4_A2_1_REVIEW 为准。

### S4-A2.2 — 多行文本和菜单交互

后续范围见 [S4_A2_2_PLAN](S4_A2_2_PLAN.md)。字型下拉默认使用 RCam 原创
ASCII 线条字体；中文选择本地系统轮廓字体。文本支持多行，Enter 换行，
鼠标模式确定后隐藏弹窗，Esc 恢复原草稿，画布单击提交一次 Undo 事务。
“插入 → 文本…”和“编辑 → 删除”提供菜单入口；编辑、工具、图层、视图补齐现有功能入口。
默认线条字体输出真实有限宽度 Line，轮廓字体继续使用 S4-A2.1 Region / Line / Arc 路径。
基线距离 0 为自动 1.3 × 字高；128 字符上限保留。
实际测试和未执行项见 [S4_A2_2_REVIEW](S4_A2_2_REVIEW.md)，Windows 未执行。


### Global Units / Manufacturing Precision (current)

See [plan](GLOBAL_UNITS_PRECISION_PLAN.md) and the corresponding review for current evidence.
S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded）。
Display uses camera-relative local f32, safe zoom clamp and last-good-frame.
Windows deferred / not executed；不宣称完整 V1、P100K 或完整 CORE10 release。

### S4-B1 Multi-Gerber Workspace（阶段历史状态；最终 PASS，Mac-first）

| 能力 | 边界 |
|---|---|
| 多 Gerber Workspace | 每个导入文件是独立 Layer（`layer-{n}`，来源 `src-{n}`，计数器不复用）；对象/光圈带命名空间；批量导入原子 |
| Gerber 文件生命周期 | 只 Import / Export；无 source link / mtime reload / 写回原文件；当时 `.rcam` Save/Save As 尚未启用 |
| 单层导出 | `gerber.export_layer`；沿用安全写出流水线与 `overwrite` 策略；不清 dirty；使用文档级精度 |
| New Empty Layer / 删除 | 空层直删 + Undo；非空需 `allow_non_empty`（GUI 强确认）；dirty/generated 更强确认；允许删除最后一层；Undo 恢复同 LayerId/z-order/样式 |
| Layer View State | 颜色 + 自动配色、Visible/Selectable/Locked、Active、Solo、Z 序、Filled/Outline/ZeroWidth、DisplayClass 分类样式；不改变 writer bytes |
| 有效状态 | `effective_visible/selectable/locked` = 层与类别的组合；锁定由服务强制 |
| 渲染/命中 | layer-aware RenderSnapshot / RenderIndex；隐藏层和类别不进入渲染与命中候选 |
| 占位类型 | LayerKind::Drill、Board/CoordinateTransform2D/ComponentPlacement、BlockDefinition/Instance、Snap 类型、Command/Keymap：**仅类型与测试，无产品入口，不构成已支持能力** |
| 未做 | 当时为 `.rcam`、Block core、Object Snap 全功能、Drill 导入、PnP/RefDes、DXF/SVG/PLT、图层合并/跨层 Boolean、Windows、P100K |

**S4-B1 Multi-Gerber Workspace = PASS（Mac-first）**：Mac 上 fmt/check/clippy/test/release 与 Metal parity（180+288 例）、
固定 ZIP 交付与 fresh extract（318/318）、§107 逐项原生检查（14/14）均已完成，见 S4_B1_REVIEW。

## 2026-09-23 S4-B2 Block Core + `.rcam` schema v1（PASS，Mac-first）

| 能力 | 边界 |
|---|---|
| Reusable Block | `BlockDefinition`（`SemanticDocument.block_definitions`，项目级）+ `BlockInstance`（`SemanticGeometry::BlockInstance`，层内，只 translation/rotation/mirror）；无 nested（`BlockObjectGeometry` 类型层面不可表示实例） |
| Block Service API | `blocks.list_definitions`、`blocks.get_definition`（只读）；`blocks.create_definition_from_objects`、`blocks.create_instance`、`blocks.update_instance_transform`、`blocks.rename_definition`、`blocks.explode_instance`、`blocks.delete_definition`（被引用时 `BLOCK_DEFINITION_REFERENCED`）；Move/Rotate/Mirror/Duplicate 复用既有 `objects.*` |
| Definition 共享 | 修改 Definition（含 revision）对全部引用它的 Instance 生效；Instance 编辑（transform）只影响自己；bounds/hit-test/metrics 均按需 resolve，不物理复制几何 |
| Display | definition display cache + instance rigid transform；Filled/Outline/ZeroWidth、Layer/Category color、Visible、Selected；整实例 identity/highlight；Metal 对完整参考 renderer 零 RGBA 差异 |
| Export | Gerber 单层导出展平 `BlockInstance`（RCam Block ≠ Gerber `%AB`）；RectangularSweep 在非 90° 合成旋转下 fail-closed；round-trip 校验按展平后的图形比较 |
| `.rcam` schema v1 | 新 crate `rcam-project`（无 GUI/GPU 依赖）；ZIP 容器（manifest + project + layers/\* + blocks/\*），store-only 确定性编码；entry/bytes/string length/JSON depth/path/hash/version/mandatory type 全链路 fail-closed |
| `.rcam` 不持久化 | Solo、Selection、Undo 历史、AppPreferences（快捷键/面板宽度/最近颜色/主题）——schema 本身没有对应字段，不是运行时过滤 |
| 未做 | `File → Open/Save .rcam`（S4-B3）；完整 GUI Block Editor；任意角 scale/shear；Drill 持久化；PnP/RefDes；Windows；P100K |

S4-B2 结项时，`system.capabilities.stage` 为 "S4-B2 Block Core + .rcam schema v1 (Mac-first bounded)"；当时 `blocks.*` 8 个操作已移入
`supported_operations`（对应测试全部通过后才移入，见 `block_core_workflow.rs` 的
`capabilities_advertise_every_block_op_as_dispatchable`）；`project.open (.rcam)`/`project.save (.rcam)` 继续留在
`unsupported_operations`。同一 clean commit 的格式/检查/clippy/workspace tests、Block/codec gates、release build、
Metal parity、原生 GUI Block fixture、400×100 性能、固定 ZIP/sidecar/fresh extract 全部通过，因此
**S4-B2 = PASS（Mac-first）**。Windows deferred；不声称完整 V1、P100K 或完整 CORE10 release。见
[S4_B2_REVIEW](S4_B2_REVIEW.md)。

## S4-B3 `.rcam` Project Lifecycle（PASS，Mac-first）

`ApplicationService` 新增受 `FileAccessPolicy` 限定的 `project.new/open/info/save/save_as`，并将对应 JSON 操作列为 supported。`.rcam` Open 先完整解码再装载候选工程；Save 使用临时文件、完整复核与原子发布，成功后才更新 project path/hash/dirty。GUI File 菜单正式区分 `.rcam` New/Open/Save/Save As 与 Gerber Import/Export，新增关闭确认、Recent 和本机 Recovery；Solo/Selection/Undo 与 AppPreferences 不进入 `.rcam`。Mac 原生和交付门禁见 [S4_B3_REVIEW](S4_B3_REVIEW.md)；Windows、完整 V1 仍未执行。
S4-B3 预冻结修正：mm/inch/mil/µm 四种 DisplayUnit 均一一持久化；历史三单位 v1 文件仍可打开。Recovery 写入失败不会把当前脏版本永久标记为已恢复。

## S4-C1 Full Object Snap（PASS，Mac-first bounded）

| 能力 | 当前边界 |
|---|---|
| 几何与类型 | f64 解析 Manufacturing Boundary：Rectangle/Circle/Polygon/Obround/Line/Arc/Region/AM/CompatibilitySolid/BlockInstance；Endpoint/Vertex、Midpoint、Center、Quadrant、Intersection、Nearest（默认关闭） |
| 查询与性能 | 11 physical px candidate query → WorldIndex 邻域 → lazy features/nearby edge pairs；8 px acquire / 11 px release；无全局 snap-point 库；BlockDefinition 局部 cache 按 revision 失效 |
| 过滤/解析 | effective visible + selectable；locked selectable 允许；Object 优先于 Grid；距离为主、类型为辅；8 px acquire / 11 px release hysteresis |
| GUI/工具 | 固定物理像素 marker、统一单位 status、F3 开关、Alt 临时关闭；Direct Drag、Text、Measure、Pick Base Point 共用 resolver |
| Project | `.rcam v1` 保存 enabled/kinds/radius/boundary/path；旧文件缺新字段安全默认；candidate/hysteresis 不保存 |
| 明确未做 | 标准 aperture hole boundary snap、Tangent/Perpendicular 完整 UX、Grip Editing、Block Editor、Alignment、Drill、PnP/RefDes、Windows、P100K、S4-C2 |

最终 PASS 与证据路径只看 [S4_C1_REVIEW](S4_C1_REVIEW.md)。

## 2026-09-24 密集真实 Gerber 导入修复

GUI 派生显示上限现为 2000000 primitive + point，RenderIndex 最多 1000000 对象，原 1000000 引用／16384 单格候选／采样工作预算仍生效。指定 230409 矩形 Flash 的 `.GPT` 在 Mac 原生窗口可导入和显示；`0727SMT` 批量兼容结果见 [验收记录](GERBER_COMPAT_20260924_REVIEW.md)。百万对象预算只表示资源界限，不证明完整 CORE10、P100K、Windows 或 V1。


### INFRA1 Runtime Diagnostics（PASS，Mac-first bounded）

本地日志/诊断基础设施和 panic hook closeout 已完成 Mac-first bounded 验收。固定评审为 [`c65bee0`](../exports/INFRA1_PANIC_c65bee0/REVIEW.md)；Windows deferred / not executed。

历史复审曾记录 `85ad528` 为 ALMOST PASS；最终修复保留并调用 Rust previous/default hook，状态以 `c65bee0` fixed review 为准。

### S4-C2 Grip Editing v1（PASS，Mac-first bounded）

新增 `objects.grips` / `objects.grip_edit`。单选可编辑 C/R/O/P Flash 尺寸（COW）、
Line/合法轴向 RectangularSweep 端点、Arc 投影端点/半径、all-Solid 全 line-only Region 顶点。
Flash 局部 rotation/mirror/scale 保持；拖动预览不修改文档，release 一个事务；取消不提交。
Macro、含 Hole contour 或弧边 Region、CompatibilitySolid、GeneratedText、Block 内部、多对象 node edit 不支持；Hole contour deferred，包括全 Line 的 Solid outer + Hole inner；Arc-edged Region node Grip、Macro Grip、多对象 Grip、专用 Grip 数值弹窗及 Windows deferred。PASS 的自动化、原生与交付证据见 [`S4-C2 final review`](../exports/S4C2_f1293f2/REVIEW.md)。


### S4-C3 Block Editor v1（PASS，Mac-first bounded；2026-09-27）

C3 收口时的 `system.capabilities.stage` 为 `S4-C3 Block Editor v1 (Mac-first bounded)`。`objects.grips`、`objects.grip_edit` 与 `blocks.list_definitions`、`blocks.get_definition`、`blocks.create_definition_from_objects`、`blocks.create_instance`、`blocks.update_instance_transform`、`blocks.rename_definition`、`blocks.explode_instance`、`blocks.delete_definition` 均属于 supported operations；Block Library、Create/Base Point、Place、实例 Transform、Rename/Delete、Select Instances、Explode、Project persistence、Gerber flatten export 与 Diagnostics 均在本阶段验收范围内，制造修改仍经既有 ApplicationService。`drill.import`、`components.search`、headless `snap.resolve` 与 `layers.merge` 保持 unsupported；UI Object Snap 不代表 headless snap query 已实现。内部 Definition 编辑、Array、Alignment、PnP/RefDes 与 Windows deferred。最终验收及新交付身份见 [S4-C3 review](S4_C3_REVIEW.md)。不代表双平台 V1、CORE10 10/10 或 P100K 通过；C3 收口时 C4 为下一阶段；本次 C4 状态见下节。

### S4-C4 closeout (2026-09-27)

S4-C4 Alignment / Distribution = **PASS（Mac-first bounded）**。六种制造 world-AABB 对齐与双轴等边缘间距分布已实现，使用最后选中对象作固定 Anchor；同层原子事务、Undo/Redo、BlockInstance、完整文字组、项目/恢复/导出、诊断和资源限制已验收。语义见 ADR 0040，范围及证据见 S4_C4_PLAN、S4_C4_ACCEPTANCE_ADDENDUM、S4_C4_REVIEW；最终干净提交、原生二进制及包校验身份以 versioned exports/S4C4_*/REVIEW.md 为准。Windows deferred/not executed；不代表完整 V1、CORE10 10/10 或 P100K 通过。C4 后停止，Array/Panelization 未启动。


### S4-C5 Array / Panelization v1 (PASS, Mac-first bounded; 2026-09-28)

The user started S4-C5 on 2026-09-27. Scope is non-associative Rectangular Array only, via `objects.array_rectangular`: same-layer contiguous ordered source, complete text groups, signed f64 mm pitch, row-major cells and shared Block/Aperture definitions. Original cell is unchanged; copies follow its span in one Undo transaction. UI preview is transient and bounded. See S4_C5_PLAN, S4_C5_ACCEPTANCE_ADDENDUM and ADR 0041. Earlier “Array not started” statements describe C4 closeout. S4-C5 is PASS (Mac-first bounded); final clean source, native binary and four-file package identity are recorded in the versioned exports review and S4_C5_REVIEW. Windows deferred/not executed; no full V1/CORE10/P100K claim. Stop after C5, without PnP/RefDes or associative arrays.


### S4-D1 PCB / PnP / RefDes Foundation (PASS, Mac-first bounded; 2026-10-01)

S4-D1 is PASS (Mac-first bounded). Candidate implementation acceptance, native and complete four-file archive audits passed; final clean source/binary/package identities are recorded in versioned exports/S4D1_*/REVIEW.md. Scope: explicit bounded CSV/TSV mapping/units/Side/rotation preview, independent ComponentPlacement table, rigid Board→World registration, RefDes exact/prefix/substring search and virtualized focus/overlay, atomic project Undo/Redo and v2 Board persistence/Recovery. No component↔opening association, library replacement, Drill, assembly export or Windows signature. Historical C5 stop points are superseded only by this explicitly started stage. See S4_D1_PLAN, S4_D1_ACCEPTANCE_ADDENDUM, S4_D1_REVIEW and ADR 0042. Windows/full V1/CORE10/P100K remain deferred; stop before S4-D2.
