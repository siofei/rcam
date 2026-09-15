# S1-A.1 圆弧与旧文件兼容门禁审查

运行 ID：`s1-a1-20260915-implementation`；2026-09-15。
依据：[任务](../RCam_S1A1_NEXT_TASK.md)、[补充源码复审](../RCam_S1A_f6da9a4_Review.md)、[ADR 0007](adr/0007-s1-a1-arc-and-legacy-policy.md)。
阶段 S1-A.1；R03/R04/R06/R14/R19/R21/R22，构建关联 R01。
AT-006/013/014/019/053/055/057/080/086/088 的局部检查；不作为完整 AT 或双平台 V1 通过。
允许修改 core/io/service 能力与相关测试、公开小样、核验工具、设计/验收/策略文档、源码和证据清单。

## 结论

圆弧修正与本机无界面实现检查完成；**不签署完整阶段/双平台 V1 通过，不进入 S1-B**。
工作区 86 passed、0 failed、2 ignored；17 个新公开圆弧正负小样通过预期判定。
CORE10 为 **7/10 语义与未编辑导出重开通过**，10/10 全就绪测试保留失败退出码 101。
独立参考对照尚有差异，见下表；没有删除失败记录或把不一致改成通过。

## 实际改动

- ArcGeometry 保留输入端点/声明圆心/方向，ArcSource 保留源 FS/MO 分辨率和象限。
  单独计算两端半径和 arc deviation；G75 不再要求严格等半径。
- 明确 nonsensical center、全圆、近全圆、0° 和 f64 数值不确定度边界。
  数值不确定度超过原核心 1e-6mm 时拒绝，不能借巨大坐标扩大制造容差。
- G74 采用非负 I/J、方向、≤90°、最小 deviation；修正远离原点小半径 quarter 的浮点边界。
  多候选本身不再拒绝；运算精度内无法确定的最小值仍拒绝。
- G74 零弧为圆形 dot；导出采用等价 G01 零长度 stroke。
  Comparator 只对同位置/同宽度的真正零长度操作接受这种几何等价，不接受全圆或非零线段冒充。
- Stroke 覆盖采用明确的平均半径圆弧及径向连接，修复弧线宽内部和端帽漏覆盖。
  Region 使用经环带/单调性验证的圆形解释，并检查整个不确定环扇区与其他边的解析交点及区间。
  未证明的解释、扇区冲突、复杂度超限继续拒绝；不从显示细分或像素生成制造数据。
- Writer 保留原始非零圆弧端点/声明圆心与曝光；量化后检查端点、圆心、半径带、方向、全圆身份并重新解析。
- CORE-03/07 的 Strict/未来兼容策略已冻结；本轮不实现宽松 metadata 或 FS 自动恢复。
  实际 capabilities 与可运行专项同步核对。

没有新增 Cargo 依赖、更新锁文件、修改 GUI/GPU 路径、实现编辑/Undo/Redo 或上传私有文件。

## 规范性测试更正与回归

原两个 G74 输入字节保留，改为按最小 deviation/0° 接受。
原 `g75_invalid_radius.gbr` 字节和名称保留，其非零 deviation 是合法输入，manifest 从负例更正为正例。
这是基于规范的真值更正，不是扩大 EPSILON；更正前 manifest 保留于私有历史证据，旧源码的 G74 失败在隔离源码副本重新复现。

- 原 S0-C：18/18 文件哈希不变。
- 原 S1-A：50/50 输入哈希不变；当前 16 正例、34 负例，全部按更正后的真值通过。
- 新 S1-A.1：17 份，其中 12 正例、5 负例；含合法/危险 fuzzy Region、Ucamco 公开全圆几何重建。
- 原 `acceptance_cases.json` 中 96 个 `cases` 和退役记录内容完全保持，required_platforms 不变；仅追加根级场景说明。
- 6 项可单独运行的 Arc 专项覆盖公开输入/重开/物理点、源分辨率、量化失败保护、G74 浮点边界、capabilities、隔离 fuzzy Region。

## CORE10 结果

| 样本 | 语义 | 未编辑导出/重开 | 当前首个拒绝原因 |
| --- | --- | --- | --- |
| CORE-01 | 通过 | 通过 | — |
| CORE-02 | 通过 | 通过 | — |
| CORE-03 | 拒绝 | 未执行 | 无效日期 metadata，严格模式 |
| CORE-04 | 通过 | 通过 | — |
| CORE-05 | 通过 | 通过 | — |
| CORE-06 | 通过 | 通过 | 原 G75 拒绝已修正 |
| CORE-07 | 拒绝 | 未执行 | 字段超出声明 FS 宽度 |
| CORE-08 | 拒绝 | 未执行 | G74 中心选择已通过，后续 Region 轮廓相交检查拒绝 |
| CORE-09 | 通过 | 通过 | 原 G75 拒绝已修正，非零 Region 偏差通过环带拓扑检查 |
| CORE-10 | 通过 | 通过 | — |

