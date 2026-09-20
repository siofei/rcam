# Gerber 编辑器 V1：脚本自动化扩展接口设计

> 文档版本：1.1 · 日期：2026-09-14  
> 对应需求：R21、R22；关联平台范围用例 AT-085，接口验收 AT-086—AT-097。  
> 目标平台：Windows x64、macOS Apple Silicon。**不要求 Linux、WSL2、服务端或浏览器运行环境。**  
> 状态：待实施设计。下列类型名、操作名和 JSON 是本项目拟定的契约，不是第三方现成 API，也不是已交付的软件接口。

## 1. 本次预留的边界

第一版需要把业务能力做成**不依赖窗口和 GPU、可由代码直接调用的应用服务**，并使用真实的无界面集成测试证明它能完成打开、查询、编辑、导出和重新解析。GUI 本身也调用这套服务，不能只为测试另写一个简化编辑器。

这不等于第一版交付脚本产品。Python／Lua／JavaScript 引擎、脚本编辑器、脚本管理和录制回放、正式 CLI、JSON-RPC／HTTP 服务、远程控制、插件加载及定时任务均不属于 V1 必交范围。先冻结边界和可验证的最小协议，再根据实际使用场景选择脚本语言及适配形式。

| 层次 | V1 必须完成 | 后续再增加 |
|---|---|---|
| 业务调用 | GUI 与测试共用 `ApplicationService`，读写分离 | 脚本适配器接入同一服务 |
| 数据契约 | 带版本的命令／查询／结果 DTO；JSON 编解码与校验测试 | 跨进程传输、正式 SDK 和兼容发布策略 |
| 执行语义 | 稳定 ID、明确单位、版本检查、事务、取消、结构化错误 | 多文件工作流编排、任务持久化与恢复 |
| 文件边界 | 显式路径和保存策略、权限入口、不弹窗等待 | 多租户隔离、远程鉴权及不可信脚本沙箱 |
| 验证方法 | Windows／macOS 上运行无窗口 Rust 集成测试 | Python 等语言实际脚本示例与发布验收 |

**只有空 trait、一个未使用的 `automation` 目录或此文档，不算完成接口预留。反过来，不得为了通过接口验收，在 V1 额外搭建常驻网络服务或嵌入脚本解释器。**

## 2. 依赖方向

```text
现有 egui GUI                         未来脚本／CLI 适配器
  │ 屏幕输入 → 明确的业务参数                    │
  └────────────────┬───────────────────────────┘
                   ▼
          ApplicationService（editor-service）
           ├─ query：只读查询和能力发现
           ├─ execute：校验、事务、版本和结果
           ├─ jobs：进度、结果查询、取消
           └─ events：进程内通知接口
                   │
          ┌────────┼───────────┐
          ▼        ▼           ▼
      editor-core gerber-io  editor-text
          │        │           │
          └────────┴───────────┘
                   │
          主机提供的文件／字体／任务端口

renderer-wgpu ← 只读快照／变更集；不参与业务接口的启动
```

`editor-service` 的正常依赖闭包不得包含 `egui`、`eframe`、`egui-wgpu`、`wgpu`、`winit` 或原生窗口／文件对话框库。系统文件 I/O 不等于窗口依赖；本地文件实现可以存在于服务层的受控适配模块。测试应只构建服务包及其依赖，不靠禁用 GPU 渲染但依然启动窗口来冒充无界面运行。

公开边界使用本项目拥有的类型。不得泄露 MakerPnP AST、`egui::Context`、GPU Buffer、裸指针、可变文档引用或依赖某个 GUI 控件的回调。第一版可以将 DTO 和服务放在一个 crate；不要求引入消息中间件、动态插件 ABI 或通用依赖注入框架。

### 2.1 GUI 与服务的责任

GUI 负责鼠标坐标转换、当前选择、面板焦点、工具预览、对话框和用户确认。服务负责能力／锁定／数值校验、所有已提交的文档修改、Undo/Redo、异步业务任务和安全导出。

鼠标拖动预览不需要逐帧发业务命令。释放时，GUI 把选择集解析成明确的对象 ID，把偏移转换成毫米，再提交一次与脚本相同的 `objects.move`。缩放、滚动和选中高亮可以留在 UI 视图状态中，不制造无意义的业务历史。

**属性栏、快捷键、图层锁定以及文字创建都不得绕过服务直接写文档。** 后续脚本不需要模拟点击按钮，也不依赖某个图层恰好处于活动状态。

## 3. 请求、结果和类型约束

V1 在进程内提供强类型调用；同时为公共 DTO 提供 JSON 编解码测试。JSON 只是语言无关的数据边界，**不是已经开启的网络协议，也不表示 Rust 的二进制 ABI 稳定**。

### 3.1 请求信封

| 字段 | 契约 |
|---|---|
| `api_version` | 整数；初始值为 `1`，与文档版本 `1.1` 分开管理 |
| `request_id` | 非空字符串，由调用方生成；用于关联结果和日志，不承诺幂等或恰好执行一次 |
| `op` | 白名单操作名，如 `objects.move`；未知操作必须拒绝 |
| `document_id` | 对已有文档的查询／修改／导出必须提供；打开新文档和查询全局能力时不提供 |
| `expected_revision` | 修改已有文档及启动导出时必须提供的十进制整数字符串；只读查询可省略 |
| `params` | 对应操作的强类型参数；V1 对未知字段及不支持的参数组合明确报错，不静默忽略 |

