# S4-C2 Grip Editing v1 复审记录（Mac-first）

**最终状态：PASS（Mac-first bounded）。固定最终评审为 [`exports/S4C2_f1293f2/REVIEW.md`](../exports/S4C2_f1293f2/REVIEW.md)，final commit `f1293f2abc9dceb0e3069bf8b0fa4d44cae56fa4`。Windows deferred / not executed；不代表双平台 V1、CORE10 10/10 或 P100K 通过。**
下文在 “FINAL NATIVE INTERACTION CLOSEOUT” 之前记录的 038440c 候选状态、ALMOST PASS 与未执行项均为最终 closeout 前的历史记录；最终原生/自动化结论以固定评审为准，历史事实保留供审计。
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

## FINAL NATIVE INTERACTION CLOSEOUT（2026-09-27）

038440c 基线的原生主链已完成，证据为 `exports/S4C2_038440c/public-evidence-staging/native-probe/20260927-run2/`；上文关于无法连接 CUA 的早期阻塞已由该 run 解除。基线仍为 ALMOST PASS，取消/Alt 等不得借此视为通过。

本轮范围只补 C2-GRIP-01/04/05/06：Esc（含同帧 release）、blur、PointerGone、同帧 tool/modal、Alt、selected hidden/selectable、连续 idle ID 和 diagnostics。使用现有 `native_probe` 的独立 opt-in `RCAM_NATIVE_CLOSEOUT=1`，且必须匹配公开 fixture SHA/provenance；事件由 eframe 官方 `raw_input_hook` 注入，随后经过真实 InputState、EditorApp::update、Grip Session 和原子服务路径。明确标记 instrumented native，不冒充人工硬件操作。Probe 不能指定外部输入路径或直接执行 Grip helper；同帧 context 注入位于早期取消检查之后、release guard 之前。

源码变更涉及 app 验收探针、Esc 的诊断原因标识（原取消行为不变）和下述 non-selectable 选择策略。Core/Service/Snap/几何 preview/commit 源码保持 038440c；旧主链按相同几何业务代码继承，非相同 binary。最终新 release 的取消、Alt、权限、稳定 ID 和 marker 观测必须重新运行。新 clean commit 完整重跑门禁并绑定 binary SHA；最终结果、逐项前后状态、基线来源和打包核验写入版本化 exports 的 `REVIEW.md`、`acceptance_summary.json`、`native-cancel-observations.json`，未完成这些之前仍不标记 PASS。

Windows deferred / not executed；完成本收口后停止，不进入 S4-C2+。

Selection follow-up: the closeout request requires visible/non-selectable selected objects to retain identity. App selection refresh now retains visible objects; edit_targets refuses non-selectable edits. A new regression proves no Grip, Delete rejection with unchanged manufacturing state, and Grip restoration. Hidden objects still leave selection. Native permission cases must be rerun on the final binary.

The same guard also covers a DragMove captured before selectability changed: the app checks current editable targets and exact selection identity before service submission. Direct-drag arming refuses non-selectable selections. The focused regression covers this stale gesture with zero revision/history change; final native smoke checks normal release and Undo/Redo remain valid.
