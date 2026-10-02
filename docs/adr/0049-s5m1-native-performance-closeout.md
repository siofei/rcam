# ADR0049 — S5-M1 existing renderer native performance closeout

2026-10-01；状态：采纳测量协议、实施/验收in progress。用户明确授权，基线INFRA2 2cde953。六项原AT Mac局部要求见任务/PLAN/addendum；不新增业务operation、API/schema、依赖或制造语义。

复用既有Resources/Callback/有序world-bins（设计中的ProductionRenderer职责）；保留reference renderer。新增internal-only S5证据驱动和有界观测，不改写旧D2 runner或伪称新renderer。导航/selection只走正常app输入/worker/service/callback；CPU、app帧、GPU完成与物理present各自区分。idle采用外部采样，证据驱动不得强制连续重绘来掩盖wake问题。

固定existing P100K；旧P10K circles只是历史补充，另补合格C/R/O/P sample，先冻结所有SHA、协议、点位和独立序列真值。60Hz临时同几何模式、真实physical画布、全部run失败/尾部保留。独立checker只计算已完整收集的原始数据，不接受缺帧/估算内存/未知绘制成功。

内存固定RSS1GiB/GPU512MiB、20−5差100MiB为验收指标，不是显示或query准入；D1放宽显示政策、ADR0046无限累计marquee work、ADR0045有序canonical哈希复用不回退；点选与制造安全限制不变。需要扩大架构/API/制造解释先报告并停止扩大部分。

源码冻结独立复审→最终原生→用户授权阶段commit→新clean四件套/无Git实际app构建身份→独立最终复核。Windows/双平台AT/V1/CORE10及后续S5-M2/CircuitCAM不在本轮。