调用上下文如文件权限、任务预算和调用来源由主机传入，不接受请求自己声明 `trusted=true` 来提升权限。

以下为参数形状示例，ID 必须来自真实查询结果，不能直接照抄示例标识执行：

```json
{
  "api_version": 1,
  "request_id": "move-001",
  "op": "objects.move",
  "document_id": "doc-demo",
  "expected_revision": "12",
  "params": {
    "layer_id": "layer-demo",
    "object_ids": ["object-a", "object-b"],
    "dx_mm": 5.0,
    "dy_mm": -3.0
  }
}
```

### 3.2 统一结果

结果包含 `api_version`、原样返回的 `request_id`、`status`、`document_id`、`revision`、`result`、`warnings`、`error`、`job_id`。不适用字段为 `null`；状态只使用 `completed`、`accepted`、`confirmation_required`、`error`。

`accepted` 仅表示长任务已排队，绝不等同于操作完成。最终结果通过 `jobs.get` 或进程内事件取得。失败及需要确认的调用，不得先修改部分文档再让调用方处理错误。

```json
{
  "api_version": 1,
  "request_id": "move-001",
  "status": "completed",
  "document_id": "doc-demo",
  "revision": "13",
  "result": {"changed_object_ids": ["object-a", "object-b"], "undo_entries_added": 1},
  "warnings": [],
  "error": null,
  "job_id": null
}
```

`error` 使用机器可判定的 `code`、中文 `message` 和结构化 `details`，不要要求脚本解析一整句弹窗文本。至少区分：`INVALID_ARGUMENT`、`UNSUPPORTED_API_VERSION`、`UNSUPPORTED_OPERATION`、`UNSUPPORTED_FEATURE`、`NOT_FOUND`、`LAYER_LOCKED`、`REVISION_CONFLICT`、`RESOURCE_LIMIT`、`PERMISSION_DENIED`、`CONFIRMATION_REQUIRED`、`FILE_CONFLICT`、`IO_ERROR`、`VALIDATION_FAILED`、`CANCELLED`。非法 JSON／类型／未知字段统一落到 `INVALID_ARGUMENT`，并报告具体字段位置。

### 3.3 单位、精度和身份

业务距离统一为 `f64` 毫米，角度参数使用明确后缀 `_deg`；对 JSON 数字按 f64 读取并校验有限值。NaN、Inf、超范围数值、非法光圈尺寸和空目标集合都要有明确结果。核心制造精度沿用设计第 6 节，不能为脚本接口改为屏幕整数或 f32。

文档／图层／对象／任务 ID 使用不透明字符串，不是数组索引、DCode、文件名或内存地址。稳定性范围为当前文档会话；删除后撤销可以恢复原 ID，但被删除对象的 ID 不能分配给另一个对象。关闭后重新导入允许重新分配 ID；跨会话自动化必须根据源文件哈希和明确的几何查询重新定位，不得将临时 ID 当永久工程标识。

`revision` 用十进制字符串承载递增整数，避免未来某些脚本语言数值表示范围造成精度丢失。每次成功的内容修改以及 Undo／Redo 都推进 revision；**Undo 恢复内容，不倒退 revision**。是否已保存另用内容基线判定，不能把 revision 与脏状态混为一谈。

## 4. V1 应暴露的最小业务能力

这些操作对应既有第一版功能，不扩大 Gerber 支持子集。操作的参数定义和测试需要随实际实现补全并冻结在仓库内；不能只有操作名称而无实现。

| 能力组 | 操作与最小参数／结果 |
|---|---|
| 能力发现 | `system.capabilities`：返回 API 版本、已实现操作、支持的 Gerber 子集、精度、资源上限与功能开关 |
| 文档生命周期 | `document.open`：明确输入路径，返回文档／图层 ID、revision、能力报告；`document.import`：导入到指定文档；`document.close`：未保存内容必须有明确放弃策略 |
| 只读查询 | `document.get`、`layers.list`、`objects.query`、`objects.get`：返回只读 DTO、revision 和需要的分页信息 |
| 图层 | `layer.update`：显隐、锁定、名称等已实现的 V1 属性；不能隐藏写入活动层副作用 |
| 基础编辑 | `objects.move`、`objects.rotate`、`objects.mirror`、`objects.duplicate`、`objects.delete`、`objects.set_properties`：指定图层和对象 ID；旋转中心／镜像轴必须明确 |
| 文字 | `text.create`：内容、目标图层、位置、毫米字高、字距、对齐、旋转、显式字体来源／哈希；返回生成对象 ID |
| 原子编辑 | `edit.batch`：同一文档内多个可逆对象编辑组成一个事务；`history.undo`、`history.redo`：明确文档和预期 revision |
| 校验和保存 | `document.validate`、`gerber.export_layer`：明确图层、目标路径、覆盖和元数据策略；返回几何诊断、导出 revision、文件 SHA-256 |
| 长任务 | `jobs.get`、`jobs.cancel`：指定 job ID；不依赖窗口消息循环或弹窗 |

图形剪贴板是 GUI 辅助功能。无界面复制应直接使用 `objects.duplicate` 的源 ID、目标图层和位移，返回新对象 ID，不要求读取系统剪贴板。

