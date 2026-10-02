# S5-M1 Mac local acceptance addendum（in progress）

原ACCEPTANCE_V1/acceptance_cases.json字节、schema2/96 cases/AT079退役/双平台required_platforms保持不变。本表仅六项macos-arm64局部结果；每项都须真实画面/交互可审阅记录，自动controlled driver不能代替物理人工输入。

| 局部项 | 冻结标准 | 当前结果 |
|---|---|---|
| M1-067 / AT067 | 导航零parse/零全量制造重建，选择只叠加；上传/分配/draw/可见量真实记录，保护coverage不丢孔不重排 | pre-review 本机检查通过；最终clean/独立复审待验 |
| M1-068 / AT068 | P10K无操作60s redraw/CPU；真实worker完成能wake，非永久刷新 | pre-review 本机检查通过；最终clean/独立复审待验 |
| M1-069 / AT069 | P100K20开关+LOD；RSS20−5<=100MiB、无持续线性增长，RSSpeak<=1GiB/GPU显式<=512MiB | pre-review 本机检查通过；最终clean/独立复审待验 |
| M1-071 / AT071 | P10K/P100K冷热各3次完整可操作画面<=3/10s，保留全部失败 | pre-review 本机检查通过；最终clean/独立复审待验 |
| M1-072 / AT072 | 1600×900physical/60Hz，warm10s+60s轨迹×3，每次p95<=33.3ms/p99<=66.7ms/max<=200ms；原生录像 | pre-review 本机检查通过；最终clean/独立复审待验 |
| M1-074 / AT074 | 固定200点CPU p95<=20ms/接收input到高亮p95<=100ms；100k全框<=300ms/完整曝光序ID，无cap/截断 | pre-review 本机检查通过；最终clean/独立复审待验 |

资源/制造回归：R05/09/10/11/14/16/17；局部组Move→Undo/Redo→Export→Reopen一次事务、源保护、writer/precision不变；D1配准/dirty-cache signed-zero和D2 candidate仍纯临时选择。数值/几何及point-query/import/export/事务限制保留。B0失败阻止生产输出；必需B1不算可忽略优化。门禁/ignored分类见PLAN与任务；所有结果另存run-id/schema2，不覆盖历史。当前已有原生性能实测，最终clean身份和独立源码/包审仍待完成，不标阶段PASS。

实际数值、口径及保留失败见 S5_M1_REVIEW。AT069两次完整20轮趋势限定评估与原始序列同时送审；原六项双平台总状态不改。

## S5-REV-01/02 证据整改口径（2026-10-02；待独立复审）

本补充不改原 AT、阈值、样本或轨迹。点选允许局部 scene；完成帧必须与本点正常输入前的 document/revision/scene/count 和完成 SelectionSet 一致，制造对象仍为100000、scene非空且含 primary。实际 callback paint 与成功 GPU fence、focus、1600×900、非 busy/pending、无 blocked/display_error/error/ui_error 均须可查。框选完成帧还须完整100000 scene/selected objects，并与 marquee-start 场景及单独完整有序 ID 集合绑定。

导航冻结帧范围为 `(navigation-start.frame_id, navigation-end.frame_id]`。start 记录包含进入60秒阶段的 phase_origin_ns、该输入 frame/time；end 是 input 时间首次跨越60秒的正常导航帧。每帧记录同一 Instant 起点的 input/observed/next-input acknowledgement 纳秒。必须首尾边界匹配、数量为 end−start、逐帧连续、上一ack等于下一input、input≤observed≤ack；interval 与 input 时间差相符，elapsed 与阶段 origin 相符。首段和全部间隔仍受200ms停顿门槛；nearest-rank 使用全部区间间隔，包括 start-input→首导航input。不以任意最小帧数证明覆盖。缺旧字段的历史记录不得推断补齐或迁移成新协议通过，原成绩和失败独立保留。

导航只按有效显示检查 busy 期间的每帧：后台 RecoveryWrite 可使通用 busy=true，但不阻止 pan/zoom，也不改变 scene。该标志不代表显示未完成；display_pending、blocked/error、完整100k、focus/paint、同文档/revision/scene 与时钟/性能检查仍逐帧强制。选择完成继续要求非 busy。本轮首个原生导航的39个 recovery busy 帧有39个不同相机样本，全部完整有效绘制，原误拒绝日志保留。
