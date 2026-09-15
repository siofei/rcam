# S1-A 语义核心与安全 Writer 审查

运行 ID：`s1-a-20260915-implementation`；日期 2026-09-15。
任务依据：仓库根目录 `RCam_S1A_NEXT_TASK.md`；设计基线 1.1、ADR 0005。
本报告区分本机实现检查、CORE10 语义就绪与完整双平台验收。

阶段 S1-A；R01–R06/R14–R16/R19–R22；AT-001/003–020/053/055–059/080–081/
085–089/092–095 的局部或前置检查，不是完整 AT 通过声明。
允许修改 core/io/service、对应测试、公开新小样、核验脚本、API/能力/审查文档与源码清单。
Luna 实施产品代码；主代理独立建立几何真值、审查与执行验收。

## 改动

- editor-core：自有标准/宏光圈与实例、Flash/Line/Arc/Region/矩形扫掠；
  f64 毫米与真实曲线，按图层保持 Dark/Clear 顺序，局部孔洞不擦除已有对象。
- gerber-io：目标子集状态机、源格式与元数据诊断、预算、规范化 Writer 与安全新路径发布。
- editor-service：授权目录内打开、文档/图层/对象查询、语义验证、revision/元数据检查与导出重开。
  无 egui/wgpu/winit 正常依赖；不弹窗、不读 stdin。
- 新增公开合成真值和独立 Rust 几何/接口回归、本地 gerbv 固定物理点对照脚本。
  未修改原 18 个 S0-C 输入和 96 个有效验收身份；私有证据保持 Git 忽略。
- 更新 API/能力说明与源码清单入口；未增加依赖或更改工具链/Cargo.lock。

## 范围限制

默认服务仍只有 S0 入口；宿主显式提供文件权限后才开放七个 S1-A 操作。
导出只允许新路径，不提供覆盖；元数据损失先返回结构化待确认结果。
物理矩形查询当前仅开放圆 Flash、圆线段与轴向矩形扫掠；其他形状可按层/类型枚举，
精确矩形过滤明确拒绝，不冒充完整选择能力。

AB、复杂 SR、其他宏 primitive、非恒等旧图像变换、未证明的 IO/变换组合仍拒绝。
无 Move/Undo/Redo、完整 GUI 文件流程、文字、后台任务/取消、批次编辑、脚本/网络服务。
现有窗口保留 S0 演示。本轮不自动进入 S1-B，不授予 EditableV1/ReadOnlyExact 或生产发行资格。

## 环境与证据

本机 macOS arm64 26.5.1（25F80），Rust 1.89.0 与已有 `.tools` 缓存；不是干净环境构建。
CPU/RAM 的 sysctl 读取受沙箱限制，实际失败已登记，不猜测硬件信息。
证据根目录：`evidence/s1-a-20260915-implementation/`。所有重试保留独立目录，不覆盖失败历史。
最终分发源码由 `MANIFEST.sha256` 标识；私有文件路径不进入报告或源码清单。

## 早期失败与独立审查

- first/second/third-geometry-review 记录宏旋转、极小孔、圆弧端帽、Region 解析覆盖、
  自交与退化边等失败；fourth-geometry-review 的 11 项独立核心测试已通过。
- first/second-io-check 和 first/second-semantic-review 保留集成编译及首坐标、旧格式等失败。
- third-semantic-review 发现前置注释误判、真实宏参数预检误拒绝、G91 模式切换限制；
  同轮输入预算、全格式与临时文件保护已通过。不得将这些早期失败日志替换成成功日志。
- 主代理另外覆盖缩放写出精度、参数宏、G74 歧义、元数据保留、写出/验证副本预算。
- reference-source-points-v3：本地独立 gerbv 对 14 个公开正例的固定物理点检查通过。
  工具自报 v4.0，安装目录 2.11.1，以原始输出和二进制哈希为准。
  这些是辅助显示对照；制造几何真值由 f64 数值断言提供，不从像素反推 Gerber。
