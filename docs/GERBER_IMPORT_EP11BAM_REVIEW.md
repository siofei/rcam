# EP11BAM-A_top 真实 Gerber 导入诊断与尝试

阶段：S4-B3 后有界导入缺陷处理。关联 R02/R04/R06/R19，AT-005/015/020/080 的局部检查；完整用例、Windows、CORE10 均未据此通过。环境：macOS arm64，2026-09-24。源文件只读。

## 结果

| 项目 | 结果 |
|---|---|
| `tests/GERBER/10/.../P0.6 0202LED面钢网-1.GPT` | PASS，密集真实文件的 Mac GUI 导入已由 `401fdcc` 修复；原始验收见 `GERBER_IMPORT_DENSE_REAL_REVIEW.md` |
| `tests/GERBER/13/EP11BAM-A_top_0mm_202607241511.gbr` | 阻塞：当前源文件不能安全作为可编辑制造数据导入；等待 CAD/CAM 规范重新导出后复验 |
| 本次代码 | 接受重复的 `%LN...*%` 段标签；仅将明确的旧式 `G1/G2/G3` 绘图命令交给 parser 前规范化为 `G01/G02/G03`。保留段名作为源元数据，Gerber Export 的元数据确认流程识别该类别。未放宽圆弧和 Region 拓扑检查 |

指定 top 文件 1,865,189 bytes，SHA-256 `81ce83695fcdb2b5cc692d16d98365dc29e739d1e348f75694da74c0f6f211f0`；`0727SMT/13/` 副本哈希相同。测试后源文件哈希未变。

## 失败路径和修复尝试

1. 原导入在重复的 `%LNMark*%` 报错；修复语法处理后，真实文件到达命令 673，因 G74 无可接受的单象限圆心而拒绝。原始运行日志：`evidence/gerber-import-20260924/second-before.log`、`second-final-real-import.log`。
2. 在隔离的代码试验中放宽单象限角度判定后，top 文件的 Region 闭合点仍差一个源坐标单位（`0.00001 mm`）；bottom 同源文件有 15 处。只读坐标审计：`second-region-closure-audit.log`。规范要求轮廓最后顶点与首顶点**精确一致**，不能由 G37 自动闭合。
3. 仅在试验路径处理闭合点后，Region 圆弧不确定范围重叠；继续尝试圆弧规范化与退化边处理后，又检测到轮廓自交。这些相继失败证明不能靠统一容差或简单吸附完成安全导入。对应日志：`second-after-angle-probe.log`、`second-after-closure-probe.log`、`second-after-canonical-probe.log`、`second-after-zero-probe.log`。试验性的放宽代码已撤销。
4. 独立工具 `gerbv` 对原件重新导出 RS-274X 到 `/tmp`，退出 0；导出件将 `0.00010 mm` 光圈写成 `%ADD11C,0.0000*%`，RCam 拒绝零尺寸。只在 `/tmp` 试验副本中恢复该光圈真实尺寸后，RCam 仍拒绝 Region 圆弧不确定范围重叠；因此不能把此转换件作为验收替代品。原始日志：`second-gerbv-reexport.log`、`second-gerbv-reexport-import.log`、`second-gerbv-reexport-aperture-probe.log`。源和两个试验副本的哈希见 `second-attempt-hashes.txt`。

Ucamco [Gerber 2026.05 规范](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf?eac6410808ad1d0b3f977429ddaab7d0=)要求 Region 显式精确闭合且不得自交；旧式 G74 及其 90°限制见同规范。保留制造安全拒绝；独立查看器能渲染图像不证明几何可编辑或输出安全。私有样本未上传。

## 复验条件

接收源 CAD/CAM 重新导出的 top Gerber 后，先只读清单与哈希，再执行 parser、无窗口应用服务、Mac 原生 GUI 导入；核对图层对象数、错误状态、独立几何和导出往返，并保留原始日志。当前这些新文件复验步骤未执行，不能记为 PASS。
