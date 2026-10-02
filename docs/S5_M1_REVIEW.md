# S5-M1 P100K native loop — 源码送审报告（in progress）

本轮为 S5-M1，核心 R05/R09/R16/R17，关联 R01/R08/R10/R11/R14/R18/R19/R20/R21/R22 不回退。仅 AT-067/068/069/071/072/074 的 macos-arm64 局部验收。阶段未标 PASS，未提交、未 push、未启动 S5-M2；还需冻结源码独立复审、最终 clean 身份原生证据和新四件套/fresh-extract/终审。Windows deferred/not executed，完整 V1/CORE10/完整双平台AT未通过。

## 基线、范围与实现

唯一入口为已收口 INFRA2 `2cde95380ab9a7161713cf5f1c15a843edb47803`，隔离工作树 `.worktrees/s5-m1`、分支 `codex/s5m1-p100k-native-closeout`。原任务全文根文件 SHA619c7637c79631637c82bee79158bfbc19bfc1b95d0215f9a92a9098db06bb4b；PLAN/BASELINE/addendum/ADR0049 编码前建立。允许文件及回归驱动补充见 PLAN。主目录31个既有 tracked 修改哈希与入口快照一致，不覆盖旧工作区。

产品行为修复仅是完整视口缓存：首轮100k完整场景导航仍因viewport边界判定触发6次全重建；现以 WorldIndex 结果是同一快照每层有序子集、层ID/对象数相等判定完整覆盖，避免完整场景无必要失效。部分视口、隐藏层、真实编辑、LOD与数值可表示性仍正常失效。送审前发现并补齐极远Pan显式数值rebase：与现有Scene::scalar的0.1physical-pixel精度检查共用，触发正常viewport worker重定位，不恢复显示工作准入限制；近距离仍复用。独立回归核对新原点空视口/返回完整场景与revision/dirty/history/制造snapshot零变化。没有新renderer/shader、core/service/writer/schema/DTO/依赖或业务operation改动；制造 f64、曝光顺序、局部孔洞、ADR0045 canonical字段/顺序/signed-zero与D1显示策略、无限累计精确框选全部保留。

其他改动为 internal-evidence 原生驱动、真实 worker/hit/CPU patch/GPU分配/上传/paint/fence观测及合成协议、13项独立checker拒绝测试。legacy bench/probe/autoload入口在默认public禁用。D1 driver 仅适配基线中已有的未配准候选预期错误，不改变业务语义；对phase8/REGISTRATION_REQUIRED精确绑定并独立断言零修改。

## 冻结环境与口径

macOS26.5.1/25F80 arm64，Apple M1 CPU8/GPU8、16GiB RAM、AC Power、真实Metal。UP27R3 3840×2160pixels / 1920×1080logical / PPP2，临时同几何60Hz，运行后实际恢复原144Hz。画布1600×900physical（不是logical或整个窗口）。release、10秒warm+60秒冻结sin Pan/Zoom、3独立进程，nearest-rank ceil(n*q)-1，逐帧全保留。实际app帧/正常callback+GPU fence的完成上界不等于scanout或人类输入；原始窗口录像/surface截图本体保留。

P100K复用旧1000×100 C0.5/1mm pitch row-major/allDark，1aperture，SHA8111ecada7a66defe30f614cd3851a328d861bbab4c595cb12ee3131fa465a31。P10K缺合格C/R/O/P，新增100×100每种2500、4apertures，SHA97b50c1af6fce5ab740455f0657bd011cd7871a49e3564ae6b89b97f26eb7b30。样本/生成器/边界/200点算术真值/全框独立100k顺序冻结于fixtures/synthetic/s5m1/manifest.json和protocol.json；protocol SHA196a7888ec13936e0f1fbd1fcc5bf266a7b22216e84147c2e580077d318e290e。未在测量失败后改样本、轨迹或阈值。

首次完整画面从正常Open入队开始计时，包含真实read/parse/display/upload/可操作完整scene GPU帧，作为实际读文件到显示耗时的保守上界。冷=独立进程无app缓存，OS pagecache不purge；热=同进程Close→Open，每个fixture三独立进程各一次冷+三次热，全部报告。点选换相机在计时外，等待真实viewport可用再注入正常RawInput，未直接写SelectionSet；正常gesture/ProbeDrag/worker/ApplicationService/消费/callback链路，不走D2候选。

## 已有原生实测（pre-review git-dirty，尚非final clean成绩）

