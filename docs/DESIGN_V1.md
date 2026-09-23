# Gerber 编辑器 V1：设计与开发指导

> 文档版本：1.1 · 基线日期：2026-09-14  
> 产品定位：面向钢网／Gerber 图形操作的轻量桌面编辑器，而不是 PCB 布线软件或完整 CAM350 替代品。  
> 技术路线：Rust + MakerPnP gerber-parser／gerber-types + 自有语义与编辑模型 + egui／eframe + wgpu。  
> 文档性质：待实施的设计与验收基线。**不表示软件已经开发完成，也不表示任何测试已经通过。**

配套文件：`ACCEPTANCE_V1.md` 为验收操作与判定标准；`acceptance_cases.json` 为同一套用例的机器可读版本；`AGENTS.md` 为 Codex 仓库规则；`AUTOMATION_API.md` 为脚本自动化扩展接口契约。

**1.1 修订：只支持 Windows／macOS，取消 Linux 构建、CI、打包与兼容性门槛；新增无 GUI 的统一业务服务与脚本扩展契约。V1 验收接口预留的真实可用性，不要求交付脚本引擎、正式 CLI 或网络控制服务。** 变更及用例迁移见 `CHANGELOG.md`。

**2026-09-16 架构补充：**冻结未来多格式交换方向：Gerber 继续直接进入制造语义模型；DXF／SVG／HP-GL(PLT) 等非 Gerber 矢量格式先进入独立 `VectorScene`，再经显式 Manufacturing Conversion 转换为制造模型。该补充不扩大 V1 需求、能力声明或 96 个有效验收用例；正式实现时需另立阶段、需求和验收。设计决策见 [ADR 0017](adr/0017-vector-scene-and-format-interchange.md)。

建议阅读路径：先读第 1–4 节确定范围，再读第 5–12 节实现架构与正确性，最后按第 14–18 节建立性能、测试和分阶段交付。第一版最终判定以配套验收文档为准。

## 1. V1 到底要交付什么

第一版必须跑通以下实际工作流：

```text
打开一个或多个 Gerber
    → 核对图形、单位与尺寸
    → 选中开孔／线段／圆弧／Region
    → 移动、复制、删除、旋转、镜像
    → 输入中英文文字并生成真实矢量图形
    → 撤销／重做
    → 将当前图层另存为 Gerber
    → 在本软件及独立查看器中重新打开、对比尺寸和图形
```

**验收优先级：不损坏文件 > 几何和曝光语义正确 > 编辑闭环完整 > 操作流畅 > 外观。**

### 1.1 冻结的产品边界

| 项目 | V1 决定 |
|---|---|
| 主要用途 | 钢网开孔和 Gerber 图形的基础编辑、加字、复制与重新输出 |
| 主开发环境 | Windows 原生；Rust MSVC 工具链；Codex 辅助开发 |
| 第二开发与验收环境 | macOS 原生 Apple Silicon，重点验证 Metal、中文输入和触控板 |
| Linux／WSL2 | 不在产品支持、构建、测试、CI 和发行范围；不要求核心层兼容，不为其开发适配代码 |
| 数据源 | `.gbr`、`.gbx` 及其他经内容识别的 RS-274X Gerber 文件 |
| 导出目标 | V1 支持子集内、可重新解析和验证的 Gerber 几何文件 |
| 是否自动恢复 PCB 网络／封装结构 | 否；Gerber 图形编辑不等于恢复原始 PCB 工程 |
| 是否取代 CircuitCAM 专有 `.cam` 工程 | 否；V1 不承诺读写 CircuitCAM 私有工程格式 |
| 是否提供所有 Gerber 标准能力 | 否；必须公开支持矩阵，范围外安全拒绝，不能悄悄漏画 |
| 是否必须依赖 libgerbv | 否；可以作为独立回归对照工具，不进入默认运行时依赖 |
| 是否必须依赖 lib_gerber_edit | 否；只作为可选辅助适配器，不能代替自有编辑模型 |
| 是否需要数据库／服务端 | 否；V1 是本地桌面程序，无数据库和常驻 Web 服务 |
| 脚本自动化扩展 | 必须预留无窗口的应用服务、命令／查询 DTO、结果／任务接口，并通过真实无界面集成测试 |
| 是否在 V1 内置脚本语言 | 否；Python／Lua／JavaScript、脚本控制台、正式 CLI、远程 API 均留到后续版本 |
| 文件安全 | 默认另存为；禁止导入时改写源文件；任何失败不得破坏已有文件 |

这是根据前面讨论形成的实施基线。新增功能必须同步更新设计、用例、风险与阶段范围，不能让 Codex 在实现中自行扩张。

### 1.2 第一版“必须有”与“以后再做”

**必须有：**打开和拖放、独立图层列表、显隐和锁定、鼠标与触控板导航、坐标／单位／网格、单选／多选／方向性框选、移动／复制／删除／旋转／镜像、数值属性编辑、Undo/Redo、中英文矢量加字、测距、Gerber 另存为、重新打开验证、结构化诊断、基础性能与 Windows／macOS 双平台实测，以及 GUI 共用的无界面业务调用与契约测试。

**V1 不作为必交功能：**任意光圈宏编辑、AB／SR 的交互式解组、图层合并、阵列／自动拼板、Excellon 编辑、线段拟合圆弧、钢网开孔自动识别与聚类、布尔开孔编辑、自动文字搭桥、二维码、Gerber 差异检索、DXF／SVG／HP-GL(PLT) 等非 Gerber 矢量导入、SVG／PDF／PNG／DXF 正式导出、通用 `VectorScene` 交换核心、专有工程保存、崩溃后完整会话恢复、插件系统、自动更新、浏览器版、脚本解释器、脚本编辑／录制回放、正式自动化 CLI、HTTP／JSON-RPC 控制服务。

这些能力应保留扩展位置，但不能以“未来要做”为理由在 V1 引入一个未完成的通用 CAM 引擎。脚本扩展以第 5.2 节与 `AUTOMATION_API.md` 的最小业务边界为准：只把既有功能解耦并验证调用闭环，不提前实现脚本产品。**自动把碎线重组为一个开孔并不属于基础框选；V1 可以手动多选，但不得宣传已经实现开孔语义识别。**

## 2. 对前面技术判断的校正

以下是已查阅的一手资料与本项目决策，不应混为一谈。

| 核实事项 | 本次查阅结果 | 对设计的影响 |
|---|---|---|
| `gerber-parser` | README 以 2024.05 规范为参考，声明覆盖非废弃命令，部分旧命令有解释注意事项 [S1] | 解析成功不等于语义、显示、编辑、输出全部正确 |
| 最新官方规范 | Ucamco 下载页在本次查阅时提供 2026.05 规范及测试文件 [S4] | 不能将 parser 的“2024.05 参考”描述为已验证完整覆盖 2026.05；S0 必须做差异审查 |
| `gerber-types` | 提供低层类型和代码生成，明确不做语义检查 [S2] | 必须自建导入和导出语义校验器 |
| `gerber-viewer` | README 仍列有 exposure 仅 additive、Thermal 和部分旧功能限制，圆弧离散也有局限 [S3] | 可参考，不直接作为生产级曝光合成及正确性基准 |
| `lib_gerber_edit` | 文档确认了图层平移、缩放、合并、阵列和 ASCII 文字等功能；`LayerTransform` 是平移接口 [S5] | 不假定存在完整对象编辑、任意旋转 GUI 或 Undo 引擎 |
| ASCII 与中文 | `AsciiText` 是 ASCII 字符转 Gerber，不等于中文字体轮廓引擎 [S6] | 中文生成必须另行实现；不能把 `AsciiText` 当作中文已完成 |
| 图层模型 | `lib_gerber_edit::Board` 按图层类型组织，类型有唯一性约束 [S7] | 编辑器必须使用独立 `LayerId`，允许多个相同类型的图层 |
| 自定义 GPU 画布 | `egui-wgpu` 官方提供 `Callback`／`CallbackTrait` [S8] | 优先在 eframe 管理的 GPU 生命周期内接入，不另造第二个主窗口 |
| 双平台 GPU | 本项目使用 wgpu 的 Windows DX12／Vulkan、macOS Metal 后端 [S9] | 库具备其他平台能力不等于本产品承诺支持；仅验 Windows／macOS |
| 性能数字 | 本项目尚无原型及测试数据 | 10 万／50 万／100 万图元的帧率不是保证；第 14 节仅为待测验收目标 |

**选型结论：保留 Rust + egui + wgpu；把曝光语义、编辑模型、保存校验掌握在本项目中。** 不直接 Fork 整个 MakerPnP 主程序，不把 viewer 的渲染结果当作原始几何。

### 2.1 依赖与许可证边界

MakerPnP 的 parser、types、viewer 仓库分别声明 MIT／Apache 双许可证；MakerPnP 主程序 README 的许可状态不能与这些独立库混用。[S1][S2][S3][S10]

`lib_gerber_edit` 的确切版本、完整 LICENSE、捆绑字体和样板来源必须在 S0 归档审查。本次未完成其许可证全文及所选发布包的逐文件审查，因此**不把它列为无条件商业可分发依赖**。`gerbv` 仓库声明 GPL-2.0；不要未经审查把它静态／动态链接到准备闭源发布的程序中，也不要把“外部进程调用”当作自动豁免许可义务。[S11]

这里是工程依赖管理要求，不是商业发行的法律意见。发行前提交 `THIRD_PARTY_NOTICES.md`、依赖版本与许可证清单，并按实际分发方式核查。

## 3. 需求编号与范围

后面的设计章节、验收用例和 Codex 提交必须使用以下需求编号。

| 编号 | V1 需求 | 核心判定 |
|---|---|---|
| R01 | 可复现构建与依赖锁定 | 锁定工具链、Cargo.lock；干净环境构建 |
| R02 | 本地导入、中文路径和拖放 | 文件不被改写；错误可定位 |
| R03 | 坐标、单位与模态解释 | 省略坐标、FS、MO、D 码状态正确 |
| R04 | 标准图元 | C/R/O/P Flash、圆形光圈线／弧、Region 正确 |
| R05 | 极性与局部透明区域 | 顺序曝光、跨层隔离、光圈孔洞语义正确 |
| R06 | 能力检测与安全降级 | 范围外不漏画、不允许危险保存 |
| R07 | 图层管理 | 相同类型独立，显隐／锁定／活动层明确 |
| R08 | 画布与高 DPI | Pan/Zoom、坐标变换、裁剪与缩放正确 |
| R09 | 命中测试与选择 | 不按包围盒冒充精确点选；选区稳定 |
| R10 | 变换和基础编辑 | 移动、复制、删除、旋转、镜像及属性编辑 |
| R11 | Undo/Redo 与事务 | 一次用户动作一个事务；取消不改文件 |
| R12 | 中英文矢量文字 | 真正写入几何；字洞、尺寸、缺字策略明确 |
| R13 | 网格／吸附／测距 | 屏幕与物理单位分离；输入值可验证 |
| R14 | Gerber 导出与往返 | 独立校验；导出几何与编辑结果相符 |
| R15 | 保存安全与文件冲突 | 失败不损坏原文件；外部修改不覆盖 |
| R16 | 异步工作与资源边界 | 不在 UI 线程解析／大规模细分；可取消 |
| R17 | GPU 缓存和交互性能 | 定义样本、指标、测量方法与结果证据 |
| R18 | 平台、打包和中文交互 | Windows／macOS 本机实测；不依赖开发目录 |
| R19 | 自动测试、真实样本和证据 | 需求—用例—样本—日志可追溯 |
| R20 | 合规与隐私 | 本地处理；字体及依赖通知；不自动上传 Gerber |
| R21 | 无界面统一业务接口 | GUI 和无窗口测试共用应用服务；完整打开、查询、编辑、导出，不依赖 GPU／窗口／当前选择 |
| R22 | 脚本扩展契约与安全边界 | 版本化 DTO、稳定 ID、显式单位、修订冲突、原子批次、任务取消、非交互保存与权限策略 |