能力发现只列实际实现并通过对应检查的操作；AM／SR／Excellon 等范围外能力按当前支持矩阵拒绝。不能因为底层 parser 认识语法，就向脚本宣称可以安全编辑并保存。

### 4.1 查询不能依赖当前界面

`objects.query` 至少支持明确图层、对象类型、物理矩形以及 `contains`／`intersects` 几何关系；可组合光圈类别等已实现条件。范围查询返回精确几何判断结果，不能仅靠空间索引的 AABB 候选冒充最终结果。

默认查询的是文档对象几何，不随缩放、当前选择、可见状态或活动图层改变；图层必须明确指定。需要按显隐或锁定筛选时用显式参数。UI 点选的“最终可见对象”语义与脚本几何查询不同，应分别命名，不偷换为同一种查询。

结果顺序固定为文档图层顺序、层内曝光顺序，必要时以稳定 ID 作最终排序。分页游标绑定文档 ID、revision 和查询条件；期间文档变化，返回 `REVISION_CONFLICT`，不得让调用方漏选或重复编辑。

## 5. 事务、并发和 Undo

### 5.1 单写者和版本检查

每个文档由服务串行提交修改；耗时读取、解析和文字轮廓生成可以在后台完成。开始操作时及提交时检查预期 revision，关闭文档或版本变化后不得提交过期编辑结果。

GUI 查询后产生编辑命令同样携带 revision；版本冲突时重新查询或提示，不自动套用到“最新选中对象”。失败、拒绝和预览取消不推进文档 revision，不新增 Undo 记录。

`request_id` 只是关联 ID。V1 不提供跨进程去重、持久重试或 exactly-once 保证；未来适配器不得在超时后盲目重发复制／移动命令。应先查询任务或文档状态，再决定如何恢复。

### 5.2 批量原子操作

一次 `edit.batch` 包含同一文档、已有对象 ID 可表达的可逆对象编辑。V1 可以限定为移动、旋转、镜像、复制、删除和属性修改；不要求步骤变量绑定、嵌套批次或跨文件事务。

先预检操作类别、目标、能力、锁定、数值、Undo 和临时内存预算，再以差量／暂存结果验证每个步骤。中途有任何失败或提交前取消，全部回滚，文档内容、对象顺序、光圈引用及历史不变。成功后整体推进一次 revision，并只增加一个 Undo 事务。

`document.open/import/close`、导出文件、字体文件 I/O、启动其他任务及外部进程不能混进可回滚编辑批次。**内存事务回滚不等于已写入文件也能撤销。** 后续多文件自动化需单独设计计划、逐文件输出和失败清单，不能误用本事务承诺全流程原子性。

第一版不开放远程 `begin/commit` 长期持锁会话。批次大小和内存受配置上限约束，不能依靠深拷贝百万对象文档来实现任意规模回滚。

## 6. 非交互式确认和文件安全

应用服务不能弹出文件选择器或确认框，也不能调用 `stdin` 等待输入。需要确认的状态通过结构化结果返回；GUI 收到后显示对话框，未来脚本适配器按显式策略处理。

### 6.1 文件与字体路径

输入路径、输出路径、字体来源由调用方明确传入。相对路径按主机配置的固定工作目录解释，不按某个窗口最近打开目录或临时进程工作目录猜测。Windows／macOS 的中文、空格和 `#` 路径沿用既有验收。

使用主机侧 `FileAccessPort`／策略入口约束允许读取和写入的路径，GUI 通过用户选文件授权相应范围，无界面测试注入明确的临时目录权限。路径检查要考虑规范化后的实际目标、符号链接及 Windows 重解析点，不能只比较字符串前缀。权限拒绝时不读写目标、不先创建输出文件。

这是一条应用访问边界，不是对任意不可信本地代码的操作系统沙箱保证。未来嵌入脚本运行时时仍须单独审查文件、网络、进程和执行资源隔离。

字体在无界面调用中使用明确来源及哈希；缺失时返回错误，不依赖“系统默认中文字体正好存在”，也不进入字体选择弹窗。本包不附带或分发字体文件。

### 6.2 导出策略

`gerber.export_layer` 至少包含：

```json
{
  "layer_id": "layer-demo",
  "path": "outputs/钢网_修改版.gbr",
  "overwrite": {"mode": "deny"},
  "metadata_policy": {"mode": "require_confirmation"}
}
```

目标不存在且没有需确认的数据损失时，可以正常导出。目标已存在时，默认 `deny` 不覆盖，返回 `confirmation_required`，说明原因与当前目标哈希。调用方只有在明确授权后，才能使用 `replace_if_unchanged` 并传入 `expected_sha256` 重试。

需要移除元数据时，默认 `require_confirmation` 返回具体类别；明确确认后改用 `drop_listed` 并提供获准移除的 `categories`。实际需要移除的类别超出已授权列表时仍需确认，不能用一个全局 `force=true` 绕过诊断。

覆盖授权只针对目标文件身份／哈希，不绕过 Gerber 能力、几何、数值、权限或资源校验。目标被外部修改返回 `FILE_CONFLICT`。使用与 GUI 相同的临时文件、验证、同步和平台安全替换流程，禁止复制一套简化 writer。

跨进程竞态不是完整文件系统事务；优先导出到新路径。未通过该平台并发／故障验证的覆盖实现不得启用自动覆盖，不能声称哈希检查消除了所有外部写入竞态。

### 6.3 快照与保存状态

