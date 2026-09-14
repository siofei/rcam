# S0-B 修补、回归与范围冻结交付

运行 ID：`s0-b-20260914-235515`；2026-09-14 开始，2026-09-15 完成验证；设计基线 1.1。

**结论：当前 S0-B 代码修补与本机自动/Metal离屏验证通过；S0 整体仍阻塞，不进入 S1，不是 V1 编辑器完成。**

阶段 S0-B；关联 R01/R03/R04/R05/R06/R08/R18/R19/R21/R22；
AT-001/008/009/012/020/070/081/085/086/088/095 前置或局部检查。
允许修改 core/io/service/app 及测试、CI、公开合成小样清单、审计/证据脚本、配套说明；未实现编辑/撤销/文字/保存。

## 来源、范围与保存

起点 commit `81b8c8288fdb01944e0282abfc8433d7acf56343`；当前为未提交工作区，最终代码用逐文件哈希标识，
不能将起点 commit 当作最终构建源码。Cargo.lock、DESIGN_V1、ACCEPTANCE_V1 和 acceptance_cases.json 的
字节哈希与修补前一致。用户预存的 .gitignore 修改及下一步任务文档保留，未提交/推送远端。

任务引用的 `RCam_81b8c82_Review.md` 和 `regression-tests/` 在工作区不存在。
因此“原附件的三份测试原样迁入”属于阻塞；本轮按任务文字编写同名 review_* 测试，明确不是原附件。
修复前先运行可编译测试：parser 1 失败、core 1 失败、service 4 失败（共 6 失败），原始日志和
修补前源码/hash 保存在 baseline-regressions、before-source、before-hashes.json；没有修改预期迎合旧实现。
旧 13 项测试全部保留，仅将原 JSON Err 检查迁移为统一响应 error.code 检查。
历史独立 35 条断言迁入可交付测试组 historical_35_checks，断言数仍为 35。

## 改动位置

| 文件/行 | 内容 |
|---|---|
| `crates/gerber-io/src/lib.rs:34` | 类型化 ResourceLimit/实际预算；修复纯 D03 Flash |
| `crates/editor-core/src/lib.rs:44` | 孔洞内侧保留；f64 半径下溢拒绝 |
| `crates/editor-app/src/main.rs:172` | f64→f32、几何与中间算术受检预览边界 |
| `crates/editor-app/src/canvas.wgsl:24` | CPU/GPU 统一短线退化与投影 |
| `crates/editor-service/src/lib.rs:227` | 统一响应/请求关联/结构化诊断/能力预算 |
| `crates/editor-service/tests/dependency_boundary.rs:16` | 真实 cargo tree 白名单失败门禁及负例 |
| `crates/editor-service/tests/historical_audit.rs:5` | 迁移历史 35 条断言，保留原测试 |
| `scripts/audit_core10.py:64` | 冻结输入命令/光圈使用审计；未知类别保留 |
| `scripts/source_manifest.py:13` | 当前源代码清单生成/验证，不含私有数据 |

其他新回归入口：core 的 review_geometry_regressions（3 项），io 的 review_parser_regressions（2 项），
service 的 review_service_regressions（6 项），app 的 gpu_tests（2 项普通+1项显式原生GPU）。
ADR 0003 记录响应迁移/数值边界/未来生命周期；ADR 0004 记录真实输入最小范围提案。
`.github/workflows/s0.yml` 把依赖树打印替换为失败门禁，仅保留 Windows/macOS。
无新增 Cargo 依赖；wgpu 27.0.1 API 按本地锁定包源码核对。

## 实际环境和命令

macOS 26.5.1 (25F80)、arm64、Apple M1/Metal IntegratedGpu；Rust 1.89.0。
复用既有 `.tools/rustup` 工具链、`.tools/cargo` 缓存、`.tools/target`，不是干净环境构建。
首次 cargo fmt 因子命令尝试使用无权限的全局 rustup 目录失败，日志保留；核对本项目已安装工具链后，
才设置 RUSTUP_HOME 指向既有 `.tools/rustup`，没有创建空工具链目录或改全局环境。

原始证据目录：`evidence/s0-b-20260914-235515/`。

