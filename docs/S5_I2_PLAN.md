# S5-I2 Unified Base Point / Selection Composite Metrics / Interaction UX

2026-10-03 · IMPLEMENTING, NOT ACCEPTED. Parent accepted I1 Mac-first bounded; I2 is authorized for direct implementation without a user plan review.

## Frozen implementation baseline and stages

HEAD f1939fdb458b8271b92183aabc92813f10dcfba4, branch codex/s5-i2-unified-base-point, isolated worktree /Volumes/外接硬盘/rcam/.worktrees/s5-i2-unified-base-point. Initial status clean. I1, PMIX, K1, historical evidence and main checkout are protected. No push or upload. No directory deletion authorized. Reuse one existing Cargo target; evidence archives on the external volume. Disk at intake: internal 18 GiB free, external 75 GiB free.

This S5 task is split into reviewable running loops; all are required for final I2 completion:

A. Manufacturing selection composite area, first moments, centroid, perimeter, bounds; cancellable service query, background integration and bounded cache. Public analytical fixtures, all supported geometry families, version/cancel matrix and release computation measurements.
B. Common world-point state and input widgets: numeric/custom, contour Pick preserving selection, bounding center, area centroid. Move/Copy/Rotate/HV Mirror and every applicable existing tool in the inventory below. True target contour Snap excludes moving objects. One atomic cross-layer Undo.
C. Separate persistent drag/Grip preferences, three canvas cursors, fixed-width status fields, full-window menu flicker investigation in the prescribed real performance scenes. Three frozen release/native rounds, automated gates, portable evidence and tamper matrix.

Each stage requires independent review before its stage commit, no push. Final Source/Evidence must refer to the same clean commit and pass fresh extraction. Stages cannot be omitted or represented by inactive buttons. Parent arranges independent review. PMIX resumes only after parent scheduling. K1 native supplementation remains user-deferred.

## Frozen semantic refinements (supersede historical draft limitations)

- User confirmed selected-only per-layer ordered Dark/Clear composition, same-layer overlap counted once, cross-layer area and moments summed. Block children are applied at the instance's exposure position; aperture holes and macro Clear stay local transparent. Region uses frozen Gerber winding/cut-in semantics, not SVG rules.
- Perimeter is the length of the same final composite material boundary, including holes, removing interior seams; sum independently per layer. A zero-width/zero-area entity contributes no two-dimensional material perimeter; explicitly report ZeroArea rather than invent a center or use centerline length. Separate path length remains the existing measurement concept.
- Standard aperture hole contours are included in dedicated point Pick and drag target contour Snap. Existing global Snap preferences are not silently modified. Selected objects are included for picking B and excluded for picking drag T.
- Geometry center is the world AABB envelope of selected objects, including Clear source boundaries. Numeric/Pick values are transient tool data, not model fields. Full supported geometry types, original exposure order, cross-layer ownership and full text groups are retained.
- Initial migration defaults proposed for implementation: drag enabled, Grip enabled, system cursor (preserve existing behavior). Store explicit user changes only; do not rewrite user preferences on startup. Test all four switch combinations and all cursor modes. No new Scale, arbitrary mirror axis, geometry clipboard product or associative array.

## Numerical algorithm and admission contract

Use an analytical arrangement of semantic line/circular-arc boundaries, split at true intersections and coincident endpoints. Membership is evaluated against the same exact manufacturing semantics on two certified separated sides; retain only intervals separating material from void, orient material left, deduplicate coincident intervals. Integrate line/arc Green integrals in local coordinates with compensated sums. Never integrate hit-test witness seams without classification; never use renderer geometry, polygon tessellation or integer Boolean as an accuracy substitute. Sweep connected source-envelope components permits separated objects to take an independent analytical fast path while every overlap/Clear interaction is still classified in original layer order.

Admission/error accounting must cover boundary construction, intersections, membership, integral accumulation and world-coordinate reconstruction. Return PrecisionUncertain for unresolved near-coincidence, tangency, tiny intervals, unsupported numeric extent or error bound above epsilon; never fall back to AABB. Epsilon per centroid coordinate = min(1e-5 mm, manufacturing_resolution_mm / 10). Normal nondegenerate fixtures of every currently supported geometry family must succeed. Numerical-limit fixtures can reject explicitly; all-Unsupported is a task failure. Query does not quantize the model. Bounds remain available even when area is zero/unresolved.

