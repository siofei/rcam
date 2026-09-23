# 文档变更记录

## 2026-09-24 · 密集真实 Gerber 导入修复

GUI 显示容量及 RenderIndex 的整格扩展规则阻断 230409 个矩形 Flash 的真实 `.GPT`。提高有界显示预算，并把索引边界扩展收窄至 f32 舍入误差；导入失败时保留原始错误。使用 `0727SMT` 真实 Gerber 进行解析筛查与多格式 GUI Model 导入回归，实际结果见 `GERBER_IMPORT_DENSE_REAL_REVIEW.md`。不改变正式 V1、Windows 和 CORE10 验收门槛。

## 2026-09-24 · S4-B3 DisplayUnit / Recovery closeout

修复 `.rcam` 保存时 µm 显示单位被降级为 mm：v1 增加 `micrometers`，GUI/Project 四单位一一映射，
保留旧三单位文件兼容与默认 mm；制造几何、precision 和 Gerber 输出不变。Recovery 写失败后不再永久抑制同一脏版本，
在原有最小间隔后可重试。证据见 S4_B3_REVIEW；不进入 S4-C。

## 2026-09-24 · S4-B3 `.rcam` Project Lifecycle（PASS，Mac-first）

新增 ADR 0033/0034 和 S4_B3_PLAN/REVIEW；ApplicationService 正式接入 `.rcam` 项目会话、分阶段打开、原子保存与 JSON project 操作；GUI 增加 File New/Open/Save/Save As、项目 dirty 关闭确认、Recent 本机偏好与 Recovery。Gerber 继续只 Import/Export。Mac 原生 GUI、Recovery、Metal、固定包门禁通过，证据见 S4_B3_REVIEW；Windows deferred。

## 2026-09-23 · S4-B2 Final Closeout + complex text renderer

修复 BlockDefinition 在当前 ManufacturingPrecision 下的导出语义，完成 BlockInstance display/cache、整实例选择、
`.rcam` string/depth 显式预算、UI Component Foundation、Mac Metal/GUI/400×100 性能与固定交付门禁。
复杂文字 Region 改用精确水平边分箱，修复 STSongti-SC-Light、3 mm、补偿 0、`sdf 点` 可见时的缩放/桌面卡顿；
生产 shader 与完整轮廓参考保持零 RGBA 差异。S4-B2 = PASS（Mac-first）；Windows deferred；不启动 S4-B3。

## 2026-09-22 · S4-B1 Final Closeout

Delete 对话框只讨论 RCam 工程风险；Layer 行回到 compact 设计（Active 指示、显示模式小菜单、无 inline 展开）；
新增 session-only Recent Colors；Canvas 错误文案不再建议“缩小视图”；新增 view-style Metal parity 矩阵、
10×1K release 性能采集、原生 GUI 探针与验证脚本、`run_s4b1_final_gates.py`。修复发丝线包围盒随缩放被剔除的显示缺陷
（`LOD_MAX_ZOOM_OUT`）。是否 PASS 只看同一 clean commit 生成的 evidence 包。

## 2026-09-21 · S4-B1 Multi-Gerber Workspace + 长期架构指导合并

新增 ADR 0029（Workspace / Gerber Import-Export / View State）、ADR 0030（Block/Snap/Command/Board 长期预留）、
S4_B1_PLAN / S4_B1_REVIEW。DESIGN_V1 新增第 22 章“长期架构方向（钢网设计）”并修订 12.3/12.4 的 Save 语义；
AGENTS.md 合并 Forward Architecture Reservations、Layer UI 与阶段顺序；IMPLEMENTATION_PLAN 冻结 S4-B1→B2→B3→C 路线；
AUTOMATION_API 增加多层 Workspace 扩展；Global Units 收口为 PASS（Mac-first bounded）。
S4-B1 原生 Mac 验收未执行，未宣称 PASS；`.rcam` 未冻结；不自动开始 S4-B2。

## 2026-09-20 · Global Units / Manufacturing Precision

四单位显示与后缀解析统一，默认制造分辨率 0.1 µm，与 FS 编码、显示位数、网格和曲线预算分开。
服务增加文档精度及独立 export_policy_dirty；导出私有快照量化后仍通过原语义与重开验证。
文字输入在当前会话中保留上次内容、字体、布局和定位参数；取消仅清除预览及提交状态。
范围及实际证据见 GLOBAL_UNITS_PRECISION_PLAN / REVIEW。未启动 S4-B1；Windows deferred。

## 2026-09-20 · S4-A2.2

按用户 CircuitCAM 交互参考添加默认原创 ASCII 线条字型、多行布局与基线距离，
确定后鼠标放置、Esc 返回保留草稿。Enter 在文本框内仅换行；异步只读预览期间
保持文本可编辑。插入菜单提供文本入口，删除归入编辑菜单，补齐现有功能菜单。
保留轮廓精度、128 字符限制和单次原子 Undo；不新增依赖，不分发本地字体。
范围与实测证据见 S4_A2_2_PLAN / S4_A2_2_REVIEW。