长时间导出冻结开始时已校验的文档 revision，输出文件明确记录本次 `exported_revision` 和 SHA-256。期间用户继续编辑，不改变已冻结的导出几何；完成时若当前内容已不同，必须继续保持当前内容未保存，并在结果中标明当前 revision 与导出 revision 的区别。

重新保存或覆盖不属于 Undo 的文件系统回滚范围。取消或失败不得破坏原目标、错误清除脏标记或向调用方返回成功。

## 7. 任务、事件和资源预算

长任务使用 `JobId` 和状态 `queued → running → completed / failed / cancelled`。提交返回 `accepted`；通过 `jobs.get` 可取得阶段、可用进度、诊断、结果、输入文档 revision 和输出证据。无法准确计算百分比时返回阶段／已处理数量，不伪造精确进度。

`jobs.cancel` 发送取消请求，在安全检查点终止。修改提交／文件替换已完成时，不能返回“已取消且没有产生任何影响”；返回 `too_late` 或最终完成结果，并报告实际副作用。取消回馈和资源释放门槛沿用设计第 13／14 节。

进程内事件可定义为 `DocumentChanged`、`JobProgress`、`JobFinished`、`DiagnosticRaised`，携带 request ID、job ID、文档 ID／revision 等适用字段。不强制 V1 实现外部订阅协议或持久事件日志。事件队列必须有界；进度允许合并，但任务最终状态必须可由查询取得。

UI、任务与未来脚本共用相同的对象上限、字体轮廓限制、Undo 预算、线程数和取消检查，不开放一个“脚本无限制”入口。日志默认只记录操作、数量、耗时、结果和必要定位，不自动写出整个私有 Gerber 或上传远端。

## 8. 最小无界面验收流程

在 Windows 和 macOS 上，用仅依赖 `editor-service` 的 Rust 集成测试完成：

```text
构造服务，注入临时目录／字体授权和任务配置
  → system.capabilities
  → document.open（冻结的小型 Gerber 样本）
  → layers.list / objects.query（按明确几何条件取得对象 ID）
  → objects.move / objects.duplicate / objects.rotate / objects.mirror
  → objects.set_properties / objects.delete（另用小样覆盖）
  → text.create（固定字体标识与哈希）
  → edit.batch / history.undo / history.redo
  → document.validate
  → gerber.export_layer（新目标文件）
  → 等待任务终态
  → 在新文档重新打开并执行独立数值断言
```

每次修改都读取上次返回的 revision，不把示例字符串当真实结果。查询对象 ID 不能通过共享内存读取私有字段获得，必须走公共查询入口。

无界面输出与 GUI 使用相同业务参数后的输出，要比较制造几何、曝光顺序、关键孔洞和尺寸。不要求跨平台临时 ID、绝对路径、时间戳或整份 Gerber 字节哈希相同；对确定性规范化输出可额外比较字节，但不能替代语义比较。

同一快照内 JSON 序列化／反序列化须保持有限浮点参数和 ID 不丢失。跨平台几何比较按设计容差，不因字体文件不同而放宽；需要中文场景时先固定合法字体哈希。

开发期计划执行命令如下；只有实现对应包和测试后才可运行：

```bash
cargo tree --locked -p editor-service --edges normal
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
```

测试必须实际完成文件和命令操作，不能只是 mock 返回成功。可以用端口注入文件故障、拒绝权限和取消时机，但正常流程需要真实 parser、核心命令和 writer。自动化验收不替代既有 GUI／IME／GPU 真机检查。

## 9. 对应阶段和后续接入

| 阶段 | 本次新增的落点 |
|---|---|
| S0 | 确认服务依赖边界、DTO／版本规则和后续语言未定的 ADR；不加载解释器 |
| S1 | 无窗口打开、查询、能力诊断和导出；实现文件权限／确认结果边界 |
| S2 | GUI 使用只读服务查询和共享快照；视图状态与业务状态分开 |
| S3 | 所有 V1 编辑通过服务；实现单写者版本检查、原子命令／批次、Undo/Redo |
| S4 | 文字、保存策略、任务取消与非交互错误闭环 |
| S5 | 两平台无界面契约测试及 GUI 一致性检查，原有性能门槛不降低 |
| S6 | AT-086—AT-097 通过并归档；公开说明仅预留业务 API，未发布脚本引擎 |

未来接入 Python、Lua、JavaScript 或命令行时，新增薄适配层：参数编解码 → 服务调用 → 结果／事件转换。禁止在适配层重新写几何变换、直接改文档内部对象或绕过安全保存流程。语言选型、绑定库、通信协议及安全模型在那一阶段单独决策。

## 10. 第一版不通过的情形

无界面调用必须创建 `eframe` 窗口或 GPU；脚本只能模拟点击；GUI 与测试使用不同编辑实现；命令依赖当前选择／当前活动层；没有明确单位；未知命令默默成功；失败批次留下部分修改；Undo 回退 revision 导致旧请求重新有效；保存等待弹窗；自动覆盖与权限判断被绕过；导出失败仍返回成功；只有文档接口没有真实测试——以上均不满足本次预留要求。

## S0-B 实施注记（2026-09-15）

