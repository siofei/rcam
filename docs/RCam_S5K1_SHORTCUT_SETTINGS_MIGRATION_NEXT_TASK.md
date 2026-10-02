# 下一轮 RCam 任务 S5 K1 快捷键设置与迁移 v1

> 执行顺序：完成现状分析与本任务书，再修改产品代码。
>
> 用户已授权任务书完成后直接交给实现模型，不需要等待用户审查任务书。
>
> 当前功能基线：S5-M1 PASS（Mac-first bounded）。本任务优先于 S5-M2。Windows 原生验收继续 deferred，CircuitCAM 相关工作继续排在最后。
>
> 本任务是应用级快捷键设置与迁移，不是重做 S4-D2，也不改变已有制造操作的业务含义。

## 1 任务目标

用户原始需求：

> 需要把快捷键绑定增加到设置里，需要能保存到单独的文件／或者实现导出、导入快捷键文件的方式方便迁移。

本轮交付一个闭环：

```text
打开设置 → 查看真实生效的快捷键 → 录制并确认修改
→ 自动保存独立快捷键文件 → 立即使用新快捷键
→ 导出文件 → 在独立干净配置中导入 → 重启后仍正确
→ 可以恢复单个命令或全部默认值
```

用户不需要手工编辑 JSON，也不需要复制整个 preferences.json 或 .rcam 项目。

文件格式、冲突检查、运行时执行、菜单提示必须来自同一份有效配置。不能只做一个可编辑但未真正接入 GUI 的设置页。

## 2 基线和执行位置

本次只读调查对应：

```text
base_commit = 0f3ec046640c4293b53e402305ad3e7f47000929
branch = codex/s5-shortcut-settings
worktree = /Volumes/外接硬盘/rcam/.worktrees/s5-shortcut-settings
```

调查时产品代码 tracked diff 为 0，仅存在本轮未跟踪规划草稿。开始实施前重新记录 HEAD、branch、git status、未跟踪文件清单；核对这些草稿，不覆盖或删除未知内容。

若 HEAD 已变化，先比较变化是否影响 command、preferences、输入处理、设置 UI 或打包；重新审查受影响条款后再继续，不能把旧调查视为新代码事实。

每个实施阶段需要本地 Git commit、独立审查和对应验证。全程不 push，不替用户创建发布或合并动作。

阶段归属为 S5 的应用偏好子任务，关联 R09/R10/R12/R18/R19/R20。重点覆盖 AT-043 文本焦点安全与 AT-078 平台快捷键／IME 的 Mac 部分，并保留 AT-082/083/084 的隐私、证据真实性及能力边界要求。不得因此把双平台 AT-078 或全部 96 个用例标为 PASS。

## 3 参考文件的用法

已完整阅读用户提供的 `RCam_S4D2_REFDES_STENCIL_CANDIDATES_NEXT_TASK(1).md`，1868 行、20499 bytes。

复用它的任务深度：

- 先说明目标与禁止越界事项
- 明确数据、调用、状态不变量及资源界限
- 给出正例、反例、原生操作和证据要求
- 从最终 clean commit 验证 fresh-extract 包
- 只有全部门禁通过才 PASS，并在完成后停止

不继承其中 D2 的接口名称、ADR 编号、文件名、阶段编号或制造候选功能。

## 4 已确认的代码事实

以下是当前基线的只读调查结果，必须进入仓库内的分析记录并由实施者核对具体符号和行号。

| 位置 | 已有事实 | 本轮影响 |
| --- | --- | --- |
| `editor-core/src/command.rs` | 已有 42 个稳定 `CommandId`、`standard_commands`、`Key`、逻辑修饰键、`Platform`、`Keymap`、`Resolver`、`Dispatcher` | 在现有体系上扩展，不重造另一套命令 ID 或快捷键引擎 |
| 当前 `Keymap` override | 按 `(key, context)` 替换；不能直接表示“给同一命令换键”；冲突可能被后写覆盖掩盖 | 必须先构造完整候选配置再查冲突，并支持按命令替换全部旧绑定 |
| `main.rs:1254–1326` | GUI 仍硬编码 Primary+N/O/I/W/S、Primary+Shift+S/E/Z、Primary+Z/Y/D、Delete/Backspace、F/F3 | 设置必须替换真实执行路径；旧硬编码分支不可继续旁路触发 |
| `project_ui::file_shortcut` 和 F3 路径 | 每次构造 `Keymap::standard()` | 必须使用同一有效快照，不能绕过用户配置 |
| registry 与 GUI | V/M/T/G/ShiftF/PrimaryShiftN 等注册默认项尚未接入 GUI | 默认值不能机械等同 registry；必须明确兼容策略，防止本轮意外启用新热键 |
| 当前 `Dispatcher` | 仅覆盖 Array/Block/Arrangement/Grip/Snap，其他菜单使用专门函数 | 需要 GUI 命令适配入口，继续调用已有 handler 和 enabled 检查 |
| `preferences.rs` | 有未使用的 `BTreeMap<String,String> shortcut_overrides` 占位字段 | 不能把占位字段当作已支持的迁移格式或运行时功能 |
| 偏好文件 | `HOME/Library/Application Support/RCam/preferences.json`；现存写入上限 64 KiB，固定 `.tmp` 加 rename | 新快捷键文件独立；不直接照搬为跨平台原子替换方案 |
| `command_widgets.rs` | 快捷键提示硬编码 Mac `⌘` | 提示需来自有效 keymap 与平台格式化 |
| `main` 的 `raw_input_hook` | 已保留事件时文本焦点和 IME 信息；通用 guard 未传 `ime_event/ime_active`；Escape 分支未检查 text_focus | 必须统一把真实事件状态交给路由，录制优先于旧分发 |
| `platform.rs` | macOS `NSOpenPanel/NSSavePanel`；Windows picker 未实现 | 复用 Mac picker，加 JSON 类型与错误结果；不宣称 Windows 导入导出已原生验证 |
| 配置及对话框错误 | 部分错误目前静默回默认或取消 | 新功能必须区分未存在、损坏、版本不支持、读取失败与用户取消 |
| INFRA2 包规则 | Python/Rust 已全量收录 docs；根目录仍有白名单约束 | 本任务文档放 docs，不随意新增根文件造成包清单不一致 |

原文逐项核对确认 registry 有 42 个 ID；本轮必须接通并覆盖 41 个可配置命令，`grip.cancel`／Esc 保持固定安全交互。`file.new` 没有独立 GUI 入口，与 `file.new_project` 共用现有 `new_workspace`，适配中必须显式映射。两个命令 ID 仍分别稳定，不按名称合并。

## 5 实施前必须冻结的命令清单

在写运行时代码前生成机器可核对的清单，至少包含：

```text
稳定 CommandId
中文或现有显示名称
分类
原 registry 默认绑定
当前已发布 GUI 实际生效绑定
上下文和焦点条件
是否有菜单或按钮入口
现有 handler 和 enabled 条件
是否允许重绑定
v1 默认绑定
差异原因
对应测试
```

必须同时扫描 registry、主事件循环、`project_ui::file_shortcut`、F3、菜单提示、工具专属快捷键。不能只 `grep CommandId` 后声称“已覆盖全部快捷键”。

冻结原则：

1. 当前实际生效的应用快捷键全部纳入本轮覆盖或明确列为固定交互键。
2. 现有同一命令的别名全部保留：Delete/Backspace 和 Redo 的 Primary+Y/Primary+Shift+Z。
3. registry 中尚未实际生效的默认键不因采用 `Keymap::standard()` 而自动启用。
4. 已有安全 handler 的命令可纳入可配置目录；若此前无 GUI 默认键，默认保持未绑定，允许用户主动分配。
5. 附录 A 已确认 41 项可配置命令均可复用既有动作（包括 file.new 同义映射），本轮必须接通。若实施核对发现 handler 或 enabled 语义与调查不符，记录具体差异并修订规划；不能把该项悄悄移出范围或伪装成可用。
6. 如必须改变一个原有默认键或启用一个原未启用的默认键，先在分析文档写明原因、影响与测试，交独立审查；不得作为实现细节隐藏。

命令清单、默认快照和保留键矩阵是本轮的基准测试数据。名称可以变化，稳定 ID 不能随翻译或排序变化。

## 6 本轮明确不做

