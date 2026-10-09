# S5 统一图形编辑器：真实现状审计与最小实施草稿

状态：DRAFT / SOURCE AUDIT ONLY / NOT IMPLEMENTED / NOT TESTED。2026-10-09。

本草稿读取产品源码 `cb9c8c741dd591b79149ab53b1754b7a3cf3a8c8`（直接父 `d8e8b87788c7779d561916d2baeb72325870e174`，708源文件，manifest SHA256 `ed223218d40c28f9aac42137a84a3ec2482e6344a1cdb588a1f11ae6d0c6d593`），需求依据独立规划 `13f1f970adaaf736a938a86247e552d9458dc429` 的 §2、§13、§15、§16、§26.1。两种基线不能混为一个产品身份。本次仅新增文档；不修改 cb9 源码、包、证据或门槛。cb9 已交唯一 Mac 线程，原生页签隔离仍待验收；其新回归优先于本草稿。

范围建议：S5/P3，R05/R09/R10/R11/R12/R16/R17/R18/R21/R22；兼容 AT-030/031/034/035/036/037/038/039/040/041/043/064/065/066/067/078/086/089/090/093。UE编号是新增专项草案，不复用AT，也不声明旧用例已通过。允许实施模块拟为 editor-core 编辑事务与只读规划、editor-service 显式命令、editor-app 独立编辑会话/UI/预览/路由及专项测试。此处列范围不是本轮源码修改授权。

## 1. 审计结论

已有跨层单步编辑和单层批次，但没有统一多步编辑会话。最小可交付第一片应为“一个窗口中的跨层 Move/Rotate/Mirror 草稿会话，内部步骤历史，最终一次制造提交”，先补会话/事务闭环，再分别加入尺寸、拓扑及参数镜像。第一片不承诺面积缩放、四边改形、内缩/圆角/斜角已经支持，也不放置这些操作的空按钮。

| cb9真实路径 | 已有能力 | 不能据此声称已完成 |
|---|---|---|
| `editor-service/src/selection_edit.rs`：`objects_edit_selection`；`editor-core/src/selection_edit.rs`：`SelectionEdit` | 单个 Move/Rotate/Mirror/Duplicate/Delete 跨多个层；全部权限先检查；核心只规划后一次提交；`Operation::Selection` 组织逐层差量 | 跨层有序多步骤会话；窗口内部Undo/Redo；最终把全部步骤合一 |
| `editor-service/src/lib.rs`：`BatchParams { layer_id, steps }` / `edit_batch`；`editor-core/src/edit.rs`：`EditHistory::edit_batch` | 一个层内有序 Move/Rotate/Mirror/标准Flash属性修改，暂存工作几何，末端校验并形成一次Undo | 不能逐层循环该API来实现跨层一次Undo；不能每次“执行本步”提交到主文档，再用逆变换模拟取消 |
| `editor-app/src/point_transform.rs`：`Session/Request/point_preview/point_apply` | 单步点变换；原制造几何只读预览，Context/选择匹配及预算/取消；真实提交走service | 当前预览是显示路径或简化包围盒，不能用作工作制造几何；当前单步Session不是新统一编辑会话 |
| `editor-app/src/point_input.rs`：`Context` / `Draft`；`point_adapter.rs` | TaskVersion/selection epoch/ProjectId/共享选择身份；显式f64点值、单位、中心与拾点适配 | Context本身不含新窗口会话和step身份，仍须叠加owner route与新会话版本，不能以相同ProjectId代替owner |
| `editor-service/src/selection_geometry.rs`：`geometry_selection_centers`；`editor-core/src/hit_test/selection_geometry.rs` | 制造包围盒中心、各层独立曝光合成的面积/面积中心；孔洞/真实弧/不确定性、预算和取消 | 已有的是原文档查询，非工作草稿查询；面积可返回ZeroArea或Uncertain，不是任意输入均有合法中心；不是缩放算法 |
| `editor-core/src/edit.rs`：`resize_flash/resized_shape`；`editor-core/src/grip.rs` | 标准Flash尺寸COW与有限Grip；R/O侧边按局部轴，保留对边；Macro等安全拒绝 | 任意Region/圆弧/组合面积缩放、世界四边伸缩；定义级全引用编辑；不能把局部Grip当四边命令 |
| `editor-text/src/offset.rs`：私有`material` | 字形局部多边形整数offset，Round join；有误差/工作预算，收缩时拓扑签名变化会拒绝 | 任意Gerber真弧/孔的通用内缩；用户要求分裂保留所有片，与此处收缩签名拒绝不同，不能原样搬用 |
| `editor-app/src/multiproject.rs` / `session.rs` | cb9页签文档容器与真实owner路由的源码候选 | Mac尚未完成，不能将其普通测试或历史PASS当统一编辑窗口隔离已验收 |