| Mac局部项 | 实际结果及证据 | 状态 |
|---|---|---|
| AT067 | 三次导航均0重新Open/0full-build/0patch/0scene-index分配；200点选择场景/静态storage复用；单Move1CPU patch/0full-build/1scene allocation/13,555,644 bytes真实storage init upload/400,000 bytes selection upload。4项制造缓存回归+9组Metal语义/孔洞/跨层/文字/Block/样式保护 | 本机检查通过，独立复审待定 |
| AT068 | P10K60秒静置3次draw、112 CPU样本p95=0.0%；真实worker仅1次request_repaint，输入排队到绘制完成上界30.70ms；驱动闲置及等待worker不做永久重绘。revision/dirty/history/真实writer bytes不变 | 本机检查通过 |
| AT069 | 两次同冻结20轮LOD2/4/Fit/Close/5秒静置；RSS20−5分别+29.42MiB/-148.84MiB，实际wait4 app峰值283.48/386.06MiB；Metal最大观测76.66MiB，每轮关闭后52,625,408 bytes恒定。两份趋势和后暖段拟合完整保留 | 数值通过；趋势限定评估支持通过，独立复审待定 |
| AT071 | P10K三个冷92.82/100.42/97.51ms，9个热77.63–94.01ms；P100K冷828.43/761.58/750.09ms，9个热612.05–752.10ms；每次完整10000/100000制造/scene对象与正常GPU帧 | 本机检查通过 |
| AT072 | 三次60s逐帧4153/4088/4136帧，p95=16.825/17.290/16.947ms，p99=17.636/19.614/17.884ms，max20.866/27.851/22.293ms。3份80s原生窗口录像、surface截图、每帧焦点/1600×900/全部100k记录 | 本机检查通过 |
| AT074 | 最新200点CPU p954.766583ms，接收input到对应高亮p9537.435416ms/max39.335875ms，全量框选183.649667ms、object-1..object-100000完整有序，无截断；有限3对象Move→Undo/Redo→Export→Reopen和单对象Move/Undo实际完成 | 严格checker通过 |

关键原始目录：`exports/S5M1_NATIVE_PREREVIEW_20261001T225311Z`（12个正常完成观察，首份selection高亮checker失败明确保留）；修正高亮初次重跑为其select-p100k-2-bound-highlight。最新计数绑定/200点和第二次20轮位于`exports/S5M1_NATIVE_COUNTER_BIND_20261001T2338Z`，二进制SHA7a52451d47db3722176fcf0d4c1b7dad9c80d22fe7f778a06768f7a087c997f1。最新D1协议适配另绑定新内部二进制，不把不同二进制结果伪装为最终clean同一版本。

第一份memory后暖段有正斜率（5–20轮2.462MiB/cycle，R².523；10–20轮4.097/R².669），没有忽略。第二份同20轮先升至第7轮后释放，5–20轮斜率-11.285/R².867；每轮真实GPU allocation恒定，没有复现持续线性增长。原始数据/拟合与限定判断见`exports/S5M1_MEMORY_TREND_REVIEW_20261001.json`；拟合只是描述，未引入新宽松阈值，不保证任意时长绝无泄漏。

## 保留的失败与整改

- 首轮Close(false)被正常工程dirty确认拒绝：控制合成测试改为明确discard=true，不改产品确认逻辑。
- 首轮点选换相机后过早输入，被正常display-pending拒绝：等待实际viewportready后再输入，不绕过usable。
- 首份有效200点身份都正确，但199个GPU完成记录仍属于上一高亮：撤销31ms那份E2E成绩。严格checker原失败保留，驱动等待actual selected_primary与scene_serial匹配后重跑37.17ms通过；进一步GPU计数完成绑定后重跑37.44ms通过。
- 单Move GPU计数曾在update、callback前记录，修正为当前document/revision/scene/selection完成帧；禁止把旧0GPU allocation当成绩。新strict拒绝测试覆盖该错误。
- D1旧driver在未配准Focus导致的正常D2候选拒绝处提前失败，基线同样不接受此预期错误；增加精确错误和零修改断言后重跑。首次外部checker调用遗漏必需--out、及重试失败原文全部保留。

以上失败在各run-id中保留，未覆盖、未放宽checker或几何/制造门槛。

## 实际门禁与未完成项

