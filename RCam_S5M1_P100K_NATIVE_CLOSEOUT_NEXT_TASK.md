# RCam S5 M1 P100K 原生渲染与交互收口任务

状态：规划正式稿，尚未执行。仅在构建身份追踪整改通过独立复审、其最终 clean commit 被确认并明确启动本任务后执行。S5-M1 是本轮新增的 Mac-first 子阶段编号，不是既有 S4-D3，也不是完整 S5 或 V1 的完成声明。

## 1 结论与依据

下一步先补齐固定 P100K 样本的真实产品渲染、导航、点选和框选闭环，并修复实测暴露的瓶颈，不继续增加候选关联、库替换或新格式能力。

理由：S4-D2 独立审核已确认候选查询、100k 组件查询和受控原生流程通过，但明确不覆盖产品 P100K 门槛；快的局部查询不能证明完整画面、连续导航或输入到高亮的端到端性能。原 DESIGN_V1 第 14、17 节及 ACCEPTANCE_V1 的 AT-067/068/069/071/072/074 已经给出这些要求，无需发明新功能范围。

主要依据是通过 git show 从完整提交 `8512a7c3d1d9000f7374f1d1484d413bbb9b41cb` 导出的《RCam_8512a7c_基线规划文档原文.md》。已读取全部 7,278 行，核对其中七个仓库文件，包括 schema 2 的 96 个有效用例、退役 AT-079，以及本轮六个 AT 的 windows-x64/macos-arm64 双平台要求。定位如下，行号为该原文快照行号：

| 当前依据 | 已核实事实 | 本任务处理 |
|---|---|---|
| CAPABILITIES 2089–2096；DESIGN_V1 2739–2751 | Production Renderer 已有有界有序 world-space bins，保留 reference renderer；已有候选去重/排序、选择标志和原生 benchmark | 复用并优化现有路径，不从零新建渲染器 |
| CAPABILITIES 2216–2218；DESIGN_V1 3493–3501 | 历史大图显示数字已被后来的显式大资源策略修订：对象数、轮廓存储、cell 密度和估计像素工作量属于优化目标/诊断，不再作为显示准入门槛 | 不重新施加旧显示拒绝限制；数值可表示性、有限几何及制造/导入/导出安全不变 |
| DESIGN_V1 3511–3516、ADR0046 引用 | 精确矩形框选无固定累计工作 cap；worker、稳定曝光顺序、原子错误处理仍须保留；point-query 策略不变 | 完整精确框选，不能通过工作 cap 拒绝或截断来获得低延迟；不得顺带放宽点选策略 |
| DESIGN_V1 3503–3509、ADR0045 引用 | dirty 基线已有分块 canonical JSON SHA-256 复用；结构编辑/代次不连续时重建 | 保留现有性能维护，不改哈希语义/schema/DTO |
| BASELINE 1918–1951 | M1/16 GB/macOS 26.5.1 有记录；显示器/DPI/刷新率/性能画布仍未登记，S0 样本与资源段为历史文本 | 新建 S5_M1_BASELINE，不能把 S0 文案当作当前实测或恢复旧数值策略 |
| DESIGN_V1 第 14 节；IMPLEMENTATION_PLAN 3548–3559 | S5 原目标就是固定样本性能与原生平台实测 | 从现有 S5 取本轮六个 Mac 局部用例，不降低原门槛 |
| S4_D2_REVIEW 3801、3857–3876 | D2 是局部查询和 100k PnP＋四 Flash 原生 smoke，明确不代表产品 P100K | 不复用 D2 查询耗时冒充本阶段完整产品成绩 |

交叉依据：

- RCam_S4D2_8512a7c_独立审核报告.md，2026-10-01，第 4/5/8/9 节
- RCam_S4D2_REFDES_STENCIL_CANDIDATES_NEXT_TASK.md，第 79/87/88/90/92–97 节
- 当前提交的 DESIGN_V1、ACCEPTANCE_V1、BASELINE、CAPABILITIES、IMPLEMENTATION_PLAN、S4_D2_REVIEW、acceptance_cases.json
- RCam_LONG_TERM_ARCHITECTURE_GUIDANCE.md 第 2/16/18 节；RCam_GERBERCOMPAT_119c633_FINAL_Review.md 第 25/26/31 节，仅作历史补充

