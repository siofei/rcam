# S5-I1 独立审查整改（2026-10-03）

状态：整改中，P1-03 历史原因仍未确定；不批准 commit，不代表阶段 PASS。阶段 S5-I1；R07/R08/R09/R10/R11/R14/R16/R17/R19/R21/R22；AT-022/026–043/065–067/086–092。原任务允许模块之外，gpu.rs 只新增 internal-evidence paint 观测钩子，不改变生产渲染或制造路径。新的交互需求另立 I2 规划，不混入本整改。

## P1-01 输入/动作契约

原生记录从 Debug 字符串改为结构化 PointerMoved/PointerButton/Key/Focus/PointerGone；记录注入前平台输入、焦点、手势、Grip 和请求序号，及 egui 实际消费后的事件/指针状态。生产 send 成功入队后独立记录 ProbeDrag、CanvasSelect 的 point/world/mode/camera/canvas/ppp、DragMove delta 与 Box bounds。

Verifier 按每步 world 输入重算 screen，绑定按钮、按下/释放、modifiers、pointer flags、gesture start/last/mode、4 physical px 门槛、动作序号、release→selection→完成帧。Shift/Ctrl 不得进入 Grip。I1 tick 移到本帧快捷键路由之后，确保状态中的 sequence 与该帧被接受动作一致；逐帧检查 input_before.sequence、动作连续序号及 UI sequence。此前记录早于快捷键路由的观察仅留作诊断参考，不绑定最终冻结。点击期间有轮廓外/非协议位移不得通过；不能仅凭 step.input 的协议副本宣称真实输入正确。

## P1-02 原始帧完整性

独立 capture-ledger 由 raw input hook、UI update、GPU paint callback 和生产 send 各自记流水；不是从 observations.frames 反推。捕获区间明确为 1..capture_end，结束时当前 terminal_pending_frame 未绘制且明确排除。input/update 全覆盖区间并各含一个终止帧；paint_ids/count 与已完成每帧逐一匹配；任何首/中/尾缺帧、仅保留事件帧、重复帧、缺输入/更新/paint 均拒绝。初始化 frame=0 的唯一 NewWorkspace 为明示启动动作，不冒充捕获区间的原生输入。

无画布首帧可以没有 paint；有画布最终步骤必须有对应 GPU 完成帧和正确操作后 state。Ledger 与逐帧文件不同生产位置，但都不是防恶意整体伪造的硬件签名。本契约拒绝相互矛盾或不完整的数据，不声称能验证攻击者重写整个源码和全部观测。

真实落盘负例由 test_verify_i1_package.py 在隔离副本执行，修改后重新计算 EVIDENCE_MANIFEST 及帧摘要，调用整包 verifier；不能只使用 mock read 来证明修复。最终 Evidence 另含三种因果诊断的原始记录和独立 verify_i1_diagnostic 校验，全部绑定同一个冻结 source/binary。

## P1-03 历史 Shift 异常：已缩小范围，未证实唯一原因

原 native-baseline-01、源码 ZIP、步骤/日志继续保留。历史第6步 frame935发 Probe，936确认，937释放但无后续 busy/action；第7/8步仍发请求。根据这些记录可以把问题缩小到 Shift Gesture 的释放/取消路径，但旧包没有 raw events/gesture/video，无法判定具体触发条件。

本轮同一新 internal release binary 受控诊断：

- foreign_move：在 Probe 回复前插入 +4 logical px 的 PointerMoved；DPI=2，超过4 physical px 阈值。frame中先见 Remove/moved=false，再见 Remove/moved=true，回原点释放无 CanvasSelect；其后 Ctrl 与普通点击均保持原选择，与历史表象相符。生产行为符合修饰拖动不误编辑的契约，属于“输入污染可解释这一表象”的确定性证明，**不是当时确有系统位移的证明**。
- pointer_gone：在同一时段插入 PointerGone，Gesture 被安全取消，无 CanvasSelect；也产生相同表象，因此不能唯一历史归因。
- early_release：在 UI 还未确认 Probe 的输入帧注入 release；随后发出准确 Remove CanvasSelect，选区为空。该次次序没有复现产品缺陷；不由此声称覆盖所有系统调度。
- 增加真实 Model/Gesture 回归：中途位移后回原点不可清除 moved latch；释放先于确认的0–7帧延迟保留 mode 并执行正确选择动作。
- 正式新矩阵在46个原步骤之后增加72次 Shift/Ctrl/普通点击及0–4帧不同按住长度，保留前46步的实际导出文件再导入核对，共119步。所有步骤仍需完整真实帧和动作契约。

**P1-03 未自动关闭。** 无证据可以在两个已证明可产生相同表象的原因中选择一个当作历史事实。本阶段以保留未知历史失败、冻结二进制的定向诊断与更严格新证据提出受限风险关闭建议，交独立审查者判断是否足够。后续成功次数不能替代该裁决。没有为使旧错误通过而改变产品选择语义，也没有滤除平台原始事件。

## 菜单闪烁调查边界

用户追加问题抵达时本代理未运行GUI或性能驱动。之后运行的是独占 I1 双层四 Flash 功能采样，不是 PMIX/P100K。源码检查：GPU callback 使用 viewport/clip 交集 scissor；egui-wgpu 0.33.3 在 callback 后重设后续渲染状态；callback 没有全窗口 Clear。首轮70秒录像的静态顶级菜单ROI差分和相邻帧核对尚未确认可见闪烁。初步脚本漏读浮点科学计数法指数的问题已修正，原记录及修正说明保留。该小样本不覆盖用户的性能场景，问题不标修复或 PASS；后续统一交互任务继续调查。若复现生产B1，须单独修复并重审受影响矩阵。

