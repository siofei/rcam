# S2-B3.1 可扩展显示索引

2026-09-20；Mac-first S2（复用 S3 事务），Windows deferred / not executed。
需求 R04/R05/R07/R08/R09/R10/R11/R16/R17/R19/R21；局部 AT-010–018、022–025、029–032、039–040、062–070、075、081、086–087。
允许修改 editor-app 的显示派生结构、GPU、验证代码，公开 synthetic 样本、脚本和文档。
保持 core/service/parser/writer/Metrics/Hit Test/Selection/Undo 真值不变；不增加依赖。
本轮不签署 P100K、完整 AT-075、CORE10 或双平台 V1，不改变验收阈值。
启动工作树包含上一阶段未提交成果；原始 diff/status 在 evidence/s2b3-1-development-20260920。
外部 RCam_S2B3_dbde56e_Review.md 已补充并读取：保留 Metrics/cache/API/UTF-8 package，实现 renderer 结构修复和证据补全；不沿用其未实跑的 Rust/GPU 成绩。

实施顺序：保留 reference；有界有序 world bins；覆盖和选择边缘均查询候选；preview 平移边界；
CPU 索引回归与原生 GPU parity；P1K/指标/事务回归；实际执行门禁并记录尚缺的 GUI 证据。
