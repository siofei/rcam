# RCam Gerber editor — S5-K1 shortcut settings / migration（candidate）

当前阶段从最终 S5-M1 基线 `0f3ec046640c4293b53e402305ad3e7f47000929` 独立启动快捷键设置与迁移，优先于尚未启动的 S5-M2。实现和证据等待独立终审及阶段提交，未标记 PASS。见 [候选审查索引](docs/S5_SHORTCUT_REVIEW.md) 与 [使用说明](docs/SHORTCUT_SETTINGS.md)。Windows/完整 V1/CORE10 仍延期。以下阶段摘要保留历史语义。

S4-B3 将已有 `.rcam` v1 格式接入 File → New/Open/Save/Save As；Gerber 仍只 Import/Export。工程保存使用受控路径、同目录临时文件、完整解码校验和原子发布；项目 dirty、关闭确认、最近工程和本机恢复副本见 [S4-B3 plan](docs/S4_B3_PLAN.md) 与 [review](docs/S4_B3_REVIEW.md)。当前阶段验收以 review 中的实测门禁为准，Windows 与完整 V1 尚未完成。
S4-B3 预冻结修正使 mm/inch/mil/µm 四种显示单位均能随 `.rcam` 保存、重开；Recovery 写失败后会按原间隔重试。

当前阶段状态：S4-C1 Full Object Snap、INFRA1 Runtime Diagnostics、S4-C2 Grip Editing v1、S4-C3 Block Editor v1 均为 **PASS（Mac-first bounded）**。C1 范围和证据见 [review](docs/S4_C1_REVIEW.md)，INFRA1 与 C2 证据分别见 `exports/INFRA1_PANIC_c65bee0/REVIEW.md`、`exports/S4C2_f1293f2/REVIEW.md`，C3 最终摘要见 [review](docs/S4_C3_REVIEW.md)。S4-C4 Alignment / Distribution 已按 Mac-first bounded 完成验收。Windows deferred / not executed；不代表完整 V1、CORE10 10/10 或 P100K 通过。

S3-FINAL 正式收口基础编辑：GUI 制造修改统一走 `ApplicationService`；补齐标准 C/R/O/P
Flash 尺寸写时复制、单事务 `edit.batch`、完整事务边界的 Undo 预算淘汰、对象端点/中心优先
Snap、mm/in 显示以及同一 release build 的 Mac 原生 P1K/编辑/另存重开门禁。范围、证据映射和
边界审计见 [S3 coverage](docs/S3_FINAL_COVERAGE.md)、[service audit](docs/S3_SERVICE_BOUNDARY_AUDIT.md)
与 [final review](docs/S3_FINAL_REVIEW.md)。S4-A1 headless vector text core 已实现；S4-A2.1 PASS（Mac-first），S4-A2.2 PASS（Mac-first）。Windows、完整 V1 与双平台验收仍未完成。

历史 `S2-B1/S2-B2/S2-B3/S2-C1/S2-C2` 名称和文件继续保留；其中编辑、Undo/Redo、
Grid/Snap/Measure 实际覆盖正式 S3 范围。自 S3-FINAL 起恢复 DESIGN_V1 的正式阶段编号。

S2-C2 将已有 `objects.rotate` / `objects.mirror` 接入属性面板：任意有限角、快捷 ±90°、
选择集制造边界中心/世界原点/自定义 Pivot，以及明确显示坐标的水平/垂直世界轴镜像。
多选只提交一个服务事务/Undo；Grid Snap 不量化 Transform 参数。见
[本轮计划](docs/S2_C2_PLAN.md) 与 [ADR0023](docs/adr/0023-s2c2-transform-gui.md)。

S2-C1 增加网格、显式 Grid Snap、光标毫米坐标与两点测距，见 [本轮计划](docs/S2_C1_PLAN.md)。
测距现可同时保留多条，并在线中显示距离与相对世界 +X 轴的逆时针角度；Esc 清除全部。
Grid/Measure 仅为视图状态；鼠标抓取点吸附，数值 Move 保持精确输入。