- reference-normalized-points 首次独立核对发现 Region Arc 漏写 G75：产品内部默认象限错误，
  导致自身重开未发现差异。已修复显式 G75 与默认单象限；reference-normalized-points-v3
  的 14 个文件、40 个固定点通过。该早期 B0 失败完整保留，不输出生产文件。
- lexical-reference：空分隔符、打包恒等头、跨行 AM 和 AD 后 IO 四组独立 gerbv 物理点通过。
- core10-scan-v1 因测试工作目录下的相对 manifest 路径失败；后续使用显式绝对路径。
  v2/v3 保留逐文件首次拒绝原因，未换样本。格式超宽、无效属性和圆弧检查不自动忽略。
- [ADR 0006](adr/0006-s1-a-macro-template-validation.md) 记录主代理新增模板负例的规范性更正；
  原样本字节和更正前 manifest 都保留，新增实际 AD 缺参负例。原 18 个 S0-C 输入不变。
  DESIGN_V1/ACCEPTANCE_V1 仅补充这一解释，原 96 个机器用例身份、步骤和阈值不变。

## 最终门禁

本机 S1-A 实现检查通过：完整工作区 80 通过、0 失败、2 忽略；release 构建通过。
两个忽略项分别是原生 GPU 测试和需显式授权私有路径的 CORE10 测试。
CORE10 已另行显式运行，5/10 语义通过、整体就绪断言失败（101），不能算作忽略后通过。
服务契约专项 4/4、真实无界面流程专项 2/2 通过；正常依赖树无 egui/eframe/wgpu/winit。
Windows 原生证据缺失，S0-C platform gate 保持 blocked。

所有 Cargo 命令使用仓库 `.tools/cargo` 作为 CARGO_HOME、`.tools/rustup` 作为 RUSTUP_HOME，
PATH 首项为仓库 `.tools/cargo/bin`；没有下载或更新依赖。