以上各节仍是完整 V1 契约。本轮只实现四个原有 S0 只读操作的统一响应与结构化错误，
保留 document.open_s0 的源文本/调用方文档 ID 临时形状；不是正式 document.open 路径接口。
`execute_json` 的 Rust 返回类型已迁移为 JSON Value，成功/业务失败/解码失败均有同一信封。
可可靠取得的 request_id 回显，歧义/畸形输入为 null。对应回归测试与尚未启用的生命周期设计见 ADR 0003。

## S1-A 接口形状（2026-09-15）

本节描述本轮实现的窄接口；实际执行结果见 `S1_A_REVIEW.md`，不代表完整 V1 契约或双平台通过。
S0 演示仍使用原四个接口。宿主须通过 `ApplicationService::with_file_access` 注入
`FileAccessPolicy::new(working_directory, read_roots, write_roots)`，才启用 S1 文件入口。
默认构造器没有文件授权；请求不能增加授权目录。路径规范化后检查真实目标，读入时限制字节数。

| 操作 | params | 结果要点 |
|---|---|---|
| `document.open` | `{path}`；无 document_id | 新文档/图层 ID、revision="0"、读入时计算的 source_sha256、诊断 |
| `document.get` | `{}` | 文档来源和只读信息 |
| `layers.list` | `{}` | 图层信息数组，含 layer_id/name/object_count |
| `objects.query` | `{layer_id, geometry_type?, region_mm?, relation?, limit?, cursor?}` | 有序 objects、total、next_cursor、revision |
| `objects.get` | `{layer_id, object_id}` | 对象只读 DTO |
| `document.validate` | `{}` | 真实语义校验结果 |
| `gerber.export_layer` | `{layer_id,path,overwrite,metadata_policy}` | exported_revision/current_revision、path、sha256、bytes |

除 open 外须提供 document_id；export 还须提供十进制字符串 expected_revision。
本阶段没有内容编辑，revision 保持 0，旧版本导出请求仍返回 REVISION_CONFLICT。
未知字段（包括嵌套字段）、版本和操作严格拒绝。所有新操作均在进程内同步调用，无窗口依赖；
尚未接入完整 GUI 的后台任务，也未实现 Move/Undo/Redo、jobs、文字、脚本或网络服务。

查询 region_mm 字段为 min_x_mm/min_y_mm/max_x_mm/max_y_mm，关系为 contains/intersects。
分页上限 1000，游标绑定文档、revision 和过滤条件。当前物理矩形查询仅开放已经实现的
圆形 Flash、圆形线段、轴向矩形扫掠精确判断；其他几何或变换组合明确返回 UNSUPPORTED_FEATURE。
未指定物理矩形时可按图层/类型枚举对象。这不等于完整 AT-089 或 GUI 选择验收通过。

导出只接受 overwrite.mode="deny"；已有目标返回结构化待确认结果，但本阶段仍不提供覆盖实现。
元数据默认 require_confirmation；实际有待移除类别时不写目标，调用方使用
`{mode:"drop_listed",categories:[...]}` 明确授权全部受影响类别后重试。
IN/LN 与属性均作为来源诊断处理，不作为稳定 ID；无损失时不重复确认。
共享 IO Writer 负责语义/数值校验和规范化往返核对，服务负责主机权限、revision 和元数据策略。
临时文件只发布到新路径，失败不覆盖已有目标。

### S1-A.1 圆弧查询说明

现有对象 DTO 的 `Arc` 保留输入端点、声明圆心、方向和全圆身份；可选 `source` 包含
`resolution_mm` 与 `single_quadrant`。这些是自有只读数据，不暴露 parser AST 或覆盖缓存。
G74 零弧导出可规范化为同位置/同圆光圈宽度的 G01 零长度 stroke；重新打开后对象类型可能为 Line，
但 dot 覆盖必须保持。不能以原始 G74 零弧的 ObjectId/类型要求跨会话恒定身份。
本轮不新增操作名，capabilities 明确公布受测圆弧/Region 边界，详见 ADR 0007。

## S1-B1 已实施接口（Mac-first）

本节覆盖前文 S1-A 的无编辑限制；整体 V1 设计仍有未实施操作。依据 ADR 0008。
宿主授权后的 capabilities 新增 objects.move、history.undo、history.redo、document.close；read_only=false。
默认无文件授权构造器与 S0 文档仍只读。公开 params/result DTO 可 JSON 编解码。

| 操作 | params | 结果 |
|---|---|---|
| objects.move | `{layer_id,object_ids,dx_mm,dy_mm}` | revision、changed_object_ids、undo_entries_added=1、undo_entries、redo_entries、dirty |
| history.undo / history.redo | `{}` | revision、changed_object_ids、undo_entries_added=0、undo_entries、redo_entries、dirty |
| document.close | `{discard_changes?:false}` | `{closed:true}`；脏内容默认 confirmation_required，明确 true 才放弃 |

这些操作均要求真实 document_id 和 expected_revision。成功 Move/Undo/Redo 推进一次 revision；
同调用的对象按曝光顺序一起提交。空集、重复 ID、非有限/越界位移拒绝；未知目标 NOT_FOUND，
锁定层 LAYER_LOCKED，预算超限 RESOURCE_LIMIT，旧版本 REVISION_CONFLICT，无历史 INVALID_ARGUMENT。
失败不变更内容/revision/历史，也不清除 Redo。关闭移除该会话历史，新打开分配新身份。
S1-B1 当时未开放 layer.update；S1-B2c 已迁移到 service workspace，见文末 ADR 0012 契约。

