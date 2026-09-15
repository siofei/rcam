# S1-B1.1 / S1-B2a 交付与验收记录

日期：2026-09-15。文档检查点：9675664；当前阶段运行：s1b2-20260915-120724。
**结论：macOS 本阶段实现门禁通过；完整 V1 未完成。**

## 范围与实现

阶段 S1-B1.1 → S1-B2a。R01/R04/R05/R09/R10/R11/R14/R15/R16/R19/R21/R22。
局部关联 AT-030/031/035/036/038/039/040/041/042/054/056/086/088/089/090/091/093/095/097。
允许修改 editor-core、editor-service、gerber-io 的来源适配与合法空图层守卫、相关测试、文档及证据清单。
无新增依赖；editor-app 源码、锁定依赖、私有 Gerber、字体和 CORE10 样本均未修改。

- 修复精确零和 f64 舍入后不变的 Move：INVALID_ARGUMENT，不推进 revision、不新增历史、不清 Redo。
- NOT_FOUND.details 明确 entity/id；导出后 source_path/source_sha256 保留，last_saved_path 单独记录成功目标。
- ObjectOrigin 区分 Imported/Generated；当前曝光顺序以 layer.objects 为准，不伪造 source_command。
- 实现真实 objects.duplicate / objects.delete，严格 expected_revision、输入、锁定及预算检查。
  复制按当前源顺序逐源插入其后，保留极性和局部光圈引用；失败不消耗 ID，删除／撤销后不复用。
- 历史操作分为 Modify/Insert/Delete；删除恢复完整对象和原索引，Redo 恢复同一批身份及顺序。
  结构事务保存有预算的前后 ID 顺序守卫；不克隆整份文档制造几何。
- 删除所有对象后仍可导出和重开合法空图层；空输入、缺 FS/MO/M02、未知命令、非法引用继续拒绝。
- 旧任务移到 docs/archive/tasks；README 改正过期说明，源码 manifest 跟随当前任务和公共证据目录。

## 实际执行结果

环境：macOS 26.5.1，arm64，Rust 1.89.0。
使用现有 .tools 工具链与缓存，不声称干净环境构建。每个命令输出及退出码如下。

| 命令 | 退出码 | 原始日志 |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | `evidence/s1b2-20260915-120724/gates/01.log` |
| `cargo check --workspace --all-targets --locked` | 0 | `evidence/s1b2-20260915-120724/gates/02.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `evidence/s1b2-20260915-120724/gates/03.log` |
| `cargo test --workspace --locked` | 0 | `evidence/s1b2-20260915-120724/gates/04.log` |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 | `evidence/s1b2-20260915-120724/gates/05.log` |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 | `evidence/s1b2-20260915-120724/gates/06.log` |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | `evidence/s1b2-20260915-120724/gates/07.log` |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 | `evidence/s1b2-20260915-120724/gates/08.log` |
| `cargo tree --locked -p editor-service --edges normal` | 0 | `evidence/s1b2-20260915-120724/gates/09.log` |
| `cargo build --release --locked -p editor-app` | 0 | `evidence/s1b2-20260915-120724/gates/10.log` |

源码清单在完整文档／证据归档后运行 `python3 scripts/source_manifest.py --check`，退出 0，日志为同 run 的 gates/11-source-manifest.log。
全量结果 **132 passed / 0 failed / 2 ignored**；S1-B1 专项 **17/17**，S1-B2a 专项 **27/27**。
2 个 ignored 为原有原生 GPU／显式私有 CORE 审计入口，未计入通过。
editor-service 正常依赖树已检查，无 egui/eframe/egui-wgpu/wgpu/winit。
96 个 cases 与 retired_cases 逐项和检查点比较完全一致；185 个逐平台完整验收槽仍未执行。

## 几何与失败证据

专项走真实 JSON 服务的查询、复制／删除、Undo/Redo、validate、Export、新文档重开。
覆盖 Clear/Dark 重叠和非重叠副本；用独立圆方程和顺序覆盖公式核对输出物理点，
矩形扫掠角点与坐标独立断言；圆弧／Region／局部宏孔洞保留原几何和真实方向，输出重开对照。
未知／重复／空 ID、非有限／后段越界、锁定、历史条数／字节预算、版本和分页冲突均验证零修改。
源文件字节及 source_sha256 保持，失败导出不改保存位置／dirty／历史。

开发中发现的失败保留在公共证据 development 子目录：
1. 测试误把合法的 1e8 mm 位移判为越界，更正为实际越界输入并新增后段越界原子性测试。
2. Delete All 暴露旧解释器“至少一个图元”的限制，补充设计／用例后修复，完整语法检查保留。
3. clippy 枚举后缀重复，更名为 Modify/Insert/Delete；没有添加忽略规则。
4. 旧契约测试将新能力列为 unsupported，迁移到 supported 并继续检查 rotate/mirror/text 未实现。

## 源码／二进制与脱敏交付

- 本轮原始证据：`evidence/s1b2-20260915-120724/`。
- 可随源码交付证据：`evidence-public/s1-b2a/s1b2-20260915-120724/`；包含请求／响应、合成输入输出、原始门禁、失败记录和脱敏哈希映射。
- Rust/Cargo 源码哈希：上述目录 tested-source-hashes.json，归档时再次核对当前工作区一致。
- release：`.tools/target/release/editor-app`，SHA-256：`c6f1eacc014d2fc98d318ddfc62d404116175423abcacb0c9dc4f40ff6b04390`。
- 完整可交付文件清单：根目录 MANIFEST.sha256（避免把自身或私有样本纳入）。
- S1-B1 历史补交：`evidence-public/s1-b1/s1b1-20260915-final/`。
  历史日志汇总为105/0/2、专项17/17；其源码哈希已和6b5d9c0 Git对象核对一致。
  历史二进制SHA仅保留原记录，不声称重新构建旧版。

## Rotate/Mirror ADR 结论与未完成项

ADR 0010 选择先限制到当前模型可无损表示的集合。RectangularSweep 首阶段仅整数90°倍数旋转
（奇数次交换宽高）及水平／垂直轴镜像，其余整批拒绝；任意角完整支持需后续模型迁移。
Flash 必须组合局部光圈变换，Arc／Region 弧镜像必须翻转方向，Undo 保存原状态。
本轮只冻结决策，未实现这些操作，capabilities 未公布。完成 S1-B2a 后停止，未自动进入 GUI。

以下均未执行／未完成，不得据本轮签署完整 AT 或双平台 V1：

- Windows 原生构建／DX12；macOS GUI 编辑／Metal／IME／发行环境。
- 完整 CORE10 10/10 编辑流程，CORE-03/07/08 原兼容性缺口；未替换样本。
- 独立外部 Gerber 查看器对本轮输出复核，以及已有圆弧参考差异；本轮有独立解析几何公式，但无新增外部查看器结果。
- Rotate/Mirror、跨图层剪贴板、edit.batch、文字、完整 GUI、异步任务、覆盖保存、配置预算／历史淘汰交互。
- 性能／长时稳定性／生产加工符合性、签名与公证。
