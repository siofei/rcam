# ADR 0003：S0-B 服务信封与数值边界

状态：S0 修补已实施；生命周期扩展仅设计，未启用。
阶段 S0-B；R03/R04/R05/R06/R08/R21/R22；AT-008/009/012/020/070/081/086/088/095 局部检查。
允许修改 core/io/service/app 及其测试、CI、公开合成样本清单、审计脚本、交付说明；不进入 S1。

## 当前实现

四个操作仍是 system.capabilities、document.open_s0、document.snapshot、document.analyze_s0。
它们是只读技术验证：源文本打开，不是正式路径 API。Rust execute_json 现在直接返回统一 JSON
响应（从旧 Result<Value, ServiceError> 迁移），已同步现有调用与断言；强类型 open_s0/snapshot
继续返回 Result。api_version 保持尚未发布的 S0 v1；不宣称跨版本 SDK 已稳定。

成功/失败均返回 api_version、request_id、status、document_id、revision、result、warnings、error、job_id。
合法信封回显 request_id；未知字段或类型错误若仍能独立完整解码唯一字符串 ID，则回显该 ID。
畸形 JSON、重复 request_id 或非字符串 ID 返回 null，不猜测。解码错误 details 包含定位范围、行列和诊断；
参数错误标注 params；资源错误使用类型化 ResourceLimit，details 有 resource/limit/actual，
不再解析英文 message。能力查询没有文档 revision，返回 null；已打开文档 revision 为 "0"。

单输入为 2 MiB（2,097,152 字节），单文档 100,000 个圆 Flash，预算检查在真实 parser 前完成。
字节上限的 actual 是源字节数；对象 actual 是扫描至拒绝点已计数对象数（首次超过为 100,001），
不声称为未扫描文件的最终对象总数。能力查询直接使用同一常量。
总文档资源预算和 JSON 信封总长度上限尚未实施，不能将单输入预算当作进程总内存保证。

圆孔内侧覆盖不减去固定 EPSILON，避免小孔被填。f64 半径下溢至零明确拒绝；合法孔洞保留于制造模型。
CPU 与 WGSL 对长度平方 <= 1e-12 mm² 的线段使用起点圆盘，其他线段除以真实长度平方。
圆/线外边界仍使用 1e-6 mm 覆盖比较容差，孔洞内侧使用严格几何比较。
此为现有 S0 模型约定，不表示已完成退化 Gerber 线段的规范/导出验收。

GPU 上传前验证几何、有限值、f32 正常数和误差；每个数转换误差 <= min(1e-6 mm, feature/16)，
f32 分辨率估计 abs(value)*f32::EPSILON <= feature/16。feature 为外半径/孔半径/环宽中的最小项，
线段为线半径与非退化长度的最小项。拒绝改变线段退化分类的量化，拒绝孔半径变零/与外径合并。
另外将预览数值绝对值限制在 1e18 mm 内，feature 不低于 16×sqrt(f32::MIN_POSITIVE)，防止距离平方溢出/下溢。
这些是明确拒绝的 S0 显示边界，不改变核心输入或 V1 支持尺度。
失败仅拒绝整个预览，f64 文档不变；不缩小导入尺度、删对象或更换 CORE10。
正式 renderer 的局部原点、缓存、误差预算仍在后续阶段处理，本轮不扩大 4 层/16 对象演示数组。

原生离屏测试直接执行生产 WGSL 的 coverage 函数，固定解析真值点检验短线、零长度、极小孔、
D/C/D、局部孔与跨层 Clear。它不验证窗口导航、裁剪、DPI、IME 或设备恢复。

## 后续最小服务生命周期设计（未实现）

- 服务实例拥有文档会话；正式 open 分配不透明文档/图层/对象 ID，同实例不复用已关闭 ID。
  close 显式文档 ID，未保存内容需明确 discard 策略；释放几何、来源、历史、索引和预算。
- 每文档串行提交，内存编辑/Undo/Redo 携带 expected_revision，成功递增一次；失败不变。
  dirty 按内容基线，绝不按 revision 倒退。文件 I/O 不进入内存原子批次。
- 主机显式传入总文档数、总源字节/模型内存、临时峰值和队列预算。先预留，成功转占用，
  失败/取消/close 归还；超限有相同 RESOURCE_LIMIT 结构。数值需真实样本测量后冻结，不虚报已实施上限。
- 主机提供有界任务执行/取消端口；长任务带输入文档 ID、revision 和 request_id。
  查询最终状态不依赖 UI；close/修订变化后不能提交旧结果。文件/字体权限显式传入。

以上操作不加入 supported_operations，不增加脚本解释器、HTTP/RPC、数据库或占位 trait。
服务正常依赖门禁使用已审查包名白名单，新增包必须审查；测试注入窗口/GPU/对话框及未知包确认会失败。
CI 只在 Windows MSVC x64 与 macOS arm64 执行；跨平台依赖允许项包含 chrono 的 Windows 时间支持，
不将其视为窗口 API 授权。