## 收口限制

本整改只提交新候选供复审，不 commit/push。验证命令、失败、原始资料、三轮新二进制采样、输入/paint完整性、真实落盘负例均需按新 manifest 绑定。Windows/K1/PMIX/完整V1/CORE10/P100K状态不变。任何未经执行的测试不写通过。

## 新冻结前发现并修复的恢复写入竞争

review-fix-06 第二轮119步压力矩阵在第112步原生 frame7463 超时：raw press 正确、focused=true，但同帧 accepted action 为 RecoveryWrite，busy=true、Gesture=None，之后没有 ProbeDrag，直到180s native timeout。进程退出0不代表功能通过；verifier已拒绝。第一轮成功与此失败均保留在 native-review-fix-06，不用于最终矩阵。

真实原因是 tick_recovery 在画布输入路由之前调度，检查旧 busy=false 后抢占 worker。修复将 idle recovery 放到完整 UI/快捷键输入路由之后，并在 drag/Grip 生命周期中继续延后恢复写入；空闲时仍照常写入，不关闭恢复功能。增加 overdue recovery 与 pending/held Gesture 的队列回归；最终每轮压力测试第112步强制 overdue timing（internal-evidence only），记录 recovery_collision_due，verifier要求同帧仅接受 ProbeDrag并建立 Gesture。缺该竞争见证的重新manifest负例也必须拒绝。该修复改变生产调度顺序，须重新全 gates 与三轮 release native，不沿用7229b28d冻结成绩。

追加独占既有 M2B 场景调查：冻结7229b28d internal binary，P100K circles选5000对象，10s warmup+10s move，驱动/20s应用窗口录像exit0。固定菜单ROI [380,148,700,34] 共1108对解码帧；窗口存活区间平均灰度差分最大1.40773/255，18.416667s的219.787对应窗口关闭后黑帧，截图确认并单列。未确认用户所报可见菜单闪烁；不写修复或PMIX通过。录像、PID19164、窗口2859、原始差分和帧图保存在 intake/review-fix-01/flicker-probe-06。PNG查看工具曾transport closed，改为本地ffmpeg缩小后直接图像查看；不影响原始MOV。此调查绑定修复恢复竞争前的源码，仅作历史调查，不冒充最终冻结矩阵。

## 95b7a233复审后的有限整改（P1-01D / 当前Shift产品B1）

独立复审REQUEST CHANGES，报告及原反例保存在exports/S5I1_intake_20261003/review-fix-02。P1-02已达到受限关闭标准，不代表整体PASS。当前执行器rustc1.89.0可正常运行，不需要安装或更换工具链；审查者缺compiler导致其动态测试未执行的限制不外推到本机。

原样加入审查者两项Model.run_task(RecoveryWrite)真实回归，在生产修复前实际运行，2failed/0passed、exit101：恢复完成后cycle_present=false，普通点击重选首候选；下层Shift仍保留选择。这是当前可执行路径，不归因为历史baseline-01。失败日志与原回归保留。修复仅让RecoveryWrite成功收尾不清空cycle；仍通过原TaskContext/ApplicationService正常写入，不关闭恢复，不延长恢复间隔，不改循环语义。回归核对制造info、layer内容、selection、history/dirty不变，再核对普通推进和下层Shift移除。

新原生129步协议保留原119与第112同帧竞争，追加三次真实后台恢复完成、普通前进/下层Shift减选/绕回。internal-evidence只读捕获完整cycle、navigation_epoch及TaskReceipt；可移植门禁检查cycle前后相同、completed receipt与六维TaskVersion相同、实际恢复bytes/metadata hash。新增steps仍需完整input/update/paint、截图与录像；stress录像90秒、应用至少96秒留出完整录制（非stress仍70秒/76秒），不改变产品或性能门槛。

P1-01D：正式与诊断共用button/focus、screen/world、camera/canvas/ppp/navigation与Probe容差/selection action context。诊断只允许指定foreign move/PointerGone，不接受其它焦点取消、错误coords/context或额外生产动作。原三项focus/accepted point错误接受加到整包真实落盘重清单回归，并覆盖PointerGone focus、camera/canvas/ppp/navigation、Probe world及按钮修饰，共28项。只更新实际改变文件的manifest hashes；完整package verifier仍逐项校验全部覆盖文件，并非mock read或跳过hash。

旧95候选、历史失败、主dirty目录、暂停PMIX与基点/UX规划不覆盖。外接盘空间受限，新完整未压缩证据放内部/tmp，外接盘保留新portable ZIP及报告。只回收本任务已经结束且与完整外接归档逐文件SHA256相同的/tmp runner重复副本，保留核验记录；不清理历史证据、旧候选或构建target。历史Shift唯一原因继续not attributable，菜单闪烁OPEN属后续I2，不无穷追溯。修复后须新source/gates/native/28负例/fresh全部真实执行并再次独审；不commit/push，不自封PASS。

751a8eb8的首次129步原生完成后，整步verifier在step120失败；逐步原始state显示产品已正确执行三次恢复、普通前进/下层Shift移除/绕回。原因是verifier自身对step119框选保留了旧expected cycle，与真实框选重置契约冲突。补充真实state.click_cycle必须None的框选断言并重置预期cycle；该次原始记录重新用于诊断检查通过，绝不冒充新source/binary正式轮次。精确751源码580文件原hash逐项核验后留存FAILED_SOURCE_751a8eb8.zip、原gates09和原native/失败日志；随后再次冻结新身份和完整gates/native/28负例/fresh。