## 4. Gerber 支持矩阵

### 4.1 强制支持的 `EditableV1` 子集

| 能力 | 导入 | 编辑 | 输出 | 实现约束 |
|---|---|---|---|---|
| FS／MO；毫米与英寸；绝对坐标 | 必须 | 统一毫米模型 | 毫米 | 按 FS 解析，禁止猜小数位 |
| 省略 X 或 Y 的模态坐标 | 必须 | 必须 | 可输出完整坐标 | 使用上一坐标而非零 |
| D01／D02／D03、Dnn 选择 | 必须 | 必须 | 必须 | 校验未定义光圈和非法状态 |
| C／R／O／P 标准 Flash | 必须 | 位移／旋转／镜像；尺寸属性 | 必须 | 包括规范允许的中心孔配置 |
| 无孔圆形光圈的 G01 线段 | 必须 | 必须 | 必须 | 真实扫掠面积，圆端帽；不套用错误的 SVG 接头 |
| 无孔圆形光圈的 G02／G03、G75 | 必须 | 必须 | 保留圆弧 | 跨象限、方向、全圆、I/J 与终点都校验 |
| G36／G37 Region | 必须 | 作为完整对象变换 | 必须 | 多轮廓、凹多边形、合法孔洞及弧边；校验闭合与填充语义 |
| LPD／LPC | 必须 | 保留操作顺序 | 必须 | 不等同于图层显隐或透明度 |
| LM／LR／LS 的合法作用域 | 必须 | 规范化后编辑 | 等价输出 | 光圈变换不应误当作全图坐标变换 |
| 常见 FS 前导／尾随零处理 | 必须 | 规范化 | 统一输出格式 | 格式及合法性以固定规范和样本为准 |
| G04、文件结束及属性语法 | 必须识别 | 见第 12 节 | 见第 12 节 | 不把注释内容当作绘图命令 |

除第 4.4 节明确冻结的有限扩展外，无孔圆形光圈以外的 D01 扫掠、旧增量坐标模式、旧图像变换等，不因 parser 能解析就自动进入这个子集。

### 4.2 范围外功能的固定处理

V1 对未纳入第 4.4 节限定扩展的 AM 光圈宏、AB、SR、G74、旧式图像命令、Excellon、RS-274D 外置光圈，以及无法证明语义正确的输入，默认采用 **`RejectedUnsupported`：不进入可编辑画布，不允许另存为加工文件**。

错误窗口应列出不支持的命令类别、出现位置、已识别单位等诊断信息；可以显示源文本，但不得显示不完整画面并标记为“正常打开”。

未来可以引入 `ReadOnlyExact`：只有完整语义解释和完整显示均经独立回归验证，而编辑／写回尚未验证时才允许只读精确预览。**“未知内容原文还在，所以可以安全编辑后保存”不是合法降级。**

如果真实钢网样本大量包含上述功能，S0 必须把这些功能升级为明确需求并补齐用例，或将 V1 标记为“不适合目标样本”，不能用这个默认拒绝策略掩盖产品不可用。

### 4.3 三层能力不要混淆

```text
ParseSupported      能读出命令
SemanticSupported   能正确解释每条命令的图像效果
EditRoundTripSafe   修改后能输出等价且合法的 Gerber
```

只有三项都通过，文件才可进入 `EditableV1`。能力判断必须覆盖整个文件；遇到未支持的命令不能在后半段被忽略。

### 4.4 S0-C：EditableV1-Core 目标扩展（2026-09-15）

依据本轮用户指定开发指南与冻结 CORE10 使用审计，将 ADR 0005 的限定范围纳入**必须实施目标**。
包括 AM primitive 1/4/21 与受限表达式、G74、FS I/G91、CORE 的旧 FS（含全宽 D）、
无孔未变换矩形光圈水平/垂直 G01、恒等 SR、恒等旧图像命令、G54/G71/IN/LN、ICAS 和前置 IO。
IO 非零须实际应用；矩形线段不得用圆形扫掠替代。这两项对 CORE-04 是必需的。
具体合法条件、拒绝边界、样本和证据要求以 [ADR 0005](adr/0005-s0-c-editable-core.md) 表格为准。

本节修订原 4.2 对这些有限特征的最终目标排除，但不意味着当前 S0 运行时支持；
未完成语义与往返核验前仍拒绝，不能授予 EditableV1-Core 或 ReadOnlyExact。
第 4.1 节其他要求不删除；核心容差、安全保存、96 个用例与双平台门槛不变。

S1-A 实施澄清见 [ADR 0006](adr/0006-s1-a-macro-template-validation.md)：未实例化 AM
仍完整扫描语法/能力，但未绑定模板参数不等于一次缺参调用；每条实际 AD 仍严格拒绝未定义变量。
不以虚构参数预检模板轮廓，不采用隐式零值，也不降低实例几何和资源验证门槛。
相关完整追加场景同步到 ACCEPTANCE_V1 和 acceptance_cases.json（schema 2），
S0-C 局部审计通过不代表完整 AT 或 CORE10 往返通过。

## 5. 总体架构与模块依赖

```text
当前 V1 Gerber 路径
────────────────────────────────────────────────────────────────────
GUI 操作（现有）       无界面测试（V1）       脚本／CLI（未来）
     └───────────────────┼──────────────────────┘
                         ▼
              editor-service：ApplicationService
              命令／查询、版本校验、任务、文件策略
                         │
         ┌───────────────┼───────────────────┐
         ▼               ▼                   ▼
     gerber-io       editor-core          editor-text
 parser／语义解释     Manufacturing         字形与局部轮廓
 能力扫描／writer     Model／稳定 ID         固定制造误差
                         │
             ┌───────────┴──────────────────┐
             ▼                              ▼
    只读快照／变更集                   校验后的 Gerber
             │                              │
    renderer-wgpu                  临时文件 → 安全替换
    缓存／层掩膜                            │
             │                     结构化结果／诊断
        egui / eframe

未来多格式交换路径（Post-V1；未实现，不属于当前能力）
────────────────────────────────────────────────────────────────────
DXF ── import-dxf ──┐
SVG ── import-svg ──┼──► VectorScene ──► Manufacturing Conversion ──► Manufacturing Model
PLT ── import-hpgl ─┘          │                    │
                               │                    └─ 单位／层映射／线宽／填充／拟合策略
                               └─ 通用矢量几何，不承载 Gerber Dark/Clear/Aperture 语义

Manufacturing Model ──► Gerber Writer
                    ├─► Vector Export Adapter ──► SVG / PDF / DXF
                    └─► Raster Export Adapter ──► PNG
```

`editor-core` 和 `editor-service` 不依赖 egui、wgpu、窗口句柄和操作系统 UI API。`gerber-io` 通过适配层连接第三方类型，不让第三方 AST 变成所有模块的公共 ABI。渲染层只读文档快照或变更集，所有正式修改由应用服务提交到核心命令系统。

服务层是在进程内组织业务能力，不是新增后端服务器。文件、字体和任务执行通过受控端口接入；正常业务不要求窗口消息循环或 GPU 设备。未来脚本适配器只做参数／结果转换，不复制一套编辑引擎。

### 5.1 建议仓库结构

```text
gerber-editor/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── AGENTS.md
├── README.md
├── THIRD_PARTY_NOTICES.md
├── crates/
│   ├── editor-core/
│   │   └── src/{document,geometry,commands,spatial,diagnostics}/
│   ├── gerber-io/
│   │   └── src/{detect,parser_adapter,interpret,capabilities,writer,validate}/
│   ├── editor-service/
│   │   ├── src/{api,commands,queries,jobs,events,ports}/
│   │   └── tests/{automation_contract.rs,headless_workflow.rs}
│   ├── editor-text/
│   ├── editor-render/
│   │   ├── src/{cache,mask,selection,camera,metrics}/
│   │   └── shaders/
│   ├── editor-app/
│   │   └── src/{ui,tools,platform,workers,settings}/
│   └── xtask/                 # 开发期实现的验收和样本工具
├── fixtures/
│   ├── synthetic/
│   ├── public/
│   └── private/              # Git 忽略；按实际数据授权管理
├── tests/
├── docs/
│   ├── DESIGN_V1.md
│   ├── ACCEPTANCE_V1.md
│   ├── acceptance_cases.json
│   ├── AUTOMATION_API.md
│   ├── automation_request_examples.json
│   ├── CHANGELOG.md
│   ├── BASELINE.md
│   ├── CAPABILITIES.md
│   └── adr/
└── evidence/                 # 默认忽略；验收时归档
```

初期也可以减少 crate 数量，但语义层、模型层、应用服务、UI 层和渲染层的依赖方向不能改变。不要为每个小类型拆一个 crate。服务包必须能够被单独构建和测试，不因工作区包含桌面程序而把窗口／GPU 依赖带入无界面路径。

### 5.2 为以后脚本自动化留下可执行的接口

本节是 V1 必需架构，不是后续可选优化。GUI 与未来脚本统一走 `ApplicationService`；应用服务调用现有命令／Undo 系统，而非为脚本另写变换和 writer。详细字段、操作及异常约束见 `AUTOMATION_API.md`。

| 接口边界 | V1 验收要求 |
|---|---|
| 只读查询 | 文档、图层、对象、能力和任务结果均返回只读 DTO；按明确图层与几何条件查询，不读取当前 UI 选择 |
| 写入命令 | 移动、复制、删除、旋转、镜像、属性、文字、图层修改及 Undo／Redo 统一入口，接收明确对象 ID 和毫米参数 |
| 版本化契约 | `api_version=1`、请求 ID、结构化结果／错误；JSON 编解码测试；拒绝未知操作、版本与参数 |
| 并发与事务 | 同文档串行提交；`expected_revision` 校验；批量失败整体回滚；Undo 恢复内容但 revision 持续递增 |
| 文件与确认 | 明确路径、覆盖／元数据策略、权限检查；服务只返回待确认结果，绝不弹窗或等待控制台输入 |
| 任务与事件 | 复用有界后台任务和取消；提供 job 查询与进程内事件口，不要求网络服务器 |
| 真实证明 | 无窗口、无 GPU 的测试完成打开 → 查询 → 编辑／文字 → Undo／Redo → 导出 → 重开复核，且与 GUI 业务结果一致 |

接口预留不包括 Python／Lua 绑定、远程鉴权、CLI 产品、脚本管理或插件运行时。不得只写空接口冒充完成，也不得为预留接口过度扩张第一版范围。Windows 和 macOS 分别运行上述无界面测试；这不增加 Linux 支持义务。

### 5.3 未来多格式交换架构：`VectorScene`（Post-V1）

本节冻结长期架构方向，**不是 V1 已实现能力，也不新增当前验收门槛**。具体决策以 [ADR 0017](adr/0017-vector-scene-and-format-interchange.md) 为准。

`VectorScene` 是非制造语义的通用矢量交换模型，用于承接 DXF、SVG、HP-GL/PLT 等格式解析后的几何；它不是 `SemanticDocument`／Manufacturing Model 的替代品。两者职责必须分离：

| 模型 | 负责 | 明确不负责 |
|---|---|---|
| `VectorScene` | 通用二维矢量几何、层/组、局部变换、路径、线、圆/圆弧、椭圆、Bezier/Spline、Polyline/ClosedPath、源格式属性 | Gerber Dark/Clear、Aperture 状态、DCode、Region 曝光顺序、加工安全承诺 |
| Manufacturing Model | 可制造语义、曝光顺序、Dark/Clear、Aperture/Region、稳定对象身份、编辑事务、Gerber 安全写回 | 还原任意源格式的全部版式/样式语义 |

