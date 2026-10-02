# S5-M1 P100K native loop — PLAN（in progress）

2026-10-01 启动。任务全文：../RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md（18536 bytes，SHA256 619c7637c79631637c82bee79158bfbc19bfc1b95d0215f9a92a9098db06bb4b，Library libfile_e44564c012cc81919ef645c913a8ccb8）。唯一基线 2cde95380ab9a7161713cf5f1c15a843edb47803，隔离分支 codex/s5m1-p100k-native-closeout，主工作区旧改动不碰、不push。

核心 R05/R09/R16/R17；关联 R01/R08/R10/R11/R14/R18/R19/R20/R21/R22 不回退。仅 AT-067/068/069/071/072/074 的 macos-arm64 局部验收；96 AT、AT-079退役记录和所有required_platforms原样，双平台/完整V1/CORE10/P100K其他阶段不标PASS。决策 ADR0049；结果单独addendum。先基线/原生实测，再按实测修复既有路径，不创建新renderer。

## 现状表（入口核对，不是本阶段成绩）

| 项目 | 当前入口/已有测试 | 最近证据与本轮缺口 | 分类 |
|---|---|---|---|
| 有序production/reference renderer | gpu.rs::Resources/Callback + editor.wgsl、render_index.rs；native_metal_reference_production_pixel_parity | S2旧门禁/后续Metal语义，缺新commit P100K1600×900 | 已实现未验当前门槛 |
| P100K | fixtures/synthetic/s2b3_2/P100K_CIRCLES.gbr、generate_s2b32.py v1 | 100000 C Flash、1 aperture、旧冻结SHA；复用，不换布局 | 已有固定样本 |
| P10K | 旧P10K_CIRCLES仅C，10000对象 | 不满足本任务4形状各2500；保留旧样本，补新C/R/O/P冻结样本 | 样本缺口 |
| AT067缓存 | display.rs、state.rs::refresh/rebuild_changed、gpu.rs::prepare_measured/Callback | Arc scene/index/selected复用，PrepareStats有CPU/candidate访问；缺完整parse/rebuild/上传/分配计数和当前保护覆盖 | 已实现，计数证据不足 |
| AT068闲置/唤醒 | main.rs worker request_repaint、无任务阻塞recv；dirty recovery 1秒定时 | INFRA2只有受控D2 smoke；缺60秒idle CPU/有效wake测量，不能用D2永久刷新 | 已实现未验 |
| AT069生命周期 | state.rs::Close/reset、Resources替换、last_good/scene缓存 | 缺20次打开/关闭+LOD/RSS/实际GPU分配观测 | 未有当前证据 |
| AT071首次完整画面 | Open→worker Model/Scene→app view→production paint | 缺读文件起点与可操作surface完成绑定，解析返回不算完成 | 已实现未验 |
| AT072导航 | 原native_bench::RawInput/painted stamp（P1K，30秒） | 旧脚本不是P100K60秒×3；新增S5 runner，旧结果保留 | 驱动不符合本任务 |
| AT074点/框选 | main::gesture/ProbeDrag→state::hit/select_rect→ApplicationService | WorldIndex/f64精确查询已实现；缺200点完整input-to-highlight、100k全序列结果<=300ms | 已实现未验 |
| public evidence边界 | D1/D2驱动feature-gated，旧s2b32/native_probe仍含默认环境入口 | 新S5必须internal-only；记录旧入口，不把S2/D2改写成S5成绩 | 需核对/收紧主动测量入口 |

8512a7c→2cde953共13文件，仅构建身份/packaging/tests和文档，core/service/render/UI行为未变，见 exports/S5M1_BASELINE_20261001T2125Z/entry-baseline.json。旧S0输出q/对象显示预算不适用当前；ADR0044/0046取代ADR0021历史work准入，保留ADR0045 canonical ordered/signed-zero cache和所有仍有效point/query/edit/import/export限制。

## 实际文件边界（编码前冻结）