S2-B3 增加独立对象解析面积/周长、会话lazy cache和后台属性面板。多选只显示对象合计；复杂几何明确不可计算，不代表图层最终开口面积。任务范围见 [S2-B3计划](docs/S2_B3_PLAN.md)。

默认应用现已接入真实 Gerber 文件：原生打开、Finder 拖放、图层工作区、f64 导航、
精确几何点选/框选、多选、属性、数值移动、整组拖动、原位复制、删除、撤销/重做与新路径另存为。
范围和限制见 [ADR 0016](docs/adr/0016-s2a3-gui.md)。本轮范围见 [S2-B3.1计划](docs/S2_B3_1_PLAN.md)，上一阶段证据边界见 [S2-B3报告](docs/S2_B3_REVIEW.md)。Windows与完整V1仍未通过。

Mac 启动：`cargo run --release --locked -p editor-app`。
测试应用包：先构建 release，再运行 `python3 scripts/package_macos.py --out target/gui-package`。
应用包未签名/公证，仅供本阶段本机验证。S0 演示保留为 `cargo run --release --locked -p editor-app --example s0-demo`。

单文件，Ctrl-click 加选，Shift-click 减选；空白拖框左→右 Window、右→左 Crossing。Clear 和锁定对象可查看，制造编辑仅同层且无锁定成员。中键拖动或双指滚动平移，捏合或 Cmd+滚动缩放，F 适合窗口。
移动只使用 ΔX/ΔY 毫米；Cmd+Z 撤销，Shift+Cmd+Z / Cmd+Y 重做。文本字段保留文本撤销。
Gerber 导出必须使用新路径；导入源文件不会被覆盖。元数据丢弃必须按提示确认。
显隐、锁定和显示名保存在 `.rcam` 工程中，改变它们会标记 project dirty，但不改变制造 revision 或 Gerber writer。
显示采用 camera-relative local f32、safe zoom clamp 和 last-good-frame；资源预算失败保留诊断，不降低制造精度。

macOS 界面从系统字体加载中文，不打包用户字体；S4-A1 已支持中英文制造矢量文字核心；S4-A2 已接入 Text GUI、系统字体搜索列表（保留文件选择）、异步 Preview 和显式定位；最终验收状态见 S4_A2_REVIEW。
Windows、完整 CORE10、性能与双平台 V1 验收仍未完成；不能作为生产发行声明。

Rotate preserves true manufacturing geometry; mirrored arcs reverse direction.
Rectangular sweeps support exact multiples of 90 degrees only (odd turns swap aperture dimensions);
mirror axes are world horizontal/vertical. Unsupported mixed selections reject atomically.
Workspace display name/visibility/lock now live in the service, independently of manufacturing dirty and history.
`layer.update` checks both manufacturing and workspace revisions. New edits respect locks; Undo/Redo remain available.
See ADR 0012 and `docs/S1_B2C_REVIEW.md` for scope and evidence.

## Workspace

```text
crates/editor-core/       f64/mm geometry and layer coverage
crates/gerber-io/         strict finite parser adapter
crates/editor-service/    UI-free S2-B3 service and JSON request envelope
crates/editor-app/        eframe + egui-wgpu editor and S0 regression example
fixtures/synthetic/       immutable S0-C inputs and independent S1-A truth
docs/DEPENDENCIES.md      locked dependency and alternative record
THIRD_PARTY_NOTICES.md    direct dependency source and licence notices
evidence/                 local verification logs and review evidence
```