导入方向固定为：

```text
Gerber
  └─► gerber-parser / gerber-io semantic interpreter
       └─► Manufacturing Model

DXF / SVG / HP-GL(PLT)
  └─► 各自 parser / importer
       └─► VectorScene
            └─► Manufacturing Conversion
                 └─► Manufacturing Model
```

Gerber **不得为了形式统一而强制先转换到 `VectorScene`**。这样会削弱或丢失已经验证的 Dark/Clear、Aperture、Region、旧命令兼容和曝光顺序语义。

`Manufacturing Conversion` 必须显式接收导入策略，而不能猜测。至少包括：单位、源 Layer 到目标 Layer 的映射、无宽中心线的制造线宽、ClosedPath 的 stroke/fill/Region 策略、颜色/笔号映射、Bezier/Spline 的拟合误差、无法精确表达图元的拒绝策略。歧义不能静默选择默认值后直接生成生产 Gerber。

未来建议模块（名称为规划，不表示 crate 已存在）：

```text
crates/
├── vector-core/             # VectorScene / Path / Arc / Bezier / Polygon / Transform
├── import-dxf/              # DXF -> VectorScene
├── import-svg/              # SVG -> VectorScene
├── import-hpgl/             # HP-GL/PLT -> VectorScene
├── vector-to-manufacturing/ # VectorScene -> Manufacturing Model
└── vector-export/           # Manufacturing/VectorScene -> SVG/PDF/DXF；PNG 走 raster adapter
```

导出也必须从语义/几何模型出发：

```text
Manufacturing Model
  ├─► Gerber Writer
  ├─► Vector Export Adapter ─► SVG / PDF / DXF
  └─► Raster Export Adapter ─► PNG
```

**禁止 `renderer mesh / GPU tessellation / 屏幕像素 -> Gerber/SVG/PDF/DXF`。** Renderer 产生的数据只用于显示；圆弧、Bezier、Region 等原始几何身份不能因为显示细分而丢失。PNG 作为栅格输出可以使用离屏渲染，但必须冻结物理窗口、DPI、背景、抗锯齿和 Dark/Clear 合成语义。

在真正开始某种格式实现前，先单独建立该格式的需求编号、能力矩阵、样本与验收；不得把本节规划写入 `CAPABILITIES.md` 的已支持列表，也不得提前把 `AUTOMATION_API.md` 冻结成未验证的多格式协议。

## 6. 数据模型与不可破坏的约束

### 6.1 核心结构契约

下列为设计契约，不是已经存在的第三方 API，也不是可直接编译的完整代码。

| 对象 | 必备字段／责任 |
|---|---|
| `Document` | 文档 ID、图层顺序、活动图层、版本号、保存状态 |
| `Layer` | 独立 LayerId、名称、角色、显隐／锁定、源文件信息、有序对象列表 |
| `SourceRecord` | 原始文件字节或受控只读副本、SHA-256、路径、导入诊断、能力等级 |
| `ApertureDefinition` | 独立定义 ID、标准形状与参数、局部孔洞、版本号；禁止原地污染共享使用者 |
| `DrawObject` | ObjectId、Geometry、光圈引用或 Region 轮廓、局部变换、极性、曝光顺序、来源关联 |
| `Geometry` | Flash／Line／Arc／Region；不存 UI 像素坐标作为制造坐标 |
| `Arc` | 起点、终点、圆心或规范化 I/J、方向、显式全圆标志；避免以起终点相同误判为零长度 |
| `Region` | 轮廓集合、直线／圆弧边、经规范验证的填充语义；孔洞属于对象局部几何 |
| `Selection` | 稳定对象 ID 集合、活动锚点；不能把数组索引当持久 ID |
| `TextGroup` | 原始字符串、字体标识与 SHA-256、字号定义、锚点、轮廓子对象；字体字节不进入分发包 |
| `Command` | 核心原子可撤销变更、前后状态／增删记录、内存估算、诊断；不是第三方 AST |
| `ApiRequest`／`ApiResult` | 应用服务的版本化请求／结果 DTO；ID 为字符串，参数明确单位，错误可被程序分类 |
| `DocumentRevision` | 会话内单调递增；包括 Undo／Redo；与内容保存基线分开，接口以十进制字符串传输 |
| `ExportPolicy`／`JobInfo` | 明确覆盖／元数据／权限策略及任务阶段／取消／结果；不包含窗口回调 |
| `RenderCacheKey` | LayerId、对象／光圈版本、显示 LOD、设备代次；不得只按 DCode 缓存 |

### 6.2 精度方案

内部制造几何统一为 **`f64` 毫米**。保留导入时的 FS、MO 和原始数字文本以便审计；不得直接拿 parser 的整数坐标除以一个猜测倍率。

建议 V1 数值基线如下，S0 必须结合输出格式确认并冻结：

| 项目 | 基线 |
|---|---|
| 核心计算比较容差 | `1e-6 mm`；边界相交／退化判断按场景单独处理，不能全局随意放大 |
| Gerber 默认输出 | 毫米、6 位小数；计划采用经过规范校验的绝对坐标格式 |
| 输出制造量化步长 q | 文档 ManufacturingPrecision，默认 `0.0001 mm`；每轴舍入误差不得超过 q/2 加数值舍入余量。FS 编码保持 `0.000001 mm`，不是制造 resolution（ADR 0026） |
| 字体曲线制造逼近误差 | 最大轮廓偏差 `0.001 mm`；与屏幕缩放无关 |
| 屏幕圆弧细分误差 | 常规静态显示不超过 `0.35` 个物理像素 |
| 可接受的运行坐标范围 | 由冻结的输出整数位和几何运算范围共同决定；所有坐标及 I/J 都要检查 |

禁止 NaN／Inf、整数乘法溢出、非法光圈尺寸和无限细分。不得承诺“任意尺寸”“无限缩放”。超出有效范围应在修改提交或导出前明确阻止，而不是截断坐标。

GPU 可以使用局部 `f32`，但模型、导出、测距、命中测试仍使用 `f64`。优先采用分块局部坐标和相机相对块偏移：先在 CPU 用 `f64` 相减，再转换可控范围的 `f32`，避免远离原点时抖动。不能把全局毫米坐标直接转换成单精度并据此保存。

### 6.3 对象与开孔不是一回事

一个 D03 Flash 通常可作为一个对象，一个 Region 可作为一个完整对象；由数百条独立 D01 组成的轮廓，在 V1 默认仍是多个绘图对象。框选后的整体移动依靠选择集事务完成，不能依据同一 DCode 自动认定它们是一个开孔。

文字可以在当前会话内作为一组编辑。导出为普通 Gerber 后，这种编辑器分组信息不保证保留；重新导入时按 Gerber 对象解释。完整可编辑文字工程保存属于后续版本。

### 6.4 共享光圈的写时复制

选中一个 Flash 修改宽度时，先复制光圈定义并让目标对象改用新定义；其他使用原定义的对象保持不变。V1 不提供一个模糊的“修改 D10”按钮同时改变所有对象。

源文件 A 的 D10 与源文件 B 的 D10 不一定相同。导出按结构化定义重新分配编号，不能把显示名称或原始 DCode 当作全局主键。

## 7. 语义解释与曝光合成

### 7.1 解释器必须维护的状态

至少包括：单位、坐标格式、当前坐标、当前光圈、插补模式、象限模式、Region 状态、对象极性、光圈变换、属性作用域、文件结束状态，以及范围外命令检测。解释顺序与输入顺序一致。

错误必须区分：语法错误、语义错误、未支持、资源超限、文件 I/O 错误。诊断尽可能包含字节区间／行号／命令序号；底层未提供准确位置时标记“定位范围”，不能伪造精确行号。

### 7.2 必须区分三种“负”

**图层曝光 LPC、光圈内部透明孔洞、宏内部局部曝光，不是同一个作用域。**

例如一个带中心孔的正片环形 Flash：孔内是“这个 Flash 没有覆盖”，而不是“从整个图层上删除孔内所有已有图形”。若底下已有一条穿过圆心的正片线，该线应继续可见。用一个实心圆加一个全局 LPC 小圆来替代该 Flash，会破坏下面的线。

V1 即使不支持宏，也必须先解决标准光圈孔洞与 Region 孔洞的局部语义。否则后续接入宏只会放大错误。

**Gerber Region 不能未经验证直接继承 SVG 的 even-odd／nonzero 填充规则。** 导入时按冻结规范解释合法轮廓；输出带孔局部形状时，使用经独立验证的合法轮廓／cut-in 编码或等价的无孔区域分解，不能随意输出嵌套轮廓后假设查看器会自动挖孔。字体轮廓同样遵守这一约束。

### 7.3 极性与操作顺序

对同一图层，概念上按顺序计算覆盖集合：

```text
初始层图像 = 空集合
Dark 对象：layer = layer ∪ object_coverage
Clear 对象：layer = layer \ object_coverage
```

若顺序为“加图形 A → 清除 C → 再添加 B”，B 可以覆盖 C 曾经清掉的位置。不能先画完所有 Dark 再统一减去所有 Clear。

不同图层先独立得到覆盖结果，再按图层颜色与透明度合成。上层的 Clear 不能擦除下层图像。

GPU 可以批量绘制，但只能在已证明结果等价的连续范围和合成作用域内合批。**禁止为减少 draw call 而把整个文件按 DCode、形状或颜色重新排序。**

### 7.4 变换规则

几何对象变换与 Gerber 中的光圈局部变换分别建模。旋转和镜像必须同时处理对象坐标、局部光圈方向、Region 轮廓以及圆弧方向。

对于镜像，二维线性变换的行列式为负时，圆弧顺逆时针要翻转；同时应用两次镜像时方向恢复。V1 的交互变换仅包含平移、旋转和镜像；**不开放非等比缩放**，避免把圆弧变成椭圆却仍输出 G02／G03。

导入合法 LS 可作统一缩放处理。变换后导出的圆弧必须在量化以后再次检查圆心、扫角、arc deviation 和制造语义，不能把两端严格等半径当作 Gerber 输入合法性条件，也不能只校验内存中未舍入的圆弧。

## 8. egui / eframe 与 wgpu 的边界

### 8.1 UI 布局

```text
顶部：文件／编辑／视图／工具／帮助
工具栏：选择、移动、复制、旋转、镜像、文字、测距
左侧：图层列表、活动层、显隐、锁定
中间：Gerber GPU 画布
右侧：所选对象类型、位置、尺寸、极性、数值变换
底部：X/Y、毫米／英寸、网格、选中数量、诊断／任务进度
```

普通控件交给 egui；画布图形用自定义 wgpu 绘制。egui 本身也需要渲染，不能将整个程序误解为“UI 不占 GPU”。

### 8.2 推荐集成路径

使用 eframe 的 wgpu 后端，并通过 `egui_wgpu::Callback`／`CallbackTrait` 接入画布。准备阶段更新必要缓存与图层掩膜，绘制阶段在画布矩形内合成结果和叠加层。具体接口以 S0 锁定版本的官方示例为准。[S8]

共享 eframe 管理的 Device／Queue／Surface 生命周期，不在普通控件内部重复创建 GPU Device。窗口缩放、画布裁剪、DPI 变化及后台任务完成后，都必须正确触发重绘。

`egui`、`eframe`、`egui-wgpu` 使用兼容的一套版本；自定义渲染侧优先使用其 re-export 的 wgpu 类型，避免两个不兼容 wgpu 大版本共存。依赖版本不能写 `*` 或用本次文档未经编译验证的版本组合替代基线。

### 8.3 第一版渲染管线