Limits: 10,000 selected objects; 100,000 expanded primitives; 1,000,000 source edges; 2,000,000 bounded work operations; 128 MiB temporary admission; 64 MiB/64-entry selection cache. Existing stricter limits still apply. Deadline 2 seconds; cooperative cancellation checked in source expansion, sweep, pair intersection, interval classification and accumulation. First measure the longest preparation segment, then add checkpoints where justified. Error categories: InvalidArgument, UnsupportedGeometry, NumericOverflow, PrecisionUncertain, ResourceLimit, Cancelled; proved empty composition returns ZeroArea. Cancel UI <=500ms; controlled termination/resource release <=2s. A source preparation budget rejection cannot be mislabeled cancellation success.

Release performance gates: at least 30 independent cold/hot observations; 10k disjoint bounds p95<=100ms, composite analytic fast path p95<=200ms, cache hit<=10ms; complex query <=2s success or explicit budget/precision error, with successful normal complex fixtures mandatory. Contour query P100K CPU p95<=20ms and visible feedback p95<=100ms; application navigation p95<=33.3ms/p99<=66.7ms with no >200ms pause. These are frozen goals, not measured achievements. Actual display refresh is recorded, not silently altered. UI performs no Boolean or whole-selection scan each frame.

All prior BP-01..18, followup UX/cursor/Snap/flicker scenes and all-TaskVersion stale matrix below are mandatory. Add explicit source/binary identity, status layout rect stability, real cancellation timing and raw memory/resource observations. Menu flicker remains OPEN until actual reproduction and repair or user-scene evidence; no nonreproduction PASS. Windows deferred/not executed; no full V1/CORE10/P100K/PMIX claim.

## Historical intake draft (for full inventory and test detail)

The following is historical planning retained in full. References to pending I1, old manifests or no Cargo apply to its earlier read-only intake only; this header and ADR 0055 govern current I2 work.

# S5-I2 统一基点与双中心：只读盘点和独立任务书草案

日期：2026-10-03。状态：PLANNED / NOT IMPLEMENTED；不是验收 PASS。

用户已确认面积重心口径：**只合成选中对象，各层按原 Dark/Clear 顺序合成，同层重叠只计一次，跨层面积相加，未选对象不参与。** 确认原文为“按上述选中对象合成口径（推荐）”。两种中心均为必需功能：**几何中心（整体外接矩形中心）**、**面积中心（填充面积重心）**。

执行顺序：I1 独立审核、修复、阶段提交和干净包收口 → 本任务正式冻结和实施 → 独立审核与本阶段干净收口 → 父任务安排 PMIX。当前只读盘点，未修改 I1；不恢复 K1 原生补验，不提前恢复 PMIX，不做 CircuitCAM 4.4 完整兼容。

## 1. 实际基线与保护范围

- 工作树：`/Volumes/外接硬盘/rcam/.worktrees/s5-i1-multilayer-cycling`。
- 分支：`codex/s5-i1-multilayer-cycling`。
- HEAD：`b1635d10485ae61358e1e444f8f852f35c055b1e`，并非 I1 候选的完整源码身份。
- I1 候选 MANIFEST SHA-256：`85020fb392ad6206c5ff842475b7c00ef02595238502b5e7e12d69ebb22d90a6`。读取时 577 项全部吻合。
- 工作树有已知 I1 未提交修改。完整状态和 1,931 个 tracked/untracked 文件散列记录于 `STATUS_BEFORE.txt`、`BASELINE_BEFORE.json`；不得 reset、清理、覆盖或在此工作树提前实现本任务。
- 候选：`../S5I1_candidate_85020fb3_20261003/`。候选通过不等于最终干净提交收口。
- 原始主目录、`.worktrees/s5m2-c-pmix`、历史证据、已有 target 均受保护。本任务不运行 Cargo，不启动应用，不新增构建缓存。
- 正式实施必须重新核对 I1 最终 clean commit，以它建立独立工作树；本文中的行号和能力盘点仅对应上述候选。

## 2. 阶段、需求与允许模块

阶段建议名：**S5-I2 Unified Base Point / Bounding Center / Area Centroid**，属于 S0–S6 中的 S5 交互与性能补正，不新增其他格式产品。

关联 R04/R05/R07/R08/R09/R10/R11/R12/R13/R14/R16/R17/R18/R19/R21/R22。主要继承 AT-011–018、022、025–037、039–045、048–051、065–067、086–092；新增定向用例使用 `S5I2-BP-*`，不复用 AT-079，不改变 96 个正式用例、schema_version=2 或原 required_platforms。

