# 实施计划与职责

基线：DESIGN_V1 1.1，acceptance_cases schema 2；2026-09-14 开始。
历史阶段采用主代理规划/验收、Luna 实现的分工；S2-B1 由主代理实现并核验；本次 S2-B2 由主代理实现/自动测试/审核，用户已取消全部 CUA 交给 Luna 的临时安排，恢复默认执行。每轮仅交付一个阶段内的小闭环。

## 历史任务 S0-A：技术风险验证

需求：R01/R03/R04/R05/R06/R08/R17/R18/R19/R20/R21/R22。
关联验收：AT-001–004、AT-006–011、AT-017–019、AT-085–086、AT-088 的前置检查；不是上述完整用例的通过声明。

允许 Luna 修改：工作区 Cargo 文件与工具链、.gitignore、crates/editor-core、gerber-io、editor-service、editor-app、fixtures/synthetic、阶段 manifest、README、THIRD_PARTY_NOTICES、docs/DEPENDENCIES、双平台 CI、自己的 evidence 运行目录。
主代理维护：本计划、BASELINE、CAPABILITIES、ADR、只读真实样本清单和独立审核证据。
不得改变设计/验收原文或阈值，不引入脚本运行时或生产导出。

交付闭环：真实 parser → 自有 f64 毫米小样模型 → ApplicationService 只读 DTO → eframe 管理的 wgpu Callback 画布；独立验证曝光顺序、局部孔洞、跨层隔离、缩放/裁剪。S0 仅暴露实际完成能力；不使用 EditableV1 或 ReadOnlyExact 名称暗示完整文件安全能力。

技术门禁：fmt/check/clippy/test/release；服务正常依赖无 GPU/窗口；API 严格拒绝未知字段/版本/操作；小样坐标与点覆盖有独立断言；启动与 GPU 证据按平台分别记录。缺失的 headless_workflow 必须写未实现，不能放空测试充数。
S0 整体退出还要求：REAL30/CORE10 预先冻结、规范差异审查、许可证归档、Windows/macOS 最小画布证据。缺口未清零不得宣布 S0 或 V1 完成。

## 后续阶段与门禁

| 阶段 | 小闭环与主要需求 | 验收范围 | 模块 |
|---|---|---|---|
| S1 | 标准语义与安全写回，R02–R06/R14–R16/R21–R22 | AT-005–020、053、055–059、086、088、092–094 | core/io/service，真实 headless_workflow |
| S2 | 图层、导航、精确选择，R07–R09/R17/R21 | AT-021–030、062、067–068、089 | app/render，core 查询/service DTO |
| S3 | 原子编辑与撤销、数值/网格/测距，R10–R11/R13/R22 | AT-031–045、090–091 | core/service/app |
| S4 | 矢量文字与完整保存，R12/R14–R16/R21–R22 | AT-046–061、063–065、087、092–096 | text/io/service/app |
| S5 | 固定样本性能与双平台实测，R16–R19/R21–R22 | AT-025、043、046、063–078、086–097 | render/app/验收工具 |
| S6 | 发行、CORE10、最终证据，R01–R22 | 全部96个有效用例按 required_platforms | 打包/文档/验收报告 |

表格为阶段分配，正式判定以每条原始用例为准，不改变其完整要求。失败先定位、由 Luna 修复、主代理重新验证；不替换失败样本。Windows 实机缺失不能用交叉编译替代。

## 样本处理

用户于 2026-09-14 授权只读使用 /Volumes/硬盘盒/0727SMT。仅在本地分析，不上传、不修改源文件。先盘点已展开文件，压缩档不自动解包到源目录。私有清单和路径保存在 fixtures/private 与 evidence（Git 忽略）。CORE10 选择须覆盖业务文件并在语义测试前固定，不能按最终成功率事后挑选。

## 历史任务 S0-C：范围收口与原生基线

依据 RCam_S0C_S1_Development_Guide.docx；关联需求/AT/允许修改范围见 ADR 0005。
本轮仅修改审计工具、测试、公开小样与配套文档；无产品代码实现任务，无需委派 Luna 产品实现。
交付：CORE10 逐命令/实际成像依赖审计、18份公开小样、有限目标支持与拒绝边界、
独立工具对照、服务拒绝边界回归、运行ID证据和退出清单。Windows实机缺失保持阻塞。
下一轮 S1-A 先完成目标子集语义及 writer；S1-B 实现 Open→Query→Move→Undo/Redo→Export→Reopen。
Move 是首个编辑动作，不提前同时铺开旋转/文字/完整GUI。阶段正式切换受 S0-C 退出门禁约束。

## 历史任务 S1-A：语义核心与安全 Writer