```text
Command Palette
多步 chord，例如 Ctrl+K 再 Ctrl+C
宏、脚本、任意 shell 或 URL 执行
全局系统热键或后台键盘监听
云同步、账号同步、多个可切换配置方案
每项目或每文档的快捷键
改写 .rcam schema
重新设计制造 Command / Undo 体系
自动开始 S5-M2
Windows 原生 PASS
CircuitCAM 后续工作
```

这里的“组合键”仅指同时按下的修饰键加一个主键。配置文件不能携带可执行内容。

## 7 冻结的产品决策

本轮采用下列冻结决策，不再把核心行为留给实现者临时选择。此前本地草稿中的稀疏 overrides 格式与全局 draft/apply 文案，由本任务的完整快照和单项确认保存语义取代；统一四份本地规划文件后再实施。

- 设置内新增快捷键区域，可搜索、查看、录制、清除、恢复单项默认、恢复全部默认、导入、导出
- 单次有效编辑确认后自动持久化，不额外要求用户保存整个设置页
- 一个命令可保留多个单次组合键，以兼容现有别名；上限为 4 个，实际默认数量须由清单验证
- 导出当前有效完整快照，包括明确未绑定的已知命令
- 导入采用整套替换语义，不做隐式 merge；文件缺少的本版本命令使用本版本默认值，并在预览列出
- 未知命令、未知键、冲突、超限或不支持的 schema 整份拒绝；不静默丢弃未知条目后部分成功
- 缺文件正常使用默认；损坏或不可读取时使用默认保证程序可用，同时明确告警并保护原文件
- 设置、导入、导出、恢复默认本身不得改变制造数据或项目状态

“自动保存”从用户点击确认分配、清除或恢复按钮开始。录制中的临时候选键不保存、不生效。v1 不另设全局未应用草稿；交换两个已有键采用“清除第一项 → 改第二项 → 改第一项”的安全三步，或准备完整文件后一次导入，不另加批量交换编辑器。

## 8 稳定命令 ID

持久化使用现有 `CommandId` 的稳定机器表示。

禁止把以下内容当 ID：

```text
菜单显示文案
中文名称
枚举内存序号
列表 index
Debug 输出
函数地址
平台 scan code
```

如果现有 `CommandId` 尚无稳定序列化，新增显式映射并以测试锁定；采用现有 ID 命名习惯，不为了本任务重命名全部命令。

命令重命名需要显式、受测试的旧 ID 迁移表；不能根据显示名称模糊匹配。命令 scope、危险性、enabled 条件来自程序目录，不由导入文件指定。

## 9 按命令替换与完整预检

不能继续把用户编辑实现为：

```text
旧 keymap + 单条 (key, context) override
```

这种做法可能留下旧键，也可能先覆盖冲突证据。

正确语义：

```text
复制当前配置描述
→ 删除目标命令的全部旧绑定
→ 写入该命令新的完整绑定列表
→ 规范化
→ 对完整候选配置检查冲突与资源界限
→ 编译可执行快照
→ 持久化并安装
```

清除绑定表示明确的空列表，不等于“缺字段所以恢复默认”。

修改、清除、恢复单项、恢复全部、导入必须走同一验证和提交路径；不能各自拼接 `Keymap`。

## 10 默认快照与版本变化

v1 默认快照必须由第 5 节的实际 GUI 兼容清单生成，不直接把未接入的 registry 默认值全部打开。

为每个已知可配置命令生成一项，包含默认绑定列表或空列表。

新版本新增命令时：

- 旧文件中已有命令保持其显式绑定，包括空列表
- 新增而文件中没有的命令尝试采用新版本默认
- 若新增默认与用户已有绑定冲突，保留用户绑定，将新增命令置为未绑定并给出可见提示
- 不得因新增命令的默认冲突导致已有全部自定义配置被丢弃
- 本轮没有正式历史 schema 时不虚构旧格式自动迁移

导入和启动补全缺失命令都要使用此规则并给出同样的摘要。由用户文件明确提供的两个绑定相互冲突则整份拒绝，不用上述“新增默认退让”处理掩盖错误。

## 11 作用域与冲突定义

复用现有 context 类型，建立“两个上下文是否可能同时响应同一次按键”的显式关系表。

当前 context 精确为 `Global`、`Canvas`、`Modal`、`TextInput`、`TextPlacement`、`ObjectEdit`；优先级为 TextInput > Modal > TextPlacement/ObjectEdit > Canvas > Global。保留此 resolver 优先级及 TextInput/Modal 阻断下层的安全语义。

同一个规范化组合键，在可同时激活的两个上下文中分配给不同命令，v1 配置层保守判为冲突；不能借优先级覆盖来隐藏用户配置冲突。

不能只比较 context 名称是否相等。应用级与画布级若可同时激活，也必须检查交集。

同一命令重复配置相同组合键也作为输入错误报告，避免导出不确定、UI 出现重复行。

互斥上下文之间的复用，仅在该互斥关系有测试证据时允许。不能靠“通常不会一起打开”判定。

错误提示必须指出冲突的命令名称、组合键及上下文；不得只显示“无效配置”。默认不提供“强行覆盖另一个命令”捷径，用户先显式清除冲突项。

## 12 平台修饰键与规范化

继续使用现有逻辑修饰键和 `Platform` 模型，实际枚举名称以源码为准。

必须区分：

- 跨平台主要操作修饰键，通常在 Mac 显示 Command、Windows 显示 Ctrl
- 物理 Control
- Shift
- Alt／Option
- 框架支持且本轮明确允许的其他修饰键

禁止把 Mac 的 Command 简单序列化为物理 Ctrl。禁止持久化 `⌘` 等显示字符后靠字符串替换实现迁移。

规范化要求：

```text
四个修饰键字段齐全且只能为 bool
重复 JSON 修饰键字段拒绝
主键使用明确的稳定 token
区分字母键与文字输入
重复的等价表示产生同一冲突键
平台不可表达的组合明确拒绝
```

v1 键集合冻结为现有 `Key`：Char 仅小写 ASCII 字母和数字，F(1) 至 F(24)，以及现有 Delete/Backspace/Enter/Escape/Tab/Space/四个方向键。特殊键是否可分配仍受第 13 节限制。录制字母规范化为小写；导入不规范 uppercase、非 ASCII Char、F(0)/F(25) 等明确拒绝。新增键类型需修订协议与测试，不能悄悄保存为 Unknown。

当前源码已确认四个逻辑字段为 `primary`、`secondary`、`shift`、`alt`：Mac Primary=Command、Secondary=Control；Windows Primary=Control、Secondary=Windows 系统键。本轮沿用这项语义，不新增“物理 Control”序列化字段假装它与 Secondary 在所有平台相同。跨平台导入含 Secondary 时必须提示其物理键变化。Windows 目标上的 Secondary 组合本轮拒绝，避免占用系统键；Mac 的 Control 则仍可按保留规则使用。目标平台归一化和保留检查之后再检查冲突。

同一文件跨平台的映射必须有纯函数测试。但 Windows 原生 picker、键盘行为及包未测试前，状态仍为 deferred。

## 13 保留键和固定交互键

本轮先从实际代码和目标平台行为生成一份分类表，再写验证器。

分类至少包括：

1. 系统占用或不能可靠接收的组合
2. 应用必须保留的关闭／取消／导航交互
3. 文本编辑或 IME 所需的键
4. 在画布上下文允许的普通字符或 Delete／Backspace
5. 用户可配置的应用命令

所有 Escape 组合均不可分配业务命令；`grip.cancel` 的未修饰 Esc 是只读安全项。未修饰 Enter/Tab/Space/方向键保留交互协议，不允许绑定普通命令。带修饰特殊键仅经明确保留规则后允许。

初始保留表包括 Mac Cmd+Q/H/M、Cmd+Space、Cmd+Tab、Cmd+Option+Esc、Ctrl+Space 和由系统占用的 Control+方向键；Windows 的任意 Secondary、Alt+F4/Tab/Space、Ctrl+Alt+Delete。实现前以目标 OS 官方说明或本机行为逐项核对，记录条件和证据，不为避开冲突而更改系统快捷键设置。F3/Fn/Fn-lock 的系统分配同样如实记录，不能将系统未送达的事件计为成功。

对已经存在的应用级标准组合，先核对是否属于正常命令；不要泛化为“所有系统风格快捷键都禁止”，从而破坏 Cmd+S 等现有操作。

已知系统保留组合若事件能够到达，应给出明确拒绝；事件被操作系统截获时，不得声称可以替用户抢占。不开启系统级监听或请求辅助功能权限来绕过这一限制。

