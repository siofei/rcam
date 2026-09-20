# S3-FINAL Mac-first Closeout Review

## 决议

`S3 = PASS（Mac-first）`，条件是本文件随附的同一交付包中 `gates/gates.json`、
`native/native-results.json`、`native-validation.json` 与 `package-verification.json` 全部为 PASS。
权威 commit、clean status、环境、二进制 SHA-256 和输入/输出哈希来自这些机器生成文件，避免在
tracked 文档中嵌入无法自指的最终 commit/hash。

Windows 保持 `deferred / not executed`。本决议不代表完整 V1、双平台、CORE10 10/10、96 个 AT
全通过，也不授权本轮开始 S4 文字。

## 收口改动

- 恢复 DESIGN_V1 的正式阶段编号，保留历史 S2-B*/S2-C* 文件和引用。
- 标准 C/R/O/P Flash width/height 通过 aperture COW 编辑；GUI/service、Undo/Redo、metrics cache、
  writer/reopen 共用闭环。
- 新增 `edit.batch` 的 Move/Rotate/Mirror/SetProperties 子集；成功一次 revision/Undo，失败零修改，
  外部 I/O step 预检拒绝。
- Undo 预算按完整事务淘汰最旧记录并公开 truncation；超大单事务修改前拒绝。
- Snap 补齐 endpoint/center、8 logical-point 半径、稳定优先级、Alt temporary disable；Grid/坐标/
  Measure 的 mm/in 切换不修改制造模型。
- 保留并回归 Dark/Clear/Dark 复制顺序、完整 headless 编辑保存闭环、GeometryMetrics 和 P1K。
- source package 排除 `.git/target/.tools/private/evidence`，含 PACKAGE_INFO/PACKAGE_MANIFEST；
  fresh extract 重新执行 source manifest 与 package tests，并比较 payload manifest。

## 证据与门禁

证据根：`evidence/s3-final-20260920/public/`。Final gates 使用
`scripts/run_s3_final_gates.py` 从 clean commit 运行；原生 app 使用同一 release binary/package，
`scripts/run_s3_final_native.py` 记录完整命令、exit code 和 binary hash，
`scripts/validate_s3_final_native.py` 独立复核 P1K 与 S3 phases。

原生流程覆盖：Open/Fit、Window/Crossing、P1K 三轮 10 秒 Drag、Grid Snap、Measure、Duplicate、
Delete、Undo/Redo、Rotate、Mirror、Flash size COW、Save As/Reopen。Metal production/reference parity
另由 release ignored test 明确运行。Drag cancel 的 PointerGone/Esc 自动链通过；真实 window blur/lost
capture 无可靠事件注入，记录 NOT TESTABLE，不伪造人工证据。

## 保留缺口

- Windows 所有 required_platform evidence 未执行。
- AT-054/087/097 的中英文文字属于 S4；AT-094 jobs/cancel、AT-093 覆盖授权流程未实现。
- P100K、Final Layer Boolean Area、DXF/SVG/PLT、签名/notarization 均不在本轮。
- 结构编辑 history order guard 仍为 O(N) 内存计费；当前 correctness 与 P1K 门禁通过，不把后续性能
  风险误记为本轮 correctness failure。

完成本交付后停止；S4-A1 需在复审后由新任务明确启动。