10 份原样本的扫描前/后哈希均与冻结 manifest 相同。原始首错类别、当前状态、deviation 统计和全部哈希见
[脱敏 CORE10 结果](../evidence-public/s1-a1/core10-redacted-summary.json)。
CORE-08 的拒绝需要后续独立判定，不据此声称原文件必然错误；本轮没有跳过该轮廓或输出残缺文件。
未进行任何对象编辑，7/10 不等于 CORE10 编辑往返通过。

## 独立对照与尚未清零的差异

本机使用独立安装的 gerbv；实际自报版本/二进制哈希及全部参数保留于公开日志。

| 对照 | 结果 |
| --- | --- |
| 原 S1-A 有固定点的 15 份输入/输出 | 各 15/15 通过 |
| 新 Arc 正例源输入 | 10/12 通过；大 deviation 与原始 G74 零弧不同 |
| 新 Arc 正例规范化输出 | 11/12 通过；G74 dot 已通过，大 deviation 仍不同 |
| CORE-06、CORE-09 源/输出本地 1024×1024 二值采样图 | 均 0 个差异像素；只是补充证据，不能代替完整制造几何核对 |
| CORE-08 | 本地源文件渲染已执行；产品拒绝后无输出可比较 |

大 deviation 输入允许一定范围的曲线解释，gerbv 与本项目选定覆盖不同；本轮没有证明参考结果的完整规范符合性，
也没有把对照差异算作 PASS。原始 G74 非零 I/J 的零弧在本地 gerbv 中漏画；等价 G01 dot 输出通过独立点检查。
零弧外侧近边界点采用更高 DPI 的固定窗口复测，保留原低分辨率失败记录。

已下载并哈希核对官方测试 archive；其中带弧的完整文件另含 AB/thermal macro。
公开全圆小样是对应数字几何的重建，不是完整官方文件通过声明。官方原包不纳入分发。
详见 [参考检查摘要](../evidence-public/s1-a1/reference-check-summary.json) 与 [官方文件清单](../evidence-public/s1-a1/official-test-inventory.json)。

## 执行环境、命令和证据

macOS arm64 26.5.1（25F80），锁定 Rust 1.89.0；使用既有 `.tools/cargo` / `.tools/rustup` 缓存，不是干净环境构建。
所有下列最终构建/测试命令退出 0；原始日志见 [命令摘要](../evidence-public/s1-a1/cargo-test-summary.txt)。

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test s1a_semantic_truth
cargo test --locked -p editor-service --test s1a_independent
cargo test --locked -p editor-service --test headless_workflow
cargo test --locked -p editor-service --test s1a1_arcs
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
python3 scripts/test_audit_core10.py
```

服务正常依赖树不含 egui/eframe/wgpu/winit。Python 原 7 项审计测试通过，新审计/参考脚本实际运行，所有脚本语法检查通过。
默认忽略的两个 Rust 测试是原生 GPU 与需要显式私有授权目录的 CORE10；后者已单独真实执行，退出 101，不能据忽略状态算成功。
独立 Arc 参考脚本源/输出各退出 1；原公开 S1-A 参考各退出 0。初次 clippy 常量风格失败及早期拒绝策略结果留在历史日志。

公开证据：[environment](../evidence-public/s1-a1/environment.json)、[hashes](../evidence-public/s1-a1/source-hashes.json)、
[Arc 结果](../evidence-public/s1-a1/arc-fixture-results.json)、CORE10 与 reference 摘要及脱敏原始日志。
私有原始诊断、导出和图像位于 `evidence/s1-a1-20260915-implementation/`，不进入 ZIP。
分发内容由根目录 `MANIFEST.sha256` 固定，工具 `scripts/source_manifest.py --check` 复核；ZIP 解包后按同一清单逐文件核验。

## 未完成与剩余风险

- Windows 原生构建/无界面闭环/GPU 证据：**阻塞**；本轮 macOS GUI/IME/Metal/性能：**未执行**。
- CORE-08 Region 拒绝与大 deviation 独立参考差异：**未清零**，不可据此签署完整阶段通过。
- 复杂/高偏差 Region 仍可能保守拒绝；非窗口查询对 Arc/Region 的精确矩形选择仍未开放。
- 全部适用 AT、CORE10 编辑往返、生产输出、S1-B Move/Undo/Redo：**未执行/未交付**。