实施允许模块：editor-core 的制造边界/矩/变换纯计算及定向测试；editor-service 自有只读 DTO、版本栅栏、缓存/预算/取消和现有原子编辑入口；editor-app 的基点状态、工具面板、Snap 接入、预览和原生证据；公开合成 fixtures、验证脚本及阶段 docs/ADR/API 文档。renderer 仅允许显示标记/预览接入，不作为制造数据源。gerber-io 仅回归测试及必要的共享语义接口提取，不改变曝光、导出量化、Region 解释规则。若提取需要扩大修改范围，先记录设计调整。

正式实施先新增任务书、验收附录与 ADR（届时分配未占用编号），更新 DESIGN_V1、AUTOMATION_API 与需求映射。依赖变更需单独记录用途、替代方案、许可证、版本与平台条件。当前草案未改变依赖或公共 API。

## 3. 现状盘点：目前究竟支持什么

路径均相对冻结 I1 工作树，以下为读取代码所得，不代表本次重新通过测试。

| 操作 | 当前行为与源码 | 本任务需要补足 |
|---|---|---|
| 数值移动 | `editor-app/src/state.rs:1393`，ΔX/ΔY；I1 服务支持分层目标一次原子提交 | 增加自定义基点 B 与目标 T，使用 Δ=T−B；保留 Δ 输入 |
| 鼠标拖动 | `editor-app/src/drag.rs:73` 起，以按下位置为起点，目标可 Snap | 独立的“拾取基点”状态；先选好对象，再点其轮廓选 B，不改选区 |
| 复制 | `state.rs:2060` 附近，Duplicate(0,0) 原位复制，再由用户拖动 | 基点到目标的一次复制事务；保留原位复制快捷命令，不能先复制再移动冒充一个 Undo |
| 旋转 | `state.rs:1484`、`main.rs:886–954`，选择集制造 AABB 中心、World 原点、数值 XY 自定义中心 | 接入画布拾取、双中心和公共基点状态 |
| 镜像 | `state.rs:1529`、`main.rs:959–980`，GUI 固定经过选择集 AABB 中心的水平/竖直轴；服务可接受轴坐标 | 轴经过自定义 B，仍需显式水平/竖直方向；数值和画布拾取均可 |
| Block 创建/放置 | `block_ui.rs:111–190,635–665,770–861`，已有创建基点、实例放置 canvas session 与 Snap；创建限同层 | 复用公共点输入/双中心，保留定义原点与实例临时世界基点的区别 |
| Block Transform | `block_ui.rs:680` 附近，绝对 XY/角度/局部镜像 | 世界任意基点整体变换走普通 selection transform；不得偷偷重设共享 Definition 原点 |
| 文字 | `text_tool.rs:235–281`，绝对/鼠标/相对放置，已有参考点拾取 | 共用点拾取和可用选区双中心；文字自身对齐/布局原点语义不变 |
| 测距 | `tools.rs:145–`，两个端点和 Snap | 共用点输入，可用双中心作为端点；保持只读、不新增 Undo |
| Grip | `grip.rs`，稳定 feature/node/尺寸参数预览 | 目标点输入复用；参数固定支点不得因“统一基点”改成任意缩放 |
| Rectangular Array | `array_ui.rs`，行列和有符号 pitch，同层，原 cell 不动 | 用 B→T 定义 pitch 向量的明确辅助模式；不添加数学上无效的平移基点参数 |
| Alignment/Distribution | `state.rs:1414`，最后选中对象是固定 Anchor，world AABB 对齐/等边距 | 保留 Anchor-object 语义；“选区中心移到某点”由 Move B→T 完成 |
| Board Registration | `components_ui.rs:768–819`，两对 Board/World 点，World canvas 拾取 | 仅复用已有点输入/拾取；不改组件模型或增加 PnP/RefDes 工具 |

**几何剪贴板/粘贴：** 对当前 editor-app/editor-service 搜索 clipboard/Paste/paste 未找到已实现的应用内几何粘贴路线。I1 文档中保留相关契约不能作为“已有实现”的证据。本阶段覆盖已有 Duplicate，不顺带新建完整剪贴板产品；若 I1 最终基线新增相关入口，再明确加入适配矩阵。文本输入继续走系统文本剪贴板。

删除、Undo/Redo、导入导出、图层样式、重命名等不存在空间基点，不添加伪参数。没有现成 Scale/Shear，本任务不新增。镜像第一版为现有水平/竖直轴穿过自定义 B，任意倾斜镜像轴不是此次必需范围。

## 4. 两种中心的冻结语义

### 4.1 几何中心（整体外接矩形中心）

