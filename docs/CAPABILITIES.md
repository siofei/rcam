# 能力与实施状态

当前开发切片为 S1-A.1，尚不是 V1 编辑器。完整要求继续以 DESIGN_V1 第4节为准。
以下 S0/S0-B/S0-C 段落保留当时的实施状态；S1-A 当前状态见文末及 `S1_A_REVIEW.md`。

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
