# S4-C2 Grip Editing v1 专项验收附录

状态：已冻结计划，以下均**未执行**。编号 `C2-GRIP-xx` 仅用于本阶段，不占用或改写 `AT-xxx`；`ACCEPTANCE_V1.md` 的 96 个有效用例、`acceptance_cases.json` schema_version=2、原阈值和逐项 `required_platforms` 保持不变。所有结果按运行 ID 另存，Mac-first 局部 PASS 不代表 Windows 或完整 V1。

| 专项案例 | 关联 R / AT（局部） | 必要断言 | 状态 |
|---|---|---|---|
| C2-GRIP-01 身份与权限 | R07/R09/R10/R22；AT-022/030/089/090 | 同一未改对象跨帧 Grip ID 稳定；单选显示；隐藏/不可选/锁定无可编辑 Grip；不存在对象、删除层、修订冲突失败零修改 | 未执行 |
| C2-GRIP-02 C/R/O/P 与 COW | R04/R10/R11/R14；AT-037/039/040/054 | C 外径/孔约束；R/O 侧边、角、旋转/镜像局部轴；P 仅直径；两 Flash 共享光圈时只改一个，Undo 移除、Redo 恢复同一生成 ID | 未执行 |
| C2-GRIP-03 Path/Arc/Region | R04/R10/R14；AT-013/014/015/055 | Line/RectSweep 端点与零长度；CW/CCW Arc 角投影、半径、full circle、`ArcSource` 清除；line-only Region 普通/闭合顶点；自交/退化拒绝；复杂 Region 等明确不支持 | 未执行 |
| C2-GRIP-04 预览事务 | R10/R11/R21/R22；AT-032/039/040/041/087/090/091/092 | 每帧零 revision/dirty/Undo；预览=提交；一次释放一次 revision/Undo；Esc/失焦/PointerGone/工具/选择变化零修改；旧 revision 拒绝 | 未执行 |
| C2-GRIP-05 Snap 与显示 | R08/R09/R13/R18；AT-025/043/044/062 | Object/Grid/Alt 复用 S4-C1 resolver；编辑对象排除；8/11 physical px、Retina ppp=2；目标坐标/单位和 snap 状态与提交一致 | 未执行 |
| C2-GRIP-06 Service/文件/诊断 | R14/R15/R19/R20/R21/R22；AT-054/058/086/088/090/093/095 | `objects.grips/grip_edit` DTO；.rcam Save/Open 保持身份/尺寸；Gerber Export/Reopen 语义相符且原源文件 SHA 不变；INFRA1 操作可见而无 geometry/path dump | 未执行 |
| C2-GRIP-07 预算与回归 | R16/R17/R19；AT-063/069/074/075 | 100K 对象单 Flash 与 2K 边 Region release 证据；Grip 生成不随文档规模线性扫描，feature 超限可控拒绝；INFRA1/S4-C1/compatibility/compression/S4-B3/Block/multi-layer/text/Grid-Snap-Measure 回归 | 未执行 |

原生 Mac 必须实操 Circle 直径、Rectangle 角、旋转 Rectangle 边、Line 端点、Arc 半径、Region 线顶点、Object/Grid/Alt、Esc、Undo/Redo、共享光圈两个 Flash、locked、Save/Open、Export/Reopen 和诊断 ZIP；Apple Silicon/Metal 与 Retina 物理像素观测均保存原始日志。完成固定 clean commit 的 fmt/check/clippy/workspace tests/release build、专门测试、source/public-evidence ZIP、哈希和 fresh extract 后方可复审 Mac-first PASS。Windows deferred / 未执行；生产导出遇 B0 几何或数据安全失败立即停止。