证据边界：本规划已核对上述提交内的原始文档和验收 JSON，未独立检查 Rust 实现或运行产品。实现入口、测试文件及 identity-fix 后差异由执行者在下节核实；已实现且已满足门槛的部分只补新提交证据，不重复开发。该基线没有 STAGE_STATUS 文件，阶段索引用 IMPLEMENTATION_PLAN。

## 2 进入条件与当前源码核对

1. 以独立审核通过的构建身份整改 commit 为唯一基线，记录完整 commit、Cargo.lock、工具链和源码清单 SHA。不得在身份整改未通过前把本任务文件当成下一阶段启动令。
2. 从该 commit 建立隔离、干净工作树。保留主目录已有未提交改动；不得直接覆盖主目录、reset 或把旧工作树内容混入新阶段。
3. 先读取实际 AGENTS.md、相关 .agents/skills、README、DESIGN_V1、CAPABILITIES、IMPLEMENTATION_PLAN、BASELINE、AUTOMATION_API、当前 ACCEPTANCE_V1 与 acceptance_cases.json、S4_D2_REVIEW；另读 S2_B3_2_PLAN/ADR0021、S4_D1_LARGE_WORKSPACE_FIX、S4_D1_EDIT_SPEED/ADR0045、ADR0046，以及现有 renderer、性能脚本和测试说明。不得按历史 S0 或较早 CAPABILITIES 段落恢复已被后续决策取代的限制。
4. 形成一页现状表：每项要求对应当前代码入口、现有测试/脚本、最近有效证据、证据缺口。区分「已实现未验」「已验但旧 commit」「确认缺陷」「未实现」，不能把缺证据直接记成产品失败。
5. 核对 P10K/P100K 生成器、参考与生产渲染路径、缓存计数器、事件/帧采集入口的实际存在性。复用已有实现；不要仅因历史文档写着 Production Renderer 就从零重建第二套引擎。
6. 先将本任务写入本地项目根目录 `RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md`，建立 `docs/S5_M1_PLAN.md`、`docs/S5_M1_BASELINE.md`、`docs/S5_M1_ACCEPTANCE_ADDENDUM.md` 和未占用编号的 ADR，再改产品代码。当前 IMPLEMENTATION_PLAN 没有 S5-M1，启动时仍复查身份整改是否新增同名内容。保留当前 ACCEPTANCE_V1/acceptance_cases.json 字节、96 个有效 AT、AT-079 退役记录和 required_platforms 原样；本阶段结果写入独立 addendum。

若 identity-fix 与 8512a7c 有差异，列出并复核；仅构建身份修复不应改变上述行为。当前 ACCEPTANCE_V1 明确制造 q 由文档精度指定，默认 0.0001 mm；FS 编码 0.000001 mm（ADR0026）。继续使用现行 ManufacturingPrecision，不能把 S0 BASELINE 的旧 q 文案硬编码回产品。

## 3 本阶段范围

核心需求：R05/R09/R16/R17，关联 R01/R08/R10/R11/R14/R18/R19/R20/R21/R22 的不回退约束。

允许工作：

- 冻结并校验现有 P10K、P100K 合成样本、测量轨迹和 Mac 参考环境
- 建立可重跑的原生产品性能证据与失败报告
- 按实测瓶颈修正 renderer 缓存、可见集、上传、app 结果消费或既有选择索引路径
- 必要测试、诊断白名单、阶段文档、包装清单更新

允许目录：`crates/editor-app` 内现有 renderer、性能/调度/选择与 internal-evidence 路径；`crates/editor-core` 的既有 WorldIndex、精确命中/框选路径；`crates/editor-service` 仅在已复现瓶颈确需时调整同一查询/测量路径；上述 crate 的相关测试、`fixtures/synthetic`、`scripts`、`docs`、根任务/README/CHANGELOG 和源码包装清单。PLAN 在编码前列明实际文件。不得改 core 制造几何/Writer 语义、project schema、公共 API，或引入新运行时依赖。服务与核心正常依赖仍不得引入窗口或 GPU。

