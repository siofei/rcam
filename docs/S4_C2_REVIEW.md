# S4-C2 Grip Editing v1 复审记录（Mac-first）

**状态：实现完成，阶段验收未完成；不是 S4-C2 PASS。**
范围为 R04/R07/R08/R09/R10/R11/R13/R14/R15/R16/R17/R18/R19/R20/R21/R22；AT 局部映射见
[S4_C2_ACCEPTANCE_ADDENDUM](S4_C2_ACCEPTANCE_ADDENDUM.md)，设计见 ADR 0038。
冻结的 ACCEPTANCE_V1.md / acceptance_cases.json 未修改。

## 已实施

- core 从单个制造对象/光圈生成稳定 ID；C/R/O/P resize，R/O 局部轴对侧锚定；Line/合法轴向 RectangularSweep 端点；Arc 投影端点/半径，保持 full-circle/zero-sweep 身份；all-Solid、全 line-only Region 闭合顶点。Solid outer + Hole inner 即使全为 Line 也无 Grip，Hole contour deferred。
- Flash 尺寸复用 numeric properties 的 aperture COW 核心；中心与生成光圈同一个历史事务。Undo 删除生成定义，Redo 恢复相同 ID。无操作拒绝，不污染历史。
- `objects.grips` / `objects.grip_edit` 有真实 Rust/JSON 入口、能力表、revision 与 active/visible/selectable/lock 最终校验。服务仍无窗口/GPU依赖。
- 单选画布 marker 为 8 physical px，hit radius 10 physical px；Grip 优先于原 Direct Drag。预览只复制目标对象/光圈，release 一个 Action/服务事务；Esc/blur/PointerGone/modal/tool/selection/layer/revision 变化取消。
- 共用 S4-C1 resolver、8/11 physical px hysteresis、Grid fallback、Alt 临时关闭和自身排除。状态显示 Grip/目标坐标/Snap；显示轮廓不进入制造模型。
- INFRA1 记录 grip.begin/cancel 和 objects.grip_edit 的 BEGIN/OK/ERROR、修订、耗时、几何/Grip 数字类别；无 geometry/target payload。
- 单对象 Grip 预算 50,000；专项性能只计选定对象特征生成，不把 100K fixture 创建或对象查找计入这一测量。

## 自动与原生证据边界

开发聚焦测试已运行：core、service JSON/permissions/project/export、app runtime/Snap、diagnostics ZIP、release performance；具体最终数量、命令与退出码由固定提交的 `gates.json` 绑定。
原始开发证据保留 `evidence/s4c2/20260925-run1/`，最终门禁与打包结果另存 `exports/S4C2_<shortsha>/`。
开发完整门禁中首轮 fmt 因并发新增测试尚未统一格式化失败，原日志保留；最终固定源码必须重新执行，不能以此轮代替最终门禁。

2026-09-26 Region scope 回归：Core 对内部构造的全 Line `Solid outer + Hole inner` 返回 `grip_features() == []`，Vertex 预览返回 `UnsupportedTransform`；Service JSON `objects.grips` 返回空数组。测试使用 internal semantic construction，因为 public project/document validator 拒绝独立 `Hole` contour；这些回归不表示 Gerber 或 `.rcam` 支持导入该语义。Core/Service crate 格式检查通过。同期 workspace `cargo fmt --all -- --check` 因并行的 `crates/editor-app/src/native_probe.rs` 格式差异退出 1；统一源码固定后仍需重跑。

2026-09-27 native fixture follow-up：公开 `fixtures/synthetic/s4c2/grips.gbr` 保留原三角 Region，新增 Obround、Polygon、合法轴向 RectangularSweep、full-circle Arc，以及 `(10,18)-(14,18)-(14,22)-(10,22)` 的 all-Solid 四边 Line Region；真实 `gerber_io::parse_s1` 回归确认全文件 12 objects 和 Region 精确边序。对象坐标、命令和原始日志见 `evidence/s4c2/20260926-closeout/scope-tests/README.md`。这些 focused checks 不替代 final release binary 的真实原生鼠标验收，也不标记 S4-C2 PASS。

真实原生进程启动观测到 Apple M1 / Metal / pixels_per_point=2。
但 Computer Use 对新测试应用路径连接超时，随后不能解析新 bundle ID 或正在运行的裸进程，
所以真实鼠标 Grip、COW 视觉比较、Object/Grid/Alt、Esc 拖动取消、原生 Undo/Redo、Save/Open、Export/Reopen、Help 诊断 ZIP 长链均**阻塞/未执行**。
自动 app runtime、无窗口业务往返或既有 Metal 回归不能替代这些操作。

发布前仍须完成上述 Mac 原生链、检查视觉预览与提交一致、Retina marker/命中实际尺寸并复审。
当前交付是可审查的未验收候选，不能作为本阶段 PASS 或完整 V1 生产验收声明。

## 明确范围

Grip 专用 numeric entry 暂缓；现有数值属性编辑保留。Macro primitive、Region Hole contour/arc-node、CompatibilitySolid、GeneratedText glyph、Block 内部与多对象 node edit 不开放。Core 与 Service 回归使用 internal semantic construction 覆盖 line-only Hole contour 返回空 Grip；public project validator 仍拒绝独立 Hole contour，不表示 `.rcam` 或 Gerber 导入支持它。
RectangularSweep 继续原有轴向合法性；非法斜线端点拒绝，不扩大导出能力。
Windows deferred / 未执行；完整 V1、CORE10 10/10、P100K、Block Editor、Alignment、Array、PnP/RefDes 未开展。