The toolchain is pinned by `rust-toolchain.toml`. Use an existing installation
of Rust 1.89.0 with rustfmt and clippy; do not switch RUSTUP_HOME to an empty
folder. The original machine has its existing toolchain and caches in
`.tools/`; those paths are local evidence, not prerequisites for other hosts.

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p editor-app
cargo run --release --locked -p editor-app
```

The real parser/service contracts live in `automation_contract.rs` and
`headless_workflow.rs` under `crates/editor-service/tests/`. They exercise
open/query/validate/export/reopen without a window. This is an unchanged
document baseline; the S1-B1 Move workflow and S1-B2a Duplicate/Delete and S1-B2b Rotate/Mirror workflows are implemented; the full V1 headless automation workflow remains incomplete.

```text
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo test --locked -p editor-service --test s1a_semantic_truth
cargo test --locked -p editor-service --test s1a_independent
cargo test --locked -p editor-core --test s1a_independent_geometry
cargo tree --locked -p editor-service --edges normal
```

Read `AGENTS.md`, `docs/DESIGN_V1.md`, `docs/ACCEPTANCE_V1.md`, and
`docs/AUTOMATION_API.md` for the governing design and acceptance boundaries.

## S0-B regression and evidence

```
cargo test --locked -p gerber-io --test review_parser_regressions
cargo test --locked -p editor-core --test review_geometry_regressions
cargo test --locked -p editor-service --test review_service_regressions
cargo test --locked -p editor-service --test historical_audit -- --nocapture
cargo test --locked -p editor-service --test dependency_boundary -- --nocapture
cargo test --locked -p editor-app --example s0-demo native_gpu_coverage_regressions -- --ignored --nocapture
python3 -m unittest discover -s scripts -p test_audit_core10.py
python3 scripts/source_manifest.py --check
```

The GPU check requires native Metal or DX12 hardware; it fails if no adapter
is available. It is explicitly ignored in ordinary workspace tests, not counted
as passed. Windows can use `python` for the dependency-free audit/hash scripts.
The original review attachment and its three regression files were absent;
new regressions are project-authored and do not claim to reproduce that attachment.
See `docs/S0_B_REVIEW.md` for actual runs, failures, platform gaps and file hashes.
Current source hashes are in `MANIFEST.sha256`; the old design-package manifest
is preserved under `docs/archive/`. Binaries have separate per-run hashes.

The S0 canvas release-frame drag fix and its regression/runtime evidence are
recorded in `docs/DRAG_FIX_REVIEW.md`.

S2-B3 GeometryMetrics implemented；完整 S2-B3 在 renderer 修复前受 1000 对象原生门禁阻塞。
S2-B3.1 active：生产显示改用有序 world-space bins，原 renderer 保留为 reference；见 [ADR0020](docs/adr/0020-scalable-render-index.md)。最终 native 门禁以本轮独立 evidence 报告为准，不由代码实现推断通过。

S2-B3.2：viewport-local preparation/budget 与 opt-in 原生 Metal benchmark，见 [计划](docs/S2_B3_2_PLAN.md) 和 [ADR0021](docs/adr/0021-viewport-native-gates.md)。最终收口结论由同一 clean commit 的独立 evidence 提供；P100K、PMIX 全 AT-075、Windows 与 V1 仍未通过。


### S4-A2.1 — 参数弹窗、文字轮廓和浮动放置

当前实现与验收边界见 docs/S4_A2_1_PLAN.md、docs/S4_A2_1_REVIEW.md、
ADR 0027 / 0028（docs 内路径去掉 docs/ 前缀）。
参数型功能使用独占 Modal；连续画布操作保持直接交互。
Vertical slab text geometry = retired；contour/Line/Arc Region = production path，
每个材料连通组件一个对象，字洞使用局部 retraced cut-in，writer 不经过 slab。
Mouse 文字先生成再浮动，仅平移预览，左键提交一个事务；取消不改制造内容。
GeometryMetrics 周长排除 cut-in 接缝，但对象合计不是图层最终布尔周长。

S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded，见 GLOBAL_UNITS_PRECISION_REVIEW）。
**S4-B1 Multi-Gerber Workspace = PASS（Mac-first）**（见 S4_B1_REVIEW，固定 ZIP + fresh extract 已完成）。
S4-B2（Block Core + `.rcam` schema v1）见下；DXF/SVG/PLT、Final Layer Boolean Area、Windows 仍未启动。
阶段实现不等于全部原生验收；实际状态以各阶段 REVIEW 为准。

### S4-A2.2 — 多行文本和菜单交互

后续范围见 [S4_A2_2_PLAN](docs/S4_A2_2_PLAN.md)。字型下拉默认使用 RCam 原创
ASCII 线条字体；中文选择本地系统轮廓字体。文本支持多行，Enter 换行，
鼠标模式确定后隐藏弹窗，Esc 恢复原草稿，画布单击提交一次 Undo 事务。
“插入 → 文本…”和“编辑 → 删除”提供菜单入口；编辑、工具、图层、视图补齐现有功能入口。
默认线条字体输出真实有限宽度 Line，轮廓字体继续使用 S4-A2.1 Region / Line / Arc 路径。
基线距离 0 为自动 1.3 × 字高；128 字符上限保留。
实际测试和未执行项见 [S4_A2_2_REVIEW](docs/S4_A2_2_REVIEW.md)，Windows 未执行。


### Global Units / Manufacturing Precision (current)

See [plan](docs/GLOBAL_UNITS_PRECISION_PLAN.md) and the corresponding review for current evidence.
S4-A2.1 PASS（Mac-first）；S4-A2.2 PASS（Mac-first）；Global Units & Manufacturing Precision = PASS（Mac-first bounded）。
Display uses camera-relative local f32, safe zoom clamp and last-good-frame.
Windows deferred / not executed；不宣称完整 V1、P100K 或完整 CORE10 release。


### S4-B1 — Multi-Gerber Workspace（当前切片，原生验收部分完成）

一个 Workspace 可同时包含多个独立 Gerber 图层（Top/Bottom Paste、Outline、Mark…）。计划与边界见
[S4_B1_PLAN](docs/S4_B1_PLAN.md)，实际证据与未执行项见 [S4_B1_REVIEW](docs/S4_B1_REVIEW.md)，
决策见 [ADR 0029](docs/adr/0029-multi-gerber-workspace.md) 与 [ADR 0030](docs/adr/0030-reusable-blocks-and-forward-reservations.md)。

- **Gerber 只 Import / Export。** 导入后与磁盘文件解耦（仅保留文件名、SHA-256、导入时间作 provenance）。
  菜单为 **Export Gerber…**（单层，不清 dirty、不建立 source link）；S4-B3 的 Save / Save As 只写 `.rcam`。
- 新建空图层、多选/拖放批量导入（全部成功或全部不加入，一次 Undo）、同一文件可导入两次成为两个独立图层。
- 每层独立颜色（确定性自动配色 + 预设/最近/拾色器）、Visible / Selectable / Locked / Active / Solo / Z 序、
  Filled / Outline / ZeroWidth；按对象类别（Stroke/Circle/Rectangle/Obround/Polygon/AM/Region/文字…）着色与过滤/锁定。
  这些都是 View state，不改变 Gerber 输出。
- 删除图层：空层直接删除，非空强确认，含修改/生成内容更强确认；一步 Undo 恢复同一 LayerId、层序与样式；允许删除最后一层。
- 面板可调宽，名称超长时省略号，完整名称见 tooltip / 设置 / 重命名。
- 长期架构约束（Block、Object Snap、Command/Shortcut、Board Coordinate、Drill）已合并进 AGENTS.md 与 DESIGN_V1，
  S4-B1 只保留占位类型。

状态：**S4-B1 Multi-Gerber Workspace = PASS（Mac-first）**。Metal parity（180+288 例精确 RGBA）、固定 ZIP 交付、
fresh extract（318/318）与 §107 逐项原生检查（14/14）均已完成，见 [S4_B1_REVIEW](docs/S4_B1_REVIEW.md)。
Windows、完整 V1、P100K、完整 CORE10 均未宣称。

### S4-B2 — Block Core + `.rcam` Native Project Model / schema v1（已验收）

范围与边界见 [S4_B2_PLAN](docs/S4_B2_PLAN.md)，最终证据与剩余边界见 [S4_B2_REVIEW](docs/S4_B2_REVIEW.md)，
决策见 [ADR 0031](docs/adr/0031-rcam-native-project-format-v1.md) / [ADR 0032](docs/adr/0032-block-core.md)。

- **Reusable Block**：`BlockDefinition`（项目级几何，存在 `SemanticDocument.block_definitions`）+ `BlockInstance`
  （层内 `SemanticGeometry::BlockInstance`，只允许 translation/rotation/mirror）。Move/Rotate/Mirror/Duplicate
  复用既有 `objects.*` 服务；新增 `blocks.create_definition_from_objects`、`blocks.create_instance`、
  `blocks.update_instance_transform`、`blocks.rename_definition`、`blocks.explode_instance`、
  `blocks.delete_definition`（被引用时拒绝）、`blocks.list_definitions`/`blocks.get_definition`。
  第一版无 nested block（`BlockObjectGeometry` 在类型层面无法表示实例）；Gerber Export 展平实例，
  RectangularSweep 在非 90° 旋转下 fail-closed。BlockInstance 已进入 renderer，definition display cache 按
  `(definition id, revision, rotation, mirror)` 共享，Filled/Outline/ZeroWidth、颜色与整实例选中均通过 Metal parity。
- **`.rcam` schema v1**：新 crate `crates/rcam-project`（无 egui/eframe/wgpu/winit 依赖，`dependency_boundary`
  测试核对），`.rcam` = ZIP 容器（`manifest.json` + `project.json` + `layers/*.json` + `blocks/*.json`），
  store-only、hand-rolled、确定性编码，读取全链路 fail-closed（路径穿越/重复路径/entry/string/JSON depth
  超预算/哈希不符/未知 `format_version`/未知 mandatory 类型均拒绝）。encode/decode 只是内存/测试路径，本阶段没有
  `File → Open/Save`；`system.capabilities` 继续把 `project.open/save (.rcam)` 列为 unsupported。
- **S4-B2 = PASS（Mac-first）**：同一 clean commit 的 workspace/codec/Block gates、Metal parity、原生 GUI
  synthetic Block 冒烟、400×100 release 性能、固定 source/public-evidence ZIP、sidecar 与 fresh extract 全部通过。
  同轮修复 Lisong/Songti Light `sdf 点`（3 mm、补偿 0）可见时的 GPU 轮廓扫描卡顿；详见 S4_B2_REVIEW。
  Windows、完整 V1、P100K 与完整 CORE10 release 仍不宣称；S4-B3 的正式文件生命周期见本文开头及阶段 review。


### INFRA1 Runtime Diagnostics（PASS，Mac-first bounded）

INFRA1 Runtime Diagnostics 与 panic hook closeout 已通过 Mac-first bounded 验收。最终结论、门禁和平台边界见 [fixed review](exports/INFRA1_PANIC_c65bee0/REVIEW.md)；Windows deferred / not executed。

历史复审曾记录 `85ad528` 为 ALMOST PASS；最终修复保留并调用 Rust previous/default hook，状态以 `c65bee0` 的 fixed review 为准。

S4-C2 Grip Editing v1 = **PASS（Mac-first bounded）**。单对象标准 Flash、Line/RectangularSweep、受约束 Arc 与 all-Solid line-only Region 顶点编辑经统一服务提交。Hole contour 与 Arc-edged Region node Grip、Macro Grip、多对象 Grip 和 Windows 均 deferred；详细证据见 `exports/S4C2_f1293f2/REVIEW.md`。


### S4-C3 Block Editor v1（2026-09-27）

S4-C3 Block Editor v1 = **PASS（Mac-first bounded）**。支持 Block Library、Create/Base Point、Place、实例 Transform、Rename/Delete、Select Instances、Explode、Project persistence、Gerber flatten export 与 Diagnostics；制造修改仍经既有 ApplicationService。内部 Definition 编辑、Array、Alignment、PnP/RefDes 未开展。范围、验收结论与最终交付身份以 [S4-C3 review](docs/S4_C3_REVIEW.md) 为准；Windows deferred，完整 V1、CORE10 10/10 与 P100K 均未通过。C3 收口时未启动 C4；本次 C4 状态见下节。

### S4-C4 closeout (2026-09-27)

S4-C4 Alignment / Distribution = **PASS（Mac-first bounded）**。六种制造 world-AABB 对齐与双轴等边缘间距分布已实现，使用最后选中对象作固定 Anchor；同层原子事务、Undo/Redo、BlockInstance、完整文字组、项目/恢复/导出、诊断和资源限制已验收。语义见 ADR 0040，范围及证据见 S4_C4_PLAN、S4_C4_ACCEPTANCE_ADDENDUM、S4_C4_REVIEW；最终干净提交、原生二进制及包校验身份以 versioned exports/S4C4_*/REVIEW.md 为准。Windows deferred/not executed；不代表完整 V1、CORE10 10/10 或 P100K 通过。C4 后停止，Array/Panelization 未启动。