不在本轮：

- PnP 持久关联、footprint/pad 几何、库替换、Drill/装配输出、Shortcut Settings/Command Palette 新产品功能
- 新增 Gerber 方言、改变制造语义或现有安全拒绝策略
- PMIX/PPOL 的完整性能验收与 1000 对象拖动性能、PSTRESS 专项
- CORE10 业务签署、人工 IME/触控板全流程、跨显示器 DPI、干净普通用户发行环境、签名/公证
- Windows、Linux 产品支持、完整 V1、CircuitCAM 4.4 完整兼容、VectorScene/DXF/SVG/HPGL 新功能

如果性能修复确实需要更换架构、改变仍有效的安全上限、修改 API/schema 或改变制造解释，先提交最小复现和设计决策，停止该扩大部分；不能为了通过本任务静默改变边界。显示优化目标与验收峰值不是通用输入/框选拒绝开关；不得重新引入 D1 已移除的显示准入/固定累计框选工作 cap。仍有效的 point-query、D2 候选 10000/分页 500、制造事务及导入导出安全策略不变。

## 4 冻结样本与测量口径

- P10K：10,000 Flash，C/R/O/P 各 2,500
- P100K：100,000 重复 Flash，最多 8 个标准光圈，固定布局、视口和曝光序列
- 保护样本：既有 Dark→Clear→Dark、孔洞、跨层 Clear、局部透明、Block/文字实例语义测试
- 检查实际生成器/样本库存；BASELINE 声称尚未生成只代表 S0 历史，不能据此覆盖后续已冻结样本。存在合格冻结样本就复用；缺失时仅按上述固定定义补齐。记录生成器版本/种子、文件 SHA、对象数、几何统计、边界、固定 200 点位/预期 ID、全视图框选区域和独立预期值，不能以 RCam 当前输出充当唯一真值
- 使用 release、实际 Metal、同一 Mac 适配器；1600×900 物理画布、60 Hz 基准、非最小化/非远程桌面呈现、预热 10 秒、冻结 60 秒 Pan/Zoom 轨迹，独立重复 3 次。Mac pixels_per_point=2 时不能把 1600×900 逻辑点冒充该物理画布；测量并记录真实像素区域
- 记录实际 OS/build、芯片、RAM、GPU/backend、显示器/缩放、物理画布、电源状态、二进制 SHA、commit、采集时钟/定义。已有审核机器是 macOS 26.5.1/M1；不得因此声称已验 macOS 15
- 区分应用帧间隔、CPU/GPU 时间和实际呈现间隔。无法采实际呈现时明确写「应用帧」，附原生录像/截图，不称实测显示 FPS
- 所有重跑保留原始失败结果，单独新 run-id。不得只报告最好一次，也不得测试之后放宽阈值或更换样本

## 5 必须完成的验收

以下是 Mac 平台局部收口项，原 AT 双平台总状态不因此改成通过。

### AT-067 缓存与制造正确性

在 Pan、同 LOD Zoom、点选和单对象 Move 时，记录解析次数、几何重建、GPU 上传、draw call、可见/总对象数及分配量。导航不得重解析或全量重建制造几何；选择只影响叠加层。优化前后对保护样本作独立覆盖断言，不能丢小开孔、填孔或重排 Dark/Clear。

### AT-068 闲置和唤醒

P10K 静置 60 秒保存重绘次数和 CPU 采样，无任务/动画时不持续 60 FPS 重绘。后台任务完成能唤醒并显示真实结果，不能靠永久刷新掩盖消息丢失。

### AT-069 长期内存

原生产品连续打开/关闭 P100K 20 次，固定每轮采样和静置口径，额外触发 LOD 缓存并回原视图。第 20 次与第 5 次静置内存差 ≤100 MiB，且无持续线性增长。记录进程峰值和显式 GPU 分配。本阶段按设计初始目标冻结进程峰值 ≤1 GiB、显式 GPU Buffer/Texture ≤512 MiB；如果执行者找到更严格的已批准 Mac 基线则采用该基线并记出处，不可测试失败后放宽。这些是固定样本验收指标，不是新的显示准入拒绝限制。统一内存统计可能重叠，不直接相加；缺真实采样就记阻塞，不能填估算通过。