静态检索未找到通用制造内缩/圆角/斜角API、通用面积/四方向变换、入口参考+累积工作草稿+内部历史的完整闭环。该结论绑定上述源码，不用名称相似的文字offset、显示缩放或已有Grip填补缺口。用户自有v11算法是闭合Region近圆/分段优化，不能代替这些通用拓扑编辑命令。

## 2. 第一片 UE-A：仅刚体累积编辑与最终事务

### 会话及数据边界

进入一个固定宽度、受视口约束的窗口。绑定准确owner/DocumentId/revision/workspace与权限/精度/选区身份、唯一draft_session_id；锁定入口有序跨层目标。入口参考是不可变制造快照，只留本次窗口；默认不可选择/不可编辑，第一片也不将参考加入Snap索引。参考Snap属于待设计扩展，不能把该首片限制写成永久用户选择。

沿用一个真实worker/service；草稿是受预算的临时制造快照/规划数据，不另建第二GUI服务，不为每步在主文档commit/undo。工作几何来自f64制造快照，保留每层曝光顺序、稳定对象/定义引用、原始格式和真实弧；不从Scene/Mesh/point_preview.paths反推。显示预览只是工作快照的派生视图。用不可变入口+受预算的工作检查点/差量，明确入口、当前工作、当前步预览、内部Undo与Redo、当前请求/结果的版本。窗口内改参数以本步执行前的固定工作几何重新计算，不在每次键击/回复上累加。

只接 Move、Rotate、Mirror 三类已有制造变换：复用现有f64刚体变换、点输入、适用Snap/Alt与取消。旋转/镜像基点显式解析，不能用显示坐标；执行前校验完整Text组、定义/层/对象权限和现有不支持项。每步显式目标组是入口目标的合法子集；“应用全部选中”只展开该步范围，执行后不继承到下一步。异步计算离开UI，带入口/working_generation/step_id/request_id与参数身份；连续改参取消旧请求，旧/重复/跨工程回复只能丢弃，不能改工作或关窗口。

### 草案命令（均未新增到现有API）

| 候选类型化动作 | 输入/行为 | 历史及结果 |
|---|---|---|
| `draft.begin` | 显式owner/doc/revision/有序groups，取得入口制造快照及准入 | 主文档零修改；给draft身份/能力/预算 |
| `draft.preview_step` | 类型化Move/Rotate/Mirror、显式基点/单位/本步groups、working_generation | 从固定本步前几何重算；成功/错误均不改变已执行工作或主文档 |
| `draft.execute_step` | 接受当前版本的已验证参数/结果；稳定step身份避免重复执行 | 推进工作版本，加入一次内部历史；重复点击/回复不再累加；新步骤清内部Redo |
| `draft.undo` / `draft.redo` | exact draft+内部历史游标 | 恢复保存的前后差量/检查点，不靠逆旋转；主revision/dirty/Undo保持 |
| `draft.reset` | 用户“复原” | 精确恢复入口，清全部已执行步骤/内部Redo/未执行预览，窗口保持打开；旧请求失效 |
| `draft.apply` | 显式有序已执行步骤或可验证差量+入口fence | service重新规划/验证，一次跨层制造事务、一次主Undo；仅收到确切成功终态才退出 |
| `draft.cancel` / 关闭 | 有未应用工作变化时确认丢弃；Esc优先于同帧Apply | 丢弃草稿零制造事务，完整保留主内容/dirty/主Redo；释放临时资源 |

