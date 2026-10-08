# S5-I1 局部验收补充

全部初始状态：未执行。正式 96AT 及 schema_version=2 保持。

| ID | 独立期望 / 关联 AT |
|---|---|
| I1-01 | 两个真实导入层框选→数值/拖动共同平移；原 LayerId、ID、顺序不变；一次 revision/Undo/Redo、选择保持。AT029–032/039/091 |
| I1-02 | 跨层共用 pivot 旋转/镜像，圆弧方向、矩形与 BlockInstance 解析几何，精确 Undo；不适用任一目标整组拒绝。AT033/034 |
| I1-03 | Duplicate 每层新ID/相邻曝光顺序、文字组独立；Delete/Undo/Redo 精确恢复；共享光圈/定义不变。AT035–040 |
| I1-04 | 后一层锁定、类别锁定、隐藏/Solo/不可选、缺失ID/层、重复ID/层、过期revision、无穷/NaN/超预算、部分文字组：模型/历史/dirty/选择零改变。AT022/031/039/042/090/091 |
| I1-05 | 循环跨两层部分/完全重叠及近邻对象，top-to-bottom逆曝光，绕回；稳定重复/重开；真实孔洞/凹口/斜线与容差边界。AT026–028 |
| I1-06 | 2physicalpx锚点边界，pan/zoom/viewport/DPI/candidate/order/visibility/revision/框选/其他选择 reset；文字逻辑组与Block单候选。AT024/025/028/030 |
| I1-07 | Ctrl加/Shift减/双键减优先、不推进；低层已选对象被覆盖仍拖完整选择；Probe不推进；4px拖动阈值/Esc/blur/PointerGone零误提交。AT032/043 |
| I1-08 | JSON实际接口与GUI同服务，未知字段拒绝、能力表、无窗口依赖、两层查询编辑导出重开。AT065–067/086–092 |
| I1-09 | A2取消/TaskVersion和M2B单层拖动回归、全部自动gates、真实Mac release Metal交互证据与候选身份。Windows未执行；不外推完整V1。 |

原生结果与具体日志在 versioned exports 另存；不得覆盖失败记录或通过降低期望集获得 PASS。

95b7a233 旧候选的原生固定协议共 119 步：保留前 46 步，并增加 72 次不同按住帧数的 Shift/Ctrl/普通点击及一个 camera 重置步骤。前 46 步包含 `.rcam` 保存/重开、两层分别 Export Gerber、另建空工作区并重新导入这两个实际导出文件；独立检查原始几何/光圈/曝光顺序及导入 provenance SHA256 与导出字节绑定。项目重开不替代 Gerber 重新导入。

独审整改追加：I1-10 结构化按钮/坐标/modifiers 与生产 Gesture/Action 的 world、mode、时序绑定；I1-11 完整连续 input/update/frame 区间及独立 paint 流水逐项对应；I1-12 真实落盘篡改并重算清单仍被整包拒绝；I1-13 同冻结二进制的 extra-motion/PointerGone/early-release 因果诊断，制造/历史/dirty零改变并保留旧失败不可追溯状态。关联 AT032/043/065–067/086–092；初始均未执行。

I1-14：自动恢复到期与用户按下同帧时，用户 ProbeDrag 优先；已建立 drag/Grip 时不得被恢复 busy打断。三轮119步矩阵第112步各强制到期，输入/Gesture/accepted-action及完整内容/历史契约均需验证。恢复服务本身保留原门槛。

第二轮独审整改的新固定协议为129步：保留原119步（第112步强制恢复到期与同帧按下），追加框选重置、普通点击建立候选、三次实际后台RecoveryWrite完成及后续普通推进/下层Shift移除/绕回。I1-15：恢复任务完成前后完整click_cycle、制造info/geometry/history/dirty/selection相同，TaskReceipt completed且六维input/result版本相同，实际恢复.rcam与metadata SHA绑定；恢复后普通/Shift点击必须仍按当前候选执行。关联I1-06/07与AT028/030/032/043。

I1-10/13补强：正常与诊断分支共用实际焦点/按钮/指针、screen/world、camera/canvas/ppp/navigation、Probe容差和CanvasSelect上下文校验。仅明确注入的foreign_move/PointerGone及early release确认次序有特殊差异；无关失焦、错误动作上下文/坐标或额外动作必须拒绝。整包落盘重清单负例新增三项独审反例及其余诊断边界，共28项。

I1-16 (near-contour follow-up, native not executed): check the full matrix in `S5_NEAR_CONTOUR_SELECTION.md`, including distance reversal/ties/direct and interleaved-text priority, true holes/arcs/Block/Clear, six DPI values, anchor/glyph/membership/radius identity, editable incoming Add/All with inspection Remove, atomic locked mixed Move, cancellation/error equivalence and80k immutable snapshot reuse. Existing I1-05/06/07 near-only ordering and incoming Add/All lock expectations are amended explicitly; original geometry, gesture and native acceptance thresholds remain.
