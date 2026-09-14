# Gerber 编辑器：Codex 开发规则

## 项目定位与必读文件

这是仅面向 Windows／macOS 的本地 Gerber 图形编辑器，技术基线为 Rust + gerber-parser／gerber-types + 自有语义模型 + egui／eframe + wgpu。

开始任何工作前，阅读 `docs/DESIGN_V1.md`，再阅读 `docs/ACCEPTANCE_V1.md` 中对应用例。机器可读规范在 `docs/acceptance_cases.json`；应用服务和脚本扩展契约见 `docs/AUTOMATION_API.md`。当前文档基线为 1.1，变更见 `docs/CHANGELOG.md`。尚未创建项目时，从 S0 开始；这些文件是设计要求，不表示已有代码或已通过验收。

不要直接 Fork MakerPnP 主程序。`lib_gerber_edit` 是可选适配器，不是本项目的对象模型、撤销引擎或中文文字引擎。

## 范围与实施方式

每个任务写明 S0–S6 阶段、R01–R22 需求编号、AT-xxx 用例编号和允许修改的模块。只实施一个可运行、可测试的小闭环，不一次生成整套空按钮或未接入的占位模块。

第一版必须完成：导入、图层、导航、选择、移动／复制／删除／旋转／镜像、数值编辑、Undo/Redo、网格／吸附／测距、中英文矢量文字、Gerber 安全另存为与重新打开；GUI 共用的无界面应用服务及真实接口集成测试。

范围外功能按设计安全拒绝。不能为了兼容而跳过未知命令，也不能把“拒绝全部真实文件”当作 V1 可用；S0 需冻结 REAL30 与 CORE10，核心 10 份真实样本最终必须 10/10 编辑往返成功。

新增需求或调整门槛先更新设计、用例和决策记录。不得通过删失败用例、换简单样本、忽略错误或临时降低阈值来获得通过。

## 不可破坏的架构与几何约束

- `editor-core`／`editor-service` 不依赖 egui、wgpu、winit 或窗口 API。第三方 AST 通过 `gerber-io` 适配；UI 必须经 `ApplicationService` 提交，不能直接修改文档字段。
- 制造模型、命中、测距与导出使用 f64 毫米。保留真实圆弧和原始格式信息；GPU 局部 f32 与显示细分只用于显示，严禁从 Mesh／像素掩膜反推 Gerber。
- 每层按原曝光顺序组合 Dark／Clear；不同图层独立。标准光圈孔洞和字体孔洞是对象局部透明，不得用全局 Clear 擦除原有内容。
- Region 的填充和孔洞必须符合冻结的 Gerber 规范；不得直接套用 SVG 的填充规则。镜像圆弧时处理方向，输出量化后重新检查合法性。
- 使用稳定 LayerId／ObjectId。共享光圈采用写时复制；修改一个实例不得污染其他实例，源 DCode 不作为跨文件主键。
- 所有修改通过原子命令。一次拖动／粘贴／文字生成是一个 Undo 事务；Esc 取消不修改内容。不得靠反复逆旋转实现无损撤销。
- 图形复制为应用内几何剪贴板；输入框仍走系统文本剪贴板。快捷键必须尊重焦点和 Windows／macOS 差异。

## 脚本自动化扩展边界

V1 只预留并验收业务 API，不实现 Python／Lua／JavaScript 引擎、脚本控制台、正式 CLI、HTTP／JSON-RPC 服务或插件运行时。

