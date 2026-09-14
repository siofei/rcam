# S0-C 开发与验收交付

运行 ID：`s0-c-20260915-implementation`；日期 2026-09-15。
**结论：本机范围收口与回归完成；S0-C 退出阻塞，尚未进入 S1，也不是 V1 通过。**

依据用户提供的 `RCam_S0C_S1_Development_Guide.docx` 第 10 节执行 S0-C。
阶段 S0-C；R01/R03/R04/R05/R06/R14/R18/R19/R20/R21/R22；
AT-001/003/004/006–019/053/055–057/080/081/085/088/095 的前置或局部检查。
允许修改审计/证据工具、公开小样、服务测试、设计/用例/能力文档和清单；未修改产品实现或依赖。

## 改动与发现

- CORE10 10/10 完成新一轮只读使用审计，原冻结 SHA-256 前后匹配；不是10/10可编辑往返。
- 审计增加 FS/G91 坐标模式、单位、实际成像的操作/光圈形状、全宽字段检查、IO/ICAS分类、
  宏变量/赋值、SR恒等候选、使用前定义与重复定义检查；未终止的宏块明确报错。
- CORE-04 有 **6192 次轴向矩形光圈 D01**（3856垂直/2336水平），不是零长度，必须补入目标范围。
- IO 是非零图像偏移；ICAS为ASCII声明；CORE-08有491次增量成像及1023个全宽坐标字段。
  IO不能作为恒等命令删除，FSD/G91不能按绝对坐标处理。
- `docs/adr/0005-s0-c-editable-core.md` 冻结目标边界；设计4.4/能力表/阶段计划和机器规范同步。
  追加6组完整场景，保留96个有效AT、185个平台槽、AT-079退役及原有步骤/数值阈值。
- 18份项目自编公开样本及哈希：`fixtures/synthetic/s0c/manifest.json`。
  覆盖AM/G74/G75/Region/矩形D01/旧FS/G91/IO/IC/旧恒等命令；不复制私有样本数据。
- 新服务集成测试确保未实现目标仍拒绝、不留下失败文档、不修改已有文档；保留原有圆Flash格式能力。
  未新增 operation、依赖、GPU容量、writer、脚本或网络服务。

## 实际执行与环境

macOS arm64，项目已有 Rust 1.89.0 与 `.tools` 缓存；不是干净环境构建。
OS/起点commit见 `evidence/s0-c-20260915-implementation/environment.json`。
起点 `407bbf9a65f222c5beeb67d7478858afcc161d08`；交付仍是工作区修改，以最终源码清单为准。
Cargo.lock、工具链文件及用户Word指南哈希未变；未提交或推送Git。

证据根目录：`evidence/s0-c-20260915-implementation/`。

| 实际命令 | 退出码 | 日志 |
|---|---:|---|
| `cargo fmt --all` | 0 | checks/01.log |
| `cargo fmt --all -- --check` | 0 | checks/02.log |
| `cargo check --workspace --all-targets --locked` | 0 | checks/03.log |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | checks/04.log |
| `cargo test --workspace --locked` | 0 | checks/05.log |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | checks/06.log |
| `cargo test --locked -p editor-service --test s0c_scope_boundary` | 0 | checks/07.log |
| `cargo test --locked -p editor-service --test dependency_boundary -- --nocapture` | 0 | checks/08.log |
| `cargo tree --locked -p editor-service --edges normal` | 0 | checks/09.log |
| `cargo build --release --locked -p editor-app` | 0 | checks/10.log |
| `python3 -m unittest discover -s scripts -p test_audit_core10.py` | 0 | final-audit-checks/01.log |
| `python3 scripts/audit_core10.py --manifest fixtures/private/manifest-s0-20260914-2232.json --out evidence/s0-c-20260915-implementation/private-audit-verified` | 0 | final-audit-checks/02.log |
| `python3 scripts/check_s0c_reference.py --out evidence/s0-c-20260915-implementation/reference-pairs` | 0 | reference-check/01.log |
| `cargo test --locked -p editor-service --test headless_workflow` | 101 | missing-headless/01.log |
| `git diff --check` | 0 | checks/12.log；final-integrity/01.log |
| `python3 scripts/source_manifest.py --check` | 0 | final-integrity/02.log |