授权任务书：`RCam_S1A_NEXT_TASK.md`。允许在 Windows 证据等待期间开展核心开发；
S0-C platform gate 仍为 blocked，不能据本机结果标记双平台通过。
需求：R01–R06、R14–R16、R19–R22；相关局部验收：AT-001、003–020、053、055–059、080–081、085–089、092–095。
允许修改：editor-core、gerber-io、editor-service 及其测试，必要的依赖声明（须记录理由）、
公开新增测试样本、开发期核验脚本、能力/API/实施/评审文档和源码清单。
保留既有 GUI/S0 行为，不改原 18 个 S0-C 样本真值、96 个 AT 身份和阈值。

Luna 负责产品代码，按 core/io 和 service 划分文件边界；主代理负责独立测试、
只读 CORE10 核验、构建门禁与交付结论。新闭环为 Open → Query → Validate →
Export(new path) → Reopen，范围严格按 ADR 0005；Move/Undo/Redo 留待 S1-B。
证据另存 `evidence/s1-a-20260915-implementation/`，包含早期失败，不覆盖历史。

## 历史任务 S2-B1：单对象直接操作（Mac-first）

需求 R07/R08/R09/R10/R11/R14/R15/R18/R19/R21/R22；局部 AT-022/025/026/030/032/035/036/038/040/041/043/054/086/087/090/091。
允许修改 editor-app 状态、输入、显示 uniform/shader、测试，以及阶段文档和源码清单；保留 core/io/service 契约与制造算法。
已选对象按下时后台 exact hit_test 确认，4 physical px 后预览；预览仅 GPU translation，release 一次真实 Move。
Esc、PointerGone、失焦、文档/任务切换取消。Cmd+D 原位复制选中新 ID；Delete/Backspace 删除；文本焦点不抢快捷键。
Windows 延后，本轮不代表双平台 V1 通过。保留所有原96用例及门槛。

## 历史任务 S2-B2：精确框选与多选

任务 RCam_MAC_FIRST_S2B2_MULTISELECT_NEXT_TASK.md；需求/AT/允许修改模块见 ADR 0019。
专用 objects.select_rect 的 Window/Crossing、SelectionSet、Ctrl-click 加选/Shift-click 减选、空白按下框选、
同层整组 Move/Duplicate/Delete 与 Undo/Redo，后台一次精确查询和 UI-only 预览。
Macro 复用材料边界，Region/Arc 复用制造解析几何，不使用 AABB 冒充相交。
锁定/跨层整批拒绝；当前几何选择模式保留 Clear/locked 可查看的阶段边界。
GeometryMetrics 只冻结 ADR 0018，本轮不实现实际指标。
Windows deferred / not executed，最终双平台 V1/CORE10 要求不变。

下一轮明确为 **S2-B3 GeometryMetrics + 属性面板**，本轮完成后停止。
Rotate/Mirror GUI、Grid/Snap、测距、文字、生产renderer及格式交换均不加入本轮。

### Post-V1 交换边界

见 ADR 0017：Gerber 直接进入 Manufacturing Model；非 Gerber 经 VectorScene 和显式 Manufacturing Conversion；本轮不新增格式依赖/API/菜单。

## 历史任务 S2-B3

主代理实施与核验，范围见S2_B3_PLAN.md。GeometryMetrics解析计算、lazy cache、独立查询与属性面板、UTF-8源码包回归。
Mac-first；Windows deferred / not executed。完成后停止，不扩展最终Layer Area或后续阶段。

## 历史任务 S2-B3.1
GeometryMetrics已实现，完整S2-B3在renderer修复前被1000对象显示预算阻塞。
按S2_B3_1_PLAN.md和ADR0020推进有界world bins、reference parity与P1K原生门禁。
保持原验收阈值，原生十秒拖动三轮缺证据时继续B1，不扩展后续功能。

## 2026-09-20 S2-B3.2

当前范围：视口候选去重/曝光排序、局部预算、事件时 selection flags、原生窗口 benchmark。详见 S2_B3_2_PLAN.md 与 ADR0021；冻结 50ms/300ms 门槛不变。P100K preview 成本采集保留预算拒绝，不能声明 P100K 性能通过。最终结果在独立 public evidence 中绑定受测 clean commit；不启动后续功能。

## 2026-09-20 S2-C1

Grid/Snap/Measure 已阶段性通过，范围与证据见 S2_C1_PLAN.md、ADR0022、S2_C1_REVIEW.md。

## 2026-09-20 S2-C2