## 14 焦点和 IME 门禁

当前 `raw_input_hook` 已有事件时焦点与 IME 信息，本轮必须把它们贯穿实际 resolver 路径。

禁止仅用当前帧末的“是否有 TextEdit 聚焦”推断事件发生时状态。

必须覆盖：

```text
文本输入焦点
IME composition 开始、进行、提交和取消
模态窗口
快捷键录制器
浮动工具窗口
应用失去焦点
忙碌或不允许执行的状态
菜单展开和文件对话框
```

文本输入时不触发画布普通字符键、删除制造对象或工具切换。IME 激活期间不将组合过程中的按键当作应用命令。

全局文件命令是否允许在文本输入时触发，应保持经清单确认的现有策略并单独测试；不能一刀切禁止所有键，也不能绕过全部文本保护。

Escape 的文本焦点缺口必须在本轮输入路径中修正，并证明编辑文本时不意外取消制造工具或其他操作。

## 15 录制器独占状态

推荐交互：点击“重新绑定”或“添加快捷键”进入局部录制状态，显示“请按组合键；Esc 取消”。

进入录制状态时：

- 显式获得录制焦点
- 先于普通命令路由接收键事件
- 不由旧的 Cmd+S、Delete、F3 等分支先消费
- 不使用普通文字输入事件反推出键值

只接收 `pressed && !repeat` 的一个主键；单独修饰键不构成绑定。

录制到候选键后只预览，不立即执行对应命令或保存。用户点击“确认分配”后才进入验证与提交。

未处于 IME composition 时，未修饰 Esc 先执行“取消录制”，不关闭整个设置窗口；控制键判定先于候选录制。窗口关闭、切换设置区域、失焦、模态窗口覆盖都必须安全退出或显式暂停。取消不能改变有效配置。

IME composition 期间不录制，不把输入法确认键当主键，显示可理解的提示。IME 提交／结束的同帧也不录制，待释放屏障后恢复。

| 录制与 IME 状态 | Esc | Enter | Tab |
| --- | --- | --- | --- |
| 录制中，IME 未激活 | 取消录制，保留旧绑定 | 未修饰键保留确认协议，不生成候选；候选已有时可确认录制预览但仍需分配按钮 | 移动焦点并安全结束录制，不生成候选 |
| 录制中，IME 激活或该帧有 composition 事件 | 交输入法取消 composition，录制暂停，不穿透取消工具 | 交输入法确认，不生成候选 | 由输入法／焦点处理，不生成候选 |
| 不在录制中 | 沿用已有模态／工具取消层级 | 沿用当前控件确认 | 沿用当前控件导航 |

任何时候都保留可点击“取消录制”按钮。带修饰 Enter/Tab 等只在 IME 未激活、非保留且平台支持时可录制。

## 16 录制结束后的残留事件

确认或取消后，不允许同一按键序列继续落入普通路由。

必须处理：

```text
仍按住主键
仍按住修饰键
自动 repeat
同一帧中剩余 pressed/released 事件
点击确认后下一帧仍收到旧事件
```

使用可测试的事件消费或释放屏障，不依靠任意延时来掩盖问题。

例：录制 Cmd+S 后确认，不能额外保存项目；录制 Delete 后取消，不能删除画布对象。

## 17 一次物理按键只执行一次

每个独立非 repeat press 最多解析并派发一个命令，按原始事件顺序处理。不可把“防重复”实现为无条件丢弃同帧第二个独立按键。

一旦某次派发进入制造 busy、打开模态、改变文档或导致焦点上下文失效，立即停止本帧后续应用命令派发；剩余事件不排成隐藏的延后执行队列。再次派发前重新核对事件时 owner 与当前上下文。toggle 类同帧防重复应绑定相同 press 身份或显式单帧规则，不能导致制造双事务。

不能由硬编码分支执行一次，又由有效 keymap 执行一次。不能为了容忍冲突而“两个都执行”。

key repeat 仅对源码清单明确允许重复的原有交互保留；文件新建、删除、复制、Undo/Redo、打开对话框默认按初始 press 执行一次。

对于按住按键连续动作，若当前版本确有这样的行为，单独记录规则，不以本任务顺手改掉。

## 18 单一 GUI 命令适配入口

沿用现有 `CommandId` 和 Dispatcher；在 GUI 层补齐必要适配，将键盘、菜单和按钮映射到同一现有业务 handler。

这里要求的是语义统一，不要求把所有业务强行迁入一个巨型 match 或改写 core Dispatcher。

统一入口必须保留：

```text
当前文档与选择条件
enabled / can_execute
忙碌与模态检查
锁定层或不可编辑对象限制
已有错误显示
已有 transaction / revision / Undo 规则
```

不得为快捷键新增一个绕过 Service 的制造编辑路径。

旧硬编码分支迁移后应删除或只作为受测试的固定交互路径保留。为所有残留键读取写明原因，静态审查与测试同时防止旁路回归。

## 19 提示与实际绑定一致

菜单、工具提示、设置行、快捷键帮助文案使用同一有效快照及平台格式化器。

修改后这些位置立即刷新；重启后显示一致。

已清除的命令不继续显示旧键。多个别名有稳定显示规则，例如主项加“另有 1 项”，详情可见全部。

移除 `command_widgets.rs` 中硬编码 `⌘` 的路径。显示格式不参与存储、解析或冲突判断。

## 20 设置页内容

每行至少显示：

```text
命令名称
分类
作用域
当前全部绑定
默认值
是否已修改
修改／清除／恢复默认
```

搜索可匹配名称、稳定 ID、分类或显示键，不创建新命令。

页面还需要：

```text
导入快捷键文件
导出快捷键文件
恢复全部默认
当前存储状态
错误或降级状态
```

优先复用现有设置布局、设计 token、按钮和错误提示风格，不重做整个设置界面。

键盘能到达所有控件，焦点可见；长命令名、长组合键和窗口缩放不得遮住确认、取消或错误内容。

## 21 自动保存反馈

确认编辑后状态应准确区分：

```text
正在保存
已保存并生效
保存失败，原绑定仍生效
已生效但持久性存在警告
```

最后一种只用于提交点之后发生无法保证持久性的真实异常，不可作为普通失败吞掉。

写入中不接受另一项会覆盖它的提交。可以继续浏览，但修改控件禁用，不排队额外写操作；不得让后一个保存悄悄覆盖先一个未知结果。

页面关闭前若有未确认录制或未提交候选，安全丢弃候选；若磁盘提交已开始，界面不能把它误报为“已取消”。

## 22 独立存储位置

macOS 新文件与已有 preferences.json 同目录，固定为：

```text
HOME/Library/Application Support/RCam/shortcuts.json
```

实际路径函数复用现有应用配置目录解析，不在多个模块复制 HOME 拼接。

文件不位于项目目录、不写入 .rcam、不加入项目 recovery。不得把用户绝对 HOME 路径带进导出内容或公开诊断。

`shortcuts.json` 及导入导出均使用本任务的完整快照策略，不与“稀疏 overrides 文件”混用。内存实现可生成已有 Keymap 所需差分，但持久化语义只有一套。

现有 `shortcut_overrides` 占位字段不能继续成为第二个写入来源。确认没有历史实际消费者后停止使用它，并保持旧 preferences.json 的兼容读取，不为此重写其他用户偏好。

若磁盘上已有非空旧占位内容，明确报告未支持的旧数据状态；只有证实存在历史正式格式和完整测试后才能做自动迁移，不能把任意字符串猜成键。

## 23 文件格式

采用 UTF-8 JSON，固定格式标识、整数 schema 版本和命令绑定列表。文件自身不包含 scope、handler、路径、脚本或可执行参数。

正式字段冻结如下，Key 使用已存在 core serde 的 externally tagged 表示，Modifiers 使用四个布尔字段；新增严格配置 DTO 不得默认忽略未知或重复字段。下面是合法的部分输入示例，正式导出必须枚举全部 41 个可配置命令：

```json
{
  "format": "rcam-shortcuts",
  "schema_version": 1,
  "modifier_encoding": "logical-v1",
  "source_platform": "macos",
  "bindings": [
    {
      "command_id": "edit.duplicate",
      "shortcuts": [
        {
          "key": { "Char": "d" },
          "modifiers": { "primary": true, "secondary": false, "shift": true, "alt": false }
        }
      ]
    },
    { "command_id": "view.fit", "shortcuts": [] }
  ]
}
```

