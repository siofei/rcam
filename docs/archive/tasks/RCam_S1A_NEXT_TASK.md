# RCam 下一轮 Codex 任务：S1-A 语义核心与安全 Writer

> 前置阶段：S0-C 范围冻结已完成本机部分；Windows 原生基线仍为阶段退出阻塞。
> 允许在等待 Windows 硬件证据期间并行开发 S1-A 核心，但不得宣称 S0-C 已通过或 V1 已进入正式验收通过状态。

## 1. 先读文件

开始前阅读：

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/ACCEPTANCE_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/CAPABILITIES.md`
- `docs/S0_C_REVIEW.md`
- `docs/adr/0005-s0-c-editable-core.md`
- `docs/IMPLEMENTATION_PLAN.md`

不要继续执行仓库根目录旧版 `RCam_CODEX_NEXT_TASK.md` 中的 S0-B 任务；本文件取代它作为当前开发任务。

## 2. 本轮目标

实现 **S1-A：Gerber 语义核心 + 安全 Writer**。

本轮不做完整 GUI 编辑、不做脚本引擎、不做 HTTP/RPC、不做 Linux/WSL2 支持、不做大规模 renderer 重写。

最终目标是为下一轮 S1-B 的真实闭环提供可靠基础：

```text
Open → Query → Move → Undo/Redo → Export → Reopen → Validate
```

本轮只负责把“Open / Semantic Model / Validate / Writer / Reopen”的底层能力做正确。

## 3. 必须实现的目标子集

实现范围必须以 ADR 0005 为唯一新增边界，不能擅自扩大：

### 3.1 坐标、单位和格式

- FS 绝对坐标现有子集继续支持。
- 增加 CORE10 所需旧 FS：4.5、4.4、3.4、2.6、3.5、2.5、4.3。
- 正确支持 L/T 零抑制规则。
- 支持 FS I / G91 增量成像：
  - X/Y 为增量；
  - 省略轴表示该轴增量为 0；
  - 转换后进入统一 f64 毫米绝对模型；
  - 圆弧 I/J 仍按对应历史语义解释，不可重复累计。
- MO 与历史 G70/G71 的单位关系必须经过状态校验，冲突拒绝。
- 非有限值、位数错误、声明冲突、超范围值均 fail-closed。

### 3.2 基础成像

- D01 / D02 / D03 状态机。
- C/R/O/P 标准光圈。
- 圆形光圈 G01 线段。
- 无孔、未变换 R 光圈的水平/垂直 G01 矩形扫掠。
- G75 圆弧。
- G74 单象限圆弧：圆心候选、方向、边界与歧义必须独立验证。
- G36/G37 Region，保留真实曲线边，不用显示 Mesh 反推制造几何。
- LPD/LPC 按源曝光顺序组合。

### 3.3 AM 最小子集

只支持 CORE10 已实际使用的：

- primitive 1
- primitive 4
- primitive 21
- 有限常量与变量
- 变量赋值
- `+ - × ÷`
- 旋转
- 局部 Dark/Clear 顺序

必须设置宏复杂度预算，并拒绝：

- 未支持 primitive
- 未定义变量
- 除零
- 非有限结果
- 非法轮廓
- 预算超限

不能因为某个宏定义未被使用，就跳过全文件语法/能力扫描。

### 3.4 旧命令的限定处理

仅实现 ADR 0005 已冻结的形式：

- `SR X1Y1I0J0` 及合法结束：验证作用域后规范化；其他 SR 拒绝。
- IR0 / IPPOS / OF0 / MI0 / SF1：按数值及合法位置校验，不按字符串删除。
- G54：必须关联有效 DCode。
- G71：必须与当前单位状态一致。
- IN/LN：作为源元数据登记，不能作为稳定对象 ID。
- ICAS：仅 ASCII 声明；其他输入编码拒绝。
- IO：仅前置、单位明确、A/B 为有限值的图像偏移；只应用一次。未证明的 IO+其他变换组合拒绝。

AB、复杂 SR、其他宏 primitive、其他旧图像变换继续完整拒绝。

## 4. 语义模型要求

所有受支持输入先规范化到自有模型：

- f64 毫米绝对坐标
- 稳定 LayerId / ObjectId
- 真实 Line / Arc / Flash / Region
- 光圈定义与实例分离
- 原始曝光顺序不可丢失
- Dark/Clear 不得为了优化重排
- 保留写回所需的来源/元数据诊断

第三方 parser AST 只能存在于 gerber-io 适配层，不能成为 editor-core 公共 ABI。

## 5. Writer

新增安全 Gerber Writer，要求：

1. Writer 输入只能来自已验证的自有语义模型。
2. 输出使用统一、明确的 FS/MO 策略；建议优先规范化为毫米绝对坐标。
3. 输出必须完整声明所有实际使用的光圈。
4. 保留真实圆弧，不能从 GPU tessellation 回写。
5. Region、局部孔洞、Dark/Clear 顺序不能变化。
6. 输出结束标记合法且唯一。
7. 输出前进行数值与语义校验。
8. 写到临时路径后重新用产品 parser 解析并检查语义。
9. 本阶段只允许导出到显式新路径；不要默认覆盖源文件。
10. 写入或验证失败时，不得产生看似成功的加工文件。

Writer 必须支持“未修改文档规范化往返”测试，为 S1-B 编辑往返提供基线。

## 6. 测试顺序

先实现/通过公开合成样本，再使用授权的 CORE10 做只读语义验证；不要直接用真实文件调到“看起来能开”。

至少新增：

- FS 各冻结格式的正/反例
- G91 增量与省略轴
- IO mm/in 两种单位
- ICAS
- R 光圈水平/垂直 D01
- G74 CW/CCW、多候选/歧义拒绝
- G75
- Region
- AM 1/4/21 与变量表达式
- 宏除零/未知变量/复杂度超限
- SR 1×1 恒等及复杂 SR 拒绝
- 旧恒等命令合法位置与非法作用域
- 未修改模型 writer → reopen → 几何/曝光断言
- writer 错误时不留下有效目标文件

已有 `fixtures/synthetic/s0c/` 18 个文件继续作为范围测试输入，不修改其真值以迎合实现。

## 7. 资源预算

S1 不能继续使用 S0 的 2 MiB / 100,000 Flash 作为目标上限。

冻结 CORE10 至少要求覆盖：

- 最大输入：4,961,139 bytes
- 单文件至少 230,409 次 Flash

但不要简单把常量改大。分别为：

- 源文件字节
- parser token/command
- 语义对象
- AM 展开
- Region 边
- Writer 输出
- 临时验证副本

设置预算，并用类型化 `RESOURCE_LIMIT` 报错。

## 8. ApplicationService

本轮不增加脚本运行时，但为下一轮准备强类型服务：

- `document.open`
- `document.get`
- `layers.list`
- `objects.query`
- `objects.get`
- `document.validate`
- `gerber.export_layer`（仅新路径）

只有真实实现并测试通过的 operation 才能进入 `system.capabilities`。

暂不实现 Move/Undo/Redo，除非底层命令模型为 S1-B 必需且可独立测试；不要提前铺完整编辑工具。

## 9. Windows 门禁

S1-A 核心开发可与 Windows 证据并行，但阶段标签仍保持 S0-C blocked，直到同一最终源码在 Windows x64 原生完成：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
```