| 实际命令 | 退出码 | 原始日志（相对运行目录） |
|---|---:|---|
| `cargo fmt --all` | 0 | `approved-checks/01.log` |
| `cargo fmt --all -- --check` | 0 | `approved-checks/02.log` |
| `cargo check --workspace --all-targets --locked` | 0 | `approved-checks/03.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `approved-checks/04.log` |
| `cargo test --workspace --locked` | 0 | `approved-checks/05.log` |
| `cargo test --locked -p gerber-io --test review_parser_regressions` | 0 | `approved-checks/06.log` |
| `cargo test --locked -p editor-service --test review_service_regressions` | 0 | `approved-checks/07.log` |
| `cargo test --locked -p editor-core --test review_geometry_regressions` | 0 | `approved-checks/08.log` |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | `approved-checks/09.log` |
| `cargo test --locked -p editor-service --test historical_audit -- --nocapture` | 0 | `approved-checks/10.log` |
| `cargo test --locked -p editor-service --test dependency_boundary -- --nocapture` | 0 | `approved-checks/11.log` |
| `cargo tree --locked -p editor-service --edges normal` | 0 | `approved-checks/12.log` |
| `cargo build --release --locked -p editor-app` | 0 | `approved-checks/13.log` |
| `python3 -m unittest discover -s scripts -p test_audit_core10.py` | 0 | `approved-checks/14.log` |
| `git diff --check` | 0 | `approved-checks/15.log` |
| `cargo test --locked -p editor-app native_gpu_coverage_regressions -- --ignored --nocapture` | 0 | `approved-native-gpu/01.log` |
| `cargo test --locked -p editor-service --test headless_workflow` | 101 | `missing-headless-workflow/01.log` |
| `python3 scripts/audit_core10.py --manifest fixtures/private/manifest-s0-20260914-2232.json --out <新私有运行目录>` | 1 | `audit-fix/02.log` |

workspace 实际发现 30 项测试：29 通过、1 原生 GPU 测试在普通运行中忽略；该项另外在本机 Metal 显式运行通过。
35 条历史断言属于上述一个测试组，不能另算 35 个 Rust 测试。审计脚本另有 3 项 unittest，全部通过。
headless_workflow 入口不存在，尚未实现编辑/导出/重开，退出 101 如实归档，不创建 mock 冒充完成。

## GPU 证据及失败记录

原生离屏执行生产 WGSL coverage 函数，15 个预先定义覆盖点全部与独立布尔真值及 CPU 结果一致：
短线 3 点、零长度 3 点、极小孔 3 点、顺序/局部孔/跨层曝光 6 点。
沙箱内首次找不到 Metal 适配器，日志保留；沙箱外获得真实 Apple M1 适配器。
第一次原生运行前 9 点通过，但数值检查误拒绝原有演示场景，未将该次运行标为通过。
修正为分别约束实际转换误差和 feature 分辨率后，完整演示及最终 15 点通过；早期日志未覆盖。
普通回归同时证明数值溢出、下溢和无法保留的孔洞会明确拒绝整个 GPU 预览，f64 文档不受修改。

此证据不覆盖 GUI 导航/裁剪/DPI/IME、设备重建或性能，也不代替 Windows DX12 原机测试。

## 真实输入与逐平台状态

冻结 CORE10 十份源文件的哈希在审计前后均一致。最终审计 8 份可完整分类，2 份保留未知类别 IO/IC，
因此审计命令退出 1；“可分类”不是能解释、编辑或往返。
CORE-08 实际依赖 G74 及增量格式；CORE-06 的三个 AM 定义未参与成像。完整统计/提案见 ADR 0004。
原 Gerber、私有 manifest、逐条命令/光圈参数、字体未加入源码包或公开证据包，也未上传。

| 平台/范围 | 结果 |
|---|---|
| 本机 macOS 26.5.1 arm64 构建、当前自动回归 | 通过，仅当前 S0 子集 |
| 本机 Apple M1 Metal 离屏 coverage | 通过，15 点 |
| Windows x64 原生构建/测试/DX12/交互 | 阻塞：缺目标机证据；CI 修改尚未远端运行 |
| macOS 15 原生验收 | 阻塞：当前 OS 不替代该平台基线 |
| GUI/IME/DPI/独立查看器/性能/发行许可全审 | 未执行或沿用 S0 阻塞 |
| 完整 AT-086–097 编辑/文字/导出闭环 | 未实现，未通过 |
| CORE10 编辑往返 10/10 | 未实现，未通过 |
| 原审查附件/三份原回归文件 | 阻塞：未提供 |

新 acceptance-results.json 使用 schema_version=2，保留 96 个有效用例及逐项 required_platforms 的
185 个结果槽，AT-079 仅退役。所有完整 AT 均未标记通过；stage_checks 单独记录局部结果。
S0 规范差异、真实样本范围、双平台、独立几何/工具与许可等退出条件仍未完成。

## 哈希与交付

- 当前源码/文档/公开样本清单：根目录 `MANIFEST.sha256`，用 `python3 scripts/source_manifest.py --check` 核对。
- 原设计包清单：`docs/archive/design-package-1.1.sha256` 原样保留，不再冒充当前源清单。
- 本次 release 二进制：`.tools/target/release/editor-app`，SHA-256 `2fffa4f3cc669faa328d90d4382db3014f92b5c11f9083102a5d76e72ef7e4e3`；另存 `binary-manifest.sha256`。
- 公开固定 Gerber：`fixtures/synthetic/s0_polarity.gbr`，SHA-256 `fe00fd9ab0668d3d31ac8fe15344854f27d0ddd778ffc26a465036b03f13b43c`。
  回归输入在可交付测试源码中确定性构造，来源和源码哈希见 fixtures/synthetic/manifest.json。
- 源码包：`exports/rcam-s0-b-source.zip`；公开证据包：`exports/rcam-s0-b-public-evidence.zip`。
  包内保留原始构建/测试输出（本地路径有原始记录），不含 CORE10 私有明细。

未提交 Git，未声称 S0 已有移动/Undo/文字/保存，未输出生产 Gerber。