## 2026-09-20 · S4-A2.1

ADR 0027/0028 冻结参数 Modal、文字 contour/Line/Arc、局部 cut-in、浮动放置。
停止 slab 生产路径，保持原制造总误差与 writer 格式；Global Units 等待本阶段 PASS。
原生和自动证据分别记录，不变更双平台 V1 门槛。

## 2026-09-20 · S4-A2 Completion 与 Canvas UX 增补（实施中）

在 246f57e foundation 上继续文字 GUI、字体身份、typed 异步预览、三种定位、
真实轮廓补偿和组事务；保留冻结薄片回归并修复其生成路径。新增锁定的
clipper2-rust 1.1.0 与许可记录。Canvas 改为 f64 视口筛选、局部显示原点、
连续网格透明度与最后有效帧保护，渲染预算上限不变。原生与自动证据分开记录，
未完成项不得签署 PASS。全局单位/0.1 µm 基础层列为下一正式功能阶段前阻断项。

## 2026-09-20 · Mac-first S2-B2

新增 exact objects.select_rect Window/Crossing、确定顺序 SelectionSet、框选与整组原子编辑。
按用户最新修订：Ctrl-click 只加选，Shift-click 只减选，普通 click 单选；覆盖初始任务单的 Shift 切换规则。
ADR 0018 仅冻结下一阶段 GeometryMetrics 缓存设计，ADR 0019 记录本轮边界。
原生主要交互及自动门禁已验证；修饰键鼠标实测和完整原生日志仍有缺口，不签署阶段全部通过。
Windows deferred / not executed，96 用例及双平台完整 V1 门槛保持不变。

## 2026-09-16 · S2-A.2 精确 Hit Test

新增 objects.hit_test、严格 DTO/错误/资源契约、f64 材料距离和有序 Macro 边界查询，
新增独立几何及真实 JSON 编辑/历史回归。ADR 0014 明确失败边界与阶段退出范围。
未修改 parser/writer/制造历史，未实现 GUI 或 Windows；原完整验收门槛保持。

## S2-A.1：制造边界查询

新增 ADR 0013，按 Mac-first 小闭环实现 document.bounds/layer.bounds，供后续 GUI Fit 使用。
新增全部当前几何的解析外框和真实服务往返检查；无新依赖，不改制造模型、parser 或 writer。
S2-A GUI、点选及 Windows 仍未完成，原 96 用例身份及门槛不变。

## 2026-09-15 · S1-B2b

按下一阶段交接实现 Rotate/Mirror，复用原子 Modify 事务；修复两边 Region 的共享端点
以及 cut-in 方向相关校验。ADR 0010 冻结角度规范/精度与矩形扫掠限制，ADR 0011 记录 GUI 前
状态/文档边界和结构历史债务。新增真实接口几何测试及独立公共证据，96 个有效 cases、
退役身份、容差和逐平台门槛保持不变。完整 V1、Windows、GUI/IME/Metal 编辑仍未通过。

## 2026-09-15 · S1-A 实施记录（设计基线仍为 1.1）

根据 `RCam_S1A_NEXT_TASK.md` 实施 ADR 0005 已冻结的语义模型、parser、
规范化 Writer 和新路径服务导出。新增独立公开几何/拒绝真值、真实无窗口接口回归、
分预算检查与本地独立工具对照；更新 API、能力与源码清单。
实际命令、失败历史、CORE10 语义扫描及剩余风险见 `S1_A_REVIEW.md`。
未实现 Move/Undo/Redo 或完整 GUI 编辑，不降低原 96 个验收身份及 CORE10 编辑往返门槛。
Windows 原生证据缺失时，S0-C platform gate 保持 blocked。

## 1.1 · 2026-09-14

本次按用户新增要求修订第一版设计与验收基线，保留原 1.0 文档包，不覆盖历史交付。软件产品仍为 V1，文档基线升级为 1.1；接口 `api_version=1`，机器可读验收规范 `schema_version=2`，三者用途不同。

### 取消 Linux 支持

只保留 Windows x64 与 macOS Apple Silicon。同步移除 Linux 核心兼容、构建、CI、GUI／GPU、打包与验收义务。WSL2 不作为运行或规定开发环境；个人辅助工具不扩展产品范围。仍要求 Windows／macOS 两平台的 GUI、IME、GPU、文件安全及发行证据。

修改涉及设计范围、GPU选型表、开发环境、支持矩阵、S5阶段、CI、Codex规则、验收准备、AT-001及报告模板。旧 AT-079「Linux 构建与能力声明」退役，编号保留且不复用，不计通过或执行分母。AT-085检查新平台范围，而不是重命名旧测试冒充已执行。