`modifier_encoding` 必须为 `logical-v1`；`source_platform` 只允许 `macos` 或 `windows`，用于提示和预检，不授权改写逻辑修饰键语义。`{"F":3}` 表示 F3，`"Delete"` 等 unit variant 使用现有 serde 表示。所有字段类型严格、四个 modifier 字段必须完整；不允许 0/1 或字符串代替 bool。

固定安全项 `grip.cancel` 不进入文件；导入试图配置该已知但不可配置 ID，应作为 FixedCommand／保留交互错误整份拒绝。正式 golden fixture 使用全部真实可配置 ID。

采用“缺命令条目表示本版本默认，空列表表示用户明确取消绑定”。不同时增加另一套含义相近的 `disabled` 字段。

## 24 版本和未知内容策略

v1 必须清楚区分：

| 情形 | 行为 |
| --- | --- |
| `schema_version = 1` 且完整有效 | 可预览、可提交 |
| 缺版本、非整数、负数或版本 0 | 拒绝，不能猜格式 |
| 高于支持版本 | 拒绝并提示需要兼容版本，现配置不变 |
| 未知 `command_id` | 整份拒绝，列出有界错误摘要 |
| 本版本已知命令缺失 | 补默认并预览；新增默认冲突按第 10 节退让 |
| 未知 key 或 modifier token | 整份拒绝 |
| 多余未知字段 | v1 严格拒绝，避免拼写错误被静默忽略 |
| 同一命令多次出现 | 拒绝，不采用 last wins |
| 同一 JSON 对象重复字段 | 拒绝，不采用解析器默认覆盖 |
| `null`、注释、尾随非空内容 | 拒绝 |

应用版本变更不自动等于 schema 变更。仅在数据语义变化时升级 schema 并提供显式迁移测试。

v1 不提供“忽略未知命令继续导入”按钮。这是明确的保守兼容边界，应在帮助文案说明。

## 25 文件与解析资源界限

以下为本任务冻结的 v1 上限；若实际完整目录无法满足，必须先修改计划并说明规模依据，而非静默放大。

```text
MAX_SHORTCUT_FILE_BYTES = 64 KiB
MAX_COMMAND_ENTRIES = 当前可配置命令数量，本基线为 41
MAX_SHORTCUTS_PER_COMMAND = 4
MAX_COMMAND_ID_BYTES = 96
MAX_KEY_TOKEN_BYTES = 32，且必须符合上述具体枚举和 Char/F 范围
MODIFIER_FIELDS = primary / secondary / shift / alt，固定 4 个
MAX_JSON_DEPTH = 8
MAX_REPORTED_ERRORS = 20
```

读取最多上限加 1 byte 后判定超限，不先读取任意大文件到内存。选择的输入必须是可读取的普通文件，不能把管道、目录或设备当 JSON 长时间阻塞读取。

限制同时作用于原始字节、结构、字符串、条目和规范化后的组合；不能只做扩展名检查。

错误摘要可以截断，应用配置本身不能静默截断。错误要说明还有多少项未展示或已达到展示上限。

## 26 导出语义

导出只读取已提交的有效快照，不包含录制候选、保存中的未确定状态或未使用旧字段。

导出包含当前所有已知可配置命令，未绑定项明确写空列表。因此迁移后已知命令的行为不依赖另一机器的旧用户设置。

输出采用固定排序、稳定键 token、UTF-8、固定缩进和结尾换行。相同有效配置重复导出字节应相同，不加每次变化的时间戳。

导出不改变有效绑定、应用偏好、项目 dirty、制造 revision 或 Undo。

导出覆盖已存在文件时使用可靠原子写入；提交点前失败保留原目标。替换成功后的目录同步等异常按第 29 节返回“已提交但耐久性存在警告”，不能仍承诺旧目标未变。对话框取消是无操作，不显示错误。

禁止通过“导出”写到当前正在使用的快捷键配置目标来旁路安装流程；若选择同一实际目标，说明原因并要求换一个导出位置。

## 27 导入流程

```text
用户选择文件
→ 有界读取
→ 严格 parse 和 schema 检查
→ 命令／键／平台语义检查
→ 补全缺省并计算有效快照
→ 完整冲突检查
→ 编译候选快照
→ 显示差异预览
→ 用户点击应用导入
→ 持久化并安装整套配置
```

预览至少显示：变更命令数、清空绑定数、文件缺失而补默认的命令数、跨平台提示、因新增默认冲突而未绑定的命令。

明确写“将替换当前快捷键配置”。不把按钮写成会让人误解为 merge 的“添加配置”。

解析错误或冲突时不可点击应用。禁止边读边应用、遇到第一条有效就先安装。

导入只复制数据到应用固定配置位置，不把选中的外部文件设为长期依赖；迁移源文件不会被改写、删除或重命名。

## 28 预览与应用之间的一致性

为快捷键配置维护独立的 generation 或等价快照标识，不能借用 manufacturing revision。

导入预览绑定到：

```text
候选内容 hash
当前有效配置 generation
当前命令目录／默认策略版本
当前平台
```

预览期间有效配置变化时，旧预览必须重新验证或拒绝提交并提示刷新。不得把旧“无冲突”结论应用到新配置。

预览后的应用使用已读取并验证的同一份字节或候选对象；不能再次读取已可能被外部替换的文件却沿用原预览结论。

## 29 原子持久化与提交点

需要“文件原子替换”和“完整配置一次安装”，不许许诺文件与进程内存跨崩溃的分布式事务。

推荐实现顺序：

1. 完整验证候选配置并预先编译 runtime snapshot
2. 在目标目录创建唯一临时文件，避免固定 `.tmp` 相互踩写
3. 写完整字节，检查写入结果；flush 并按目标平台能力同步文件
4. 以可靠原子替换提交目标文件
5. 在确定提交结果后安装预编译快照，更新 generation；先保持处理中状态
6. 按平台保证执行目录同步等耐久性操作；结果确定后才显示最终成功或准确的提交后警告

不得先删除旧目标再 rename。不得直接 truncate 原文件后分段写入。

提交点之前失败：旧磁盘文件与旧 runtime 不变，候选保留供重试或取消。

原子替换成功之后：磁盘已经提交，后续异常不能再声称“什么也没改”。必须使运行时与已提交文件重新一致，并给出准确状态；禁止把新文件当失败残留再静默恢复旧内容。

Mac 原生原子替换、故障注入和强退出属于本轮硬门禁。Windows 仅完成适用共享模型／平台映射测试与隔离设计；真实文件系统 replace、picker、键盘和包的原生验证继续 deferred。不得以 Mac 测试或 mock 证明 Windows 原子性。现存 preferences 的固定 `.tmp + rename` 不能直接当作跨平台正确性证明。

## 30 并发写入和陈旧结果

同一进程只允许一个配置写入事务，runtime 快照切换在 GUI 明确消费结果时完成。

原生 picker 沿用当前 API 的 UI 线程要求；后台读取、校验或写入使用最小独立配置 worker/result，不混入制造 transaction，也不新造通用任务框架。不得每帧扫描磁盘、永久轮询或在每个 keydown 上写文件；结果到达时按现有机制请求 repaint。

对参与同一锁协议的 RCam 实例，使用可自动释放的文件锁或现有等价互斥机制，并在锁内核对旧文件 fingerprint；不同则拒绝提交并提示重新加载或重新导入，防止已提交更改被另一 RCam 实例无提示覆盖。

不参与锁协议的外部编辑器只能做尽力变更检测：写前发现文件变化就拒绝；最后检查之后仍存在竞态窗口。本轮不声称能阻止任意外部进程并发覆盖，也不把外部编辑作为支持的实时协作流程。

锁对象必须独立于被原子替换的 shortcuts.json 文件身份，避免只锁住旧 inode 后失去互斥。不要用崩溃后永远残留的 sentinel 文件充当锁；使用独立锁文件承载可自动释放的 OS 锁时，文件本身可残留，持锁状态不可残留。不能声称单次 hash 比较消除了跨进程 TOCTOU。

对于已经跨过磁盘提交点的 worker 结果，不能像普通过期查询一样直接 discard。必须完成一致性收敛后解除“正在保存”状态。

本轮不做后台热加载外部文件；重启或用户显式操作可重新加载。

## 31 首次启动和异常恢复

启动状态必须区分：

