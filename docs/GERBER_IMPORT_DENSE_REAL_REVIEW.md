# 密集真实 Gerber 导入修复与验收（2026-09-24）

阶段：S4-B3 后的有界导入缺陷修复；关联 R02、R06、R17，AT-005、AT-020、AT-070、AT-080 的局部证据。允许修改 `editor-app` 的显示场景、RenderIndex 和导入失败清理；未修改制造模型、Gerber parser、服务语义或 writer。本记录不授予上述完整 AT、CORE10、Windows 或双平台 V1 通过。

## 复现与原因

用户指定 `tests/GERBER/10/P0.6-MIP0202-80S-V1.0 灯面纳米钢网/P0.6 0202LED面钢网-1.GPT`，4,961,139 bytes，SHA-256 `46a4b2c106c59fffa378a98623d3a28baf174e2c61b177cdcaffca2e1d43b96d`。`0727SMT/10` 中同名文件逐字节哈希相同。只读检查得到 230409 个矩形 Flash；S1 parser 和 `ApplicationService.open` 成功。GUI 原先在显示场景 200000 items 上限处拒绝；提高场景预算后，RenderIndex 对每个 AABB 扩展整格，产生过量引用并触发单格候选上限。失败候选文档的关闭又因错误的 discard 参数产生二次确认，掩盖原始显示错误。

## 修复

- 派生显示场景上限为 2000000 primitive + point，RenderIndex 对象上限为 500000；引用总数 1000000、单格 16384、网格 16384 和实际采样工作预算仍受控，超限继续拒绝。
- 索引插入按 f32 网格计算的舍入误差扩展边界，移除原整格插入扩展；viewport 查询仍覆盖相邻格。保留 scene 顺序与 Dark/Clear 合成顺序。
- GUI 导入失败时丢弃未交付的候选文档，返回原始失败原因。
- 新增 230409 个矩形 Flash 的显示容量及索引回归测试；源 Gerber 文件未复制进源码、未改写或上传。

## 真实文件测试

运行环境：macOS 26.5.1 / arm64。用临时测试入口逐个执行真实字节的 `Model::open`，再执行 GUI 使用的 `Action::ImportGerbers`；检查有场景、无错误，指定样本有 230409 个对象，批量导入为一个 Undo。临时入口不随源码交付，原始命令与输出保存在本机 `evidence/gerber-import-20260924/`。

| 来源 | 实际文件 | SHA-256 前 16 位 | Model 打开 + ImportGerbers |
|---|---|---|---|
| 指定文件的同字节副本 | `0727SMT/10/.../P0.6 0202LED面钢网-1.GPT` | `46a4b2c106c59fff` | PASS |
| 实际钢网层 | `0727SMT/1/Gerber/pastetop.art` | `8abf91299385b31b` | PASS |
| 实际 `.pho` | `0727SMT/22/gerber_PSPOUTTest-E/art001.pho` | `5a8643b24e907c68` | PASS |
| 实际 `.gtp` | `0727SMT/66/.../06k027061c0.gtp` | `3495fbe1d6979b65` | PASS |
| 实际 `.GBR` | `0727SMT/41/.../gko.GBR` | `2fdbc3f6c386fdc9` | PASS |

另对 `0727SMT` 的 856 个 Gerber 扩展名候选做**解析层筛查**：225 解析成功，631 被当前 parser/语义/资源策略拒绝（语法或格式 268、语义校验 241、未支持特征 100、超 8 MiB 输入预算 15、非 UTF-8 7）。候选筛查不代表文件都符合本产品支持的 Gerber 子集，也不是 856 个完整 GUI 导入或 CORE10 通过。原始逐文件路径、大小、SHA-256、分类和报错在本机忽略目录 `evidence/gerber-import-20260924/`；没有把私有源文件加入交付包。

## 验收边界

本修复只解决指定密集 Gerber 的 GUI 显示资源拒绝，且保持既有安全拒绝。最终 `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo build --release --locked -p editor-app`、服务 `automation_contract` 与 `headless_workflow` 均退出 0；记录见 `gate-results.json` 和对应原始日志。受限沙盒内 Metal 适配器不可见；本机重跑 9 项 `native_metal` 测试全通过。

新 release 可执行文件在 macOS 原生窗口实际通过 File→Import Gerber 导入指定 `.GPT`；画布显示密集开孔，原生探针记录 1 层、`blocked=null`、`display_error=null`。随后从 `0727SMT/1/Gerber/pastetop.art` 导入第二层，原生探针记录 2 层、revision 2、Undo 2、同样无显示阻断；原始 action、observation、app log 与两张 2560×1600 截图保留在本机 `evidence/gerber-import-20260924/native-gui/`。测试后重新计算的两份 `.GPT` SHA-256 与测试前相同。

AT-005/020/070/080 仍需逐项按冻结平台、样本和判定方法验收；Windows、完整 CORE10 和 V1 未执行。验收摘要与可复核交付包放在 `exports`。