并实际启动 release `editor-app`，记录：

- Windows 版本
- CPU/RAM
- GPU/驱动
- wgpu adapter
- backend（优先记录实际 DX12/Vulkan）
- 原始进程输出
- 截图
- 构建 SHA-256

仅 CI 生成 exe 不等于 Windows 原生启动证据。

## 10. 本轮退出条件

S1-A 完成必须同时满足：

- ADR 0005 目标子集的 parser/semantic 支持完成；
- 未支持特征仍 fail-closed；
- 自有模型与第三方 AST 解耦；
- writer 能对受支持、未修改输入完成安全规范化往返；
- 所有新增公开测试通过；
- CORE10 不更换样本，至少完成 10/10 语义能力扫描并明确每份文件是否已具备 S1-A 所需语义；
- 不声称 Move/Undo/Redo 已实现；
- 不声称 CORE10 已完成编辑往返；
- Windows 证据如果仍缺失，明确保持 S0-C platform gate blocked。

完成后停止，不自动进入完整 GUI。下一任务为 S1-B：

```text
Open → Query → Move(+5,-3mm) → Undo/Redo → Export(new path) → Reopen → Independent Validate
```

## 11. 交付

提交：

- 源码修改清单
- 新增/修改测试
- 每条命令与退出码
- 原始日志
- fixture/源码/二进制 SHA-256
- capability 变更
- 未支持特征清单
- CORE10 语义扫描结果（不泄露私有内容）
- Windows/macOS 分平台证据状态
- 未解决风险

不要修改原 96 个验收身份或删除失败历史；执行结果另存。
