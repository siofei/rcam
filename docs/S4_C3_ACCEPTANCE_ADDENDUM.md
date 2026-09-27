# S4-C3 专项验收（冻结 V1 用例不变）

所有项目初始未执行；逐项结果、命令、退出码和来源记录在 S4_C3_REVIEW 与版本化 evidence。Mac-first bounded 不代替 Windows/CORE10/V1。

| ID | 必测证据 |
|---|---|
| C3-01 | Library 空/1/1000 entries、UTF-8 截断、稳定顺序、计数、Undo/Redo 即时刷新 |
| C3-02 | 同层 Create；empty/cross-layer/nested/hidden/nonselectable/locked/stale 拒绝且零修改 |
| C3-03 | Base Point raw/Grid/Endpoint/Center/Alt、Esc；一次事务、原 IDs/order 精确 Undo/Redo |
| C3-04 | Flash/Line/Arc/Region 混合；bounds/coverage/exact RGBA 前后不变、共享 aperture |
| C3-05 | Place 缓存预览、raw/Object/Grid/Alt、取消零修改、active layer 与 stale 校验 |
| C3-06 | Instance absolute/delta Move/Rotate/Mirror/Duplicate、原子选择、无内部 Grip |
| C3-07 | Explode translation/rotation/mirror 一次 Undo；rect sweep 非90度和资源超限拒绝 |
| C3-08 | UTF-8/trim/empty/长度 Rename Undo；referenced Delete 拒绝/unreferenced Delete Undo |
| C3-09 | Create/3 Place/Rename/Transform Save/Open；Recovery；flatten Export/Reopen 制造对照 |
| C3-10 | 400 objects × 100 refs，1 Definition；1000 Library entries；release 预览耗时/缓存计数 |
| C3-11 | 真实 Mac 原生主链与诊断 ZIP；instrumented 与物理操作区分 |
| C3-12 | 完整门禁、固定源码、clean source ZIP/fresh extract/hash、public evidence 原始日志 |