```text
不可变几何／编辑变更
  → 光圈局部形状缓存 + 普通图元 Mesh 缓存
  → 视口可见性筛选
  → 以源曝光顺序更新当前图层的覆盖／掩膜
  → 多图层着色合成
  → 选择轮廓、橡皮筋框、测距、网格和坐标轴
```

层覆盖缓冲采用支持的纹理格式及采样配置；S0 探测实际适配器能力。不要假定某种单通道格式在所有后端都支持同样的 MSAA。屏幕空间离屏缓冲只覆盖当前视口或可见分块，不为整个工件创建一张无限大的纹理。

清除操作影响重叠对象，单对象更新可能仍需重绘相应图层／分块；**只更新 instance buffer 并不意味着一切编辑都只需几个字节且无需重新合成。**

### 8.4 缓存与失效

| 事件 | 允许发生 | 不允许发生 |
|---|---|---|
| 平移 | 更新相机／块偏移、可见集，重绘层覆盖 | 重解析文件、重建全部制造几何 |
| 同 LOD 内缩放 | 更新相机和必要可见集 | 每次滚轮全量细分所有图元 |
| 跨 LOD 缩放 | 异步准备需要更精细的可见 Mesh | UI 线程同步细分整个文件 |
| 选择变化 | 更新叠加层、选中 ID 集 | 重写原始几何、重做导入 |
| 移动一个对象 | 更新对象实例／受影响边界及图层合成 | 全文重新解析或整份文档深拷贝 |
| 修改共享光圈的一个实例 | 写时复制；更新目标实例 | 让所有共享实例一起变化 |
| 设备丢失 | 废弃设备资源并重建缓存 | 清空文档或丢失未保存编辑 |

重复 Flash 可以实例化绘制；但必须同时满足局部形状一致、极性和顺序安全等条件。缓存设置总预算和 LRU 淘汰，不能随缩放级别无限增长。

### 8.5 圆弧细分与制造几何分离

圆弧在模型中一直保留圆弧。显示细分按屏幕误差确定，而不是固定 32／64／256 段。给定半径 r、圆心角 θ 和允许弦高误差 e，可根据

```text
每段最大角度 = 2 × acos(1 - e / r)
段数 = ceil(abs(θ) / 每段最大角度)
```

求出需求，再处理 `r <= 0`、`e >= r`、接近零角度、全圆、数值范围和最大资源限制。该式是几何推导，不是某个库提供的性能承诺。

`lyon_tessellation` 可以作为显示路径填充／描边的候选实现，其文档说明了曲线扁平化和 tolerance。[S12] 但它的浮点 Mesh 不作为制造坐标来源，通用 stroke 接头／填充规则也不能未经验证直接套用到 Gerber 语义。

## 9. 选择、命中、吸附与工具状态

### 9.1 点选

先用空间索引查找候选，再进行精确覆盖测试。索引边界必须包含光圈尺寸、线宽和圆弧极值；不能只保存中心点／端点包围盒。

默认只在**活动、可见且未锁定的图层**点选。普通模式选择该点最终可见的 Dark 对象；一个完全被后续 Clear 擦掉的区域不能仍像实体一样被选中。提供“显示清除对象／几何选择”模式，用轮廓选择 Clear 对象及被遮挡对象，并在属性栏明确极性。

重叠对象用循环选择或候选列表；排序固定，不随 HashMap 遍历顺序变化。点在环形孔中心时，不能仅因处于外接矩形而命中环本体。

### 9.2 框选

左向右：对象完整覆盖范围落在矩形内才选中。右向左：对象覆盖与矩形有实际交集即选中。边界相切的容差策略固定并提供样本。

Ctrl-click 加选，Shift-click 减选（两者同时按下时减选优先）；菜单显示实际快捷键。在文字输入框获得焦点时，删除和复制只作用于输入框，不能误删画布对象。

### 9.3 吸附与测距

吸附候选至少包括网格、Flash 中心、线／弧端点和圆心。吸附搜索半径使用 **egui 逻辑点**，再按 DPI 和相机转换为毫米；不能固定使用一个毫米半径导致放大后到处吸住。

建议默认吸附距离 8 个逻辑点，优先级为显式捕捉点 > 网格；距离相同按稳定 ID 排序。按约定修饰键临时关闭吸附。网格显示密度可变化，但制造坐标不变。

测距 V1 为两点直线距离，不等于圆弧长度或边到边最短距离。每条测量同时显示距离与
相对世界制造坐标 +X 轴的有向角度，角度按逆时针归一化到 `[0°, 360°)`。完成的测量以
app-only Overlay 保留，允许连续创建并同时显示多条；每条线段中点显示距离/角度标注，
Esc 清除全部测量。界面必须写明测量种类、角度基准与清除方式。

### 9.4 复制与快捷键边界

V1 的图形复制粘贴采用**应用内几何剪贴板**：复制所选对象的制造几何、局部定义和相对曝光顺序；粘贴生成新的对象 ID，默认进入当前可见且未锁定的活动图层。支持同一会话内跨图层复制，不承诺与 CircuitCAM、系统资源管理器或其他 CAD 程序交换私有图形格式。

文本输入框仍使用系统文本剪贴板。Windows 使用 Ctrl、macOS 使用 Command 的平台习惯；应用命令根据焦点分派。图形剪贴板的跨进程持久化、剪切和外部格式互操作不属于 V1 必需项。

### 9.5 工具状态机

至少实现 `Idle → Armed → Previewing → Commit / Cancel`。拖动中只维护预览变换；鼠标释放时提交一个事务，Esc、失去捕获或关闭文档按已定义规则取消。

批量拖动可先用选择组的预览变换，不逐帧更新所有对象的制造数据和 R-tree。提交后统一更新相关对象和索引。在异步索引更新期间，不能对过时索引给出错误选择；可短暂禁用相应操作并显示状态。

## 10. 编辑事务与 Undo/Redo

所有修改通过 `ApplicationService` 的统一命令入口，包括属性栏数值修改、删除、粘贴、文字生成和图层内对象变更。UI 和未来脚本都不得绕过应用服务及命令系统直接写模型字段；输入焦点与预览留在 UI，但正式提交使用同一参数校验及事务。

一次拖动产生一次 Undo；一次批量粘贴、一次文字创建也各占一次。取消预览产生零条历史。命令提交前先检查能力、锁定状态、数值范围和内存预算，失败则整体不变。

Undo 保存可逆的前后状态或精确差量；不得仅靠重复执行逆旋转恢复，以免累计误差。Undo 后执行新命令要清空 redo 分支。选择 ID、曝光顺序、光圈引用和缓存失效必须随事务一起恢复。

建议 V1 默认历史预算 128 MiB，最多 200 个事务，先到者生效；超过预算时按完整事务淘汰并提示。超过单事务可容纳上限的操作在修改前阻止，不允许编辑完成后悄悄变成不可撤销。

保存不清空 Undo。脏状态根据“当前内容是否等于最后保存的内容基线”判断；不仅比较历史栈长度。撤销回到保存状态应消除脏标记，再做新编辑重新标脏。

### 10.1 非 GUI 调用的事务约束

应用服务对已有文档的修改要求显式文档 ID 和 `expected_revision`。每文档串行提交，在耗时准备后再次检查版本；非法 ID、锁定图层、数值／资源超限和版本冲突必须在无副作用的状态返回。

同一文档的 `edit.batch` 将多个可逆对象编辑归并为一个 Undo 事务；任一步失败或提交前取消，整体保持原内容、顺序、定义引用及历史。不得将打开／关闭文件、导出和其他外部副作用塞进可回滚批次。V1 不承诺跨文档／跨文件事务，也不实现长时间持锁的远程 begin/commit。

revision 是并发校验令牌而非历史游标：一次成功批次推进一次；Undo／Redo 也推进，不恢复旧 revision。原有“撤销回保存状态清除脏标记”仍由内容基线决定。请求 ID 只用于关联，不承诺重复请求去重或持久重试。

## 11. 中英文文字方案

### 11.1 文字生成与 UI 字体分开

egui 能显示中文，不代表 Gerber 输出中就有中文。文字生成必须实现：

```text
Unicode 输入
  → 字体与字形存在性检查
  → 排版／字形位置
  → TrueType／OpenType 矢量轮廓
  → 按固定毫米误差转换贝塞尔曲线
  → 保留字洞的局部几何
  → 有序 Gerber 图形对象
```

候选组件：`ttf-parser` 读取轮廓，`rustybuzz` 做字形排版；以锁定版本和实际字体样本完成验证。[S13][S14] `AsciiText` 仅可作为 ASCII 快速通道，不承担中文需求。[S6]

### 11.2 V1 明确支持

常见简体中文、ASCII 数字／字母／标点，单行横排；提供文字内容、字高、字距、旋转角和对齐锚点。暂不承诺彩色 Emoji、竖排、任意复杂字体及所有变体字体。

“字高”统一定义为**整段非空白字形的可见轮廓总高度**，将该轮廓归一化到用户输入的毫米高度；基线与边距在预览中另外标示。空白字符串拒绝生成，字符串中的空格保留排版进位。

V1 生成的对象以 Dark 为默认；带孔的字符不能通过全局 Clear 去挖掉底下已有几何。字洞必须属于字体局部形状。通过“口、回、8、B”及与已有线条重叠样本检验。

缺字、字体无法读取或只有位图字形时明确报错并不提交事务，不能静默替换为问号。字体选择和文件读取需支持中文路径。

本文件包不包含任何字体文件。产品发行只能使用获准分发的字体，或让用户选择其本机字体；记录字体许可证、来源、哈希和导出使用策略。验收字体用许可证及 SHA-256 固定身份，不以文件名相同作为同一字体。

### 11.3 钢网加工提醒

普通字体轮廓可能形成孤岛。V1 **不自动生成钢网搭桥，不保证生成文字可以直接切割加工**。加字界面和发布说明应明确这一限制；几何正确不代表已通过加工工艺审查。

## 12. Gerber 写回、元数据与文件安全

### 12.1 输出的是制造几何，不是屏幕截图

Writer 从语义模型读取真实坐标和原始圆弧；不得从 GPU Mesh、SVG 屏幕路径、像素掩膜或当前缩放级别反推 Gerber。

V1 输出顺序建议：

```text
头部与明确的单位／格式
  → 已使用的光圈定义及需要的状态
  → 按曝光顺序输出对象
  → 明确闭合状态与 M02
```

每次改变极性和局部变换都显式管理状态，避免上个对象的状态泄漏。未定义光圈、未闭合 Region、非法参数、超界坐标和量化后不一致的圆弧必须阻止导出。

Writer 输出格式需要通过固定规范与独立查看器验证。不得把 `gerber-types` 的序列化成功当作合法性证明。[S2]

### 12.2 元数据策略

V1 的正式输出模式是 **“Gerber 几何文件”**。不承诺恢复或保持 X2 网络、元件、引脚和电气约束信息。

导入时保留原始属性用于诊断。编辑后导出，对不能证明仍有效的 TF／TA／TO 关联数据、旧 MD5／校验信息等不得照抄；应在导出前列明将移除的元数据类别并要求确认。可验证的生成器标识和实际图层用途可以重新生成，但不能根据扩展名猜测后写成事实。

没有需要移除的元数据时不重复打扰。存在元数据却静默丢弃，或复制后仍输出不实的元件／网络绑定，均不通过验收。

输出采用规范允许的文本编码；中英文内容通过矢量几何表示，不依赖在 Gerber 注释中塞入中文实现“文字生成”。

### 12.3 导出流水线

1. 从同一文档版本创建只读导出快照，冻结本次坐标、光圈、对象顺序和元数据策略。
2. 在内存／受控临时区域生成输出，做结构、语义和数值范围检查。
3. 将输出重新解析为独立场景，比较几何与曝光结果；检查引用、数量及包围盒，不能只比对象数。
4. 写目标同目录临时文件，检查写入与 flush／同步错误，再执行该平台经过测试的安全替换。
5. 完成后更新对应输出的 saved precision 基线；失败保留脏状态和原文件，清理临时文件。自 S4-B1 起 Export **不清除 Workspace dirty**（见第 22 章）。