| 状态 | 运行时 | 磁盘处理和提示 |
| --- | --- | --- |
| 文件不存在 | 使用默认 | 正常状态；首次真正编辑时才创建 |
| 文件有效 | 使用有效配置 | 显示已加载 |
| JSON 损坏、超限或内部冲突 | 使用默认 | 明确警告，原文件保留 |
| schema 更新而不支持 | 使用默认 | 提示版本不支持，禁止自动覆盖 |
| 权限拒绝或其他读取失败 | 使用默认 | 提示无法读取；不误报为“未配置” |
| 临时文件残留 | 只认正式目标 | 不自动导入半成品或最新时间文件 |

程序不因坏快捷键文件无法启动或无法打开设置。

处于降级状态时，后续编辑不得悄悄覆盖被保护的坏文件或新版本文件。提供明确“使用默认重新建立配置”的恢复确认，或让用户先导出有效默认，再决定替换。恢复动作成功前保持告警。

日志和公开诊断不得包含用户完整配置路径、原始 JSON 或文件内容。

## 32 强退出与掉电边界

必须验证应用强制结束后的实际文件和重启行为，不以单元测试代替原生证据。

至少覆盖：

```text
提交前结束
写临时文件过程中结束
替换前结束
替换后、UI 确认前结束
正常显示已保存后结束
```

重启后只能得到完整旧配置或完整新配置；不能得到半份 JSON 或混合绑定。

在原生环境难以精确捕获中间窗口时，使用仅 internal-evidence 构建可用的受控 failpoint，公开构建不可保留可激活控制入口。

强退出证明不等于真实突然掉电证明；报告应准确写出验证到的边界和平台耐久性保证。

## 33 恢复默认

提供单命令恢复和全部恢复。

单命令恢复也要全配置查冲突：某个默认键可能已分配给其他命令。冲突时保持旧配置，显示具体占用者，不私自清空另一命令。

恢复全部默认前说明将移除自定义绑定；确认后作为一次完整配置事务提交，成功才刷新 UI。

恢复全部默认建议写入有效默认快照，不通过简单删除文件实现，以便沿用相同事务、错误处理与审计证据。

它不重置主题、窗口、单位、最近文件、其他 preferences 或项目内容。

## 34 状态不变量

以下操作自身必须保持：

```text
manufacturing revision unchanged
project dirty unchanged
Undo / Redo stacks unchanged
selection unchanged
camera unchanged
.rcam semantic content unchanged
其他 preferences unchanged
```

适用操作：打开设置、搜索、录制、取消、成功或失败的修改、清除、恢复默认、导入预览、导入提交、导出、失败恢复。

不要用“dirty 本来就是 true”掩盖写入，测试必须覆盖初始 clean 和初始 dirty，比较前后精确状态。Undo/Redo 需比较游标与记录数量或现有等价快照，不能只看按钮颜色。

“之后使用已绑定的制造命令”按该命令原有事务规则变化；新快捷键系统不得改变它本应有的 revision、dirty、Undo 语义。

## 35 .rcam 与应用偏好隔离

导出快捷键不得包含 Gerber、PnP、RefDes、坐标、客户路径、项目名、最近文件或其他偏好。

打开不同项目时绑定保持一致。保存项目、另存项目、恢复项目不带走或覆盖快捷键配置。

旧 .rcam 仍可打开，新 .rcam 无需 schema bump。快捷键改动不制造 project recovery 快照。

## 36 错误模型

错误至少可区分：

```text
UserCancelled
UnsupportedSchema
InvalidFormat
UnknownCommand
UnsupportedKey
ReservedShortcut
DuplicateEntry
Conflict
ResourceLimit
ReadFailed
WriteFailedBeforeCommit
ExternalConfigurationChanged
CommittedWithDurabilityWarning
```

名称可沿用现有错误体系，但语义不能合并为 silent default。

用户提示说明“哪里失败、当前哪些绑定仍生效、可做什么”。开发诊断可带错误码和匿名统计，不直接泄露完整路径或输入内容。

对同一启动错误只进行适量提示，不能每帧弹窗。错误状态在设置中持续可见。

## 37 性能与资源目标

按当前 42 个注册命令、41 个可配置命令规模，键盘路径只访问已编译内存快照，不读文件、不解析 JSON、不构造完整默认 keymap。

校验在一次编辑或导入时进行；结果完整有效后才发布。

测试应记录解析、校验、快照编译和磁盘提交耗时的分项，不以某个快盘结果声称任意磁盘都不阻塞。

至少覆盖正常完整配置、上限附近配置、超限文件、快速重复点击和慢 I/O。慢 I/O 时 UI 显示进行中且可响应非冲突操作，不允许连续提交积压无限队列。

## 38 诊断和隐私

如沿用现有诊断机制，事件只记录：

```text
操作类型
schema 版本
平台
命令与绑定数量
变更／补全／冲突数量
耗时分项
成功、取消、失败类别
是否使用默认回退
```

不记录完整按键流、用户输入文本、原始文件、客户数据或 HOME 路径。不能为该功能新增持续键盘遥测。

本轮不新增快捷键 Automation API，也不把配置导入暴露成绕过 GUI 校验的外部写入口。

## 39 自动化测试矩阵

每一项必须给出具体测试名称、对应源文件、结果和失败时状态快照。下表为最低覆盖，不可只在验收文档打勾。

| 编号 | 场景 | 必须证明 |
| --- | --- | --- |
| C01 | 目录 ID 唯一且可稳定序列化 | 编码不依赖显示名称或枚举顺序 |
| C02 | 默认快照 | 与实际 GUI 基线逐项一致 |
| C03 | 改键 A→B | B 生效，A 失效，其他命令不变 |
| C04 | 单命令多个别名 | 全部可执行且单次只执行一次 |
| C05 | 清除绑定 | 空列表持久化，重启不恢复旧默认 |
| C06 | 恢复单项 | 无冲突时成功，有冲突时完整保留旧状态 |
| C07 | 恢复全部 | 全部回默认，其他 preferences 不变 |
| C08 | 同 scope 同键两命令 | 完整拒绝，不 last wins |
| C09 | 交叠 scope 同键 | 完整拒绝 |
| C10 | 经过证明互斥的 scope | 可复用，正确路由 |
| C11 | 同命令重复快捷键 | 拒绝重复 |
| C12 | 修饰键乱序及重复 | 等价规范化、重复拒绝 |
| C13 | Primary 与物理 Control | Mac/Windows 语义区分 |
| C14 | 不支持或保留组合 | 清楚拒绝，旧状态不变 |
| C15 | enabled=false | 键盘与菜单都不执行 |
| C16 | 清除后菜单提示 | 不残留原提示 |
| C17 | 快速独立 press、重复事件和 repeat | 每事件至多一命令，状态变化后安全停派，不双制造事务 |
| C18 | registry 未接 GUI 的默认 | 不被静默启用 |
| F01 | 文本输入字母 F/V/M/T/G | 不执行画布命令 |
| F02 | 文本 Delete/Backspace | 不删除制造对象 |
| F03 | 文本 Escape | 保持文本与工具取消策略正确 |
| F04 | IME 开始／更新／提交／取消，录制 Esc/Enter/Tab 状态表 | 无伪命令、无伪录制，输入法控制优先 |
| F05 | Modal、浮动工具、失焦、忙碌 | 按既定门禁阻断 |
| F06 | 录制 Cmd+S／Delete／F3 | 只得到候选，不执行原命令 |
| F07 | 只按修饰键 | 不保存、不执行 |
| F08 | 按住录制键后确认或取消 | 无残留触发 |
| F09 | 录制中关闭窗口／切页／失焦 | 无残留 capture、旧配置不变 |
| J01 | 完整导出→导入 | 规范化快照相等 |
| J02 | 重复导出 | 相同配置字节相同 |
| J03 | 导入替换 | 不残留接收端旧自定义项 |
| J04 | 缺命令／空列表 | 默认补全与明确禁用语义不同 |
| J05 | 新增默认冲突 | 用户旧绑定保留，新命令未绑定且有提示 |
| J06 | 未知命令／键／修饰键 | 整份拒绝 |
| J07 | 重复命令／JSON 重复字段 | 整份拒绝 |
| J08 | 缺／旧／未来／非整数版本 | 按版本规则拒绝 |
| J09 | 非 UTF-8、截断、null、尾随数据 | 完整拒绝，不崩溃 |
| J10 | 超大文件、深嵌套、长 ID、多条目 | 有界拒绝，无静默截断 |
| J11 | 缺失字段／未知字段 | 不猜测，不忽略拼写错误 |
| J12 | 源文件含路径／脚本字段 | 拒绝，不访问或执行 |
| J13 | 预览后当前配置变化 | 陈旧预览不能提交 |
| J14 | 预览后源文件被外部修改 | 应用已预览内容，不能偷换 |
| P01 | 文件不存在 | 默认可用，正常状态 |
| P02 | 损坏／未来版本／读取失败 | 默认可用，告警，原文件未改 |
| P03 | 写入／flush／替换前故障 | runtime 与旧文件都不变 |
| P04 | 替换后故障 | 不谎称回滚，运行时收敛到已提交文件 |
| P05 | 强退出和临时残留 | 重启读取完整旧或新配置 |
| P06 | 第二 RCam 实例竞争及已发生的外部写入 | 协作锁内不丢更新；已检测外部变化拒绝；不夸大外部竞态保证 |
| P07 | 导出取消、提交前失败及提交后耐久性警告 | 前两者目标原文件不变；后者准确报告已提交；应用配置状态均不变 |
| P08 | 不可写目录／磁盘满／失效路径 | 明确失败、可恢复 |
| P09 | 非 ASCII 路径和文件名 | Mac 导入导出正常 |
| P10 | 降级后重新建立配置 | 明确确认，成功前保护旧文件 |
| N01 | clean 项目编辑配置 | revision/dirty/Undo/selection/camera 不变 |
| N02 | dirty 项目编辑配置 | 原状态精确保持，不清 dirty |
| N03 | 绑定后执行既有制造命令 | 与菜单相同事务、Undo/Redo 结果 |
| N04 | 保存／重开 .rcam | 快捷键独立、项目 schema 不变 |
| N05 | 与 preferences 分离 | 单位、主题、最近文件等不变 |