最终提交必须新增或扩展跨层“有序变换序列”规划，不循环调用会推进revision/history的`edit_batch`或`objects_edit_selection`。每一步权限、几何、定义引用、预算和取消检查在主文档发布前全部完成；形成入口before与最终after的逐层精确差量，复用一次`Operation::Selection`类事务。核心受检规划与预览共用变换逻辑；service在当前revision重新构造/验证，UI不能提交任意可信“after geometry”绕过service。一次成功事务推进一次主revision；主Undo/Redo恢复精确内容但revision仍单调前进。全序列若结果与入口等价，则不得生成空主Undo；明确no-change终态及退出行为在实施合同冻结。

需要单独冻结草稿历史条数/字节、入口+预览+历史总临时内存、步骤/选择/展开定义工作预算和取消粒度。复用已有核心100条/64MiB制造历史、单步预览2s/128MiB的审计口径不等于自动获得同样草稿预算，也不能把Move独立大选区准入扩大为所有操作准入。先拒绝不安全准入并保留原状态；新默认/上限须在实施合同明确，不能假称用户确认。最小片不修改依赖、shader、生产Snap/Alt、轨迹或原门槛。

### 尚需冻结的首片交互

有未执行预览时最终应用不能静默丢弃或额外执行。建议提供明确选择“返回执行本步”或“只应用已执行步骤”，默认处置仍未获用户确认；不得写成已确认默认。复原保留窗口已确认；关闭丢弃提示已确认。暂不在UE-A开放参考Snap、复制/删除/新增对象、参数镜像或定义级编辑；这些涉及新身份/结构/权限合同，分别后续交付。Apply失败不退出，保留草稿和错误以供重试/取消；unknown write terminal保持受阻，不把busy释放或本地成功当service终态。

## 3. 已确认全需求 → 后续独立命令/失败合同

下表完整映射上述几何编辑会话核心合同及相关独立交互项；UE-B/C/D均未实现，不因UE-A完成自动关闭。