- GUI 与无窗口测试共用打开、查询、编辑、文字、Undo／Redo、导出实现；未来适配器只做参数／结果转换，不能模拟点击或另写 writer。
- 公共 DTO 自有且可 JSON 编解码；API 版本、请求 ID、文档／图层／对象 ID、毫米／角度单位、错误码和能力表明确。不得暴露第三方 AST、可变模型引用或 GPU 资源。
- 修改携带 `expected_revision`；每文档串行提交；Undo／Redo 也推进 revision，脏状态另按内容基线判断。失败批次整体不变，成功只占一个 Undo；不把文件 I/O 混入内存原子批次。
- 服务不得弹窗／读 stdin，文件路径、字体和权限显式传入；需要覆盖或丢弃元数据时返回结构化待确认结果。与 GUI 复用文件安全、任务取消和资源预算。
- `editor-service` 的正常依赖树不含窗口／GPU；Windows 与 macOS 的真实无界面测试必须完成查询—编辑—导出—重开。仅有空 trait、mock 成功或 JSON 示例不算完成。

## 渲染与后台任务

复用 eframe 管理的 GPU 生命周期，采用兼容的 egui／eframe／egui-wgpu 版本，通过官方支持的自定义回调接入画布。不要在组件内部重复创建 Device／Queue。

缓存光圈、Mesh 与实例；导航不重解析文件，不全量重建制造几何。合批不能重排曝光，正确性优先于 draw call 数量。不要假定任意硬件都支持相同纹理格式和采样数。

读取、解析、大量细分、索引、字体转换和导出离开 UI 主线程。任务必须有版本标识、取消和资源预算；过期结果不能覆盖新文档。设备重建只影响 GPU 资源，不丢失编辑内容。

## 保存、隐私与发行

默认另存为。导出从同一版本快照生成，做语义与数值校验，重新解析核对，再写同目录临时文件并安全替换。失败不损坏已有文件、不错误清除脏标记；覆盖前检查外部修改。

对不能保证仍有效的元数据按设计报告并确认，不能暗中丢弃或照抄失效网络／元件关联。自身往返不是唯一真值，还需独立几何断言和独立工具核对。

不得自动上传 Gerber、字体或私有样本到在线查看器、遥测服务或远程仓库。`fixtures/private/` 与 `evidence/` 默认 Git 忽略。不得将用户字体文件纳入提交或分发包；实际依赖、复制代码与字体使用方式需有版本、来源和许可记录。

Windows 原生为主开发与验收环境；macOS 原生完成第二平台验收。Linux／WSL2 不支持，不要求核心兼容，不新增相应 CI、适配代码或安装包。个人辅助终端不算产品目标；无 GPU CI／无界面测试不能替代 Windows DX12 或 macOS Metal 真机测试。

## 依赖与构建

锁定工具链、Cargo.lock 与 Git 依赖 commit；先核对锁定版本的官方 API 和示例，不凭记忆编造第三方接口。新增依赖须说明用途、替代方案、平台条件和许可证。

创建工作区后按任务实际执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
```

接口包创建后，额外执行 `cargo test --locked -p editor-service --test automation_contract` 和 `cargo test --locked -p editor-service --test headless_workflow`，并审查服务正常依赖树。尚未实现测试入口时明确未执行，不伪造日志。

不要无条件组合互斥 feature；CI 矩阵只包含 Windows MSVC 与 macOS arm64。文档中 `xtask` 是待实施工具，不得在尚不存在时声称已运行。

## 每次提交与验收证据

交付说明包含：改动摘要、关联 Rxx／AT-xxx、实际执行命令、退出结果、测试环境、证据路径、未完成项及剩余风险。

没有执行就写“未执行”；缺真实硬件、样本或依赖写“阻塞”。截图不替代制造几何测试，单元测试不替代 GUI／IME／GPU 实测。性能保存 release 构建、固定样本与机器的原始数据，不把设计门槛当成实测成绩。

验收数据使用 schema_version=2；96 个有效用例在 `cases`，AT-079 仅在退役记录中且编号不得复用。按逐项 `required_platforms` 检查证据，不能混用 1.0 基线的用例含义。

验收结果按运行 ID 另存，不覆盖历史。B0 几何／数据安全失败立即阻止输出生产文件。只有全部适用必测通过、双平台证据齐全、CORE10 达到 10/10 且 B0/B1 清零，才可标记“双平台 V1 通过”。