取当前完整选区所有对象的 **Manufacturing World AABB 并集**，B=((xmin+xmax)/2,(ymin+ymax)/2)。包含 stroke 实际扫掠宽度、圆弧真实极值、Flash 光圈外边界、Region、完整文字组与 Block transform 后的边界；跨层统一 world 坐标。不取对象中心平均，不取屏幕矩形，不取“最外两对象位置平均”。延续现有 selected_bounds/selected_center 定义，Clear 对象的制造边界参与 bounds；它与最终填充材料的 AABB 不是同一个查询。

### 4.2 面积中心（填充面积重心）——用户已选定

对每一层 l，仅保留选中对象，维持该层原曝光顺序。初始材料集合 S_l=∅。每个曝光原语的自身实际材料为 G_i：Dark 时 S_l←S_l∪G_i；Clear 时 S_l←S_l\G_i。计算 A_l=∫S_l dA，M_l=(∫S_l x dA,∫S_l y dA)。跨层 B=(ΣM_l)/(ΣA_l)。同层重叠只计一次；跨层独立计面积，即使投影重合也分别贡献；所有层采用相同单位密度，不按颜色、透明度、Z-order 或对象数量加权。

未选对象既不加材料也不擦除材料。选择与普通渲染结果可不同，这是已确认的计算范围，tooltip 必须明确“仅选中对象；各层独立合成后按面积加权”。它不是物理多材料质心，也不是整个工程最终材料的质心。

G_i 必须遵守制造语义：

1. C/R/O/P Flash 使用有孔光圈实际材料；孔洞是对象局部透明，不生成擦除此前材料的 Clear。
2. Line/Arc/RectangularSweep 使用制造扫掠及端帽的真实集合；禁止把宽线当中心线，禁止重叠端帽重复计面积。
3. Region 按冻结 Gerber contour/cut-in 语义形成材料，不直接套 SVG fill rule。
4. Macro 内部 Dark/Clear 先形成光圈自身材料，曝光到所在层时保留对象局部透明性质。未知/不支持的制造结构不能跳过。
5. **RCam Block 与 aperture macro 不同。** 当前 `block.rs:289` resolve_instance 保留子对象 exposure；`gerber-io/src/s1.rs:3809` 在实例所在序列位置按定义顺序展开。面积计算必须相同地展开并将子 Dark/Clear 施加到当前 S_l，不能先把整个 Block 合成孤立局部材料再 union。实例外层 exposure 不替代子 exposure；不新增嵌套块。实例 transforms 作用于 f64 几何，Area/Moment 对应变换后的结果。
6. 完整文字组包含实际字形材料和字洞；排版框、advance、空白字符不是材料面积。

只有证明 A=0 时返回 ZeroArea；Clear-only、全部擦除、纯零面积都不能提供面积中心。面积区间包含零但不能证明零时返回 PrecisionUncertain。NaN、溢出、未知几何、超预算、取消有独立结构化结果，不能悄悄退回几何中心。两种中心位置允许在材料外部，例如环形或凹形选区；UI 不强行吸附回材料。

## 5. 统一交互及变换契约

公共 BasePoint 使用 f64 world mm，携带来源 Numeric/GeometryCenter/AreaCenter/Feature/Grid/Raw、求值上下文和必要的误差界。来源用于说明与重新选择，不是永久关联约束；本次基点不写入制造模型或 .rcam schema。格式化坐标不回流改变原始 f64 值。

所有适用工具提供统一入口：数值 XY、画布拾取、几何中心、面积中心；无选区时两种选区中心禁用并解释，无有效面积时仅面积中心不可用。单位输入复用现有 DisplayUnit/ManufacturingPrecision；有限性、数值范围和精度校验不能另造不一致规则。

选中五个对象（允许来自多个层）→ 点击任一中心按钮 → 显示 B 标记 → Move/Copy 选择目标 T 或输入 Δ → 同一世界变换施加到全部目标，保留各自 LayerId、相对位置和曝光顺序。旋转 p'=B+Rθ(p−B)。水平镜像 p'=(x,2By−y)，竖直镜像 p'=(2Bx−x,y)；真实圆弧方向随反射正确切换。

“拾取基点”是独立模式：进入时冻结当前选择 ID、顺序和 primary；暂停 ordinary click-cycle、框选、Grip 启动和制造拖动。点击当前选中对象轮廓只获得 B，不取消或切换选择；也可拾取其他可见可选对象作为参考。右侧/状态栏明确显示当前在拾取基点或目标，避免误把一次点击提交制造修改。