### 为后续脚本自动化预留接口

新增 R21「无界面统一业务接口」和 R22「脚本扩展契约与安全边界」，需求总数由20项变为22项。新增 `editor-service`／`ApplicationService`，让 GUI 与未来脚本共用业务命令和读查询。

V1必须实现并验收：无窗口／无GPU依赖、公共DTO与JSON编解码、显式ID／单位、修订冲突、单写者提交、批次失败整体回滚、Undo递增revision、结构化错误、能力发现、非交互保存／权限策略、任务查询／取消，以及真实无界面编辑导出与GUI一致性。

V1不要求：Python／Lua／JavaScript运行时、脚本控制台、脚本录制回放、正式CLI、HTTP／JSON-RPC服务器、远程控制或插件系统。未来只新增薄适配器，不重写几何、Undo和writer。

### 验收用例变化

| 项目 | 1.0 | 1.1 |
|---|---:|---:|
| 需求数 | 20 | 22 |
| 有效用例数 | 84 | 96 |
| 退役记录数 | 0 | 1 |
| 新增用例 | — | AT-085—AT-097，共13项 |
| 退役用例 | — | AT-079 |
| 有效用例初始状态 | 全部未执行 | 全部未执行 |

96 = 84 − 1 + 13。没有用例被标记通过，也没有对尚未开发的软件执行验收。

### 文件与机器数据迁移

同步修订 `DESIGN_V1.md`、`ACCEPTANCE_V1.md`、`AGENTS.md`、`README.md`、`acceptance_cases.json`。新增 `AUTOMATION_API.md`、`automation_request_examples.json` 和本记录；重新生成完整性清单与ZIP。

验收JSON采用schema 2，明确 `supported_platforms`、`required_platforms`、`active_case_count`、`retired_cases`。结果仍独立写入 evidence；旧工具不认识schema 2时应停止并提示升级，不能忽略退役规则。`shared`只用于共用文档审计，不用于豁免本机测试。

### 修订边界

原有Gerber支持子集、几何精度、极性／孔洞、共享光圈、中英文文字、安全保存、性能阈值和CORE10业务门槛不降低。阶段S0—S6中增加接口解耦与测试，不新增脚本产品阶段。

此次为用户指定的平台范围与架构修订；既有第三方资料索引沿用1.0。接口名和JSON为本项目设计契约，不宣称来自第三方现成API；实际依赖版本、许可和API仍在S0核对。本包仅含文档与数据示例，不包含可运行编辑器、字体、脚本引擎或测试执行结果。

## 2026-09-15 · S0-B 实施记录（设计基线仍为 1.1）

修复重复 Flash、资源错误分类、短线 GPU 投影、极小孔与 f32 边界；统一 S0 JSON 响应，
将服务依赖检查升级为 CI 门禁，迁移历史独立测试并区分设计/源码/二进制清单。
CORE10 使用审计及最小扩展提案见 ADR 0004；尚不批准支持矩阵扩展，不变更原验收初始状态/阈值。

## 2026-09-15 · S0-C 范围补充（1.1-s0c-1）

根据用户提供的下一阶段指南，批准 ADR 0005 的有限目标扩展；新增审计发现的轴向矩形D01需求。
设计4.4、能力表、AT-019判定和关联完整追加场景同步更新；96个有效AT、AT-079退役、185个平台槽、
所有原阈值和CORE10身份不变。schema_version仍为2，scope_revision区分本次补充。
18个公开合成输入不代替真实业务样本。只读审计和服务边界测试不是生产语义通过。
Windows原生门禁仍阻塞，不宣称已进入S1或完成V1。

## 2026-09-15 S1-A.1

ADR 0007 澄清 arc deviation、G74 least-deviation/0°、旧 metadata/FS 严格策略；
保留 96 个有效用例、退役身份、平台门槛和私有样本身份。更正有规范依据的旧圆弧测试真值。

## 2026-09-15 · Mac-first S1-B1

按 RCam_MAC_FIRST_S1B_NEXT_TASK.md 和 ADR 0008 调整当前阶段排期为 macOS arm64，
保留 Windows 必测身份及最终双平台门槛。新增 Move、Undo/Redo、关闭会话与内容哈希保存基线；
新增独立 JSON 编辑往返测试。未扩展 Gerber 兼容范围、GUI 编辑或脚本运行时。

## 2026-09-15 · S1-B1.1 / S1-B2a

- 修复零／舍入不变 Move、NOT_FOUND 实体定位、导出保存目标身份及过期任务索引。
- 将来源与曝光顺序分离，增加同层 Duplicate/Delete、原子结构历史和稳定 ID 契约（ADR 0009）。
- 新增局部验收场景、真实 JSON 编辑往返测试、阶段原始日志脱敏打包。
- 冻结后续旋转／镜像表示能力（ADR 0010），本轮未实现；96 个用例、required_platforms、CORE10 门槛不变。