上限为每次 10000 对象、100 条历史、64 MiB 保守历史内存计费（含提交峰值/Redo）；超限整体拒绝。
Flash、Line、轴向 RectangularSweep、Arc 和完整 Region 可平移，光圈定义不变。
圆弧保留 full/zero sweep、方向和源分辨率；移动后重新运行原几何合法性检查，不扩大输入兼容范围。
查询实时读取模型，旧分页游标在修改/Undo/Redo 后失效；没有独立缓存需要更新。
现有 contains 为“对象包含查询矩形”，intersects 为相交；完整几何查询仍受 S1-A 限制。

文档信息新增 dirty/undo_entries/redo_entries，图层查询增加 locked。
成功的新路径导出以当前内容哈希更新保存基线；失败/待确认不更新，也不清除历史。
当前只支持单层导入文档，未来多层导入须扩展为逐层保存基线。
所有接口仍同步；GUI 后台任务、edit.batch、其他变换、文字和完整保存产品流程未交付。

## S1-B1.1 / S1-B2a 已实施接口（Mac-first）

依据 ADR 0009，覆盖前述阶段的复制／删除缺失说明。仍不实现跨图层剪贴板、edit.batch 或异步任务。

| 操作 | params | 结果 |
|---|---|---|
| objects.duplicate | `{layer_id,object_ids,dx_mm,dy_mm}` | 一个原子事务；changed_object_ids 为全新副本 ID，按源曝光顺序返回 |
| objects.delete | `{layer_id,object_ids}` | 一个原子事务；changed_object_ids 为删除的 ID，按源曝光顺序返回 |

两者都必须携带 document_id 与 expected_revision，返回已有 EditResult 结构；参数严格拒绝未知字段。
同一图层内，各副本插入各自源对象之后；原对象及副本各自相对曝光顺序保持，不能解释为隔离图像复制。
成功 Duplicate 才消耗单调 ID；Undo/Delete 不回收，Redo 恢复相同 ID、几何、极性、origin 和位置。
零偏移 Duplicate 仍创建新对象；零 Move／f64 舍入后所有几何未变的 Move 返回 INVALID_ARGUMENT。
空集、重复／未知 ID、锁定层、非法坐标、数量／历史预算超限均原子拒绝，Redo 仅被成功的新编辑清空。
max_edit_objects=10000（max_move_objects 保留同值）；100 条／64 MiB 历史预算继续适用，
插入还检查文档 500000 对象／2000000 Region 边上限。字节预算含前后 ID 顺序守卫与暂存合并数组。

ObjectInfo.object 的 source_command 替换为 origin：
`{"kind":"imported","command_index":123}` 或 `{"kind":"generated","operation_id":"..."}`。
这是开发期 DTO 迁移；调用方不得从来源序号推断当前顺序，顺序以 objects.query 返回数组为准。
分页游标在插入／删除／Undo／Redo 后均因 revision 改变而失效。
NOT_FOUND 的 details 统一为 `{"entity":"document|layer|object|aperture","id":"..."}`。

document.get/open 的 last_saved_path 初始 null；成功导出后为规范化目标路径。
source_path/source_sha256 始终指向打开来源，未因导出改写；GUI 后续要分别显示来源与最后保存位置。
失败导出保留 last_saved_path、dirty 和历史。Undo/Redo 不改变最后写入文件路径，dirty 仍按保存内容基线判断。
旋转／镜像仅冻结 ADR 0010，仍不在 supported_operations 中。

## S1-B2b 已实现的局部变换契约

Mac-first 无窗口服务新增 `objects.rotate` / `objects.mirror`，实现与验证见 S1_B2B_REVIEW.md。
两者使用现有信封、显式 layer_id/object_ids、expected_revision 和 EditResult；成功一次 revision
与一个 Modify Undo，失败不改内容、ID、历史、Redo 或保存基线。对象最多 10000，沿用历史预算。

Rotate params：`{"layer_id":"…","object_ids":["…"],"angle_deg":37,"pivot_mm":{"x_mm":10,"y_mm":20}}`。
正角为世界坐标逆时针；有限角规范化 [0,360)，整周零旋转和全部候选状态不变 INVALID_ARGUMENT。
Mirror params：`{"layer_id":"…","object_ids":["…"],"axis":{"kind":"horizontal","coordinate_mm":0}}`。
horizontal 表示 y=c，vertical 表示 x=c；斜轴和未知嵌套字段拒绝。所有数值必须有限且结果精度可靠。

Flash 同时变换中心和局部方向、保留光圈/scale；Arc 镜像翻转方向并保留原始圆弧语义；
Region 保持全部边序/轮廓。RectangularSweep 只支持精确 90° 整数倍旋转及上述轴镜像，
不支持的混合选择整批 UNSUPPORTED_FEATURE。capabilities 明确该限制，不宣称任意对象任意角支持。
S1-B2b 当时仍单文件/单服务文档；layer.update 于 S1-B2c 实施，其他未实施项不变。

## S1-B2c 已实现：工作区设置

详见 [ADR 0012](adr/0012-s1-b2c-workspace-state.md)。`layer.update` 使用标准信封和必填制造
expected_revision；params 必填 layer_id、expected_workspace_revision（十进制字符串），可选
 display_name（非空白，≤1024 UTF-8字节）、visible、locked。省略/null保持原值，未知字段拒绝。
