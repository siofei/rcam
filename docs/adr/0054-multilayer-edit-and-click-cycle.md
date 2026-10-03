# ADR 0054：跨层基础编辑与稳定点选循环

日期：2026-10-03。状态：Accepted for implementation，未验收。

用户明确要求跨层框选后的同时移动等基础编辑与密集对象循环点选。以 [S5-I1 任务书](../S5_I1_PLAN.md) 8 条冻结契约为决策。该决策替代 ADR0019 中“跨层仅供查看”的基础 Move/Rotate/Mirror/Duplicate/Delete 限制；其几何选择、锁定可查看、Ctrl加/Shift减及制造事务约束继续有效。尺寸/Array/Alignment/Block/剪贴板不自动扩展跨层。

新增服务操作 objects.edit_selection，以全量预校验的跨层精确 delta 历史实现，禁止逐层调用多个服务修改。点选循环是 GUI 瞬态，来源仅为服务 analytic hit；不持久化、不产生制造 revision。按下探测与释放选择分离，拖动保留当前选择。完整细节及 reset/修饰规则见任务书，验收见 S5_I1_ACCEPTANCE_ADDENDUM。

代价：跨层事务可能保守失效全局内容缓存；不降低资源门槛，不把旧 M2B/PMIX 原生证据归到新源码。无需新增第三方依赖。