### AT-071 首次完整画面

P10K 与 P100K 按冻结冷热启动口径各测 3 次，从实际开始读文件到完整可操作画面。每次 P10K ≤3 秒、P100K ≤10 秒。解析结束、后台返回、空白首帧或仅部分图形出现都不是完成点。

### AT-072 P100K 连续导航

按第 4 节固定轨迹，每次 p95 ≤33.3 ms、p99 ≤66.7 ms，持续交互无 >200 ms 停顿。保留逐帧原始数据、分位计算方法、轨迹与原生画面证据，并同时证明对象/覆盖完整。

### AT-074 点选与框选

200 个固定点位，CPU 精确命中查询 p95 ≤20 ms，输入到可见高亮端到端 p95 ≤100 ms。固定全视图框选结果就绪 ≤300 ms，UI 可响应，完整选择 ID 集合符合预期和稳定曝光顺序。无论选择多少对象，不得静默截断为 500/10000 项或绕去 D2 候选查询；不要重新添加固定累计精确框选工作 cap。

端到端起点是实际 app 接收输入事件，终点是高亮结果被正常帧消费并显示；记录排队/worker/结果消费/显示边界。不能用候选 bounds、纯 service 时间或直接写 SelectionSet 代替正常输入链路。public 和 internal 走相同业务路径，不能为了分数绕过 public 默认诊断、dirty 状态或失效检查。

### 制造及后续工具不回退

对 P100K 中明确选定的有界小对象组通过既有编辑命令作 Move→Undo/Redo→Export→Reopen，验证坐标、曝光、原文件保护及一次动作一个事务；全量框选不等于要求放宽制造编辑单事务上限。导航/选择/闲置前后 revision、dirty、history 和 Gerber bytes 不变。再验证 D2 候选选择后既有编辑闭环、D1 注册方向及失效处理。保留 ADR0045 分块 dirty 缓存全部字段、顺序、signed zero 语义；结构编辑/代次不连续仍重建。性能优化不能改 Writer、以屏幕 Mesh 回生成制造图形、漏画、降制造精度、关闭必要显示效果或降低命中精度。

## 6 测试和证据门禁

从最终 clean commit 执行并保留真实命令、退出码和原始日志：

- source_manifest.py --check 与 test_package_source.py
- cargo fmt --all -- --check
- cargo check --workspace --all-targets --locked
- cargo clippy --workspace --all-targets --locked -- -D warnings
- cargo clippy --locked -p editor-app --all-targets --features internal-evidence -- -D warnings
- cargo test --workspace --locked --no-fail-fast
- cargo test --locked -p editor-service --test automation_contract --test headless_workflow
- cargo tree --locked -p editor-service -e normal，检查正常依赖无 egui/eframe/wgpu/winit/raw-window-handle
- cargo build --release --locked -p editor-app
- 新阶段聚焦测试、适用 Metal 语义/缓存测试、D1/D2 性能回归和原生 smoke

上述标准入口已在当前 S4_D2_REVIEW 记录；聚焦入口复用 `scripts/measure_s4d1_performance.py`、`scripts/measure_s4d2_performance.py`、`scripts/verify_s4d2_native.py`，以及 app 的 `native_metal_c3_create_invariance`、`native_metal_block_instance_parity` ignored 专项。新 S5-M1 runner/checker 可按本任务新增，不能把旧 D2 runner 改写成新阶段结果。其余测试名由现源码核实，不猜测不存在的 xtask。ignored 项逐项分类，已单独执行的附其证据；未执行项仍记未执行，不能把 workspace 总数等同于完整产品验收。