## 40 原生 Mac 验收准备

在隔离的测试配置目录或有明确备份／恢复程序的环境中验证，不覆盖用户真实快捷键和其他偏好。

应用必须来自本阶段明确的 clean commit；记录 Apple Silicon、macOS、键盘布局、IME、构建类型、二进制 SHA256。

至少有一个可复现的合成项目，含可编辑制造对象和足够的 Undo/Redo 情况。不得把客户 Gerber／PnP 放进公开证据包。

用户已允许开发、测试与验收时自动启动和操控 RCam 编译程序；不需要每次请求启动批准。但此授权不扩展到上传私有数据、push、改变系统权限或安全设置。

## 41 原生正常工作流

按次序完成并保存观察与截图：

1. 使用无快捷键文件的干净配置启动，默认键与原版本一致
2. 打开设置，搜索一个已实际可执行命令，检查名称、scope、默认及提示
3. 录制一个无冲突新键；录制期间原命令不执行
4. 确认分配；看到准确的保存成功状态
5. 新键执行一次，旧键不再执行；菜单提示已经改变
6. 关闭应用并重新启动，新键仍正确
7. 清除该命令绑定并重启，仍保持未绑定
8. 恢复单项默认，原默认键恢复
9. 修改至少三个不同类别命令并导出
10. 使用另一个干净配置启动，导入、检查预览、应用
11. 验证三个命令、清空项及别名均按导出快照生效，再重启复测
12. 恢复全部默认，其他偏好不变

不能把在同一个 runtime 中导出后立刻读回当成“跨机器迁移”充分证据。至少使用独立配置目录；跨另一台真实 Mac 若未执行应诚实注明。

## 42 原生危险边界工作流

必须真实操作：

- 编辑文本时按普通工具键和 Delete／Backspace，不改变制造对象
- 中文 IME composition 期间按 Enter／Escape／字母，不误触发命令
- 录制 Cmd+S、Delete 和 F3，原操作均不执行
- 在按键仍按住时确认／取消录制，随后不误触发
- 模态文件对话框、浮动工具、忙碌和失焦时不越权执行
- 导入两个命令冲突、未来版本、未知命令及截断 JSON，原配置完整保留
- 导入预览取消、文件对话框取消、导出取消均为无操作
- 不可写配置位置保存失败后，旧键仍可用，UI 不误报成功
- 当前项目 clean 和 dirty 两种状态下修改快捷键，验证第 34 节不变量
- 使用重绑定后的一个制造编辑命令，执行、Undo、Redo 与菜单结果一致

## 43 原生强退出验收

从第 32 节各提交阶段收集证据，至少包括：

```text
退出前当前有效快照 hash
预期候选快照 hash
failpoint 或实际退出时机
正式配置文件 hash 和有效性
残留临时文件清单
重启后加载结果
UI 状态与新旧快捷键验证
```

故障注入只能操作测试配置和测试进程，不能通过破坏用户配置目录或填满用户真实磁盘模拟。

内部证据控制必须编译隔离；public release 不得暴露可激活 failpoint。

## 44 回归范围

从当前代码和 S5-M1 验收记录提取实际测试入口，保留现有门槛，对本轮影响路径执行回归。历史成绩保留原 commit identity，不把新 smoke 冒称为 S5-M1 六项完整重测。

至少回归：

```text
现有文件 New/Open/Import/Save/Export/Close
Undo / Redo / Duplicate / Delete
F / F3 的实际行为
Array / Block / Arrangement / Grip / Snap
S4-D1 和 S4-D2 的组件与候选路径
S5-M1 app/cache/rebase/validator 聚焦回归和原生导航 smoke
Project Lifecycle / Recovery
.rcam Compatibility / Compression
Gerber Compatibility
Diagnostics / Automation Contract
INFRA2 source packaging / fresh extract
Native Metal baseline
```

这是“沿用已有门禁与受影响路径回归”的要求，不代表新增性能阶段。不新增 P100K SLA，不要求本轮自动开启未执行的 S5-M2 全矩阵。若输入改动或测试结果显示可能回退已通过 S5-M1 指标，则追加相关原始场景重测，不能忽略回退。

## 45 分阶段实施和提交

### P0 分析冻结

产出：现状清单、默认差异清单、context 重叠表、保留键矩阵、文件契约、风险与测试映射。

本阶段只写 docs 和必要的只读证据，不写产品代码。

通过条件：全部当前实际热键有去向，已有 API 与本计划对应清楚；无未决核心语义。独立审查确认后提交规划 commit。

### P1 纯模型与验证

实现：稳定序列化、按命令替换、完整冲突检查、默认补全规则、strict schema、资源预算、平台规范化。

不改制造业务 handler。先用真实目录生成 golden fixture 与边界测试。

通过条件：C/J 相关核心测试通过；解析失败无副作用；纯模型独立审查通过；本地 commit。

### P2 独立存储与导入导出服务

实现：独立路径、读取状态、原子写入、提交点、并发保护、错误模型、预览对象与 generation。

通过条件：P/J 相关文件测试、故障注入、重启 fixture 通过；现有 preferences 不变；独立审查通过；本地 commit。

### P3 GUI 路由与动态提示

实现：有效快照接入当前真实路径、移除旁路 `Keymap::standard()` 与已迁移硬编码、统一 handler 适配、enabled/focus/IME guards、动态提示。

通过条件：原默认兼容、旧键停用、新键生效、一次执行、零制造状态污染；输入安全独立审查通过；本地 commit。

### P4 设置与录制和迁移交互

实现：设置行、录制器、取消与释放屏障、单项／全部恢复、Mac JSON picker、导入预览、导出、保存状态。

通过条件：完整正常与失败 UI 工作流可执行，F/P 相关测试与原生 smoke 通过；独立 UI/交互审查通过；本地 commit。

### P5 完整原生验收与交付

执行：全量门禁、故障与强退出测试、完整 Mac 验收、隐私检查、clean 四件套、fresh extract。

通过条件：第 53 节 Exit Gate 全部满足，独立最终审查 PASS 后交付并停止。

阶段可按真实依赖调整小提交边界，但不能把“先写完功能再补计划”或“最后才首次审查”作为合并理由。

## 46 独立审查要求

审查者不应仅复述实现者自测。每阶段至少检查：

```text
commit identity
差异与任务范围
关键源代码路径
测试是否真的触达生产路径
错误路径和状态不变量
证据是否来自本 commit
未执行项与剩余风险
```

P1 重点：覆盖前查冲突、稳定 ID、unknown/version、重复 JSON 字段、预算。