允许初始修改：editor-app/src/{main,gpu,state,preferences,recovery,native_bench,native_probe,display,render_index,world_index}.rs 现有路径；新增 editor-app/src/native_s5m1.rs、s5m1_tests.rs；仅必要缓存/计数/输入采集及internal-evidence隔离，D1/D2驱动仍独立不改语义。
新增 scripts/{generate_s5m1,run_s5m1_native,verify_s5m1_native,test_verify_s5m1_native}.py、fixtures/synthetic/s5m1/{P10K_CROP.gbr,manifest.json,protocol.json}；复用P100K旧文件。
文档 docs/S5_M1_{PLAN,BASELINE,ACCEPTANCE_ADDENDUM,REVIEW}.md、adr/0049-s5m1-native-performance-closeout.md、IMPLEMENTATION_PLAN/CAPABILITIES/CHANGELOG；README、MANIFEST/source_manifest.py仅包装/阶段摘要。root任务加入分发清单。
如实测确需core/service既有索引/查询优化，先记录复现及具体文件后在此追加，不改制造几何/Writer、schema、公有API、依赖或安全限制；扩大架构需先报告。

## 执行闭环

1. 基线/固定样本与协议先冻结；真实60Hz同屏模式、1600×900physical canvas、release、10秒预热、60秒轨迹3独立run，所有失败保留。
2. 新internal-only驱动使用既有egui RawInput/gesture/worker/service/production callback。Frame stamp+GPU fence定义为app帧及surface GPU完成保守上界，不称物理扫描/人工输入；导航录像及surface截图保存本体。
3. CPU查询独立起止；input接收→排队→worker→消费→高亮绘制分段；全序列ID独立预期，无D2候选替代、无截断。idle采样驱动不自发request_repaint，外部60秒计时/CPU采样，后台完成唤醒。
4. 20轮生命周期以同一进程每轮固定加载、3级LOD、返回Fit、关闭后静置5秒/RSS/真实GPU Buffer长度统计；第5→20<=100MiB、峰值<=1GiB/GPU<=512MiB。指标不是显示拒绝开关。
5. 保护覆盖、有限组Move/Undo/Redo/Export/Reopen、D1/D2回归；适用完整门禁，逐项ignored分类；冻结diff送独立复审。
6. 源码复审放行后最终Mac证据/授权阶段commit，再新clean四件套/fresh-extract实际editor-app身份与独立包复核；停在S5-M1，未通过前不commit为PASS或伪造package成绩。

Windows/人工IME/跨显示器/签名/CORE10/PMIX/PPOL/PSTRESS/千对象拖动均不在本轮；无当前真实观察就记录待验/阻塞。

## 首轮实测驱动的实现补充（2026-10-01 UTC，未验收）

允许 modal.rs 仅补 EditorApp 测试构造器的 internal-only 字段。首轮60秒导航全场景100000对象，计数仍有6次完整场景重建：f64视口查询已含所有制造对象时，覆盖边界不应因平移失效；以查询是原快照有序子集这一不变量记录完整覆盖，只放宽该几何缓存失效判断，LOD/数值/制造限制不变。新增测试必须证明完整/部分视口、Clear、隐藏层与编辑后失效。

首次 point 驱动在换相机后提前注入点击，被正常 UI display-pending 保护拒绝；失败进程/截图原始数据保留，修正只等真实viewport准备完成，不绕过 usable 或直接设选择。首轮 Open/Close 的 discard 参数与该控制错误均不计产品性能失败。Metal GPU完整分配采集使用已锁定 wgpu27 HAL Metal raw_device 的只读 currentAllocatedSize，优先保守覆盖整个真实device，而不是把没有启用的计数器零值当成绩；无新增依赖。

闲置基线先通过正常 SaveProject 保存合成 `.rcam` 后清 project dirty，避免需要后台 recovery 的未保存工程充当“无任务闲置”；不关闭产品 recovery，不直接写 dirty。结果等待阶段不发起周期刷新，仅真实 worker request_repaint 交付结果后补一次 GPU 完成确认帧。完整小组三对象闭环之外，再在重开100k文件上执行单对象 Move/Undo，单独保留 CPU patch/GPU上传统计。

AT068最终驱动在等待后台结果的phase13不循环request_repaint；真实worker先唤醒将结果消费到GUI，仅在结果已消费后安排一次确认GPU完成的后续帧。记录worker-request-repaint实际调用计数。AT067另记录P100K单对象Move/Undo的缓存与上传，独立于三对象有界编辑往返。初轮20项门禁全部退出0，不将旧门禁当作后续驱动改动已验证。

## D1 原生回归协议适配（2026-10-01 UTC，编码前范围补充）