最近完整25命令记录`exports/S5M1_GATES_20261001T231514Z/commands.json`全部exit0：manifest/generator/Python/fmt/checkworkspace-alltargets/clippyworkspace/clippyinternal/testworkspace/automation_contract/headless_workflow/servicenormaltree/publicrelease/D1perf/D2perf/9组Metal/ignoredinventory/internalrelease。workspace773passed/0failed/47ignored（83结果块），不能称47ignored也通过。正常service依赖树不含egui/eframe/wgpu/winit/raw-window-handle。D1/D2性能与9Metal已单独实际执行，剩余36ignored逐条原因见exports/S5M1_IGNORED_CLASSIFICATION_20261001.json。

该完整门禁早于最新计数与D1协议适配。后续focused clippy/internal build/S5三项测试/Python13拒绝测试已执行；23:50完整候选25命令同样全部exit0（exports/S5M1_GATES_20261001T235054Z），773/0/47；随后自查补数值rebase保护，修复前成绩保留。最新修复的最终候选完整门禁记录待追加，不能冒充当前源码已全量通过。D1/D2真实原生回归已完成，见exports/S5M1_NATIVE_REGRESSIONS_20261001T234847Z：D1独立checker23records通过；D2独立checker21observations通过、app正常exit0。内部二进制SHAa5575fc4d2dba70c27236ad5c47af49150cab41a0ebab36d08788b64453a0050。D1旧driver完成后仍留窗，harness在最终report后SIGTERM(-15)，不称正常用户关闭。默认public SHAbe418da0c84bac505a20e2b92032f0cb5c228bad802cfcb52f154798a234586b已实际启动/显示，主动evidence字符串为空、注入S5/D1/D2/legacy环境无输出/自动load，偏好及既有恢复文件前后哈希一致。实际普通启动显示既有恢复提示，未恢复/忽略任何用户副本；截图仅为空工程/提示，不含客户名或路径，不能据此宣称public用户完整导入/编辑流程。harness在smoke后SIGTERM，不冒充正常用户退出。

制造API/schema/Writer/precision/锁文件/96cases/required_platforms未改。无私有Gerber/PnP/字体纳入源分发；系统Songti只读本机字体用于实际Metal保护测试，不复制字体。证据含合成原生画面/导出，不含客户内容；本地原始日志含主机路径，尚未作为public-evidence发布。正式public evidence生成时须脱敏检查并绑定最终clean身份。

尚未执行：独立冻结源码复审、最终clean提交与该身份完整Mac复测、新Source/PublicEvidence/SHA256SUMS/fresh-extract四件套、真实无Git editor-app重建身份与mutation拒绝/restore核对、独立最终包审。未标PASS，不把候选ZIP/日志当clean交付。Windows/人类物理输入/IME/跨显示器/签名发行/CORE10/PMIX/PPOL/PSTRESS/S5-M2及CircuitCAM完整兼容未执行/范围外。

## 最终候选送审状态（2026-10-02 UTC）

最新完整25命令为 `exports/S5M1_GATES_20261002T000007Z/commands.json`：workspace **774 passed / 0 failed / 47 ignored，83结果块**；Python33tests、check、service合同/无头/依赖树、public/internal release、D1/D2性能及9组Metal均exit0。两项Clippy首次exit101，仅新测试字面量100_000.12345的下划线分组lint；修正成相同数值100_000.123_45，未改测试预期/阈值。补验 `exports/S5M1_FINAL_CANDIDATE_CHECKS_20261002T0008Z/commands.json` 中fmt/manifest/clippyworkspace/clippyinternal/四项S5聚焦测试全部exit0，首次失败日志保留。旧773计数明确是加数值rebase回归前的源码，不冒充当前774。

最终候选内部二进制 SHA `26804938a6a903380b4bbb71051b8e9d9776f7491014b235c4677b015e9ba29b` 在相同冻结协议上另做完整60秒导航确认：4176逐帧，p9516.582542ms / p9917.604375ms / max19.36325ms，真实Open/full-build/patch/scene/index分配以及storage上传增量全部0；80秒录像及surface本体保留，独立checker exit0。该补充不替代原三次独立导航，原三次原始数据与不同二进制绑定均保留，最终clean身份仍需按规定重跑三次。

最新默认public二进制 SHA `46da8b92c371919b8d851fc858a7218ba0307a432a1074b5735640f78f8f8d78` 正常启动及主动evidence入口拒绝smoke再次通过（`exports/S5M1_NATIVE_REGRESSIONS_20261002T001208Z/public-smoke`），偏好/恢复文件哈希不变。D1/D2原生回归使用上一同业务版本a557...（仅随后补的数值rebase不同），不冒充最新clean最终二进制。