## 2026-09-15 S1-B2c

ADR 0012：移除 SemanticLayer UI字段；新增 layer.update 和独立 workspace_revision；服务统一锁定新编辑，
历史不受锁定阻断。追加局部验收，不改96有效用例、CORE10或双平台门槛。无新增依赖。


## 2026-09-19 — S2-A.3 Mac GUI 实现（局部验收，非 V1）

真实编辑器替换默认 S0 演示；增加只读 render.snapshot、自有语义显示与原生文件对话框、
精确单选/数值 Move/历史/新路径另存为。HitTest 资源错误报告实际尝试工作量。
锁定与选择策略、6 physical px 容差、显示预算及未完成项见 ADR 0016。
96 用例及 required_platforms 不变；未授予 Windows 或完整 V1 通过。

## 2026-09-20 S2-B3

新增独立制造Object Metrics解析查询、会话缓存、后台单选/多选指标面板及UTF-8确定性源码打包。
未知union明确拒绝，不改变parser/writer语义、96个用例身份或双平台V1门槛。

### 2026-09-20 S2-B3.1 active
- 主renderer使用有序world-space候选；保留reference、原AA/局部材料/跨层合成。
- 预览独立索引，LOD导航复用基础索引；冻结原1000圆样本，增加像素parity和指标/writer/history不变性检查。
- 不改96用例/阈值或双平台门槛；最终门禁结果单独证据交付。

## 2026-09-20 S2-B3.2

当前范围：视口候选去重/曝光排序、局部预算、事件时 selection flags、原生窗口 benchmark。详见 S2_B3_2_PLAN.md 与 ADR0021；冻结 50ms/300ms 门槛不变。P100K preview 成本采集保留预算拒绝，不能声明 P100K 性能通过。最终结果在独立 public evidence 中绑定受测 clean commit；不启动后续功能。

## 2026-09-20 S2-C1

新增 app-only Grid/Measure、core 纯 f64 网格 helper，单选/多选 Drag 使用共同吸附位移。
数值 Move 不量化；视觉网格按 zoom 抽稀且有界。沿用服务和 Renderer，未新增依赖。
决策见 ADR0022，范围/执行见 S2_C1_PLAN.md，结果见 S2_C1_REVIEW.md。

## 2026-09-20 S2-C2

按用户追加要求将两点测距扩展为可同时保留多条的 app-only 标注；线中显示距离与相对世界
+X 轴的逆时针角度，Esc 清除全部，不进入制造模型、Undo 或 writer。
复用既有 objects.rotate/objects.mirror，把任意角、明确 Pivot、选择集中心 Horizontal/Vertical 世界轴接入 Mac GUI。
选择集中心复用 f64 制造 bounds union；多选一次服务事务/Undo，Grid Snap 不参与 Transform。
无 preview、无新依赖，不改制造算法、writer、renderer、96 个用例身份或最终双平台门槛。见 ADR0023。

## 2026-09-20 S3-FINAL

恢复正式 S3 阶段编号并建立 Requirement/AT/evidence 对照。新增标准 Flash 尺寸写时复制，
服务 `objects.set_properties` 与 GUI 属性面板共用同一路径，Undo/Redo/writer round-trip 保留定义身份。
实现 `edit.batch` 的 Move/Rotate/Mirror/SetProperties 原子子集；成功一次 revision/Undo，后段失败与
外部 I/O step 预检失败均零修改。历史上限改为淘汰最旧完整事务并报告 truncation，超大单事务预拒绝。
Snap 补齐显式 endpoint/center、8 逻辑点半径、稳定优先级和 Alt 临时关闭；坐标/Grid/Measure 支持
mm/in 纯显示切换。扩展原生 harness 覆盖 Window/Crossing、P1K drag、Duplicate/Delete/Undo/Redo、
Rotate/Mirror、Flash COW、Grid Snap、Measure、Save As/Reopen；Windows 与 S4 保持未执行。

### S4-A1 Mac-first vector text core

Scope and local AT mapping: [S4_A1_PLAN.md](S4_A1_PLAN.md); geometry, font,
resource and API decisions: [ADR 0024](adr/0024-s4a1-vector-text.md).
AT-047–052 headless coverage does not replace GUI/IME, independent viewer
or Windows evidence. Final V1 and CORE10 thresholds remain unchanged.

### S4-A2 foundation — not full GUI acceptance

Add backward-compatible curve_tolerance_mm, read-only text.preview, shared
generated-insertion preflight and explicit precision/atomicity tests. Repair
S4-A1 dependency/README state. ADR 0025 records the unchanged total error
threshold and retained thin-slab rejection. No new dependency or acceptance
identity/platform changes. GUI/IME/outline offset remain open.