| 已确认行为 | 草案命令/数据合同与实施片 | 必测要点 |
|---|---|---|
| `+0.1`执行后再`+0.2`=累计`+0.3`；击键不累积 | UE-A `preview_step/execute_step`，每步基准是上一步执行后的工作快照 | UE01/02，重复/乱序回复和step重放 |
| 入口参考底层整个会话固定，不可选择/编辑 | UE-A immutable entry；与working/preview视觉分离，参考不入制造或首片Snap索引 | UE03；多步骤/复原/缩放/设备重建后参考不变 |
| 窗口内部Undo/Redo；复原回入口且保持打开 | UE-A内部精确历史；`reset`清步骤/Redo/预览并失效旧任务 | UE04/05；主history/dirty不变 |
| 最终应用退出且跨层整个会话一次主Undo | UE-A `apply`全部预检+一次事务，exact terminal才退出 | UE06/07/08；跨层后一层失败不发布前层 |
| 本次操作“所有选中”不延续下一步 | 每个Step含显式groups和范围，不使用持久全局复选作为宏隐含状态 | UE09；两步范围不同、文字组完整、后续锁变化 |
| 面积比例，制造包围盒中心或扣孔面积重心 | UE-B `area_scale`，显式无量纲面积比和center mode；严格正有限比，均匀尺度`sqrt(r)`；从当前工作快照重算，零面积/无法证明中心明确拒绝，不能回退AABB | UE12；孔/Arc/LPD-LPC、Uncertain/ZeroArea、面积独立真值与误差界 |
| 跨层组合中心与范围可见，各层曝光独立 | UE-B显式组合语义；已有查询各层独立面积相加，可作候选，不能偷偷把多层合成擦除或冒称用户已冻结组合公式 | UE13；重叠同层/不同层面积不同，权限与工作版本fence |
| 左/右/上/下绝对mm改变相应边界，不强制保中心 | UE-B `edge_resize`，四个有符号边位移，明确坐标轴/方向和变换对象能力；原W/H与当前工作bounds是本步基准 | UE14；如宽10，左外扩1右外扩2→宽13、中心偏右0.5；非正新尺寸拒绝 |
| 四方向百分比取同一步开始W/H，上下各+5%=110% | UE-B `edge_resize_percent`，一次从固定当前W/H算四边位移；上下/左右两值和互换仅改参数 | UE15；20高→22而非22.05；第二步使用新的22，单位/百分比不能互换 |
| 轮廓内缩：外向内、孔向外，分裂保留全片；全消失对象失败 | UE-C `contour_inset`，显式正距离/对象来源→所有输出片或整对象失败；输出片稳定来源映射/曝光位次/孔义另审 | UE16/17；窄颈分裂、孔扩、岛保全、消失/退化/自交与量化重读 |
| 圆角/斜角任一角不能实现则整个对象失败 | UE-C `fillet(R)` / `chamfer(d1,d2,linked)`，先全角认证再发布该对象结果；两边距离默认联动是已接受建议，可解除 | UE18/19；后角失败前角不改、凹角/孔/真弧连接/窄缝，不能留部分角 |
| 失败对象保持原样或删除，先报数量/原因，最终一次事务 | UE-C `failure_policy=keep/delete_failed`显式参数；删除须确认exact failed源对象集合/预览版本，禁用默认静默删除；提交层面任何失败全批回滚 | UE20/21；成功对象与失败对象混合、失败列表变更、重复应用/权限变更、Undo完整源顺序 |
| 自定义基点/组合中心/轮廓拾点/Snap，鼠标移动与改形分开 | UE-A复用适用点输入；UE-B/C在工作制造几何上独立版本化查询；参考Snap另设计 | UE10/11；原Snap/Alt、焦点/IME/同帧Cancel、DPI与不得拾取参考 |
| 参数左右/上下镜像：来自当前编辑焊盘任意指定位置，目标保自身尺寸；两对象自动另一目标，多对象拾目标；默认只镜像本步，可选完整有序序列 | UE-D `mirror_parameters`绑定参数来源位置及source step/sequence、target基准、显式范围；稳定执行身份保证同一步同目标重试不重复累积 | UE22；源左+0.2→目标右+0.2、不同尺寸、顺序与预览fence |
| 防锡珠凸三角/凹三角/梯形/U及图解 | UE-D独立功能合同，原参考像素与切除/增加方向未核验，不默认把凸三角外加材料 | UE23；真实方向/孔拓扑/参数真值/失败原子性；待参考明确 |
| GUI和未来宏共用类型化步骤与最终apply | 序列记录显式units、scope/IDs、基点/中心/参数/失败政策、session和revision，草稿不发布制造成功事件 | UE24/25；headless与GUI同结果，不模拟键鼠、不混入文件I/O或跨工程事务 |
| PgUp/PgDn按当前比例乘/除同一可持久配置系数 | 独立视图命令，已确认快捷键默认可改；系数/锚点仍待冻结，不混UE-A制造会话 | 原NAV01–09；焦点/IME、逆操作、视图不改dirty/制造Undo |
| 小十字3倍与选中Block/Aperture基点显示默认开启 | cb9小十字源码目标36 physical px；既有小十字调整独立于UE-A。基点显示是后续显示偏好/设置合同，实例原点与面积/包围盒中心分开 | 原小十字/裁剪/DPI/大十字兼容回归；基点显示仅选中、可设置、默认开启、旋转镜像及定义坐标链；不借此声称native已验收 |
| 启动移动命令跟随鼠标、一次点击完成 | cb9已有Move placement单步源码闭环；沿用旧按住拖动，基点/目标、Snap/Alt、Esc和exact终态保护。统一会话是新组合路径，不把已有placement记成未做 | 既有move_place_tests及点输入取消/单次commit；会话内该入口适配另审，不能点击就提前向主文档提交 |

UE-B能力准入必须明确“世界四边尺寸变化”与“光圈局部尺寸”区别。非均匀缩放会将真实圆弧变成一般椭圆，当前Gerber语义不能靠屏幕细分替代；按支持对象集合安全拒绝，或在专项明确受误差界的制造转换，不能默默压成折线。面积均匀缩放的Aperture孔/Stroke宽度/实例坐标与共享定义COW也须全量处理，不能只动中心或Region顶点。UE-C需保持LPD/LPC顺序和AP局部孔，通用offset/倒角不以文字offset或自有近圆优化器顶替。全步骤结束的dirty/Undo/保存重开仍与UE-A同一原子边界。

## 4. UE-A准入及专项验收草案