原选择失败不是产品选错对象：200个实际ObjectId正确，但driver在normal callback绘制新选择之前使用前一帧的paint/fence，将199条上一对象高亮记为完成。strict checker指出 `point does not bind native highlight/focus`，错误31ms成绩撤销且原check FAIL保留。修复等待完成帧的selected_primary==当前真实selection、scene_serial==本点输入前scene，之后200点重跑通过。随后补GPU计数：normal update→callback prepare/upload→paint→GPU fence后再采计数，并为每条编辑记录绑定document_id/revision/scene_serial/selected_count/selected_primary；13个checker测试包含拒绝旧高亮、旧revision、绘制前计数及缺少实际GPU上传。旧checker要求未删除/放宽，新条件更严格。最新37.44ms选择结果及183.65ms完整框选对应7a5245...二进制；原失败、37.17ms初次修复与37.44ms完整post-paint版本均单独保留。

送审前以实际源码/验证器文件哈希、517文件源码清单、全部tracked+untracked差异、独立冻结副本及候选ZIP绑定；具体身份与路径由 `exports/S5M1_SOURCE_REVIEW_CANDIDATE_*/SOURCE_REVIEW_INFO.json` 和 CANDIDATE_SHA256SUMS.txt 给出。它们是 **precommit source review candidate**，不是最终clean四件套，没有伪造阶段commit/clean身份或fresh-extract结果。代码冻结后只等待独立源审；本报告不提前批准自身源码。后续源审整改若修改代码要重新冻结/受影响验证，源审放行后再按用户授权路径提交、clean原生全套、新四件套/无Git实际app构建身份/独立终审，停止S5-M1。

最终结论：**实现与本机pre-review检查已齐，源码送审就绪；S5-M1阶段仍in progress，未完成最终clean交付。** 没有已确认的制造/文件安全阻断；独立源码复审、最终clean证据及包审是明确剩余门禁，不将其标通过。原始三份导航录像总计609,314,472 bytes，最终Library单附件上限需要在正式公共包阶段处理（保留原始本体/哈希，不静默缩短、降画布或丢帧），当前未上传public-evidence或字体/私有文件。

## 独立源审 REQUEST CHANGES 与本轮整改（2026-10-02）

独立报告 `exports/S5M1_INDEPENDENT_SOURCE_REVIEW_20261002T004351Z/S5M1_INDEPENDENT_SOURCE_REVIEW.zh-CN.md`（主目录只读）要求 S5-REV-01/02 两项 B1 验证器整改。先前“严格checker通过/源码送审就绪”仅为旧候选自验，独立复审未放行；不据此授予阶段 PASS 或 commit。00:59 会话仅建立范围、三个漏洞复现与修复前哈希，实际 checker/driver 未改，本轮在入口源码完整副本基础上实施。

本轮只改 `native_s5m1.rs` 的采集字段、选择/导航 checker、拒绝测试、真实证据变异工具和本阶段文档/manifest。正常输入、worker、callback、paint stamp、GPU fence、产品渲染/选择/制造实现及冻结门槛不改。GPU成功与primary确在scene由实际状态观测；navigation-start/end与单调时间明确定义，完整区间及逐帧时间一致性由checker强制，点选保留局部场景，框选完整100k。具体规则见 addendum。本轮结果与完整送审快照另存 versioned exports，最终数值在本轮中文送审报告给出。

旧原始失败、独立漏洞复现、不同二进制记录完整保留；新严格协议拒绝缺采集字段的旧导航/选择记录，不人工回填。原高亮 driver 缺陷已由前次独立审核确认修好，本轮不重复该修复。最终 clean 六项全套、新四件套/fresh-extract 和终审仍须在源码复审及主对话授权提交后执行。本轮不 commit/push，不启动其他阶段。

### 本轮两项整改送审结果（S5M1_REMEDIATION_20261002T0125Z）