历史默认为“另存为当前图层”，S4-B1 起改称 **Export Gerber…**（Gerber 只导入/导出，不是 Save）。显式覆盖现有文件时必须确认，并在覆盖前检查该文件是否被外部修改。Windows 上的已存在目标替换、文件占用与杀毒软件锁定不能按 POSIX rename 的行为想当然处理。

可提供可配置的版本备份，但备份不代替安全替换。**进程级故障不损坏旧文件是 V1 门槛；突然断电的跨文件系统持久性需要另行验证，不作无依据保证。**

### 12.4 多图层与会话边界

每个图层独立保存为 Gerber；不能在用户点击“保存”时暗中合并所有图层。关闭程序时列出所有未保存图层，逐一保存或确认放弃。

V1 不提供专有工程文件；图层颜色、临时选择、Undo 历史和文字原文分组不保证随普通 Gerber 保存。后续 `.rcam` 工程格式（S4-B2/B3，见第 22 章；此前称 `.gproj`）可以保存这些信息，但不能以项目文件可恢复来降低 Gerber 输出验收标准。

### 12.5 往返验证不能只依赖自己

自身重新解析可以检出很多错误，但解析器和 writer 也可能犯同一种错误。需要三条证据链：独立计算的解析几何真值、独立查看器对照、真实样本人工核对。

官方 Reference Gerber Viewer 可用于非保密样本检查。真实生产文件默认只用获准的本地工具，**不得为验收自动上传到在线查看器**。[S4][S11]

### 12.6 非交互调用必须复用同一安全流水线

GUI 选择路径和确认后，将显式策略交给应用服务；服务不弹窗。无界面调用默认不覆盖已存在文件，元数据损失未授权时返回 `confirmation_required` 及具体类别，而不是卡住等待用户。后续调用必须明确授权相应覆盖目标／哈希或元数据类别，不能用万能 `force` 绕过校验。

文件读取、字体和写入经主机提供的路径权限入口；不允许接口默认访问任意路径。长时间导出可保存冻结 revision 的快照，结果返回 `exported_revision`；若用户已产生更新内容，不得把当前内容误标为已保存。具体文件冲突、取消、错误码和访问边界见 `AUTOMATION_API.md` 第 6—7 节。

### 12.7 非 Gerber 导出边界（未来）

SVG／PDF／DXF／PNG 不属于 V1 必交。未来实现时，格式编码器可以使用审查过的第三方库，但“制造几何如何映射到目标格式”的转换规则由 RCam 自己掌握并测试。

- SVG/PDF/DXF：优先保留圆弧、路径和闭合轮廓等矢量身份；不能先走显示 Mesh 再导出折线近似。
- DXF：必须区分“保留中心线可编辑性”和“保留最终制造轮廓”两种语义；不得把有宽 Gerber stroke 无说明地降级成零宽 `LINE/ARC`。
- PNG：允许从冻结的离屏渲染路径产生 RGBA，再交给成熟编码器；输出参数必须显式包含物理范围、像素尺寸/DPI、背景和透明度。
- 任何格式的第三方 writer/encoder 只负责目标文件编码，不替代 RCam 的几何/制造语义验证。

## 13. 异步执行、内存与异常恢复

文件读取、解析、能力扫描、复杂几何生成、字体轮廓转换、大批量索引构建、导出和验证都应离开 UI 主线程。后台任务返回带 `DocumentVersion` 的结果；关闭文档或版本变化后，不得把过期结果写回当前画布。

应用服务提供独立于 UI 的任务查询、取消和进程内事件接口；排队成功不等于业务完成。后台任务结束后通过 revision 校验提交，或返回快照输出的明确版本。

渲染线程只消费可用快照。采用有界队列和可取消任务，不为每个 Flash 创建线程。线程数根据 CPU 与内存预算限制，低内存机器提供较低并发设置。

V1 初始资源保护策略由 S0 冻结并写入设置及诊断：单文件字节上限、对象上限、单 Region 顶点上限、宏／块递归深度（未来支持时）、Mesh／纹理缓存、Undo 内存、后台任务并发数。超限应明确拒绝或要求调整，不得死循环或无提示退出。

解析和几何生成应在受控批次检查取消信号；取消反馈目标见验收文档。设备丢失、窗口最小化、视口尺寸为零、Surface 过期、内存分配失败均须分类处理；不能用 panic 作为普通文件错误处理机制。

首次加载可以显示进度／取消；不要先绘出半个板又在没有诊断的情况下显示“完成”。只要发生未支持特征，整个文件退出 Editable 状态。

## 14. 性能基线与验收方法

以下是**拟定的工程门槛，不是已取得的测试数据或库的性能保证**。S0 在实际机器上冻结参数；不得在最终验收时为了通过而临时换更快的机器、缩小样本或放宽阈值。

### 14.1 样本定义

| 数据集 | 固定内容 | 用途 |
|---|---|---|
| P10K | 10,000 个 Flash；C/R/O/P 各 2,500；固定网格、光圈与曝光顺序 | 入门交互、加载与内存 |
| P100K | 100,000 个重复 Flash，最多 8 个标准光圈，固定空间分布和视口 | 实例化、缓存、选择索引 |
| PMIX | 40,000 Flash + 30,000 Line + 20,000 Arc + 10,000 Region；每个 Region 固定 16 条边；另记录全部展开顶点和三角形数 | 混合几何和 CPU／GPU 开销 |
| PPOL | 20,000 个有重叠的 Dark／Clear 操作，固定交替顺序，4 个独立图层；总对象数为 20,000 | 极性合成不能被性能优化破坏 |
| PSTRESS | 500,000／1,000,000 Flash；固定生成种子 | 非硬性帧率门槛的压力和可控失败测试 |

样本生成器必须输出确定的 Gerber、SHA-256、生成种子、对象及顶点统计、格式配置和预期边界。随机样本采用冻结算法，不依赖跨语言不一致的默认随机数实现。

### 14.2 参考机器与测量条件

分别建立 Windows 参考机 A 和 macOS 参考机 B 的 `BASELINE.md`：实际 CPU、RAM、GPU、驱动、OS 版本、显示器分辨率／刷新率、DPI、窗口与画布像素尺寸、GPU 后端、构建哈希和电源模式必须填写。

前面讨论中的 M1 可以作为 B；Windows 的实际配置尚未确认，不把某个 CPU 或显卡当作已经存在的基准机。开发机器建议有足够内存同时运行 Rust 构建、IDE 和 Codex；4 GB 环境应采用单独低内存配置评估，而不是默认承诺同等性能。

主性能验收使用 release 构建、物理画布 `1600 × 900`、60 Hz 基准、不最小化、非远程桌面、同一适配器。先预热 10 秒，再执行冻结的 60 秒平移／缩放轨迹，独立重复 3 次，保存所有结果。120／144 Hz 为补充记录，不是 V1 门槛。

### 14.3 初始门槛

| 指标 | V1 建议门槛 |
|---|---|
| P10K 导入至首次完整画面 | ≤ 3 秒；从开始读取到完整可操作画面 |
| P100K 导入至首次完整画面 | ≤ 10 秒 |
| P100K 连续导航帧间隔 | p95 ≤ 33.3 ms，p99 ≤ 66.7 ms；持续交互不得出现 > 200 ms 停顿 |
| PMIX／PPOL 连续导航帧间隔 | p95 ≤ 50 ms，p99 ≤ 100 ms；不得通过少画图形达标 |
| P100K 点选 CPU 查询 | p95 ≤ 20 ms；另记录输入到可见高亮端到端 p95 ≤ 100 ms |
| P100K 全视图框选 | 选择结果就绪 ≤ 300 ms；UI 保持响应 |
| 1,000 个选中对象拖动 | PMIX 中 p95 帧间隔 ≤ 50 ms，释放后最终提交 ≤ 300 ms |
| 取消后台任务反馈 | ≤ 500 ms 显示取消反馈；受控任务 ≤ 2 秒终止并释放主要临时资源 |
| 导入失败／超限 | 明确诊断，原文档不变，无无界内存增长 |
| 闲置重绘 | 静止且无任务时不持续以 60 FPS 重绘；后台任务完成能唤醒画面 |
| 关闭后资源 | 连续打开／关闭 P100K 20 次，预热后内存无持续线性增长；第 20 次静置后与第 5 次差异 ≤ 100 MiB |
| 内存预算 | A/B 分别冻结；初始建议 P100K 进程峰值 ≤ 1 GiB，显式 GPU Buffer／Texture 分配预算 ≤ 512 MiB |

帧间隔采集需注明定义和时钟：应用帧时长、CPU 编码时间、GPU 时间、实际呈现间隔不能混成同一个数字。没有实际呈现测量能力时标注“应用帧”，并附屏幕录像；不得仅用 `request_repaint` 次数宣称 FPS。

统一内存架构下进程内存与 GPU 统计口径可能重叠，分别记录，不做无依据相加。平台间对比不得混用 Working Set、Private Bytes、RSS 和统一内存峰值。

### 14.4 正确性优先的优化顺序

先缓存几何与重复光圈、稳定帧循环、分离 UI 工作，再建立 R-tree、可见集和脏区，最后在明确瓶颈后引入更复杂 GPU 技术。Compute Shader、GPU picking、indirect draw 不是 V1 的必需品。

出现性能失败先报告：可见对象数、总三角形、层掩膜像素、draw call、CPU／GPU 时间和资源分配次数。不能用删除抗锯齿、忽略 Clear、降低制造精度或隐藏小开孔的方式让测试变快。

## 15. Windows／macOS 开发环境与打包

### 15.1 Windows 原生

Rust MSVC 目标需要对应链接器、Windows SDK 和库；按 rustup 官方要求准备 C++ 工具链。[S15] 仓库位于本机普通开发目录，Windows 与 macOS 使用各自的构建产物目录。

Windows 原生是本项目的主 GUI／GPU 验证环境。Codex 官方已经提供原生 Windows 运行与沙箱方案，不需要为了运行 Codex 而默认把整个 GUI 项目搬到 WSL。[S16]

### 15.2 macOS 原生

使用 Apple Silicon 本机构建，验证 Metal、文件对话框、拖放、中文 IME、Command 快捷键、Retina 以及触控板。不能仅因 Windows 构建成功就写“macOS 已支持”。

### 15.3 平台范围与 WSL2 边界

本项目只维护 Windows 与 macOS 的构建、测试和发行。Linux、WSL2／WSLg 不属于产品运行目标，也不要求维持核心库的 Linux 构建兼容；不新增 Linux runner、安装包、X11／Wayland 适配或专门依赖。

个人使用 WSL2 作为终端或辅助工具不被禁止，但不是项目规定的开发环境，其结果不能代替 Windows 原生测试。脚本自动化扩展接口也只需在 Windows／macOS 无界面运行，不意味着必须增加 Linux headless 服务。

开发命令和验收工具应能在 Windows 原生与 macOS 原生执行，不把仅支持 Bash 的脚本或外部 Linux 命令设为必要步骤。平台差异集中在文件对话框、路径、剪贴板、快捷键、打开文件、日志目录和安全替换模块；保持双平台解耦，不让模型直接调用 Windows UI。

### 15.4 发行支持矩阵

