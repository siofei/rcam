# S4-B3 — `.rcam` Project Lifecycle review

状态：**实施中；验收未完成**。仅 Mac-first 范围，Windows 未执行；完整 V1、CORE10、P100K、S4-C 均不在本结论中。

范围与需求/用例映射见 [S4_B3_PLAN](S4_B3_PLAN.md)，决策见 ADR 0033/0034。实现包含 service 受控 Project New/Open/Save/Save As、原子写出及持久校验、项目 dirty、GUI File 菜单/关闭保护、Recent 本机偏好和恢复副本。

| 门禁 | 当前状态 | 证据 |
|---|---|---|
| fmt / check / clippy / workspace tests | 待最终固定提交复跑 | 待填 |
| project_lifecycle / recovery / codec / Block / multi-layer | 待最终固定提交复跑 | 待填 |
| release build / source package / fresh extract | 未执行 | 待填 |
| Mac 原生 Metal parity / GUI Project smoke | 未执行 | 待填 |
| Mac 原生 Recovery / 400×100 save-open 性能 | 未执行 | 待填 |
| Windows | 未执行 | 本轮 deferred |

只有固定提交上的全部 Exit Gate 实测完成，且 `exports` 中的 Source ZIP、Public Evidence ZIP、SHA256SUMS 和 fresh-extract 报告一致，才更新为 PASS（Mac-first）。