Snap 复用 ADR0036：默认 8 physical px acquire、11 physical px retain；世界半径随 camera×pixels_per_point 换算。已捕获候选在 retain 范围内保持；新候选按距离+0.35×kind_priority 排序，再按稳定 ID 决定平局。Endpoint/Vertex=0、Intersection=1、Midpoint=2、Quadrant=3、Center/ArcCenter=4、Nearest=5。Object 高于 Grid，Alt 临时原始坐标。不得新建全工程捕捉点数据库。

**轮廓点击必须真的可用：** 专用基点拾取模式显式启用 Manufacturing Boundary 的 Nearest 解析投影，同时保留端点/中点/圆心等已有优先级，显示当前捕捉类型；不静默修改全局 Object Snap 开关或最近点偏好。标准光圈孔洞捕捉目前属于 S4-C1 限制，不宣称已支持；本任务先覆盖已有 provider 可提供的边界，并在 UI/能力中明确孔洞限制。面积计算仍必须正确扣除孔洞。若要同时支持孔洞轮廓拾取，需在正式冻结时明确增加 provider 用例，不能拿面积正确代替拾取支持。

基点拾取必须包含已选对象；移动目标拾取沿用现有排除被移动选区的规则，避免跟随自身预览吸附。Locked geometry 可作参考，但若编辑目标含 Locked 则整体拒绝；隐藏/不可选参考不进入候选。Perpendicular/Tangent 只有保留类型，不能在本阶段声称已实现。

Esc 在拾取子步骤恢复此前有效 B 和原选区，退出整个工具则取消所有预览；失焦、PointerGone、窗口关闭、切换工程同样无制造提交。返回上一步不改变 revision/dirty/history。文本/IME 焦点优先于工具快捷键。

数值、预览和 commit 共用同一 resolved transform；不能先画以鼠标点为 pivot 的预览，再用 AABB center 提交。Copy B→T 直接使用 SelectionEdit::Duplicate(dx,dy)，一次 revision/Undo；不先原位复制。Move/Rotate/Mirror 继续 ApplicationService 原子修改。Undo 保存精确历史，不靠逆变换消除误差。

选择成员/顺序/primary、文档 revision、工程身份、aperture/block definition、权限变化或 Undo/Redo 使进行中的计算/预览失效；普通 pan/zoom 不移动已确认 world B，但刷新屏幕候选和 hysteresis。工具切换不能继承过期选择中心。显示单位变化只转换输入/标签；制造精度变化重新校验精度相关结果。拾取过程中由用户明确改变选区时结束旧 session 并从新选区重新开始。

Block 创建允许 Numeric/Pick/双中心设定义基点，仍要求同层完整可编辑对象；Block 放置时选择已有定义局部参考点，再把它映射到 T，不修改 Definition。对于库中未放置对象的双中心，采用定义内部制造几何局部计算后经预览 transform 映射，不能借用不相关的当前选区。文字只对已有选区提供“以选区中心为参考”的明确动作；未提交字形的布局 anchor 仍按文字对齐规则。Array 的 B→T 辅助给出 pitch_x/pitch_y，不平移 cell(0,0)，原有有符号 pitch、行优先、完整文字组、同层限制不变。

## 6. 计算能力现状与技术路线

`editor-core/src/metrics.rs` 目前只返回 area_mm2/perimeter_mm，标准光圈、部分线弧和 Region 有解析公式，没有一阶矩或 centroid。Block metrics 将子项面积相加，不能代表曝光合成面积。Macro material union、部分多轮廓/宽弧明确 Unsupported。`editor-service/src/metrics.rs` 的 object_area_sum_mm2 是逐对象面积之和，不是本需求。现有面积缓存对 rigid transform 可复用的规则不能直接用于 world centroid。

`editor-core/src/hit_test/material.rs` 有 line/circle 交点和 Macro material 边界判断可复用；membership/distance 的 witness edge 并不自动成为已定向的合成外边界，特别是内部接缝，不能直接全量积分。

已锁定 `clipper2-rust 1.1.0`（BSL-1.0），当前由 editor-text 的 offset 使用，core 当前只有 serde。已核对本机 crate Cargo.toml 和 `src/clipper.rs`：intersect_64/union_64/difference_64 接受 Paths64 与 FillRule；这是整数直线多边形布尔，不是保留真实圆弧的现成面积重心 API。文本模块的 1e8 缩放和 1e-4 mm 弧误差不能直接成为本任务精度承诺。