| ID | 新增专项（PLANNED / NOT RUN） |
|---|---|
| UE01 | 同一入口多次参数输入预览幂等；+0.1执行、+0.2执行累计0.3；step_id重复/迟到不多执行 |
| UE02 | Move→Rotate→Mirror的非交换有序结果，用独立解析坐标/真实弧方向验证；不能把三个参数合并为无序一次变换 |
| UE03 | 参考entry始终同一制造字节，不入hit/Snap/选择/导出；工作preview在pan/zoom/DPI/device reset后正确 |
| UE04 | 内部Undo/Redo精确回滚，执行新步清内部Redo；主revision/dirty/Undo/Redo及已保存源字节全过程不变 |
| UE05 | 复原清全部草稿步骤/Redo/预览但窗口仍开；reset前正在运行/迟到/重复预览不可复活旧步骤 |
| UE06 | 两/三层含相同局部ID、Dark/Clear、孔/Arc和完整文字组的序列一次Apply、一次revision、一次Undo/Redo；对象/曝光/定义与导出重开独立真值 |
| UE07 | 后层锁定/删除对象/定义改变/未支持几何/NaN/Inf/溢出/预算失败/取消发生在最后一步，整个主文档/主Redo/dirty完全不变，草稿保留可修正 |
| UE08 | actual worker queue full/disconnect/unknown terminal/重复Apply/同帧Esc+Enter；仅真实admitted成功receipt退出，主事务至多一次 |
| UE09 | 本步全部/子集切换不继承下一步，范围可见；隐藏/锁定/incoming等入口与Apply二次权限校验不被“全部”绕过 |
| UE10 | UI固定窗口/控件矩形，短长非法/等待/失败统计、小视口滚动；键盘焦点/IME、鼠标拾点、既有Snap/Alt和child cancel保持 |
| UE11 | 两页签/恢复或打开的相同ProjectId不同owner：旧draft/preview/apply不能覆盖新记录；隔离字段分类包括新会话、内部history/任务及GPU显示身份 |
| UE12–15 | UE-B面积与四边量纲/中心/公式/能力/孔曝光/非正尺寸/不能表示的圆弧专项，另立实现与原生验收 |
| UE16–21 | UE-C内缩分裂/消失、整对象全角失败、exact失败删除列表及量化重读/Undo/提交全回滚，另立实现与原生验收 |
| UE22–25 | UE-D参数镜像/防锡珠/类型化宏及GUI共享，未明默认不擅自冻结 |

现有相关回归可以复用夹具与断言，不能当新UE已通过：

- `editor-service/tests/s3_edit_closeout.rs::edit_batch_is_one_atomic_revision_and_rejects_external_io_steps`：单层批次一次Undo、失败保redo/内容、预算/I/O拒绝。
- `editor-service/tests/selection_edit_workflow.rs::json_move_rotate_mirror_independent_coordinates_and_exact_history`、`later_layer_permissions_fail_atomically_and_locked_geometry_remains_queryable`、`unsupported_later_layer_transform_never_commits_earlier_delta`、`two_layer_edit_export_and_reopen_uses_real_writer`：跨层单步原子性/独立几何/导出。
- `editor-service/tests/unified_point_workflow.rs::three_layer_resolved_rotation_undo_redo_export_reopen_independent_coordinates`：跨层基点变换。
- `editor-app/src/point_transform_tests.rs::full_context_stale_matrix_and_bad_numeric_never_mutate`、`display_units_never_round_a_resolved_center_and_zero_area_never_falls_back`、`frame_cancel_beats_enter_repeat_and_mouse_apply_in_all_selection_tools`、`preview_budget_rejects_large_source_before_cloning_and_changes_no_state`：身份、精度、取消和资源。
- `editor-core/tests/selection_composite.rs::cross_layer_overlap_is_counted_independently`、`circle_annulus_and_polygon_holes`、`near_zero_sliver_fails_precision_without_bounding_center_fallback`：制造合成与不确定中心。

下一实施片应先冻结UE-A预算、未执行预览最终处置和无改动Apply终态，更新服务API/接受用例/ADR后实现完整最小闭环，再独立审查、精确新commit/manifest/完整源码包并交唯一Mac线程。新UE测试建议在service中新增`unified_editor_session.rs`、core中新增`selection_sequence.rs`、app中新增`unified_editor_tests.rs`，这些文件现在均不存在；不因建议路径写成“已有实现”。普通fmt/Clippy/相关tests/portable与native分别记录，严格MSRV已有阻塞保持真实状态。当前没有实现新命令、新增依赖、重跑native或修改cb9验收身份。