### S4-C5 Array / Panelization v1 (PASS, Mac-first bounded; 2026-09-28)

The user started S4-C5 on 2026-09-27. Scope is non-associative Rectangular Array only, via `objects.array_rectangular`: same-layer contiguous ordered source, complete text groups, signed f64 mm pitch, row-major cells and shared Block/Aperture definitions. Original cell is unchanged; copies follow its span in one Undo transaction. UI preview is transient and bounded. See S4_C5_PLAN, S4_C5_ACCEPTANCE_ADDENDUM and ADR 0041. Earlier “Array not started” statements describe C4 closeout. S4-C5 is PASS (Mac-first bounded); final clean source, native binary and four-file package identity are recorded in the versioned exports review and S4_C5_REVIEW. Windows deferred/not executed; no full V1/CORE10/P100K claim. Stop after C5, without PnP/RefDes or associative arrays.


### S4-D1 PCB / PnP / RefDes Foundation (PASS, Mac-first bounded; 2026-10-01)

S4-D1 is PASS (Mac-first bounded). Candidate implementation acceptance, native and complete four-file archive audits passed; final clean source/binary/package identities are recorded in versioned exports/S4D1_*/REVIEW.md. Scope: explicit bounded CSV/TSV mapping/units/Side/rotation preview, independent ComponentPlacement table, rigid Board→World registration, RefDes exact/prefix/substring search and virtualized focus/overlay, atomic project Undo/Redo and v2 Board persistence/Recovery. No component↔opening association, library replacement, Drill, assembly export or Windows signature. Historical C5 stop points are superseded only by this explicitly started stage. See S4_D1_PLAN, S4_D1_ACCEPTANCE_ADDENDUM, S4_D1_REVIEW and ADR 0042. Windows/full V1/CORE10/P100K remain deferred; stop before S4-D2.