任务 RCam_MAC_FIRST_S2C2_TRANSFORM_GUI_NEXT_TASK.md；需求 R10/R11/R14/R18/R19/R21/R22；
局部 AT-033/034/039/040/041/043/054/087/090/091。复用 S1-B2b 的 objects.rotate/objects.mirror，
属性面板提供任意角、明确 Pivot 与选择集中心水平/垂直世界轴。SelectionSet 一次请求/事务/Undo；
Grid 不参与 Transform。只读 core bounds helper、editor-app、测试、文档/manifest/证据可修改；
不改制造变换、writer、renderer、依赖或验收身份。Mac-first，Windows deferred / not executed。
完成并复审后停止，不自动开始 S4-A。

## 阶段编号校正说明（2026-09-20）

正式阶段按 DESIGN_V1 恢复为：

```text
S1  Gerber Semantic / Writer
S2  View / Layer / Navigation / Renderer / Selection
S3  Basic Editing
    ├─ Move / Numeric Move
    ├─ Direct Drag
    ├─ Duplicate / Delete
    ├─ Multi-selection / Window / Crossing
    ├─ Rotate / Mirror
    ├─ Undo / Redo
    ├─ Grid / Snap
    ├─ Measure
    ├─ GeometryMetrics（附加能力）
    └─ Renderer performance closeout（附加能力）
S4  Vector Text / Save workflow
```

历史 `S2-B1`、`S2-B2`、`S2-B3`、`S2-C1`、`S2-C2` 文件、ADR、evidence、commit 和
review 不重命名；它们保留历史身份，但其中直接编辑、事务、Grid/Snap/Measure、metrics 与
P1K renderer closeout 实际覆盖了正式 S3 的部分范围。

## 2026-09-20 S3-FINAL（历史闭环）

正式核对 R10/R11/R13，并回归 R08/R09/R14-R19/R21/R22。补齐标准 Flash 尺寸 COW、
`edit.batch` 的单 revision/单 Undo 原子事务、按完整事务淘汰的可解释历史预算、显式对象
端点/中心 Snap 和 mm/in 显示；保留既有 Move/Drag/Duplicate/Delete/Rotate/Mirror、
Grid/Measure、GeometryMetrics 与 P1K 门禁。Mac-first；Windows deferred / not executed。
不开始 S4 文字、任务系统、Post-V1 格式或 P100K。

## 2026-09-20 S4-A2（当前，服务前置闭环）

S4-A1 headless core 已实现。本次推进 S4-A2 Gate 0、精度参数和只读预览服务，
允许模块、R/AT 对应、后续 GUI/IME/offset 闭环见 S4_A2_PLAN.md。
S4-A2.1/A2.2 已通过 Mac-first 阶段验收；历史 foundation 记录保留在 S4_A2_REVIEW.md。
Windows deferred / not executed；不进入 S4-B/S5。


### S4-A2.1 — 参数弹窗、文字轮廓和浮动放置

当前实现与验收边界见 docs/S4_A2_1_PLAN.md、docs/S4_A2_1_REVIEW.md、
ADR 0027 / 0028（docs 内路径去掉 docs/ 前缀）。
参数型功能使用独占 Modal；连续画布操作保持直接交互。
Vertical slab text geometry = retired；contour/Line/Arc Region = production path，
每个材料连通组件一个对象，字洞使用局部 retraced cut-in，writer 不经过 slab。
Mouse 文字先生成再浮动，仅平移预览，左键提交一个事务；取消不改制造内容。
GeometryMetrics 周长排除 cut-in 接缝，但对象合计不是图层最终布尔周长。

S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded，见 GLOBAL_UNITS_PRECISION_REVIEW）。
**S4-B1 Multi-Gerber Workspace = PASS（Mac-first）**（见 S4_B1_REVIEW）。
**S4-B2 Block Core + `.rcam` schema v1 = PASS（Mac-first）**（见 S4_B2_REVIEW）。S4-B3 已按用户指示启动，状态见 S4_B3_REVIEW。
DXF/SVG/PLT、Final Layer Boolean Area、Windows 仍未启动。
阶段实现不等于全部原生验收；实际状态以各阶段 REVIEW 为准。


### Global Units / Manufacturing Precision (current)

See [plan](GLOBAL_UNITS_PRECISION_PLAN.md) and the corresponding review for current evidence.
S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded）。
Display uses camera-relative local f32, safe zoom clamp and last-good-frame.
Windows deferred / not executed；不宣称完整 V1、P100K 或完整 CORE10 release。

## 2026-09-21 S4-B1 Multi-Gerber Workspace（PASS，Mac-first）

Global Units & Manufacturing Precision 已收口为 PASS（Mac-first bounded）。本阶段范围、允许模块、不做清单见
[S4_B1_PLAN](S4_B1_PLAN.md)，实际证据见 [S4_B1_REVIEW](S4_B1_REVIEW.md)：全部 Exit Gate 满足，
`RCam_S4B1_f6eed93_{source,public_evidence}.zip` + `SHA256SUMS.txt` 已生成，fresh extract 318/318。