Rust workspace：32项通过，1项原生GPU检查按设计忽略；7项Python unittest通过。
35条历史独立断言仍属于一个Rust测试组，不额外计数。
headless_workflow 尚不存在，调用返回101；没有创建mock或空入口冒充真实编辑导出。
服务正常依赖树门禁通过；本轮未执行新的GUI/IME/DPI/性能测试。

## 独立证据及早期失败

本机外部 gerbv 对 IO毫米/IO英寸/ICAS/FSDI 四组公开构造输入，分别与已知规范化输入比较，
固定视口输出SVG逐字节一致且非空。完整命令、原始日志、输入/输出和工具二进制哈希在 reference-pairs。
安装目录2.11.1但工具自报v4.0，按真实输出记录；没有把它链接/打包进产品。
这些有限对照支持范围决策，**不能替代制造几何断言、CORE10完整往返、双平台或生产writer验收**。

首次 scope-test 失败：测试错误假定现有S0接受4.3格式，实际返回UNSUPPORTED_FEATURE。
核对原扫描器仅接受4–6位小数后，将样本的“当前产品行为”明确标为拒绝；
未来4.3必须支持的范围/坐标预期原样保留，产品未修改，失败日志保留，不将此计作4.3语义通过。
初次网络读取因沙箱DNS失败；后续master API返回422，查询默认分支后成功锁定公开源码与规范哈希。
引用归档在 references；相关规范段落用于本次有限决策，完整2024.05/2026.05差异表仍未完成。

既有S0-B macOS/Metal 15点证据存在，路径及哈希见 prior-metal-reference.json；本轮未重跑，
没有把历史Metal离屏结果当作新GUI验收或Windows结果。

## S0-C退出清单

| 退出项 | 状态 | 边界 |
|---|---|---|
| CORE10实际命令/光圈/成像依赖 | 通过 | 10/10使用分类；没有几何/往返通过声明 |
| EditableV1-Core目标矩阵 | 通过 | 已冻结必须实现的范围，当前运行时未授予能力 |
| IO/IC/G91/旧FS策略 | 通过 | 具体支持/规范化/拒绝边界及未来验证要求见ADR0005 |
| 新增能力回归样本 | 通过 | 18个公开输入；当前拒绝边界及有限独立对照，不等于目标功能通过 |
| macOS历史基线可追溯 | 通过 | S0-B Metal证据存在；本轮已有缓存构建成功 |
| Windows原生基线 | **阻塞** | 无Windows目标环境；构建/测试/release启动/实际GPU记录均未执行 |
| 服务架构边界 | 通过 | 正常依赖无GUI/GPU，未扩展平台与脚本范围 |
| 生产安全/遗留风险 | 未清零 | 完整语义/writer/真实独立往返未实现；当前保持拒绝/无加工导出 |

完整验收结果另存 acceptance-results.json：schema_version=2，全部96个有效身份与185个平台槽保留，
没有完整AT被升级为通过。共享文档/本机局部检查单列stage_checks；Windows缺证据明确阻塞。
原S0许可完整审查、规范差异、macOS15目标基线等历史未完成项不因本轮消失。

## 下一阶段交接

Windows可用后在同一最终源码上原生执行上述Rust门禁，启动release editor-app，记录Windows版本、
GPU/驱动、窗口中实际wgpu adapter/backend、原始进程输出及截图；仅生成exe或CI编译不算启动证据。
取得S0-C退出证据后，下一轮S1-A实现语义/writer，S1-B再打通Open→Query→Move(+5,-3mm)→Undo/Redo→
安全另存新路径→Reopen→独立核对。每一步保留源哈希、revision和未修改对象证明，不提前铺完整GUI。

当前源码清单 `MANIFEST.sha256`；构建/指南哈希在 artifact-hashes.json；最终清单副本及校验日志在证据目录。
私有manifest与逐条审计均仍位于Git忽略目录；原样本未修改/上传，未输出生产Gerber。
