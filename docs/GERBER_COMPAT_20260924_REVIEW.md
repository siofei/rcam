# Gerber 兼容导入与 0727SMT 批量验收（2026-09-24）

阶段：S4-B3 后导入缺陷闭环。关联 R02/R03/R04/R06/R14/R19，AT-005/006/009/015/020/053/080。环境：macOS Darwin 25.5.0 arm64。此记录只评估本次兼容导入，不替代这些 AT 用例的完整双平台结论。

## 实现范围

严格解析优先，失败后对可确定的旧式或不规范输入执行兼容解释，保留逐类日志、源 SHA-256 和原文件。异常 Region 补上精确闭合边或按源精度调整端点，不能证明标准拓扑的轮廓标记为兼容轮廓；这类文件仍可编辑和导出，图层与导出确认保留警告。支持范围和制造语义限制见 ADR 0035。未知命令、无法确定的几何和超资源预算仍返回明确错误。

本轮把文档、RenderIndex、工程文件的对象上限统一调整为 1000000，把解析命令预算调为 4000000，并将源文件字节上限调到 64 MiB，以便重开约 46.6 MB 的规范化大文件导出；显示引用与 Region 边预算仍独立生效。显示/选择/边界计算对已允许的非标准 Region 圆弧使用确定性源路径，制造数据不从画布反推。

## 真实样本与批量结果

输入清单：`evidence/gerber-import-20260924/0727smt-candidate-paths.txt`，原件哈希：`evidence/gerber-compat-20260924/corpus-source-sha256.tsv`。扫描只读原件，无上传或覆盖。完整首轮原始记录：`corpus-final-856-raw.log`，其中 STRICT 397、COMPAT 458、REJECTED 1、IO 0。唯一旧预算失败的 `0727SMT/107/202607274351/drill.art` 随后在调整后的同一解析器上单独重跑，见 `corpus-last-large-raw.log`：STRICT，20,092,902 字节、803,226 对象。因此逐文件解析结果合计 **856/856 成功（STRICT 398、COMPAT 458、REJECTED 0、IO 0）**。这是首轮全量加唯一失败项复测，不是再次执行了完整 856 文件扫描。
复测后再次生成 `corpus-source-sha256-after.tsv`，与扫描前清单逐字节相同。

指定 `tests/GERBER/13/EP11BAM-A_top_0mm_202607241511.gbr` 和 bottom 文件通过应用服务打开、移动、警告确认、导出和重开；`0727SMT/75/ea1hs2m01MAN_VB/art08.art` 也通过相同服务流程。原始记录分别为 `rcam-compat-service-top.log`、`rcam-compat-service-bot.log`、`rcam-compat-service-fs.log`。`art08.art` 曾在原生窗口导入后触发 Region 圆弧显示错误；修复后的 `art08-display-raw.log` 验证画布 Scene 可构建。最终 release 构建的独立 macOS 应用中再次直接导入 `art08.art`：黄色问题图层出现、图形可见、无 Region 圆弧错误；观察记录见 `native-final-observation.txt`。

最大样本 `drill.art` 的应用服务测试见 `large-service-final2-raw.log`：原文件打开、移动一对象、导出 46,583,192 字节、重开均完成。前两轮分别触及 2000000 命令和 32 MiB 源文件旧预算，失败原始日志 `large-service-raw.log`、`large-service-final-raw.log` 均保留。`large-display-raw.log` 还验证 803226 个对象、803344 个画布 primitive 的场景构建通过；这是无窗口场景测试，不代表原生大文件交互或性能门槛通过。

## 结果边界

- 856/856 是**解析兼容性**覆盖；没有对 856 个文件逐一执行编辑、导出、独立 CAM 比对。
- EP11BAM 和 art08 的应用服务往返证明可编辑与可导出路径，但内部重开不足以证明制造等价；`CompatibilitySolid`、补闭合边、2 µm 零光圈替代尤其需要独立几何与本地 CAM 核对。
- Windows、完整 CORE10、96 个正式 AT 用例、P100K 和双平台 V1 验收未执行；不得据本记录标记通过。
- 原生 macOS 窗口已实际导入 EP11BAM top 和最终 release 构建中的 `art08.art`，均显示图层警告和画布；最大样本仅完成无窗口 Scene，不宣称原生交互性能通过。原生窗口检查是自动化鼠标操作和目视画面观察，没有用户确认的物理手势证据。

## 验收证据

原始日志和哈希清单保存在 `evidence/gerber-compat-20260924/`。最终通过的门禁：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo build --release --locked -p editor-app`，对应 `cargo-*-accepted.log` 和 `cargo-test-final-pass.log`，退出码均为 0。首次工作区测试的 Recovery 用例因跨秒时旧恢复快照被正确过滤而失败，调整测试断言顺序后全套通过；原始失败日志 `cargo-test-postfix.log` 保留。交付 ZIP 只收纳源码、文档、日志和清单，不包含私有 Gerber 原件或测试导出的制造文件。