默认 public release 和 internal-evidence release 分开，绑定同一 commit 和各自二进制 SHA。受控驱动可用于可重复原生测量，但不宣称物理人工输入；public build 不得含主动 evidence-control 入口，需正常启动/显示 smoke 和相同业务路径核对。保留截图/录像文件本体，避免只有「截图已看」文字记录。

阶段未通过前，能力/阶段文档使用 in progress 或待验表述，不提前标 PASS；本任务没有新增业务操作，不能为了阶段名称把未实现操作加入 supported_operations。更新 CAPABILITIES 的当前摘要时保留明确标注的历史记录，避免旧 C4 摘要遮蔽 D2 或 S5-M1 的真实状态。

## 7 交付和独立复审

1. 正式任务/PLAN、ACCEPTANCE_ADDENDUM、REVIEW、基线与样本 manifest、逐项结果、原始性能数据、缺陷与未执行项
2. 完成阶段的实现/测试/文档 Git commit；报告最终完整 SHA。后续任何修复形成新 commit 并重跑受影响门禁，不覆盖历史证据
3. 按当前已批准包装规则产出以新 commit 命名的 Source ZIP、Public Evidence ZIP、SHA256SUMS.txt、source_fresh_extract_report.json；另绑定 public/internal binary SHA
4. Fresh-extract 校验必须证明受测源码与交付源码一致，并重新覆盖无 .git 构建身份回退，不能让刚修复的 unknown 回归
5. 独立审核针对最终提交和新包执行。已确认制造/文件安全 B0 应停止生产输出、先修复再跑全部受影响 B0；本阶段必需 B1 不能包装成可忽略优化

本阶段通过需：第 5 节每项在冻结 Mac 基线真实通过；六项原始 AT 要求的画面/交互观察亦有可审阅记录，受控原生驱动不能吞掉观察义务；语义/回归无阻断；证据与身份完整；清洁包及独立复审通过。受控输入、CUA 操作与人类物理输入分别如实标注，不能互相冒充。结论只能是「S5-M1 P100K native loop PASS（Mac-first bounded）」或逐项未完成/失败/阻塞；仍不能宣称完整 AT-067/068/069/071/072/074 双平台通过。

完成后停止于复审和交付，不自动进入下一阶段，不把 S5/S6、96 AT、Windows、CORE10 10/10 或完整 V1 标绿。

## 8 后续顺序及何时需要用户输入

建议顺序，不是本任务的自动执行范围：

1. S5-M1：本任务，P100K 原生渲染/交互与资源基线
2. S5-M2：PMIX/PPOL、1000 对象拖动、PSTRESS、任务取消/过期结果/设备恢复，主要 AT-063–066/070/073/075
3. Mac 输入与发行收口：IME/焦点、触控板、DPI、普通用户离开源码目录完整 Save/Export/Reopen；AT-025/043/046/077/078 及关联文件安全检查
4. CORE10 本地业务验收和 Mac 有效 AT 汇总：AT-080/081/082/083/084/086–097；保留 Windows 待执行，结论仍为单平台预览/限定验收
5. Windows 适配与原生证据以后再排；完整 CircuitCAM 4.4 兼容继续最后单独立项。Post-V1 VectorScene→DXF→SVG→HPGL→多格式导出只保留原路线，不在本任务提前启动

本阶段只需公开/合成固定样本，不应先要求用户上传私有生产文件。后续 CORE10 开始前，在已授权本地范围检查原有冻结 manifest、10 份文件、哈希、规定操作、独立真值和获准字体是否仍可用；已有输入可用就直接使用。确实缺失时，才请用户提供准确本地位置/访问授权或缺失的冻结输入，不能自行换一组容易通过的样本。

私有 Gerber/PnP/.rcam、客户路径/RefDes、字体原文和制造 dump 留本地；公开证据只包含已核准的计数、哈希与非敏感摘要。独立工具必须有本地可用版本/许可记录；缺工具或独立真值就明确阻塞该项，不通过上传在线查看器绕开。

物理人工输入、额外显示器或干净普通用户机器确需用户参与时，给出最小具体操作和待补证据；未拿到真实观察前不标通过。签名、公证和账号/凭据相关步骤另行处理，不要求关闭系统安全防护。