| 目标 | V1 状态与门槛 |
|---|---|
| Windows x64 | 主发行目标；Windows 10 22H2 兼容目标和 Windows 11 目标分别实测，发布说明只列实际通过的 OS 构建 |
| macOS Apple Silicon | 第二发行目标；至少在用户使用的 macOS 15 环境完成必测流程；不推断所有更早／更晚版本 |
| macOS Intel | V1 不承诺；单独编译与验证后再加入 |
| Linux／WSL2 | 明确不支持；不构建、不测试、不打包、不设兼容性门槛；未来如需加入，另立需求 |
| Windows ARM／移动端／Web | 不在 V1 发行范围 |

正式“双平台 V1”至少需要一台支持列表中的 Windows 真机和一台 macOS 真机完成对应门禁。缺少其中一个平台时只能标为“单平台预览版”，不能签署完整 V1 通过。

发行包必须在无 Rust／无源码目录的普通用户环境运行。核查所选 wgpu 后端、着色器编译器、系统运行库和外部资源是否有额外分发要求，不以“Rust”推断“只有一个 exe，绝无依赖”。[S9]

签名／公证未完成时按未签名测试构建明确标注；不能要求用户关闭系统级安全防护作为安装步骤。正式公开发行的签名、公证与更新机制单独列入发行计划。

## 16. 测试体系与证据

### 16.1 测试分层

| 层级 | 检查重点 |
|---|---|
| 单元测试 | 单位、FS 模态、圆弧、边界、镜像、光圈写时复制、撤销 |
| 语义测试 | 极性顺序、局部孔洞、Region、属性作用域、未支持命令 |
| 往返测试 | 输入 → 模型 → 输出 → 新模型；验证制造几何与覆盖 |
| 性质测试 | 平移再逆平移；四次 90°；两次同轴镜像；Undo/Redo 保持状态 |
| 接口契约测试 | DTO 编解码、明确单位／ID、版本冲突、原子批次、结构化错误、保存授权、任务取消 |
| 无界面集成 | 单独运行服务，无窗口和 GPU；完成查询、编辑、文字、导出／重开，与 GUI 业务结果一致 |
| GUI 测试 | 真正按钮与快捷键驱动，输入焦点、中文 IME、DPI、拖放 |
| GPU 测试 | 同一场景在不同后端正确，设备重建、裁剪、极性一致 |
| 异常测试 | 损坏文件、未支持、文件锁定、写失败、外部修改、资源超限 |
| 性能测试 | 固定样本、固定轨迹、固定机器、原始统计与录像 |

自动单元测试不能代替人工 GUI 操作；GUI 截图也不能代替导出几何验证。无 GPU 的 CI runner 只证明构建与核心测试，不证明 DX12／Metal 运行正确。

### 16.2 样本来源

自造最小样本用于精确真值，Ucamco 官方公开样本用于规范回归，真实生产样本用于产品可用性。官方提供测试文件，但其存在不等于本项目已运行通过。[S4]

S0 冻结至少 30 份经授权的真实 Gerber，覆盖多个生成工具／年代和中英文路径；在此基础上选择至少 10 份“必须可编辑”的业务核心样本。**这 10 份全部完成打开、修改、保存、独立复核，是 V1 业务通过的前提。** 范围外样本如期拒绝可以通过能力边界测试，但不能替代这 10 份成功样本。

本次文档整理没有读取任何历史上传的 Gerber，也没有对用户真实文件做兼容性测试。真实样本名单、哈希和授权状态由 S0 实施时补齐。

### 16.3 可见图形比较方法

优先用已知点覆盖、精确边界、圆心／半径／线宽／间距和独立解析量进行判断。栅格图仅作为补充：同一物理窗口、同一分辨率、背景、阈值、抗锯齿设置和坐标对齐后再比较。

不得只用全图差异百分比；一个小开孔丢失可能只占极少像素。对关键点、孔洞、连通区域和最小特征逐项检查。边缘栅格差异只能在冻结的容差带内存在；容差带应小于目标最小特征的约束，不能把整个小特征吞掉。

独立查看器版本、参数、对照图和输出文件 SHA-256 都进入证据。工具不一致时保留差异，不自动选择看起来更顺眼的一张作为正确答案。

## 17. Codex 的分阶段实施顺序

每个阶段都交付可运行的纵向闭环和对应测试，禁止先一次性生成所有按钮和空实现。

| 阶段 | 交付 | 退出条件 |
|---|---|---|
| S0 选型与风险验证 | 锁定工具链和依赖；Windows/Mac 最小画布；真实样本／许可证／基准机；服务依赖边界和接口 ADR | 构建与几何小样无未决阻塞；冻结仅双平台的 CI、DTO／revision 规则及脚本暂不实现范围 |
| S1 Gerber 语义与写回 | 无 GUI 核心与服务；打开／查询／诊断／导出；标准图元与极性 | 真实无窗口导入／往返通过；依赖不含窗口／GPU；范围外命令安全拒绝 |
| S2 查看与选择 | UI 外壳；GPU 缓存；图层；导航；索引；点选／框选；服务只读查询 | 真机图形与基准一致；GUI 选择状态不泄漏到业务 API，R07–R09 可测 |
| S3 基础编辑 | GUI 共用命令入口；变换／复制／删除／属性；Undo／Redo；版本冲突与原子批次；网格／测距 | 编辑往返一致；无窗口可执行同一编辑；失败回滚、取消／焦点不误改 |
| S4 中英文文字和保存闭环 | 字体轮廓／字洞；原子保存；元数据报告；关闭提示；显式非交互策略和任务取消 | GUI 与无界面加字／保存使用同一实现；异常、确认和取消用例通过 |
| S5 性能与双平台 | 固定数据集、缓存优化、DPI／IME／设备异常；Windows／macOS 的 API 契约与无界面流程 | A/B 真机性能、GUI 与无界面用例通过；原始证据齐全；无 Linux 门禁 |
| S6 第一版验收与发布 | 双平台发行包；完整用例／缺陷；能力及接口说明；脚本未发布范围 | 96 个有效规范用例的适用检查完成；AT-079 仅保留退役记录，不存在阻塞缺陷 |

若 S0 发现核心样本依赖 AM／SR 等范围外功能，先修改范围并加一个专项阶段，再进入后续阶段；不能留到 S6 才解释用户文件为什么打不开。

**Post-V1 多格式交换不插入 S0–S6 当前门禁。** 完成 V1 后按 `IMPLEMENTATION_PLAN.md` 的 F1–F5 路线另立基线：先做 `VectorScene` 与转换契约，再依次接 DXF、SVG、HP-GL/PLT 和多格式导出。正式进入某一 F 阶段前，新增对应需求/验收，而不是复用或改写现有 AT-xxx。

### 17.1 每次交给 Codex 的任务格式

```text
任务：实施 Sx 的一个可验收功能，不扩展其他范围。
先阅读 AGENTS.md、docs/DESIGN_V1.md 对应章节、docs/ACCEPTANCE_V1.md 对应用例。
需求编号：Rxx。
验收编号：AT-xxx。
允许修改的模块：明确列出。
必须保持：几何精度、曝光顺序、文件安全、现有测试。
交付：代码、测试、执行命令及真实结果、未完成项、风险、证据路径。
规则：未执行测试写未执行；不能通过降低阈值、跳过样本或模拟界面把功能标为完成。
```

`AGENTS.md` 是 Codex 官方支持的项目指令机制。[S17] 文件应短而明确，大型设计通过路径引用，不将整份设计复制进每个目录。

### 17.2 代码评审门槛

重点检查：服务正常依赖是否包含窗口／GPU；GUI 是否绕过统一入口；接口是否依赖当前选择、弹窗或不明确的单位；revision 与批次是否正确；UI 回调中是否出现 I/O 或全量细分；是否重排曝光；是否直接从 Mesh 导出；有没有 `unwrap()` 处理文件输入；属性编辑是否污染共享光圈；批量动作是否拆成数千次 Undo；渲染结果是否影响制造坐标；是否未经许可上传样本。

新功能必须先有失败用例或最小复现，再有实现，再有通过证据。升级依赖要同时复跑核心语义、往返和平台 smoke 测试。

## 18. 构建与自动验收接口

### 18.1 Rust 标准命令

以下命令用于未来创建的 Rust 工作区；本次交付只有文档，不含 Cargo 项目：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
```

依赖通过 Cargo.lock 固定；Git 依赖还应固定具体提交。Cargo 官方说明 lockfile 记录精确依赖信息，应纳入可复现构建流程。[S18]

不要无条件执行 `--all-features`：平台后端、互斥 feature 或实验功能需使用审查过的测试矩阵。CI 必须覆盖 Windows MSVC 与 macOS arm64 的核心、服务契约测试和桌面构建；不配置 Linux 构建／测试门禁。工具脚本不得要求 WSL 才能运行。

### 18.2 需要开发的验收工具契约

**以下为需要在 `xtask` 中实现的接口，不是本次文档包里已经能运行的命令。**

```bash
cargo run --locked -p xtask -- fixtures --suite v1
cargo run --release --locked -p xtask -- verify --suite v1 --out evidence/core
cargo run --release --locked -p xtask -- roundtrip --manifest fixtures/manifest.json --out evidence/roundtrip
cargo run --release --locked -p xtask -- report --cases docs/acceptance_cases.json --results evidence --out evidence/ACCEPTANCE_REPORT.md
```

工具必须校验样本哈希、保留原始输出、记录 build SHA 与环境，遇到缺失证据使用失败／阻塞状态，而不是按默认值写通过。GUI／IME／真实 GPU 检查不能由一个空壳 CLI 宣称完成。

建议退出码：`0` 表示本次适用检查全部通过；`1` 表示至少一个失败；`2` 表示环境／输入／证据缺失导致未完成。JSON 结果必须包含用例 ID、状态、版本、平台、耗时、指标、证据路径和缺陷编号。

### 18.3 无界面业务接口验收

下列命令是实施阶段需要建立的测试入口，不是产品脚本 CLI；当前文档包没有 Rust 工作区，不能声称已经执行：

```bash
cargo tree --locked -p editor-service --edges normal
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
```

第一条用于审查服务的正常依赖闭包，后两条在 Windows／macOS 原生分别运行。测试使用真实 parser、核心命令、字体轮廓和 writer，覆盖 `AUTOMATION_API.md` 的异常及正常流程。GUI 实测仍按原用例执行，不由这些测试代替。

用例 JSON 自本修订起为 `schema_version=2`：`cases` 仅存有效用例，`retired_cases` 仅存退役迁移信息；逐项 `required_platforms` 决定所需证据。报告工具不能把旧版本 AT-079 的结果计入新基线，也不能将退役用例算“通过”。

## 19. V1 不可接受的捷径

- 只显示导入图形但对象编辑是假的，或保存的是未修改的源文件。
- parser 报告警告后继续漏画，再允许输出加工文件。
- 把所有 Dark 合并再减去 Clear，或者用背景色假装清除曝光。
- 用 AABB 点击测试替代精确命中；点在孔洞也选中。
- 用 f32 GPU 顶点或当前缩放 Mesh 回写制造几何。
- UI 每帧重解析／重建全部图形；性能数字只来自空画布。
- 中文只是画面上的 egui Label，导出的 Gerber 中没有轮廓。
- 不支持中文字形时替换问号却标记成功。
- 保存错误后仍清除脏标记，或写入一半覆盖旧文件。
- 只有 Windows 能编译，就把 macOS 标为已验收；或在本基线中偷偷重新加入 Linux 支持门槛。
- 只有空自动化接口；无界面编辑需要启动窗口／GPU；未来脚本只能模拟 GUI 点击。
- GUI 与服务各写一套变换／writer，批次失败留下部分修改，或自动化保存绕过确认／文件安全。
- 非 Gerber importer 直接把 DXF／SVG／PLT AST 硬塞进 Gerber parser/AST 或绕过显式 Manufacturing Conversion。
- 从 renderer Mesh、GPU 三角形、当前缩放路径或屏幕像素反向生成 Gerber／SVG／PDF／DXF 矢量制造数据。
- 自动更新验收阈值、删除失败样本或将必测项改为可选。

## 20. 交付物与验收签署

开发完成时交付：固定版本源码与 Cargo.lock、Windows／macOS 发行包和校验值、构建说明、能力矩阵、第三方通知、测试样本 manifest、自动结果与人工记录、性能原始数据、缺陷清单、最终验收报告，以及与实现一致的业务 API 契约、参数样例和双平台无界面调用证据。无需交付 Linux 包或脚本引擎。

完整 V1 的签署要求见 `ACCEPTANCE_V1.md`。**本文件中的架构、目录、接口和阈值是待实现要求；“有代码”“能编译”“某张截图正常”均不能替代整套验收。**

## 21. 一手资料索引与核实边界

查阅日期：2026-09-14。链接指向公开原始文档；S0 应把最终采用的版本、commit、LICENSE 和样本哈希记录到项目内。以下资料用于确认库职责和边界，不代表本项目已实测其兼容性。本文没有逐条审计 2026.05 规范全文。

- [S1] MakerPnP `gerber-parser` README：`https://github.com/MakerPnP/gerber-parser`
- [S2] MakerPnP `gerber-types` README：`https://github.com/MakerPnP/gerber-types`
- [S3] MakerPnP `gerber-viewer` 支持与限制：`https://github.com/MakerPnP/gerber-viewer`
- [S4] Ucamco 官方规范与测试文件下载：`https://www.ucamco.com/en/gerber/downloads`
- [S5] `lib_gerber_edit` API：`https://docs.rs/lib_gerber_edit/latest/lib_gerber_edit/`；源码说明：`https://github.com/nicolube/lib_gerber_edit`
- [S6] `gerber_ascii`：`https://docs.rs/lib_gerber_edit/latest/lib_gerber_edit/gerber_ascii/index.html`
- [S7] `Board` 图层约束：`https://docs.rs/lib_gerber_edit/latest/lib_gerber_edit/board/struct.Board.html`
- [S8] `egui-wgpu CallbackTrait`：`https://docs.rs/egui-wgpu/latest/egui_wgpu/trait.CallbackTrait.html`
- [S9] wgpu 官方平台与后端说明：`https://github.com/gfx-rs/wgpu`
- [S10] MakerPnP 主项目与许可说明：`https://github.com/MakerPnP/makerpnp`
- [S11] gerbv 官方仓库、许可及安全说明：`https://github.com/gerbv/gerbv`
- [S12] lyon tessellation 与 tolerance：`https://docs.rs/lyon_tessellation/latest/lyon_tessellation/`
- [S13] ttf-parser：`https://docs.rs/ttf-parser/latest/ttf_parser/`
- [S14] rustybuzz：`https://docs.rs/rustybuzz/latest/rustybuzz/`
- [S15] rustup Windows MSVC 工具链前置条件：`https://rust-lang.github.io/rustup/installation/windows-msvc.html`
- [S16] OpenAI Windows 原生沙箱：`https://developers.openai.com/codex/windows/`；说明文章：`https://openai.com/index/building-codex-windows-sandbox/`
- [S17] OpenAI `AGENTS.md` 指引：`https://developers.openai.com/codex/guides/agents-md/`
- [S18] Cargo.toml 与 Cargo.lock：`https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html`