实际设置变化仅 workspace_revision +1；no-op 成功但两种版本/历史/dirty均不变。
返回 DocumentInfo（新增 workspace_revision）；信封 revision 仍是制造版本。
`layers.list` 的 name 改为 display_name，并新增 visible。状态仅在当前会话保留。
锁定层拒绝新制造编辑；Undo/Redo不受锁定阻断。查询/导出不按 visible 过滤。

```json
{"api_version":1,"request_id":"workspace-1","op":"layer.update","document_id":"从打开结果取得","expected_revision":"0","params":{"layer_id":"从查询取得","expected_workspace_revision":"0","locked":true,"visible":false,"display_name":"顶层钢网"}}
```

示例ID必须换成真实查询结果；并非已运行脚本。专项入口 `s1b2c_workspace_workflow` 实际验证服务调用。

## S2-A.1 已实现：制造边界

- `document.bounds` params `{}`；`layer.bounds` params `{"layer_id":"真实查询取得的ID"}`。
- 使用 api_version=1、request_id、document_id；只读接口不接受 expected_revision。
- result `{document_id, revision, bounds}`；bounds 为 null 或
  `{min_x_mm,min_y_mm,max_x_mm,max_y_mm}`，均为 f64 毫米。空内容为 null，
  未知文档/图层是 NOT_FOUND，非法或多余字段是 INVALID_ARGUMENT。
- revision 绑定查询快照并与信封一致；不改内容/历史/dirty/保存路径。
- 包围 Dark/Clear 制造对象，不受工作区显隐锁定影响。Macro 为 Dark primitive 保守包络，
  不扣除局部 Clear；不是最终可见区域或命中结果。详见 ADR 0013。
- `objects.hit_test` 与 S2-A GUI 仍未实现；不公布未实现的点选能力。

## S2-A.2 已实现：精确对象几何命中

`objects.hit_test` 只读 params：

```json
{"layer_id":"真实图层ID","point":{"x_mm":12.34,"y_mm":56.78},"tolerance_mm":0.05}
```

结果 `{document_id,revision,layer_id,object_ids}`；revision 与信封一致，ID 保持曝光顺序。
不接受 expected_revision；顶层/params/point 未知字段严格拒绝。坐标各轴绝对值不超过
1e9 mm、tolerance 位于 [0,1e9]，全部必须有限。INVALID_ARGUMENT 定位非法字段；
未知文档/图层 NOT_FOUND。资源预算超限 RESOURCE_LIMIT，不能证明的数值退化
UNSUPPORTED_FEATURE；两者均整次拒绝，不返回部分 ID。

命中为到独立对象材料闭包的 f64 欧氏距离 <= tolerance_mm；孔洞内距材料仍远的点不命中。
Dark/Clear 对象均可命中；工作区显隐/锁定/名称不参与；不改变 revision/dirty/history。
这不是最终可见像素 API，也不是 GUI 已实现。Macro 计算有序布尔材料的线段/圆弧边界，
不使用 S2-A.1 保守包络作真值。capabilities.resource_limits.max_hit_test_work=2000000；
普通几何线性扫描、Macro 每次查询共享准备且有边界组合预算，不宣传大规模实时性能。
详细几何、舍入和失败规则见 ADR 0014。


## S2-A.3：GUI 共用的只读显示快照

`render.snapshot` 使用 api_version=1、request_id、document_id 和空 params `{}`，不接受
expected_revision；返回 `{document_id, revision, layers, apertures}`。layers 内为有序语义对象，
apertures 为自有光圈 DTO，均可 JSON 编解码；没有第三方 AST、可变文档引用或 GPU 资源。
工作区显隐/锁定/名称仍由 `layers.list` 获取。快照不改变制造/工作区版本、历史或 dirty。
GUI 在后台转换显示数据，命中只调用 `objects.hit_test`，不从显示几何推导制造数据。

宿主 Rust 方法 `grant_file_access(path, write_directory)` 仅用于用户原生选文件/保存目录授权；
不是 JSON operation。读取授权为规范化后的单个文件，写授权为选择的目录。服务没有窗口依赖。
Hit Test RESOURCE_LIMIT.details.actual 现在为本次 charge 后尝试的累计工作量（饱和加法），
limit 仍为 2000000，不再固定为 limit+1。GUI 能力边界见 ADR 0016。

## S2-B2：精确矩形选择

新增只读 `objects.select_rect`，显式 `layer_id`，不读取工作区的显隐/锁定或 GUI selection。

```json
{"layer_id":"实际图层ID","rect_mm":{"min_x_mm":0,"min_y_mm":0,"max_x_mm":10,"max_y_mm":20},"mode":"window"}
```

mode 仅 `window` 或 `crossing`。使用既有请求信封，拒绝 expected_revision 和未知嵌套字段。
返回与 hit_test 同形 `{document_id,revision,layer_id,object_ids}`，ID 按当前曝光顺序；无分页、无部分结果。
矩形坐标有限且每轴绝对值≤1e9，min≤max，可退化为线/点；倒置拒绝 INVALID_ARGUMENT。
Window：非空对象的全部材料在闭矩形内；Crossing：材料闭包与闭矩形接触/相交。
孔洞不作为材料；Clear 作为独立对象参与；不是最终图层曝光合成的可见选择。
真实线段/圆弧边界极值、交点和内部点判断；Macro 使用顺序布尔后材料边界。
数值可靠性与 hit_test 一致，仅允许 f64 舍入量级边界余量，超过制造容差拒绝 UNSUPPORTED_FEATURE。
单次工作预算2,000,000，失败为RESOURCE_LIMIT，GUI多层调用全部成功才更新选择。
查询不改变两种revision/dirty/history/保存身份；`objects.query relation=contains` 仍是对象包含矩形。
GUI 编辑仍只允许同层选择，一个服务事务；跨层/锁定整批拒绝。详见 ADR 0019。