P2 重点：提交点、失败不乱报、坏文件保护、并发覆盖、导出不改源。

P3/P4 重点：旧键旁路、事件时焦点／IME、capture 独占、释放屏障、enabled 与菜单一致。

P5 重点：原生证据真实性、最终 clean identity、fresh extract 实际测试、public 构建不含控制入口。

有问题先修复再复审；不能用“已记录 known issue”把本任务硬门禁降为可选项。

## 47 仓库文档

本轮建议落入 docs，沿用本轮已创建的命名：

```text
docs/S5_SHORTCUT_TASK.md
docs/S5_SHORTCUT_PLAN.md
docs/S5_SHORTCUT_ACCEPTANCE_ADDENDUM.md
docs/S5_SHORTCUT_REVIEW.md
docs/adr/0050-shortcut-settings-and-migration.md
```

ADR 0050 已由本工作树规划草稿占用，当前为 Proposed；沿用并统一其内容，不盲用 D2 的 ADR 0043。命令清单可保留在 TASK 附录，不必为拆文件而拆文件。

`docs/ACCEPTANCE_V1.md` 和 `docs/acceptance_cases.json` 保持原冻结用例及 96 项有效编号，不复用已退役 AT-079；新增本阶段 addendum 和独立运行结果，不把局部通过改成全用例通过。

按实际影响更新 README、CAPABILITIES、DESIGN、IMPLEMENTATION_PLAN、CHANGELOG、AGENTS 及现有快捷键帮助文档。不要修改不相关 API 或伪造已完成状态。

schema 和设置工作流需要用户可读说明，明确导入替换语义、未知命令拒绝策略、文件位置、恢复默认和 Windows 状态。

## 48 自动化终检

从最终 clean commit 执行，至少保留仓库现有同等或更强命令：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
cargo test --locked -p editor-service --test automation_contract --test headless_workflow
cargo tree --locked -p editor-service -e normal
cargo build --release --locked -p editor-app
```

实际仓库若另有 Python、source package、public/internal-evidence 或平台特定门禁，按现有脚本继续执行，不因这里只列四个 Rust 命令而省略。

不得依赖未提交代码、未纳入包的测试 fixture 或开发机隐含文件获得通过。

focused 测试、全量测试、原生测试分别报告，不能互相替代。

## 49 Public 与 internal evidence 构建

保留已建立的两种构建：

```text
default public release
internal-evidence release
```

public build 无可激活的故障注入、按键注入或 evidence-control 入口。

内部原生操作和故障测试说明其构建类型，不把 internal-only 路径通过等同于 public 产品路径通过。

最终交付的 public binary 要至少完成真实关键工作流 smoke，并记录 binary SHA。

## 50 Clean 四件套

使用实际阶段标识和 final short SHA，推荐：

```text
RCam_S5K1_<shortsha>_source.zip
RCam_S5K1_<shortsha>_public_evidence.zip
SHA256SUMS.txt
source_fresh_extract_report.json
```

Source ZIP 保留既有：

```text
PACKAGE_INFO.json
PACKAGE_MANIFEST.sha256
MANIFEST.sha256
```

使用仓库现有 INFRA2 打包与检测脚本。重新解压到独立目录后执行 manifest 检查和 source package 测试，必要的最终构建／测试必须指向 fresh extracted payload。

禁止只在原工作树测试，再给另一个未经验证的 ZIP 写“fresh extract passed”。

## 51 证据包最低内容

```text
base HEAD 与 final HEAD
branch 与 clean before/after
命令清单和默认差异
作用域重叠与保留键矩阵
schema golden fixture 与负例结果
核心 / persistence / app routing / UI 测试结果
新旧键与动态提示一致性
focus / IME / capture / repeat 原生证据
导入预览、取消、失败及成功证据
导出和独立配置迁移验证
启动回退、不可写、强退出证据
zero mutation 状态快照
已有功能回归
性能与资源边界
public/internal 构建区别
binary SHA、source ZIP SHA
包内容清单、EVIDENCE.sha256
fresh-extract 测试结果
Windows deferred 明示
```

证据应有“步骤 → 预期 → 实际 → 结果 → 来源”的对应关系，不以几张截图替代完整结论。

所有与最终产品相关的源码、二进制和证据 identity 要一致。若后续修复改变 commit，重跑受影响门禁并更新包，不能只改文档中的 SHA。

## 52 隐私和交付清理

公开包不得包含：

```text
用户真实 shortcuts.json 或 preferences.json
HOME／客户绝对路径
私有 Gerber、PnP、.rcam
完整键盘输入流或文本输入
客户项目名、RefDes 列表
本地凭证或环境秘密
未授权字体及第三方私有资源
```

使用合成快捷键 fixture 和合成制造项目。真实用户配置如用于本地兼容性检查，只保留必要匿名结果，不直接复制进证据。

测试结束后恢复测试前应用和配置状态，停止仅用于验收的进程。不要删除用户未知文件。

## 53 Exit Gate

只有以下条件全部满足，才可标记本任务：

```text
快捷键设置与迁移 v1 = PASS（Mac-first bounded）
```

- 已完成真实命令和执行路径清单，无未解释旁路
- 当前已生效默认键保持兼容，未接入 registry 默认不被偷偷启用
- 稳定 CommandId、规范化与完整冲突预检正确
- 设置编辑、清除、单项和全部默认恢复真正影响运行时
- 文件独立自动保存，重启保持；其他偏好和项目隔离
- 导出完整有效快照，导入预览后原子整套替换
- unknown/version/重复/超限/非法组合全有确定处理
- 文本、IME、Modal、失焦、忙碌、录制及释放屏障安全
- 一次按键不重复或双命令执行，enabled 与原业务路径保留
- 动态提示与真实键一致
- 失败和启动回退可见，坏文件受保护，无部分安装
- 强退出后读取完整旧或新文件，报告不夸大掉电保证
- 设置工作流零制造 revision、dirty、Undo、selection、camera 变化
- 使用快捷键执行原业务命令的事务和 Undo/Redo 不退化
- 现有 S5-M1 及更早受影响的必需回归通过，保留历史成绩的原 identity
- 完整原生 Mac 工作流和 public smoke 有证据
- 独立审查全部通过，最终 clean commit 与四件套一致
- Windows 如未原生执行明确 deferred，不冒称跨平台 PASS

任何一条未满足，状态为部分完成或阻塞，列出准确缺项；不得提前改 capability/stage 文案为 PASS。

## 54 完成后停止

完成交付后停止本轮开发，向父任务回报：

```text
最终 commit
独立审查结论
通过和未执行门禁
原生验收结论
四件套位置和 SHA
剩余限定和风险
```

不自动进入 S5-M2，不开始多步 chord、宏、命令面板、云同步或 CircuitCAM 后续工作。

---

## 附录 A 42 项 registry 与 GUI handler 基线库存

下表“注册默认”来自唯一基线standard_commands；“实际默认”来自GUI输入路径，不混同。除了file.new无独立入口，其余41项均有现有GUI动作或固定取消行为；可配置数量仍41（排除grip.cancel，含new同义ID）。不激活已有未接通的六个注册默认，用户可以手动绑定这些真实菜单动作。

| 稳定ID | 注册默认 | context | 实际默认 | 现GUI与handler |
|---|---|---|---|---|
| `file.new_project` | Primary+N | Global | Primary+N | project_ui::dispatch_file_command → new_workspace → begin_transition(New) |
| `file.open_project` | Primary+O | Global | Primary+O | dispatch_file_command → choose_open_project → begin_transition(Open) |
| `file.save_project` | Primary+S | Global | Primary+S | dispatch_file_command → save_project(false) |
| `file.save_project_as` | Primary+Shift+S | Global | Primary+Shift+S | dispatch_file_command → save_project(true) |
| `file.close_project` | Primary+W | Global | Primary+W | dispatch_file_command → close(false) |
| `file.new` | — | Global | —（无独立UI） | 无独立UI/dispatch case；已有 new_workspace 同义动作，统一时显式映射 |
| `file.import_gerber` | Primary+I | Global | Primary+I | dispatch_file_command / layer_panel → import_gerbers → Action::ImportGerbers |
| `file.export_gerber` | Primary+Shift+E | Global | Primary+Shift+E | dispatch_file_command → save → export_layer → Action::Save |
| `edit.undo` | Primary+Z | Global | Primary+Z | history_buttons / hardcoded键 → Action::History(false) |
| `edit.redo` | Primary+Shift+Z | Global | Primary+Shift+Z / Primary+Y | history_buttons / hardcoded键 → Action::History(true) |
| `edit.delete` | Delete | Canvas | Delete / Backspace | object_buttons / hardcoded键 → Action::Delete |
| `edit.duplicate` | Primary+D | Canvas | Primary+D | object_buttons / hardcoded键 → Action::Duplicate |
| `block.create_from_selection` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_CREATE) → 既有context/目标/确认 |
| `block.place` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_PLACE) → 既有context/目标/确认 |
| `block.rename` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_RENAME) → 既有context/目标/确认 |
| `block.explode` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_EXPLODE) → 既有context/目标/确认 |
| `block.delete_definition` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_DELETE) → 既有context/目标/确认 |
| `block.select_instances` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_SELECT) → 既有context/目标/确认 |
| `block.transform` | — | Canvas | —（无键；已有菜单动作） | Block菜单/Library/属性 → block_command(BLOCK_TRANSFORM) → 既有context/目标/确认 |
| `object.move` | — | Canvas | —（无键；已有菜单动作） | 菜单/属性 → open_modal(Move) → Action::Move |
| `object.rotate` | — | Canvas | —（无键；已有菜单动作） | 菜单/属性 → open_modal(Rotate) → Action::Rotate |
| `object.mirror` | — | Canvas | —（无键；已有菜单动作） | 菜单/属性 → open_modal(Mirror) → Action::Mirror |
| `objects.array_rectangular` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → open_array |
| `objects.align_left` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Align |
| `objects.align_right` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Align |
| `objects.align_top` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Align |
| `objects.align_bottom` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Align |
| `objects.align_hcenter` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Align |
| `objects.align_vcenter` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Align |
| `objects.distribute_horizontal` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Distribute |
| `objects.distribute_vertical` | — | Canvas | —（无键；已有菜单动作） | arrangement_entries → dispatch → arrangement_action → Action::Distribute |
| `view.fit` | F | Canvas | F | 菜单/工具栏/hardcoded键 → self.fit=true（cancel drag） |
| `view.fit_active_layer` | Shift+F | Canvas | —（无键；已有菜单动作） | 工具栏/Layer Row Fit → Action::FitLayer |
| `view.grid.toggle` | G | Canvas | —（无键；已有菜单动作） | view checkbox → grid.visible / persist_project_view |
| `layer.create` | Primary+Shift+N | Global | —（无键；已有菜单动作） | 菜单/Layer panel → create_empty_layer → Action::CreateEmptyLayer |
| `layer.delete` | — | Global | —（无键；已有菜单动作） | 菜单/Layer row → DeletePending + Action::LayerSummary → 原确认流程 |
| `layer.solo` | — | Global | —（无键；已有菜单动作） | Layer row/context双击 → RowEvent::Solo → Action::SetSoloLayer |
| `tool.select` | V | Canvas | —（无键；已有菜单动作） | 工具菜单 → text.cancel / tool=Select / measure.clear |
| `tool.measure` | M | Canvas | —（无键；已有菜单动作） | 工具菜单/属性 → text.cancel / tool=Measure / measure.clear |
| `tool.text` | T | Canvas | —（无键；已有菜单动作） | 插入菜单/属性 → open_modal(Text) |
| `grip.cancel` | Escape | ObjectEdit | Esc（固定交互路径） | main cancel_drag/grip=None + dispatch(GRIP_CANCEL)（非统一ObjectEdit resolver） |
| `snap.toggle` | F3 | Canvas | F3 | checkbox/F3 → dispatch → object_snap toggle/reset/persist_project_view |

## 附录 B 既有类型定义

```rust
pub struct Modifiers {
    pub primary: bool,
    pub secondary: bool,
    pub shift: bool,
    pub alt: bool,
}
pub struct PhysicalModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS; the Windows key elsewhere.
    pub command: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
}
pub enum Key {
    Char(char),
    Delete,
    Backspace,
    Enter,
    Escape,
    Tab,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}