## 22. 长期架构方向（钢网设计，2026-09-21 合并）

> 本章合并自《RCam 长期架构指导》，用于跨阶段防止遗漏已接受的架构决策。它描述**方向和阶段归属**，不表示已实现；
> 实际状态以各阶段 REVIEW 为准。当前开发主线 Mac-first，Windows 后置。决策记录见
> [ADR 0029](adr/0029-multi-gerber-workspace.md)、[ADR 0030](adr/0030-reusable-blocks-and-forward-reservations.md)，
> 开发约束摘要在 `AGENTS.md` 的 “Forward Architecture Reservations”。

### 22.1 产品定位

RCam 的主要用途是 **PCB 钢网/Stencil 制造图形设计、编辑、检查与工程管理**。除了普通 Gerber 编辑，优先级包括：精确制造几何、
多 Layer、钢网开口编辑、可复用 Block、对象捕捉、Grid/Snap/Measure、工程项目 `.rcam`、PCB Board Coordinate/PnP/RefDes、
大量重复开口的性能、Gerber/Drill 导入导出。

### 22.2 文件生命周期

- **Gerber = Import / Export format**，不是 RCam 的原生保存格式。Import 后 Gerber bytes → RCam semantic manufacturing model，
  与磁盘源文件解耦。不支持 live source link、写回原 gbr、基于 mtime 的重载、外部文件绑定；只保存 provenance（原文件名、
  imported SHA-256、导入时间）。
- **Native Project = `.rcam`**（S4-B2 定格式，S4-B3 做 New/Open/Save/Save As、Migration、Recovery、Recent Projects）。
  Export Gerber 不能清 project dirty，也不能成为 project save target。S4-B1 不冻结 `.rcam` 任何字段。
- **S4-B3 生命周期决策**：项目路径、Gerber 导入 provenance、Gerber 导出目标三者独立；Project Open 完整解码及语义验证后才替换当前会话；Save 采用同目录临时文件、完整复核与原子发布，成功后才清 project dirty。Layer View Style/Grid/Snap/制造精度参与 dirty；Active/Camera 保存时捕获但日常切换不触发 dirty；Solo/Selection 不持久化。Recent 属本机 AppPreferences，Recovery 属本机 cache，均不写入 `.rcam`。详见 ADR 0033/0034。
- Gerber 兼容：Extended Gerber/RS-274X 从 FS/MO/AD 自动确定；Legacy/Hybrid 在无歧义时规范化；纯 RS-274-D 或有歧义时**永不猜测**，
  进入 Legacy Import Modal，由用户提供格式/单位/零压缩/光圈表。

### 22.3 Multi-Layer Workspace 与 Layer UI

每个 Layer：稳定 LayerId、LayerKind、display_name、制造内容、View/Style 状态、可选 import provenance。Workspace 支持
Visible/Selectable/Locked/Color/Z-order/Active/Solo/Filled/Outline/ZeroWidth/Category Style；**View state 不得污染 Gerber Writer**。

Layer UI 采用单列 compact list（可调宽、有最小宽度）：`▶ ≡ ■ Layer Name 👁 🔒 ▣ ⋯`；高频项 Active、拖动/Z 序、颜色、
Visible、Locked、Display Mode、More；`Selectable` 与分类细项放 Layer Settings；右键与 `⋯` 同一菜单。宽度不足时名称 UTF-8 安全省略，
优先保证控件可用，完整名称可在 tooltip/Settings/Rename modal 查看；面板宽度是 UI preference，不属于制造状态。

颜色：确定性 auto palette + presets + recent + full picker；分类色继承图层色或覆盖，未来随 `.rcam` 持久化。

分类：Stroke、Circle、Rectangle、Obround、Polygon、ApertureMacro、ApertureBlock、RegionFreeform、GeneratedText、Other，
每类有 color/visible/selectable/locked。有效状态：`effective_visible = layer.visible && class.visible`；
`effective_selectable = effective_visible && layer.selectable && class.selectable`；`effective_locked = layer.locked || class.locked`。
Locked-but-selectable 仍允许 select/measure/snap/查看属性，但不允许制造编辑。

Display Mode：Filled（真实制造 composite）、Outline（制造边界 hairline）、ZeroWidth（Stroke 中心线，Flash/Region 轮廓 hairline），
全部属于 View State。

Layer 增删：`+` 提供 New Empty Layer / Import Gerber / 未来 Import Drill。删除：空层低风险直接删 + Undo；非空强确认；dirty/generated
更强确认；始终是 RCam Document 事务；允许删除最后一层，留下 empty Workspace。

### 22.4 Drill、Board Coordinate、PnP

- `LayerKind::Drill` 必须在 Layer model 预留；至少 DrillHit/DrillSlot/Route；每个 import 独立 Tool namespace；显示同样有
  Filled/Outline/ZeroWidth；Drill 同样只 Import/Export，不与磁盘源文件 live-link。
- 内部 canonical 仍是 Manufacturing World = f64 mm。预留 Source → Board → Manufacturing World 三种坐标，统一 `CoordinateTransform2D`，
  仅 translation/rotation/reflection，避免任意 scale/shear 进入制造定位语义。
- Component Placement/RefDes 是独立模型（ComponentId、refdes、BoardPoint、rotation、side、footprint、value），不塞进普通 Gerber
  SemanticObject；未来 `R123 → ComponentPlacement → Board→World → Camera Focus/Highlight`。

### 22.5 Reusable Block（S4-B2 已实现 Block Core，见 ADR 0032；本节继续作为长期方向记录）

钢网核心对象为 `BlockDefinition` + `BlockInstance`。Definition 是项目级可复用制造几何；Instance 属于某个 Layer，含
definition_id + translation/rotation/mirror，**不支持 non-uniform scale/shear，第一版禁止 nested block**。修改 Definition 更新所有
Instance；修改 Instance 只改 transform。Gerber Export 时 flatten；不要把 RCam Block 等同于 Gerber `%AB`。`.rcam v1` 冻结前必须已有 Block core。
S4-B2 起 `BlockDefinition` 存在 `SemanticDocument.block_definitions`，`BlockInstance` 是
`SemanticGeometry::BlockInstance` 变体；Move/Rotate/Mirror/Duplicate 复用既有 `objects.*` 服务，
`blocks.create_definition_from_objects`/`create_instance`/`update_instance_transform`/`rename_definition`/
`explode_instance`/`delete_definition` 是新增的 Service API。完整 GUI Block Editor 仍属 S4-C。

### 22.6 Object Snap 与 Grip

统一 `SnapKind/SnapFeature/SnapFeatureId/SnapQuery/SnapCandidate/SnapFeatureProvider/SnapResolver`，目标支持 Endpoint/Vertex、Midpoint、
Center、Quadrant、Intersection、Nearest，后续 Tangent/Perpendicular。Region/AM/Block 从**制造边界**提供 snap，不是 GPU/显示几何。
采用 “screen-radius → 附近对象空间查询 → lazy feature generation”，**禁止全局预生成所有 snap 点**。Grid/Object Snap 由同一 Resolver 处理。
Grip 不等于 Object Snap：现在只预留稳定 feature identity（`SnapFeatureId`/`GripFeatureId`）。

### 22.7 Command / Shortcut

建立 `CommandRegistry/CommandId/Keymap/ShortcutContext/ShortcutResolver/CommandDispatcher`。Menu/Toolbar/Context Menu/Shortcut 全部调用同一个
CommandId。Context 优先级：IME/TextInput > Modal > Tool > Canvas > Global。逻辑修饰键 Primary/Secondary/Shift/Alt（macOS Cmd / Windows Ctrl）。
用户 keymap override 属于 AppPreferences，不属于 `.rcam`。Automation API 不模拟快捷键，仍直接调用 ApplicationService。

### 22.8 阶段归属

| 阶段 | 必须包含 |
|---|---|
| S4-B1 | Multi-Layer Workspace、Layer UI/style/filter、layer add/delete/order、Gerber Import 语义；Drill Layer / Board Coordinate / Block / Object Snap / Command-Shortcut 架构**占位** |
| S4-B2 | Block Core、`.rcam` Native Project Model/schema v1、Workspace state 持久化、Snap settings 持久化 |
| S4-B3 | `.rcam` New/Open/Save/Save As、Migration、Recovery、Recent Projects |
| S4-C | 完整 Object Snap、Grip Editing、Block Editor、Explode、Array/Panelization、Alignment、PnP/RefDes、Component Search、Shortcut Settings/Command Palette |

### 22.9 基本不变量

