# S5 会话状态容器与请求归属首片

范围：用户计划 P2.5-1，基于 462940c43f421fc44fec4480ffe9cd861fe89314。
关联 R07/R09/R11/R15/R16/R17/R18/R20/R21/R22；兼容 R08/R10/R13/R19。
原有 AT-022/032/038/039/040/041/043/058/059/060/064/065/066/067/069/074/075/078/086/089/090/093 契约保持；下列 MP1 是补充检查，不复用 AT 编号。

生产界面仍只有一个已发布文档。没有页签、项目切换、多个工程批量打开、后台工程恢复或并行保存入口。单窗口的 camera、Snap、工具、弹窗、IME 和 Move 状态仍由这个单文档适配器持有；此首片不声称已经完成未来页签的全部 UI 隔离。

## 实现边界

- 一个 `WorkerHost` 持有原有一个 `Model`、一个 `ApplicationService` 和全局 Scene serial。`ModelSessionState` 完整移动 View、RenderSnapshot、指标身份、WorldIndex、viewport、ppm 和 BlockDisplayCache；服务、执行取消令牌和 serial 不随文档移动。移出／装回发生于后台路由或候选生命周期，不进入逐帧画布路径。
- 内部 owner 是保留强引用的 Arc namespace、slot 和不回用的 generation。只有测试可以构造第二条登记记录；生产没有登记第二文档或切换的 API。任务 ID、owner generation 和 Scene serial 分配使用 checked 增长，队列失败不回用已分配 ID。
- 所有 worker 请求携带捕获的 owner、文档 binding 和原 TaskContext。worker 先验证 owner/binding，再运行原服务任务，发布前核对服务的实际 TaskVersion。生命周期候选的错误不能伪装成取消成功或旧文档回滚。
- UI 在 trace 观察和 Move、canvas、geometry、ordinary、viewport 处理之前验证回复归属。外来／伪造 owner、退休回复及重复回复不能安装视图或消费当前合法任务。仍保留原回执、版本、选择存储／epoch、项目、几何上下文和 Move 检查。
- 已授权后台拒绝只清理对应任务。终态未知的写入保持阻塞，不能自动关闭、继续导入或宣称撤销。旧只读任务的拒绝不清理另一个保存操作。后台断线清理所有当前 pending，并停止新请求。
- Save 的延迟转换证明绑定具体已安装的 SaveProject 终态、原 owner 和一次性 Intent serial；成功还要求 Completed、无错误和内容基线干净。普通干净视图或相似 message 不产生保存证明。Recovery 完成绑定具体 owner/task 和 RecoveryWrite 的实际安装；恢复文件移除要求实际 SaveProject 安装。
- 首次空文档导入发布新 binding 时，只有同一次 queue/current/task 的合法 publication 可以迁移剩余队列及 after_stop 操作。Current 原输入版本保留用于终态复核。失败解析后已创建空文档、提交后刷新错误、取消、未知结果仍沿用原有停止和每文件一次 Undo 规则。
- 内部 autoload 使用显式一次启动握手；等待期间不发送构造器 NewWorkspace，也不接纳竞态请求。成功与失败都有明确启动结果，正常生产启动走原有 NewWorkspace 任务。
- Open／OpenProject／RestoreProject 候选准备和旧工程关闭失败完整恢复原文档状态；清理失败也先恢复旧状态。New 的旧文档关闭失败不丢失旧 binding。全局 serial 即使候选失败也不回退；耗尽安全拒绝生成 Scene。

## 补充回归与证据边界

MP1-01 完整容器 Arc／指标／viewport／cache 身份；MP1-02 同一服务的真实 A/B Flash/Arc/Block 与相同本地 ID、不同几何、选择／编辑／Undo／dirty 隔离；MP1-03 foreign／forged／duplicate／retired／owned rejection；MP1-04 正常和内部启动握手；MP1-05 首次导入同操作迁移和原队列全部错误／停止规则；MP1-06 真实候选准备后失败、cleanup 与旧 close 失败完整回滚；MP1-07 精确保存／恢复授权与断线；MP1-08 ID／generation／Scene serial 耗尽及原有交互回归。

本片不修改制造几何、Snap/Alt、shader、公开服务 API、项目／快捷键 schema、锁定依赖或平台支持。1e-9 几何合同保持。恢复仍使用单文档原有 ProjectId 文件布局；两个同时可交互副本的恢复命名与保存路径别名隔离，必须在开放页签前另行完成。

云端辅助 Rust 检查不能替代 Mac/Metal 或 Windows 原生证据。各实际命令、退出码、原始失败与文件 SHA 随精确候选单独交付；没有将旧 native 证据改绑本片，也没有改变采样区间、删除截图帧或宣称 actual-present 性能完成。