pub enum ShortcutContext {
    Global,
    Canvas,
    Modal,
    TextInput,
    TextPlacement,
    ObjectEdit,
}
```

Platform::logical映射已核对：Mac primary=physical.command、secondary=physical.ctrl；Windows primary=physical.ctrl、secondary=physical.command，shift/alt各自保持。context priority：TextInput5、Modal4、TextPlacement/ObjectEdit3、Canvas2、Global1。Key serde 当前Char/F为externally tagged JSON，Modifiers/Shortcut尚未deny_unknown_fields；新配置严格解码不能默认忽略额外字段。

## 附录 C 已有聚焦和打包命令入口

```bash
cargo test --locked -p editor-core --lib command::tests
cargo test --locked -p editor-app preferences::tests
cargo test --locked -p editor-app modal::tests
cargo test --locked -p editor-app s5m1_tests
cargo test --locked -p editor-app candidates_ui
cargo test --locked -p editor-app components_ui
cargo test --locked -p editor-core --test pnp_foundation
cargo test --locked -p editor-service --test pnp_workflow --test pnp_input_formats --test pnp_diagnostics --test component_candidates --test candidate_diagnostics
cargo test --locked -p rcam-project --test pnp_project
cargo test --locked -p editor-service --test automation_contract --test headless_workflow
cargo test --locked -p editor-app --test build_identity
python3 scripts/test_verify_s5m1_native.py
python3 scripts/test_build_identity_mutations.py
python3 scripts/test_build_identity_incremental.py
python3 scripts/source_manifest.py --check
python3 scripts/test_package_source.py
python3 scripts/test_package_release.py
```

既有gates runner为scripts/run_s4d2_gates.py、run_s4d1_gates.py；它们带历史stage及工作树.tools相对目录，不能原样当本阶段新分支工具runtime/成绩。实施runner须固定实际共享/隔离工具路径、stage和新run ID，不修改历史runner语义。实际cargo/rustup位于主目录.tools；新worktree本身没有.tools，不能假定符号链接或以错误CARGO_HOME构建。

既有source工具：scripts/package_source.py / package_public_evidence.py / verify_source_package.py；专阶段delivery打包器如package_s4d2_delivery.py绑定D2校验，不原封复用到K1。新阶段应沿用通用确定zip/source identity与公共allowlist，新增最小K1包装入口或在任务里明确参数，不编造尚未存在的CLI。

INFRA2真实fresh-extract不能仅Python hashes：还要从真实解压payload执行editor-app release build/启动，确认archive-verified与可信外部commit/package hash一致，修改新增本阶段docs/源码后必须拒绝。具体通用工具参数实施时依其现有argparse核对。


## 附录 D 实施模块边界与计划冻结记录

允许修改：`crates/editor-core/src/command.rs` 的命令／Keymap 层；`crates/editor-app/src/main.rs`、`modal.rs`、`preferences.rs`、`project_ui.rs`、`platform.rs` 及新建最小 `shortcut_settings`／store 模块；`ui/command_widgets.rs` 与确需统一既有动作的菜单／toolbar／layer_panel／block_ui 接入；相应测试、诊断匿名计数、internal-evidence 驱动、打包和 docs。新增每个文件的责任先记录到 PLAN。

不改制造几何、Gerber parser/writer、rcam-project schema、渲染 cache/shader、冻结用例／样本门槛、editor-service API 或其无 GUI 正常依赖边界。新增依赖若确有必要，先记录用途、替代方案、平台条件和许可证，按仓库规则审查，不因实现方便升级无关依赖。

本正式任务根据以下内容统一：

- 用户详细规划优先和规划后直接实施的指令
- 当前 0f3ec046640c4293b53e402305ad3e7f47000929 的源码原文及本地只读 handler 调查
- 用户提供的 D2 任务书结构参考
- 独立任务契约审查对导出提交点、并发限制、Windows 延期、IME 控制键和逐事件派发的修正

唯一文件语义是 schema 1 完整有效快照；不采用此前草稿的稀疏 overrides。唯一设置编辑语义是单项确认后自动保存；不采用此前草稿的全局 draft/apply。导入仍先完整预览，确认后一次替换。

本文件是实施要求，不是已通过的功能或测试报告。附录 A/B/C 已补齐真实命令、类型和现存测试入口；实施前 P0 只需核对 HEAD 未变、冻结保留键证据和计划映射，再按本任务开工，不等待用户审查任务书。