### S4-D1 coordinate input follow-up (2026-10-01)

The user requested real XLSX/TXT parsing and a manual table-mapping workflow after bc97e02 closeout. See S4_D1_INPUT_COMPAT_PLAN and ADR0043: bounded data-only XLSX with explicit worksheet/header (or no header), fixed-width UTF-8 TXT preserving blank Side, and explicit headerless CSV/TSV. GUI previews raw sample rows with per-column role/Ignore controls; columns, units and conventions need manual confirmation. Unknown units require explicit selection. Optional source metadata keeps old mapping defaults; only selected fields persist. The supplied TXT convention is user-confirmed blank=Top/m=Bottom/MILLIMETERS; Excel units and rotation conventions remain unconfirmed. This follow-up does not expand into D2. Actual checks and remaining native/platform gaps belong to its versioned review; prior S4D1_bc97e02 delivery remains historical evidence.


### S4-D2 RefDes-assisted manufacturing candidate selection (PASS, Mac-first bounded; 2026-10-02)

S4-D2 is PASS (Mac-first bounded; 2026-10-02). Registered component-local rectangle queries use explicit Gerber layers and shared f64 WorldIndex, preserve Dark/Clear and object type, aggregate whole text groups and atomic blocks, and provide deterministic revision-fenced bounded pages. Candidate != Association, final Boolean opening or footprint ownership. GUI highlight/focus/Replace/Add selection are transient and follow existing permissions/edit paths. No project schema change, footprint guess or persistent link. See S4_D2_PLAN, S4_D2_ACCEPTANCE_ADDENDUM, S4_D2_REVIEW and ADR0047 (0043 was occupied by D1 input compatibility). Windows/full V1/CORE10/P100K deferred. Stop after D2.