作者文档指出整数舍入和极小边可能影响拓扑；大整数范围内仍需控制交点精度。因此“加密两次结果接近”或任意固定 tessellation 不能作为制造精度证明。[Clipper2 robustness](https://www.angusj.com/clipper2/Docs/Robustness.htm)、[Clipper2 overview](https://www.angusj.com/clipper2/Docs/Overview.htm)。上游文档与 Rust 1.1.0 版本不等同，实施 API 以已锁定源码为准。

建议先完成一个可测闭环：几何中心与统一变换状态；随后完成带矩的制造材料查询，再接入面积中心 UI；最终缺任一中心不得宣称本任务完成。所有新增 UI 在能力接通后才启用，不交付空按钮。

面积计算路线在编码前做定向技术验证并写 ADR：

1. 在局部平移坐标下计算面积和一阶矩，使用稳定累加；标准形和已证明的 line/arc boundary 使用解析积分，保留真实弧。复用 bounds/geometry，不读 Mesh/GPU/像素。
2. 同层 ordered Boolean 先获得真正的合成边界/材料分区，再积分。无相交、无遮盖的可证明快路径才可相加；不能把 Clear 自身面积无条件当负数减，也不能对子对象面积直接相加。
3. 优先研究解析 line/arc arrangement 与边界裁剪后积分；可重用已有求交但需处理重合边、相切、多连通域及零宽退化。不能声称现有 hit-test 已完成这一证明。
4. 若用整数 polygon Boolean 作为后端，必须从语义几何生成独立的受控近似，给出曲线、整数舍入、交点与退化处理共同误差界。可研究内外包围集合 G−⊆G⊆G+；Dark 时各自 union，Clear 时 S−\G+、S+\G−。Clipper 本身不自动提供这些包含关系，必须补证。不能证明时返回 PrecisionUncertain，或改用解析后端，不能静默给“精确中心”。
5. 从面积/矩区间推导 centroid 的误差。以局部坐标 |x|,|y|≤R、对称差面积 E、面积正下界 A_low，可保守使用逐坐标约 2RE/A_low 的界并核实推导条件。近零剩余材料需要更严格细化，不能仅给 area 相对误差就声称 centroid 精确。
6. 提议冻结的 centroid 每坐标绝对误差上限 ε=min(1e−5 mm, resolution_mm/10)，与 zoom/DPI 无关；输出误差界≤ε才为 Success。默认 resolution=1e−4 → ε=1e−5 mm；最细1e−6 → ε=1e−7 mm。此为待技术验证的新门槛，未测，不以显示舍入掩盖误差。真实模型不因查询被量化。
7. 对所有当前已支持制造对象（包含标准孔洞、宏、Region、Block、完整文字）建立非退化公开夹具，每类至少一个组合场景必须成功；不能用“Unsupported 全拒绝”交付面积中心。预算/数值极限场景可结构化拒绝；若正常支持对象无法达到门槛，任务未完成并报告具体阻塞。

刚体 transform 下 A 不变，M'=R M+A t（反射后材料面积仍为正，边界方向规范化）；只有整个合成集合共同变换且语义相同，才能直接变换已有 centroid。个别对象移动、曝光顺序或相互重叠改变时必须重算合成。

## 7. 服务、缓存、异步与资源预算

新增只读选择中心查询建议名 `geometry.selection_centers`，最终名字随 API 审核冻结。请求显式 document_id、expected_revision、分层 IDs、查询 kind、已确认的 selected-layer-composite 语义和精度策略，不读取 GUI 选择。结果含 computed_revision、world bounds/point、可选 area_mm2/first moments、绝对误差界、结构化状态与诊断。自有 JSON DTO；不暴露第三方 AST/路径或可变模型。

服务无窗口依赖；GUI 与 headless 共用计算。查询成功/失败/取消均不推进制造 revision、dirty 或 Undo。异步封装复用完整 TaskVersion，加 selection_epoch/fingerprint 与请求 generation；同 document_id/revision 不足以判定有效。同 revision 不同选区、切换工程、关闭重开、历史回到相同内容、权限变更及新请求到达都不得显示旧结果。取消与结果投递竞争遵循 A2 终态原子控制，迟到结果不得覆盖新 B。

缓存 key 覆盖对象几何 token、aperture 内容/revision、block definition 内容/revision、instance transform、LayerId、选中集合和原曝光顺序、曝光极性、查询语义与 precision。选择顺序对面积无影响时可规范化，但 GUI primary/选择 epoch 仍参与 session fencing。内容缓存不缓存过期 GUI 身份。坐标平移后的面积 cache hit 不代表 centroid 可直接复用。

提议初始资源上限（实施前写入正式附录并测量，不能据此声称已有成绩）：沿用总目标≤10,000；展开后原语≤100,000；生成边≤1,000,000；交点/Boolean work≤2,000,000；单查询主要临时数据≤128 MiB；中心缓存≤64 MiB、最多64个选区结果，超过按 LRU 释放。无隐式分层部分结果，无无限队列。需将既有更严格预算与这些新上限取交集，不能放宽旧约束。

单项非抢占算法调用先测最长耗时，包括 polygon Boolean；只在调用前后放 checkpoint 不足以保证2秒。若锁定后端长段不能满足≤2秒受控终止，必须选可分段/可取消实现或更严格的经测预算，不能设置取消标记后继续无限算。禁止仅把耗时搬到线程就称可取消。

## 8. 独立验收矩阵（全部尚未执行）

| 新用例 | 核心检查与独立 oracle | 关联 |
|---|---|---|
| S5I2-BP-01 | 五对象、至少三层，组合 world AABB center 数值正确；选择数量/顺序/primary 不变 | R07/R09/R10，AT022/031/033 |
| BP-02 | Numeric/几何中心/面积中心/轮廓 Nearest/端点/中点/圆心来源分别执行 Move/Copy/Rotate/HV Mirror；预览 transform 与提交一致 | R10/R13，AT031–035/044 |
| BP-03 | 点选已选对象轮廓、被遮挡轮廓、锁定参考、Alt/Grid、DPI1/2、pan/zoom；物理半径和滞回吻合；不触发 I1 click-cycle | R08/R09/R13，AT025–030/044 |
| BP-04 | Pick/Back/Esc/blur/capture-loss/tool-switch/IME；文档/选择/历史完全不变，无幽灵副本 | R09/R10/R11，AT032/039/043 |
| BP-05 | 每个操作一次 revision/Undo；Copy 独立 ID、完整文字组；多层 LayerId/曝光顺序不变；单层权限失败整体拒绝 | R05/R07/R10/R11，AT034–040 |
| BP-06 | 不等面积分离矩形：R1=[0,2]×[0,2]，R2=[4,5]×[0,1]；A=5，centroid=(1.7,0.9)，bbox center=(2.5,1) | R04/R05，AT011–018 |
| BP-07 | 重叠矩形 R1=[0,2]×[0,2]，R2=[1,3]×[0,1]：union A=5，centroid=(1.3,0.9)；拒绝逐对象加权结果 | R05，AT017 |
| BP-08 | 同层 Dark [0,4]×[0,2] 减 Clear [0,1]×[0,1]：A=7，centroid=(15.5/7,7.5/7)；再 Dark 加回该单元恢复(2,1) | R05，AT017 |
| BP-09 | 两个重合 unit square 加远处 [4,5]×[0,1]；三者同层 centroid.x=2.5；重合项分到不同层且远项与其一同层，跨层总 centroid.x=11/6 | R05/R07，AT018 |
| BP-10 | 未选 Clear/未选 Dark 不参与；只选 Clear 返回 ZeroArea；全部擦除 ZeroArea；近零且界不确定返回 PrecisionUncertain | R05/R16，AT017/020 |
| BP-11 | 有孔 Flash 叠在已有 Dark 上，孔洞不擦背景；Macro 内部 Clear 同理；Block 子 Clear 则按展开顺序影响该层此前选中 Dark | R04/R05，AT011/017 |
| BP-12 | 上半圆 r：A=πr²/2、centroid=(0,4r/3π)；圆环中心；圆帽直线 capsule 中点；偏心多孔 Region、宽弧端帽重叠 | R04/R14，AT012–015 |
| BP-13 | cut-in Region 与合法洞、镜像弧、transformed Block、共享定义、多实例相交、完整中文/英文字洞；解析或独立积分/集合分区 oracle | R04/R05/R12/R14，AT013–018/048–051 |
| BP-14 | 大坐标+小面积残留、相切/重合边、细缝、极小洞；误差区间包含独立高精度结果；精度不足必须结构化失败 | R14/R16，AT020/034 |
| BP-15 | revision/selection/workspace/definition/precision/new-request stale 矩阵及 cancel/complete 竞争；旧结果绝不修改 B/选择/制造 | R16/R21/R22，AT086–092 |
| BP-16 | Numeric/Pick/双中心接入 Block 创建/放置、文字参考、测距、Array pitch、已有 Board 点；保留各自合法性及单层限制 | R10/R12/R13，AT033/044/045/050 |
| BP-17 | GUI 同调用实现的真实 headless 查询→变换→Undo/Redo→导出→新文档重开；独立断言几何、曝光、所属层及坐标 | R14/R21/R22，AT086–092 |
| BP-18 | 下述冷/热缓存、密集拾取、长段取消、内存预算及三轮 release native；保存逐样本原始数据，不用平均值掩盖尾延迟 | R16/R17/R19，AT065–067 |

矩形 oracle 用有理数/明确分区手算；圆弧用解析 π 公式；复杂夹具用独立高精度积分或独立工具，并记录版本、容差和人工可复核的材料分区。不能复用被测函数得到 expected；随机刚体协变只是补充，不能替代绝对几何断言。自导出再导入不是唯一真值。

## 9. 性能、原生验证和交付流程

性能固定机器、AC、release、显示DPI/刷新率、样本 SHA、source/binary身份。沿用冻结 P10K/P100K，另固定五对象跨层、10k分离标准图元、同层密集重叠、Clear薄残留、Macro/Block/多孔Region矩阵。记录选中数量、展开原语数、边/交点数、精度、冷/热cache、取消最长段、峰值内存、每次耗时。

拟定本任务新计算预算：10k分离图元几何中心冷查询 p95≤100ms，面积中心解析快路径 p95≤200ms；复杂面积查询≤2秒成功或明确预算/精度失败，并显示可取消进度，不能用失败通过正常夹具；有效选区缓存命中≤10ms。每项至少30次、冷/热分列，不混合结果。拾取沿用 P100K CPU query p95≤20ms、可见反馈p95≤100ms；导航继承原 p95≤33.3ms/p99≤66.7ms 与>200ms停顿禁限，不把局部指标当完整P100K收口。新任务取消反馈≤500ms、受控终止并释放主要临时资源≤2秒。以上为拟冻结指标，不是实测成绩；实施前若需调整必须先更新设计/附录并说明，不能测试失败后暗降门槛。

UI 每帧不扫描全选区或重做 Boolean；中心随选区/内容失效后台计算，Snap 走 WorldIndex 附近查询和 lazy features。普通 pan/zoom 不重算面积。缓存压力下可重新计算但不得返回旧结果。取样需区分发起、工作完成、事件投递与可见反馈时间，不把 CPU 时间写成端到端延迟。

实施顺序：

1. I1 clean closeout 后核对新工作树并冻结正式 docs/ADR/新增用例和能力矩阵；保存当前缺失行为的失败回归。
2. 先完成统一基点与几何中心服务/变换小闭环；定向真实服务测试，不先铺全部工具空壳。
3. 面积/一阶矩/ordered材料合成技术验证，确定可证误差与可取消后端；通过独立解析矩阵后接入第二中心。
4. 逐项接通现有适用工具，完成 cancellation/stale/cache/权限和输入焦点回归；此前任何未完成不称本任务 PASS。
5. 执行 fmt、check、clippy、workspace tests、release build、automation_contract、headless_workflow、服务正常依赖树；加定向新测试和既有 A2/M2B/I1 回归，保持单 Cargo 写入者。
6. 同一冻结 release source/binary 在本机 Metal 至少三轮真实 native。每轮含五对象跨层两种中心、拾取保留选择、四类变换/复制、取消、Undo/Redo、项目/导出重开、工具复用与性能原始记录。只有真实输入/窗口证据可标 native；内部调用不能冒充物理鼠标端到端延迟。
7. 提交完整 candidate Source/Evidence、portable verifier 与篡改负例，交父任务独立审核；有缺陷继续修复和重跑受影响矩阵。
8. 审核通过后阶段 Git commit，不 push；干净构建并制作 Source/Evidence 同 commit 包，fresh extraction 验证。历史失败、run ID 和报告不覆盖。

计划执行命令（本次只读规划均**未执行**）：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo tree --locked -p editor-service -e normal
```

定向测试/脚本名待实际创建后记入正式计划，不虚构现存命令或通过记录。Windows deferred/not executed；不声称完整 V1、CORE10 10/10、完整 P100K 或 PMIX 通过。

## 10. 当前交付与剩余条件

已完成：当前源码能力盘点；两种中心明确区分；用户确认面积合成范围；统一交互、所有适用已有工具、制造语义、算法风险、误差与性能计划、独立验收及收口流程草案。

未执行：本任务代码修改、构建、自动化测试、原生实验、依赖变更、阶段提交。主要技术风险是有重叠/孔洞/圆弧的 ordered 材料合成、可证 centroid 精度和不可取消库长段；必须通过技术验证才能冻结算法，不能用现有 object_area_sum 顶替。

当前无待答的面积口径问题。正式实现等待 I1 独立审核和 clean closeout；父任务应基于 I1 最终 commit 安排本阶段。本文和只读核对证据留在外部 exports 目录，没有加入 I1 source manifest 或候选包。