| 实际命令 | 退出码 | 原始日志（证据根目录下） |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `final-gates-v3/01.log` |
| `cargo check --workspace --all-targets --locked` | 0 | `final-gates-v3/02.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `final-gates-v3/03.log` |
| `cargo test --workspace --locked` | 0 | `final-gates-v3/04.log` |
| `cargo build --release --locked -p editor-app` | 0 | `final-gates-v3/05.log` |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | `final-gates-v3/06.log` |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 | `final-gates-v3/07.log` |
| `cargo tree --locked -p editor-service --edges normal` | 0 | `final-gates-v3/08.log` |

其他实际命令：

- `cargo test --locked -p editor-service --test s1a_semantic_truth`：0，`public-review-final/01.log`。
- `cargo test --locked -p editor-service --test s1a_independent`：0，`public-review-final/02.log`。
- `cargo test --locked -p editor-service --test s1a_semantic_truth frozen_core10_semantic_scan -- --ignored --exact --nocapture`：101，`core10-scan-final-v2/01.log`；10 份均已扫描，5 份失败。
  使用显式绝对路径 RCAM_CORE10_MANIFEST/RCAM_CORE10_RESULTS；无源文件写入。
- `python3 scripts/check_s1a_reference.py --out evidence/s1-a-20260915-implementation/reference-source-points-final`：0，`reference-source-check-final/01.log`。
- `python3 scripts/check_s1a_reference.py --out evidence/s1-a-20260915-implementation/reference-normalized-points-final --normalized evidence/s1-a-20260915-implementation/normalized-public-final`：0，`reference-normalized-check-final/01.log`。
- Python 7 项审计测试通过；脚本编译缓存默认目录权限失败后指定临时目录通过，详见 `tooling-checks/`、`tooling-checks-v2/`。

最终源码文件与 SHA-256 见仓库 `MANIFEST.sha256`；生成和复核日志见 `final-source-checks/`。
二进制 `target/release/editor-app` SHA-256：
`fc96439086716f8a297937e5a4be3967c38e0a2ca215cf5e496a99b10c9d648c`。
`final-build-hashes.json` 保存二进制、Cargo.lock、工具链、全部 crate 源码哈希。
`acceptance-results.json` 为本轮 schema_version=2 逐平台结果，96 身份、185 槽位。


### CORE10 只读扫描

原冻结样本 10/10 完成扫描，5/10 通过语义解析与模型验证；扫描前后 SHA-256 均与冻结值一致。
要求全部 10 份具备语义的专用测试实际退出 101，未删除断言或换样本。
这是任务要求的能力扫描结果，不能解释为 10/10 编辑往返或全部真实文件支持。

| 样本 | S1-A 语义就绪 | 对象数／当前拒绝原因 |
| --- | --- | --- |
| CORE-01 | 是 | 9,289 |
| CORE-02 | 是 | 230,409；源文件 4,961,139 bytes |
| CORE-03 | 否 | CreationDate 属性不符合日期语法；未跳过该属性 |
| CORE-04 | 是 | 19,014 |
| CORE-05 | 是 | 999 |
| CORE-06 | 否 | G75 半径／扫角一致性检查失败 |
| CORE-07 | 否 | 坐标宽度超过声明的 FS 2.5 |
| CORE-08 | 否 | G74 无满足当前校验条件的单象限圆心 |
| CORE-09 | 否 | G75 半径／扫角一致性检查失败 |
| CORE-10 | 是 | 684 |

原始诊断在私有证据 `core10-semantic-final-v2.json`；公开报告仅保留编号与原因类别。
三份圆弧拒绝仍是待解决的真实输入兼容性问题，未据此断言源文件制造几何错误，未放宽阈值。
未对 CORE10 导出或编辑；本轮未修改源文件，也未上传样本。

### 公开真值与独立对照

- 新增 50 个固定公开小样：15 正例、35 负例；原 18 个 S0-C 小样继续回归，字节不变。
- 主代理语义真值测试 23 通过、1 个显式 CORE10 测试默认忽略；独立真实服务测试 2 通过。
- 最终源输入及规范化输出各 15 个文件、42 个物理点通过本地 gerbv 对照。
  原始日志保留 gerbv 对 LM/LR/LS 不支持的诊断；该工具仅作这些固定点的辅助对照，
  不证明任意镜像／缩放转换或整图等价。变换、曝光与局部孔洞另由独立 f64 断言检查。
- `final-integrity.json` 保存原 18 个样本、96 机器用例、CORE10、新样本和 15 个输出哈希。
  每个输出 SHA-256 与真实服务返回值独立比对一致。旧 49 个新样本字节全部保留，新增第 50 个缺参负例。
- 首次完整工作区门禁发现服务风格检查与 S0 CircleAperture 兼容回归；保留 `final-gates/` 失败日志。
  同次发布构建遇到修复期间的临时语法状态，不能作为最终同源构建证据。
  `final-gates-v2/` 保留主代理测试导入位置错误；该轮构建中止（130），后续两项未执行。
  原 S0 构造器契约已恢复，新增测试的常量／类型告警已修复；完整重跑使用 `final-gates-v3/`。

### 剩余风险与平台界限

S1-A 的公开受支持子集与安全新路径闭环已实现；CORE10 仍有上述 5 份不具备所需语义。
不授予全部真实文件支持或正式生产能力。未实现 Move/Undo/Redo，下一阶段 S1-B 未开始。
macOS 本轮执行的是缓存环境下构建和无界面测试；未运行最终 GUI、IME、Metal 或性能验收。
Windows 原生构建、无界面闭环、release 启动与 DX12/GPU 证据均阻塞。
原 96 个完整 AT 用例仍按 required_platforms 单独登记；局部检查不直接提升为完整用例通过。