无论未来增加什么功能：Manufacturing geometry = f64 mm；GPU/display = 局部/camera-relative f32；Renderer mesh/像素永不回推制造几何；
View/Workspace state 永不改变 Gerber 输出；Gerber Import 与源文件解耦；Gerber 是 Export 而非 Project Save；ApplicationService 是制造修改边界；
GUI/Shortcut/Automation 共享业务逻辑。

### S1-A.1 圆弧语义澄清

依据 [ADR 0007](adr/0007-s1-a1-arc-and-legacy-policy.md)，G75 支持合法非零 deviation 的目标，
G74 按最小 deviation 选择中心，相同端点按 0° 解释；原始字段与显示/覆盖曲线分离。
Region 中非零 deviation 仅在圆形解释位于原始环带、角度单调且不确定环扇区拓扑检查通过时接受；其余明确拒绝并公开限制。
此边界不删除 V1 要求、不改变 96 个验收身份、双平台证据或 CORE10 门槛。

### S1-B1 当前平台与编辑实施范围

按用户指定任务及 [ADR 0008](adr/0008-mac-first-s1b1.md)，当前开发/阶段验收采用 macOS arm64，
Windows 延后但最终双平台门槛不变。S1-B1 仅 Move、Undo/Redo、编辑后安全往返与服务生命周期。
CORE-03/07/08 和独立圆弧参考差异继续保留；不扩展 Gerber 兼容范围。

### S1-B1.1 / S1-B2a 来源与结构编辑

依据 [ADR 0009](adr/0009-s1-b2a-object-transactions.md)，当前顺序由 layer.objects 定义，
对象来源区分 Imported/Generated。复制按源当前顺序逐一插在源后，删除保留完整对象以便精确撤销。
历史区分修改／插入／删除，结构事务含前后 ID 顺序守卫；零 Move 不创建事务，保存身份区分来源和目标。
[ADR 0010](adr/0010-s1-b2b-transform-representation.md) 冻结后续旋转／镜像的可表示范围，
当前尚不开放其能力。这里只推进 Mac-first 服务闭环，不改变完整 V1 几何、CORE10 或双平台门槛。

S1-B2a 删除至空图层：允许合法完整 Gerber 文档含零图元，继续经过全命令／格式／单位／结束检查；
以真实空图像输出，不伪造零尺寸加工图元。详见 ADR 0009 删除至空图层补充。

### S1-B2b 实施澄清

刚性变换和表示限制依 ADR 0010 实施，局部验收不等同完整 V1。Region 两边闭合共享端点、
平行 cut-in 的旋转不变性修复见该 ADR，未提高几何容差。GUI 前状态/文档边界及结构历史债务
按 ADR 0011 单独实施；当前仍无 GUI 编辑和 layer.update。

### S1-B2c workspace 收口

ADR 0012 实施 ADR 0011 的模型边界：图层显示名/显隐/锁定属于服务 workspace，独立 workspace_revision，
不进入制造 hash、writer 或历史；新编辑检查锁，Undo/Redo 恢复制造事务不受当前锁阻断。
S2 hit-test/bounds 仅冻结精确 f64 边界，本轮不开放；其余 V1 门槛不变。

### S2-A.1 实施注记：制造边界查询

见 [ADR 0013](adr/0013-s2a-bounds.md)。先实现 GUI Fit 的 f64 制造边界前置闭环，
`document.bounds` / `layer.bounds` 与现有编辑/历史/导出服务共用同一版本模型。
包围全部 Dark/Clear 对象；Macro 为 Dark 原语保守包络，非最终布尔可见区域的最紧框。
其余 S2-A 点选/渲染/GUI 退出要求仍保留，不能用查询测试替代 Mac GUI 验收。

### S2-A.2 实施注记：对象材料点选

见 [ADR 0014](adr/0014-s2a2-hit-test.md)。`objects.hit_test` 提供独立对象材料闭包距离，
包含 Clear，不读取 workspace。支持标准/宏 Flash 孔洞和局部变换、真实扫掠、原圆弧
偏差/径向接线及 canonical Region；数值歧义/资源超限整次拒绝。
RectangularSweep 斜向仅作为独立查询算法验证，不改变轴向制造/导入/导出支持范围。
本轮停止在 S2-A.2 无 GUI 服务，S2-A.3 GUI 与双平台完整 V1 门槛保留。

Mac-first S4-B3 自动化门禁通过，最终原生 GUI/Recovery 复跑阻塞；当前停在 S4-C 前。Windows deferred / not executed；最终双平台 V1 要求保持不变。

### S2-B2 精确框选与多选

依据 ADR 0019，专用 objects.select_rect 保留 query contains 的原含义，新增 Window/Crossing
对象材料闭包关系。GUI SelectionSet 与预览不进入制造模型；同层多对象编辑复用一个原子事务。
跨层选择仅查看，制造操作整批拒绝；保持阶段几何选择的 Clear/locked 查看规则。
GeometryMetrics 的派生真值、lazy cache 和 Object/Layer Area 分界见 ADR 0018；本轮只冻结设计。

### S2-B3 对象指标

依 ADR0018，从制造几何计算独立对象面积/周长并lazy缓存，经objects.metrics与后台属性面板展示。
局部孔边计入周长；复杂union不可证明时明确unsupported。Object Metrics合计不代表最终Layer Area。
实现范围与局部AT映射见S2_B3_PLAN.md，不降低最终双平台/CORE10门槛。

### S2-C1 网格、吸附与测距

依 ADR 0022 实施 app-only Grid / Measure 状态和 f64 网格数值 helper。
局部 AT-044/045，不代表完整 Object Snap 或英寸显示已完成；最终 V1 门槛不变。

### S2-C2 Rotate / Mirror GUI

依 ADR 0023，Mac GUI 复用 S1-B2b 的 objects.rotate/objects.mirror。默认 Pivot/镜像轴来自所选对象
制造 bounds union center，任意角和自定义 Pivot 保持 f64 mm 且不经过 Grid Snap；一次 SelectionSet 操作
为一个服务事务/Undo。第一版不做 preview、Rotate Handle、Angular Snap 或自定义斜镜像轴。
Windows deferred / not executed；最终双平台 V1 门槛不变。

### S3-FINAL 实施注记

正式 S3 收口恢复设计阶段编号。标准 C/R/O/P Flash 尺寸编辑采用新 ApertureDefinition 和新
DCode 的写时复制，只重定向目标 Flash；Macro 明确拒绝。GUI 与 JSON/Rust 服务共用
`objects.set_properties`，Undo/Redo 同时恢复对象引用与生成定义，writer 重开复核定义无冲突。

`edit.batch` 当前实现已公布的 Move/Rotate/Mirror/SetProperties 子集：同一图层、显式对象 ID，
先在差量几何与光圈定义上完成全部校验，再一次提交、一次 revision、一个 Undo。打开、关闭、
导出及其他外部 I/O 不是 batch step；复制/删除仍用各自单事务操作，不冒充已实现的 batch step。

历史预算按完整事务淘汰最旧 Undo，报告累计 truncation；单个事务超过字节预算时在修改前拒绝。
Snap 使用 8 个 egui 逻辑点换算的制造半径，显式端点/中心优先于网格，稳定 ID 决胜，Alt 临时关闭；
单位切换只改变 Grid/坐标/测距显示，不改 f64 mm 模型。详见 S3_FINAL_COVERAGE.md。

### S4-A1 Mac-first vector text core

Scope and local AT mapping: [S4_A1_PLAN.md](S4_A1_PLAN.md); geometry, font,
resource and API decisions: [ADR 0024](adr/0024-s4a1-vector-text.md).
AT-047–052 headless coverage does not replace GUI/IME, independent viewer
or Windows evidence. Final V1 and CORE10 thresholds remain unchanged.

### S4-A2 foundation (historical; superseded by S4-A2.1/A2.2 PASS)

See [ADR 0025](adr/0025-s4a2-text-gui.md) for bounded configurable manufacturing
precision and read-only service preview. The default remains 0.00025 mm and
the total 0.001 mm threshold is unchanged. GUI/IME/offset subsequently passed the bounded S4-A2.1/A2.2 Mac-first gates.

### S4-A2 Canvas UX 增补与后续基础层顺序（2026-09-20）

S4-A2 同步包含网格显隐渐变、连续物理像素 LOD 和极限缩放保护。
网格视觉状态只在 app/view 层，默认 180 ms；制造网格和 Snap spacing 不变。
显示路径为缓存 f64 制造包围盒查询 → 保留曝光顺序 → camera-relative
render origin → 局部 f32。显示精度瞬态失败保留最后有效帧，制造无效仍拒绝。
不得仅隐藏“zoom out”错误或放宽资源上限。设计与数值分配见 ADR 0025。

**Global Units & Manufacturing Precision Foundation — BLOCKING BEFORE NEXT
MAJOR FEATURE STAGE**：S4-A2 后首先收口 mm/inch/mil/µm 全局显示单位和
用户要求的默认 0.1 µm 制造精度；先冻结换算、舍入与误差用例，再进入后续
正式功能阶段。本轮文字和 writer 的既有精度不会据此被静默改写。
这项排期不代表该基础层已实现，也不改变双平台 V1 退出条件。


### S4-A2.1 — 参数弹窗、文字轮廓和浮动放置

当前实现与验收边界见 docs/S4_A2_1_PLAN.md、docs/S4_A2_1_REVIEW.md、
ADR 0027 / 0028（docs 内路径去掉 docs/ 前缀）。
参数型功能使用独占 Modal；连续画布操作保持直接交互。
Vertical slab text geometry = retired；contour/Line/Arc Region = production path，
每个材料连通组件一个对象，字洞使用局部 retraced cut-in，writer 不经过 slab。
Mouse 文字先生成再浮动，仅平移预览，左键提交一个事务；取消不改制造内容。
GeometryMetrics 周长排除 cut-in 接缝，但对象合计不是图层最终布尔周长。

S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded，见 GLOBAL_UNITS_PRECISION_REVIEW）。
S4-B1 Multi-Gerber Workspace 已实现并通过云端自动测试，原生 Mac 验收待执行，**尚未标记 PASS**（见 S4_B1_REVIEW）；DXF/SVG/PLT、Final Layer Boolean Area、Windows、`.rcam`（S4-B2）仍未启动。
阶段实现不等于全部原生验收；实际状态以 S4_A2_1_REVIEW 为准。

### S4-A2.2 text interaction amendment

The explicit user request supersedes the earlier single-line-only and floating-Esc
cancel rules. See [S4-A2.2](S4_A2_2_PLAN.md): multiline text, an original built-in
ASCII centerline font, selectable local outline fonts, Confirm → floating → click,
and floating Escape → resume draft modal. Manufacturing changes still use one
ApplicationService transaction. Global Units is the current slice; Windows remains deferred.


### Global Units / Manufacturing Precision (current)

See [plan](GLOBAL_UNITS_PRECISION_PLAN.md) and the corresponding review for current evidence.
S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded）。
Display uses camera-relative local f32, safe zoom clamp and last-good-frame.
Windows deferred / not executed；不宣称完整 V1、P100K 或完整 CORE10 release。

### S4-B1 Multi-Gerber Workspace（2026-09-21）

实现范围、任务书边界和验收状态见 [S4_B1_PLAN](S4_B1_PLAN.md)、[S4_B1_REVIEW](S4_B1_REVIEW.md)。
一个 Workspace 含多个独立 Gerber Layer；Gerber 只 Import/Export（第 22 章）；Export 不清 dirty、不建 source link；
Save/Save As 为 `.rcam` 保留。View state（颜色/可见/可选/锁定/层序/显示模式/分类样式/面板宽度）不影响 writer bytes。
不冻结 `.rcam`；S4-B2 待审查后另行启动。原生 Mac 验收未执行前不声称 S4-B1 PASS。