在新 S5 内部二进制上运行旧 D1 driver，未配准 Focus 正常触发 D2 临时候选查询，服务返回 REGISTRATION_REQUIRED；D1 phase8 将该预期错误当意外失败。git show 2cde953 证实同样代码已在基线中存在，非 S5 产品回归。允许 native_d1.rs 仅为该精确 phase/error 增加查询零修改、未配准/未选择断言后继续原配准路径；verify_s4d1_native.py 增加对应记录的独立零修改校验。其他错误仍失败，业务/UI/core/service、D1配准方向和候选语义不变，旧失败记录保留。外部 runner 首次遗漏 verifier --out 的参数错误也单独保留。

## 完整缓存数值重定位保护（送审前自查）

完整覆盖可以避免无必要的几何失效，但不能取消Scene::scalar现有0.1physical-pixel精度要求。发现极远Pan的原viewport边界曾隐式促使局部原点更新；完整覆盖后需显式用同一scalar校验决定数值rebase。main.rs新增私有viewport_requires_rebase，被原needs_lod共用；s5m1_tests新增近距离复用/极远平移/空视口新原点/返回完整场景及制造零修改回归。没有新数值阈值或f32制造回写。此前完整25门禁先跑完并作为修复前记录保存，之后受影响门禁重跑。

## S5-REV-01/02 REQUEST CHANGES remediation (2026-10-02; no commit)

Independent source review requested two B1 evidence-validator fixes only. Allowed: scripts/verify_s5m1_native.py and test_verify_s5m1_native.py, new scripts/check_s5m1_evidence_mutations.py for negative mutations of actual synthetic evidence, native_s5m1.rs only additive input/observed/acknowledgement monotonic timestamps, boundary frame IDs and GPU-completed/primary-in-scene observations; docs/manifest and frozen handoff. No renderer/UI/business/model/threshold/sample/trajectory changes. Point local scenes remain allowed, full marquee requires100k; navigation boundary is (navigation-start frame_id, navigation-end frame_id], with full input-to-next-input links and raw elapsed/interval consistency. Old missing-boundary/GPU-state protocol cannot be upgraded by inferring it from surviving frames; preserve old failures/results and collect new necessary traces. Source review remains REQUEST CHANGES until independent re-review.


### 最终归档身份清单补齐（2026-10-02；实测发现）

首个阶段提交2dff495的真实Source ZIP中，Python显式source manifest包含根任务RCam_S5M1_P100K_NATIVE_CLOSEOUT_NEXT_TASK.md，而Rust build_identity::source_paths固定根列表未同步；直接编译实际Rust检查器对真实解压副本返回source manifest coverage mismatch。仅允许补齐 editor-app/build_identity.rs 的相同根文件项，更新 tests/build_identity.rs 真实归档fixture/源文件变异回归及本段文档/manifest。不改变制造/渲染/输入/API/schema/样本/门槛/身份拒绝强度；无Git仍须完整逐文件校验，不能跳过该任务文档。此前partial clean门禁与失败Source ZIP保留，新提交后重做最终门禁及真实无Git editor-app构建/启动/变异恢复核对。此补充修改另送独立审查，阶段仍非PASS。


### 最终门禁中的并行测试临时目录碰撞（2026-10-02；编码前记录）

最终clean提交eea6d85的workspace --no-fail-fast真实退出101，product_service_compatibility中两项失败使用完全相同的PID/时钟纳秒目录；保留source.gbr实际内容为MI文件尾部拼接另一测试4321D03/M02尾部，确认并行fixture_import共写并截断同一文件。不是制造/兼容解析规则失败。仅允许该editor-service测试helper复用标准库AtomicU64进程内序号补齐路径唯一性，并用create_dir拒绝意外复用；所有源输入、断言、并行运行、样本与门槛保持不变。PLAN/REVIEW/manifest更新，原workspace失败及实际串行诊断日志保留；新提交后重跑完整门禁/原生/归档，另供独立审核。阶段仍非PASS。

诊断原二进制串行6/0/2；修复后原测试8线程仍6/0/2，fmtcheck及manifest退出0。日志保留于exports/S5M1_FINAL_CLEAN_20261002T035724Z/test-isolation-failure与test-isolation-focused；全门禁失败为772/2/47，不能引用旧774/0/47替代。产品源代码不变，只有测试夹具隔离改动。