## S2-B3：独立对象面积/周长

只读 `objects.metrics` params `{layer_id,object_ids:[]}`，不接受 expected_revision/未知字段。
结果 `{document_id,revision,layer_id,items,summary}`。items 与请求顺序相同，每项含 object_id、
status=`exact` + area_mm2/perimeter_mm，或 status=`unsupported` + reason（无数值字段）。
summary 包含 exact_count、unsupported_count、object_area_sum_mm2、object_perimeter_sum_mm，
数值仅为 exact 项合计；不代表最终 Dark/Clear 图层开口面积。空请求返回空合计；重复ID拒绝。
未知文档/层/对象 NOT_FOUND；超过10000对象或2000000累计解析工作量 RESOURCE_LIMIT，整次无部分结果。
读查询不变更 revision/dirty/history/输出字节。GUI通过现有串行worker调用，任务序号校验后发布视图。
标准C/R/O/P及孔洞、Line、矩形扫掠、无重叠安全Arc子集、可证明Region边界解析计算。
偏差Arc连接、内偏移/端帽重叠、复杂Macro、无法证明拓扑的Region返回unsupported。
会话非序列化shape token与有界lazy cache复用Move/Rotate/Mirror/Duplicate和历史恢复。
缓存最多4096固定大小结果；身份最多20000项/2MiB保守计费，超限可淘汰，关闭文档全部释放。
未来尺寸/光圈/节点编辑必须携带前后新shape identity，不能延用当前仅刚性编辑的ID映射。

## S3-FINAL 已实现的编辑契约（Mac-first）

本节覆盖前述历史“未实现”注记；完整 V1 契约及双平台门槛不变。capabilities 新增
`objects.set_properties` 与 `edit.batch`，stage 为 `S3 basic editing`。

`objects.set_properties` params 为
`{layer_id,object_ids,width_mm,height_mm?}`，当前仅接受共享同一标准 C/R/O/P 定义的 Flash。
Circle/Polygon 直径使用 width，height 省略或等于 width；Rectangle/Obround 必须给 width/height。
编辑生成不冲突的新 aperture ID/DCode 并只重定向目标对象；Macro、混合定义、非法孔径或 no-op
整批拒绝。成功是一个 revision/Undo，Undo/Redo 恢复定义和引用，shape metrics cache 失效。

`edit.batch` params：

```json
{
  "layer_id": "从查询取得",
  "steps": [
    {"op":"objects.move","object_ids":["…"],"dx_mm":5,"dy_mm":-3},
    {"op":"objects.rotate","object_ids":["…"],"angle_deg":37,
     "pivot_mm":{"x_mm":0,"y_mm":0}},
    {"op":"objects.set_properties","object_ids":["…"],"width_mm":0.8}
  ]
}
```

当前 batch step 白名单仅为 `objects.move`、`objects.rotate`、`objects.mirror`、
`objects.set_properties`。同层已有对象在差量暂存上顺序执行，全部通过才一次提交；成功推进一次
revision 并新增一个 Undo。任何中间失败保留文档、对象顺序、aperture、revision 和历史。
`document.open/close`、`gerber.export_layer` 等外部 I/O、嵌套 batch、Duplicate/Delete batch step
均在 DTO 预检拒绝；Duplicate/Delete 自身仍是各一个原子事务。

历史预算不再因达到条数上限拒绝普通新操作，而是从最旧 Undo 起按完整事务淘汰；
`DocumentInfo`/`EditResult` 报告 `history_bytes`、`history_truncated_entries` 和
`history_truncated_bytes`。单个事务超过 max_history_bytes 仍在修改前返回 RESOURCE_LIMIT。

### S4-A1 implemented `text.create`

Mutation uses `document_id`, required `expected_revision`, then params:
```json
{
  "layer_id": "layer-from-open",
  "layout": {
    "text": "中文ABC123", "x_mm": 10.0, "y_mm": 20.0,
    "height_mm": 3.0, "tracking_mm": 0.0,
    "h_align": "left", "v_align": "baseline", "rotation_deg": 0.0
  },
  "font": {
    "path": "explicit-authorized-font.ttf", "sha256": "64-hex-digest",
    "face_index": 0, "license_status": "caller-supplied authorization",
    "redistribution_allowed": false
  }
}
```
Result: `generated_object_ids`, `revision`, `undo_entries_added = 1`, `font`,
`manufacturing_error_bound_mm = 0.001`. All output objects share a Generated
operation ID and append Dark geometry in one transaction. Failed font/hash,
missing glyph, empty/control-containing text, stale revision, locked layer,
nonfinite parameters or exceeded budgets leave geometry/revision/history intact.
Unknown nested fields are rejected. File access authority is host-owned.
Horizontal anchors: left/center/right; vertical: baseline/bottom/middle/top.
Limits and supported script ranges: ADR 0024. `text.preview` remains unsupported.
