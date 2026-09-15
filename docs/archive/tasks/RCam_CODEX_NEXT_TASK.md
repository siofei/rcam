# 下一轮 Codex 任务：S0-B 修补、回归和范围冻结

先阅读仓库 `AGENTS.md`、`docs/DESIGN_V1.md`、`docs/AUTOMATION_API.md`、`docs/S0_REVIEW.md`，再阅读本次 `RCam_81b8c82_Review.md`。

## 目标与边界

只维护 Windows x64 与 macOS Apple Silicon。不要新增 Linux/WSL2 支持、HTTP/RPC 服务、脚本解释器、数据库或大量未接线 GUI。当前仍是 S0-A 只读演示，不得把本任务完成描述为 V1 编辑器完成。

关联：R01/R03/R04/R05/R06/R08/R18/R19/R21/R22；AT-001、AT-008、AT-009、AT-012、AT-020、AT-070、AT-081、AT-085、AT-086、AT-088、AT-095 的前置／局部检查。完整 AT 的其他步骤没有运行时不得标记通过。

## A. 先建立可重复的基线

保留当前源码、Cargo.lock 和已有测试。记录实际 commit 或逐文件源哈希、Rust版本、OS/架构；保存原始命令输出。私有 Gerber 和字体继续留在获准本地环境，不加入公开证据包。

将 `regression-tests/` 下的3个测试文件按对应路径放进仓库，保留现有测试。新增测试先运行并保留失败，不改预期来迎合旧代码。它们尚未在审查环境编译，必要的格式／类型调整必须说明，不能删除核心断言。

## B. 修复当前子集的确定问题

1. 修复 `is_flash_command("D03*")` 的空字符串分支；坐标已建立后允许重复 Flash，第一条无坐标仍拒绝。验证相同坐标下切换光圈/极性不会丢失对象或篡改顺序。
2. 引入类型化 ResourceLimit 错误，不使用 `message.contains("limit")` 判断业务原因；大小／对象预算超限均返回 RESOURCE_LIMIT，包含资源种类、上限和实际值。非法几何与资源超限仍分开。
3. 修复 WGSL 短线段投影的长度平方钳制，统一 CPU/GPU 退化策略。先做算例，再在原生GPU或独立离屏GPU测试中验证；纯公式正确不能替代GPU通过。
4. 给 f64→f32 建立受检边界；确定极小孔洞保留/明确拒绝策略。不能接受后静默填孔或向GPU上传Inf。不要把修改支持尺度作为逃避既有核心样本的方式。
5. 修复归档清单失配，区分原设计清单、当前源代码清单和二进制清单。

## C. 补好未来编辑服务的接口边界，不提前实现脚本引擎

保留当前四个 S0 操作的显式命名和只读定位。为合法信封的成功与失败建立统一响应，回显request_id；补充结构化error.details。非法JSON无法可靠取得编号时用null及解码诊断。

能力查询应报告真实单次输入／对象预算，不声称编辑、保存或完整Gerber支持。为后续服务生命周期、close、revision、总资源预算、任务端口制定最小设计；未实现的能力不进入 supported_operations。

将服务无窗口/GPU依赖检查做成真的CI失败门禁，而不仅打印cargo tree。目标平台仍只有Windows/macOS。

## D. 固化证据并处理真实输入范围

把历史35项独立检查中不含私有信息的关键测试迁入可重复执行的测试/harness。原始失败输入不能丢失；公开小样应有来源和哈希。不得仅引用本机一个不会随交付提供的 evidence 路径就当作完整证据。

在已获准本地环境对冻结CORE10做完整命令和光圈使用审计。区分未引用定义、实际成像依赖、恒等旧命令和复杂特征；提出所需最小AM/G74/SR/旧命令子集的ADR。没有实际文件访问时登记阻塞，不用本次人工小样替换CORE10，也不请求扩大到无关用户文件。

只有完成必要的范围决策、相应规范/能力矩阵更新和已有S0退出条件，才进入S1。缺少目标平台硬件时准确记录阻塞，不通过交叉编译、Linux或截图代替原生交互。

## 运行与验收

在已有正确Rust环境的目标机执行；不要无条件把RUSTUP_HOME切换到空目录：

```text
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p gerber-io --test review_parser_regressions
cargo test --locked -p editor-service --test review_service_regressions
cargo test --locked -p editor-core --test review_geometry_regressions
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
```

另外实际测试失败响应关联、严格字段拒绝、资源预算、短线/孔洞GPU结果。历史13项+新增10项只是当前测试代码数量，执行器的实际发现和结果以日志为准；新增回归不替代原96项验收。

交付：修改列表与源码行、命令退出码、原始日志、样本/源码/二进制哈希、双平台分开的结果和未完成项。保持原验收规范初始状态不变，实际运行结果另存。不要声称S0阶段已有移动、Undo、文字或保存。

## 后续小闭环（不是本轮自动扩张范围）

在范围与门禁允许后，下一轮再做：路径打开受支持Gerber→查询对象ID→服务原子移动/复制→新路径导出→重新解析→独立几何/曝光检查。正式renderer另按缓存、实例化、视口筛选和有序层合成演进；禁止只把16对象数组扩大到10万。