同一新内部二进制 `18d9d1b3f9ed6824bf043667ef895eae88fc72e499df9309d9ce9a6abdbc04b9` 实际重跑200固定点/全量100k框选及三独立导航。点选200结果均绑定真实当前高亮，402条实际精确查询CPU总体p95=4.621791ms（不是200个唯一query的p95），高亮上界p95=37.278375ms/max80.758875ms，完整有序100k框选169.853708ms。三导航4117/4075/4073帧，完整60.0083145/60.011218875/60.006940125s；p95=17.117209/17.268416/17.257958ms，p99=18.521458/19.102875/19.056750ms，max=27.823208/24.993166/28.083125ms。全部有明确start/end、连续frame IDs、原始单调时间、完整有效100k绘制与零重解析/重建/静态上传；三录像80.015/80.003333/80.020s本体保留。受控正常RawInput、paint/fence上界及app帧仍不冒充物理人类输入/scanout。

本轮实际新证据66/66负面变异被拒绝，独立审核原三个漏洞脚本3/3拒绝；未修改真实raw记录。20份既有S5历史观察按新checker重查9通过/11拒绝（包括原失败和缺新字段），原文件哈希未变；没有重新审计旧D2。完整有界3对象Move/Undo/Redo/Export/Reopen及单Move/Undo实际完成，独立网格算术核对导出100k与仅首3对象位移，不依赖renderer或writer自比。

本轮Rust门禁fmt/check/clippy（workspace与internal）/workspace test/service合同和无头/正常依赖树/public与internal release均exit0；workspace774 passed/0 failed/47 ignored。新增checker回归共18项，全部Python38项；实际另执行2组原生Metal语义与production/reference像素一致性测试exit0，旧9组结果仅为历史证据。AT068/069/071、D1/D2原生及public GUI smoke本轮未重新采集；本次仅修复采集/验证器，旧业务源码不变，最终clean全套仍待源审通过及主对话提交指令。

首导航的额外busy=false检查误拒绝了自动Recovery期间39个完整正常绘制帧。源代码中RecoveryWrite只写恢复快照，pan/zoom不以busy禁止；raw显示39个不同相机样本、无pending/error和制造修改。修正这个额外错误约束后用同一未改raw复查通过，首个误拒绝check原件保留，选择完成仍须非busy。新增正/负测试同时证明后台busy可正常导航、display_pending仍拒绝；所有冻结数值/完整区间/逐帧规则未放宽。新增测试后首次Python源包检查因manifest尚未再生成失败，也独立记录，最终manifest及测试重新执行。

源码/验证器/二进制/原始证据哈希、完整tracked+untracked diff、仅本轮diff与中文报告冻结于本轮exports。快照是precommit独立复审候选，不是clean四件套；未commit/push，不自行判独立审核通过或S5-M1 PASS。Windows deferred/not executed，完整V1/CORE10/S5-M2仍未启动。


### R2 独立源码复审放行与最终 clean 执行入口（2026-10-02）

独立复审 `exports/S5M1_R2_INDEPENDENT_REVIEW_20261002T032631Z/S5M1_R2_INDEPENDENT_REVIEW.zh-CN.md`（主目录只读；Library `libfile_13f0f88237208191ba5006f44eb46a53`）结论为 **APPROVED FOR STAGE COMMIT，仅源码放行**，S5-REV-01/02关闭，无新B0/B1。本段只记录复审事实，不改冻结样本/阈值、产品/验证器或源审结论。用户授权阶段commit后，以新clean身份执行六项Mac最终门禁、D1/D2/Metal回归、正式四件套及无Gitfresh-extract；完成后停止等独立包审。最终结果另存版本化exports，尚不标S5-M1 PASS，不push；Windows/full V1/CORE10仍deferred。


### 最终执行中补齐根任务的无Git身份覆盖（2026-10-02；另待审）

首个授权提交 `2dff4954a2d5a01a2926b9eeda821713ebd7f034` clean，manifest/generator/Python38/fmt/check/两种clippy通过；workspace编译在发现下述真实归档身份失败后主动停止，未宣称其通过，未开始原生性能。实际Source ZIP已独立解压并用当前build_identity.rs编译检查器执行resolve，exit1：source manifest coverage mismatch，证据 `exports/S5M1_FINAL_CLEAN_20261002T0338Z/first-commit-archive-probe`（这是实际Rust身份检查，不冒充完整editor-app构建）。Python源码清单已含本阶段根任务，而Rust显式根列表遗漏该文件；后续修正只同步该项、使归档fixture包含实际根任务，并在既有正/负测试中验证任务文件改动也拒绝。对应PLAN先记录本修复；不跳过清单/哈希/身份规则，不恢复unknown，不改业务/采集/阈值。新修复提交后完整final-clean验证和无Git真实app构建仍必须执行，补充源码修改供独立包/源码审核，尚不标PASS。
