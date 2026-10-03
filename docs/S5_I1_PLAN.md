# S5-I1 多层基础编辑与点选循环任务书

状态：实施中；基线 b1635d10485ae61358e1e444f8f852f35c055b1e（M2B 已验收）。2026-10-03 用户明确优先要求；独立工作树 codex/s5-i1-multilayer-cycling。PMIX 未提交工作及原始失败证据在独立工作树和 exports/S5M2_C_intake_20261003 保留，后续使用新源码身份重跑受影响矩阵。

阶段 S5-I1（S0–S6 中 S5 性能/交互补正），Mac-first bounded。关联 R07/R08/R09/R10/R11/R14/R16/R17/R19/R21/R22；AT-022/026–043/065–067/086–092。96 个正式用例、schema_version=2、Windows/CORE10/P100K 完整门槛不变。

## 实际缺陷与允许修改模块

真实源码 select_rect 跨层收集，但 state::edit_targets、drag::editable_selection 拒绝不同 LayerId；服务 MoveParams/BatchParams 单层。topmost_hit 固定首个命中层最后对象，无循环状态。旧 cross_layer_selection_refuses_whole_edit 使用虚构层，不能证明真实多层成功。

允许 editor-core edit 原子历史、editor-service 自有 DTO/调用/能力/集成测试、editor-app 选择/手势/动作及相关 UI、定向测试与 internal-evidence 原生采集、公开合成 fixtures、脚本、阶段文档。不得改变 parser/writer/Gerber 填充、文件 schema、依赖、K1 延期状态或 PMIX 历史证据。

## 冻结实现范围

1. 跨可编辑层 Move（数值与拖动）、Rotate、Mirror、Duplicate、Delete 统一支持。一次调用、一次 revision、一次 Undo/Redo；LayerId、曝光顺序、非目标对象、共享定义保持。Duplicate 每层按原有相邻插入规则产生全局新 ID，不同文字组保持独立组身份；选中全部副本。Move/Rotate/Mirror 保留选择及主对象；Delete 清空被删除选择，历史沿用现有选择清理约定。
2. 新 objects.edit_selection：显式分层目标 groups、单个操作、expected_revision。全部层/ID/锁/可选/几何/完整文字组/总资源预算在提交前校验，任何一组失败时制造/历史/dirty/revision/选择整体不变。不得通过循环现有服务命令拼事务。核心保存所有层的精确 delta 并一次提交；Undo/Redo 在任何层修改前检查全部 delta。合计最多 10000 目标，既有历史/对象/Region 预算不变。
3. 隐藏、Solo 隐藏、不可选层/类别不可普通选择；锁定层/类别可查看选择，任何目标锁定则整体拒绝编辑（延续 ADR0019）。变换共有 world pivot，f64 mm、真实圆弧与 BlockInstance 语义保持。不适用变换整体结构化拒绝。
4. 本阶段不扩展 Alignment/Distribution、Array、Block 创建或尺寸 COW 的跨层语义；这些命令保留同层条件并明确整体拒绝。几何剪贴板保持既有单层来源/当前层粘贴契约，不能悄悄合并多层曝光。
5. 普通画布重复点击同处循环：真实 analytic hit，在所有可见可选层收集，按显示层从上到下、每层逆曝光顺序；Clear 与锁定对象沿用几何查看策略。完整文字组为一个逻辑候选，BlockInstance 为一个候选。首击最上层候选，后续取下一个，末尾回到首个。
6. 同处定义为距循环锚点 <=2 physical px、camera/viewport/DPI 完全相同且有序候选集合及文档/revision/workspace_revision 不变。没有时间超时。缩放、平移、视口变化、候选变化、空点、框选、其他选择命令、成功制造修改/Undo/Redo/工程切换重置。查询失败或取消不更改选择/循环状态。
7. Ctrl 加选、Shift 减选，二者并用减选优先。修饰点击不推进循环；同一上下文采用当前候选，否则采用首候选，随后重置循环；修饰按下不启动制造拖动。只有未越过4 physical px阈值的释放才推进循环。按下的 ProbeDrag 不推进；已循环选中的低层对象仍能拖动当前完整选择，不因上层覆盖切换目标。拖动/Esc/blur/PointerGone 不推进循环且无额外制造修改。
8. 单层旧接口和性能快路径保留。多层只 clone 目标几何/delta，不 clone 整个制造文档；保守失效缓存允许但必须测量，不能伪称已满足 PMIX。原生至少真实双层框选→拖动→Undo/Redo、循环/修饰/导航reset/锁层拒绝、旋转镜像复制删除和导出重开；固定 release 构建保留日志和操作录像。

## 流程与交付

先保存真实双层回归失败，再实现，定向测试后执行 fmt/check/clippy/test/release、automation_contract、headless_workflow、服务正常依赖树。检查 A2 取消/TaskVersion 与 M2B 拖动回归，真实原生 Metal。记录所有失败与未执行项。候选源码/证据 manifest 独立冻结并交父任务审查；缺陷继续修，审查通过才阶段 commit（不 push），干净构建、同 commit Source/Evidence 包、fresh extraction 复核。当前不声称 PASS。