## 2026-09-23 S4-B2 Block Core + `.rcam` schema v1（PASS，Mac-first）

本阶段范围、允许模块、不做清单见 [S4_B2_PLAN](S4_B2_PLAN.md)，实现证据与未执行项见 [S4_B2_REVIEW](S4_B2_REVIEW.md)，
决策见 [ADR 0031](adr/0031-rcam-native-project-format-v1.md) / [ADR 0032](adr/0032-block-core.md)。
最终 closeout 已完成；S4-B3 按后续明确请求启动。Windows deferred / not executed。

## 2026-09-24 S4-B3 `.rcam` Project Lifecycle（PASS，Mac-first）

范围、Exit Gate 及证据路径见 [S4_B3_PLAN](S4_B3_PLAN.md) / [S4_B3_REVIEW](S4_B3_REVIEW.md)，决策见 ADR 0033/0034。此阶段仅完成 `.rcam` 工程生命周期，不启动 S4-C。
预冻结 closeout 补齐 µm DisplayUnit 往返及 Recovery 写失败重试，保持 schema v1 和制造模型不变；最终状态以 S4_B3_REVIEW 的新证据为准。

## 2026-09-25 S4-C1 Full Object Snap（PASS，Mac-first bounded）

范围、Exit Gate 与实际结果见 [S4_C1_PLAN](S4_C1_PLAN.md) / [S4_C1_REVIEW](S4_C1_REVIEW.md)，架构决策见 [ADR 0036](adr/0036-object-snap.md)。本切片只实现 Object Snap；closeout 修复真实 runtime 的 11 px candidate query / 8 px acquire / 11 px release，并完成原生 Measure 与 Pick Base Point。下一阶段固定为 INFRA1 Runtime Diagnostics Foundation，但本轮不启动 INFRA1、Grip Editing、Block Editor、Drill、PnP/RefDes 或 S4-C2。

## 长期路线（S4-B1 之后，冻结顺序）

```text
S4-B1 Multi-Layer Workspace + reservations
→ S4-B2 Block Core + .rcam schema v1
→ S4-B3 Project lifecycle
→ S4-C1 Full Object Snap
→ INFRA1 Runtime Diagnostics Foundation
→ S4-C2+ Grip/Block/PnP/RefDes（需另行立项）
```

| 阶段 | 内容 | 前置/约束 |
|---|---|---|
| S4-B2 | BlockDefinition/BlockInstance core（无 nested，仅 translation/rotation/reflection，display + Export flatten）；`.rcam` Native Project Model / schema v1；Workspace state 与 Snap settings 持久化——**PASS（Mac-first）** | S4-B1 审查通过；同一 clean commit 完成 Metal/GUI/codec/package closeout |
| S4-B3 | `.rcam` New/Open/Save/Save As、Migration、Recovery、Recent Projects——**PASS（Mac-first）**；Gerber 仍只 Import/Export | S4-B2；双平台 V1 另验 |
| S4-C1 | Full Object Snap（制造边界、Intersection/Nearest、Layer/Class filter、Grid resolver、Drag/Text/Measure/Base Point、project settings）——**PASS（Mac-first bounded）** | S4-B3；Windows 后补 |
| INFRA1 | Runtime Diagnostics Foundation——**Panic hook closeout，最终状态见交付** | S4-C1 独立复审通过；S4-C2 前置 |
| S4-C2+ | Grip、Block Editor、Explode、Array/Panelization、Alignment、PnP/RefDes、Component Search、Shortcut Settings/Command Palette | INFRA1 完成后另行立项 |

每个阶段启动前先写任务书、需求/验收/ADR；架构方向见 DESIGN_V1 第 22 章与 ADR 0029/0030，长期约束见 AGENTS.md。
Windows、完整 V1、P100K、完整 CORE10 release 的门槛不因上述路线改变。


### INFRA1 Runtime Diagnostics（Panic hook closeout）

本地日志/诊断基础设施进入 Mac-first bounded panic hook 最终收口，范围、证据与限制见 `docs/INFRA1_RUNTIME_DIAGNOSTICS_REVIEW.md`，最终提交/二进制/Source ZIP 绑定见 exports 内交付报告。不自动开始 S4-C2；Windows deferred。

Panic hook 复审补充：`85ad528` 为 ALMOST PASS；需保留 Rust previous/default hook。修复后的最终状态以 `exports/INFRA1_PANIC_<shortsha>/REVIEW.md` 的最终门禁与绑定为准；详见 [INFRA1 review](INFRA1_RUNTIME_DIAGNOSTICS_REVIEW.md)。
