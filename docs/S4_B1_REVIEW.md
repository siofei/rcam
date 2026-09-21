# S4-B1 — Multi-Gerber Workspace review

2026-09-21/22。范围见 [S4_B1_PLAN](S4_B1_PLAN.md)，决策见 [ADR 0029](adr/0029-multi-gerber-workspace.md) /
[ADR 0030](adr/0030-reusable-blocks-and-forward-reservations.md)。Windows deferred / not executed。

## 结论（必须先读）

**状态：实现完成；Mac 原生 fmt / check / clippy / 全量 test / release / Metal parity 全部通过；原生 GUI 已做冒烟核查
（发现并修复 3 个 GUI 缺陷）；固定 ZIP 交付与 fresh-extract 尚未生成。**
按 AGENTS.md 规则，没有完整的原生证据链（尤其固定 ZIP、干净提交、§107 十四项逐项截图）前，**本报告不宣布 “S4-B1 PASS”**，
由评审者判断。S4-B2 `.rcam` 未开始，格式未冻结。

## 已实现

- Gate 0.1：`system.capabilities` 公布的 operation 与实际可分发的 operation 一致，测试
  `capabilities_are_consistent_with_the_supported_operations` 核对（含 `document.set_manufacturing_precision`）。
- Gate 0.2：`scripts/package_release.py` 在两个最终 ZIP 生成后写 `SHA256SUMS.txt`，只含本阶段 `RCam_S4B1_<sha>_source.zip`
  与 `_public_evidence.zip`；`scripts/test_package_release.py` 5 项通过（Mac 上另发现并修复 `write_text(newline=)` 的 Python<3.10 兼容问题）。
- Gate 0.3：README / CAPABILITIES / IMPLEMENTATION_PLAN / GLOBAL_UNITS_PRECISION_REVIEW 记为
  “Global Units & Manufacturing Precision = PASS（Mac-first bounded）”，不宣称 Windows、完整 V1、P100K 或完整 CORE10。
- Core（`editor-core`）：`workspace.rs`（LayerId/source、ClassStyle、DisplayClass、有效状态、`LayerContentSummary`/`DeleteRisk`、
  自动配色）、`edit.rs`（层事务：add/create/remove/reorder/update，one transaction）、占位 `drill/board/block/snap/command`。
- Service：多层 Workspace 记录、原子批量导入、命名空间、`layer.summary`、`document.remove_layer`（`allow_non_empty`）、
  Undo 恢复同 LayerId/z-order/样式/active、`layers.*`、`render.snapshot`（含 view style，BOTTOM-first）、`layers_list`（TOP-first）、
  单层 `gerber.export_layer`、按制造 revision 缓存的 dirty。
- GUI：layer_panel（单列紧凑行、右键/`⋯` 同菜单、Settings/Categories/Rename/Delete 对话框、New Workspace、导入多选）、
  display.rs/WGSL（逐对象层色、Filled/Outline/ZeroWidth、类别可见性进入 RenderIndex、反色 selection halo）、
  Export Gerber…（Save/Save As disabled）、`Action::LayerSummary` 先取摘要再走删除确认。

## Mac 原生证据（Apple M1 / macOS 26.5.1 / Rust 1.89.0 锁定工具链）

证据目录（Git 忽略）：`evidence/s4b1-native-dev-20260921-r3/`（完整门禁）与 `evidence/s4b1-native-dev-20260922-r4/`（GUI 修复后的
fmt/check/clippy/全量 test/release/Metal/manifest 复跑，全部 rc=0，同为 491 通过 / 0 失败），含 `env.txt`、各步 `.log`、`summary.txt`；
中间轮次 r1/r2 与 `evidence/parity_*.log` 记录了失败与修复过程。

| 项 | 命令 | 结果 |
|---|---|---|
| 格式 | `cargo fmt --all -- --check` | rc=0 |
| 检查 | `cargo check --workspace --all-targets --locked` | rc=0 |
| clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | rc=0 |
| 全量测试 | `cargo test --workspace --locked --no-fail-fast` | rc=0，491 通过 / 0 失败 / 11 ignored（原生项另行运行） |
| multi-layer | `-p editor-service --test multi_layer_workflow` | rc=0 |
| core 层事务 | `-p editor-core --test layer_transactions` | rc=0 |
| release | `cargo build --release --locked -p editor-app` | rc=0 |
| Metal（5 项） | `cargo test --release --locked -p editor-app native_metal -- --ignored` | rc=0，5/5，其中 parity 288 个精确 RGBA 用例通过 |
| 清单 | `python3 scripts/source_manifest.py --check` | rc=0 |
| 打包脚本 | `python3 scripts/test_package_release.py` | rc=0 |
| 依赖树 | `cargo tree --locked -p editor-service -e normal` | rc=0 |

在 Mac 上才暴露、已修复的问题：

1. `s4a1_text_workflow` 仍匹配旧的 `ObjectOrigin::Generated`（文本对象已改为 `GeneratedText`）→ 测试改用 `origin.operation_id()`。
2. `package_release.py` 使用 `write_text(newline=)`，Python<3.10 不支持 → 改为 `open(..., newline='\n')`。
3. **Metal parity 不一致**（`s1a/region_cutin.gbr selection=1`，353 像素）：视图 0 中像素中心恰好距区域边缘 1.5 px，
   选择光晕的探针点落在形状边界上（`p.x+e` 仅差约 3e-8），Reference 与 Production 着色器在该平局上给出不同结果。
   修复：光晕探针距离改为 CPU 预先舍入的 `1.5 × 1.0123 / scale`（写入 `camera.w`，两个着色器共用），避开精确的半像素平局；
   同时光晕在每像素只应用一次（`edge` 标志），并对包围盒判定留 1 % 余量。修复后 16 个夹具 × 3 选择 × 2 位移 × 3 视图 = 288 例全部精确一致。
   这是一次真实的显示缺陷（选择轮廓在特定缩放下的边缘平局），不是测试问题。

## 原生 GUI 冒烟核查（`RCam.app`，release 构建，Retina ppp=2）

通过 computer-use 直接驱动窗口；截图只在会话中查看，**未存为证据文件**。已确认：

- 空工作区提示、Chinese UI、系统多选文件面板（3 个 `.gbr` 一次导入）→ 3 层原子导入、自动配色（青/品红/橙）、标题栏 `*`（脏）。
- 图层 `⋯` 菜单与 Settings 对话框（颜色预设 + hex、颜色模式、Filled/Outline/ZeroWidth）；Outline / ZeroWidth 在 Metal 上渲染，
  切换只推进 Workspace 版本，制造版本保持不变。
- 非空图层删除：确认对话框 → 删除 → “撤销” 恢复到同一位置与颜色；空层新建后直接删除（无对话框，带撤销条）。
- 新建空图层（黄色，位于顶部并成为活动层）。
- 文件菜单：新建工作区 ⌘N、导入、新建空图层、导出当前图层为 Gerber…；“保存工作区 / 另存为” 灰显并注明 S4-B2 `.rcam` 提供。
- 导出 Gerber：保存面板标题“导出不会保存工作区”，导出后状态栏为“导出不改变工作区，也不建立文件关联”，
  标题栏仍带 `*`；随后“新建工作区”仍弹出“放弃尚未导出的修改？”（Export 不清 dirty）。
- Selectable 开关只推进 Workspace 版本；点击对象得到 `src-1::object-9`、图层名、反色光晕。

原生检查发现并已修复的 GUI 缺陷（r4 构建后已原生复核：重做项显示 `Shift+⌘Z`；面板最小宽度 240 px 时名称显示为 `g…` 且无控件重叠，
点击 👁 可正常隐藏图层，被遮住的 4 号图层（导出后重新导入的副本）与原图层几何重合）：

1. 图层 Settings 中 “导入内容 SHA-256：” 用 monospace 字体，中文与全角冒号显示为方框 → 用户随后要求不再显示该哈希，已从 Settings 移除（provenance 仍保存 filename / SHA-256 / 时间）。
2. 快捷键提示 “⇧⌘Z / ⇧⌘E” 中 `⇧` 显示为方框 → 改为 `Shift+⌘Z / Shift+⌘E`。
3. 面板宽度拖到最小（原 190 px）时，六个固定控件占满一行，被截断的图层名画到 👁 按钮上并吞掉点击（§100–107 “控件不得重叠”）
   → 最小宽度提高到 240 px。

## 用户反馈迭代（2026-09-22）

1. 新增“全显 / 全隐”（图层面板头部 + 图层菜单），一个 workspace revision；全显同时结束 Solo；GUI 状态测试
   `show_all_and_hide_all_are_one_workspace_revision_and_show_all_ends_solo`。
2. 图层颜色新增色盘（egui 取色器，松开指针后一次提交）。
3. Settings 不再显示导入 SHA-256。
4. Solo 改为双击图层名切换（再次双击取消）；双击不再重命名，重命名保留在菜单。
5. 图层名点击区域扩展到整行剩余宽度（短名称右侧空白也可点击 / 双击 / 右键）。
6. 色板 4×4 网格间距收紧（此前 `Grid` 默认列宽 40 px 造成左右间隔过大）。

r5 Mac 验证（`evidence/s4b1-native-dev-20260922-r5`）：fmt / check / clippy / test / release / Metal（5 项含 parity）/ manifest（313 文件）rc=0；
`cargo fmt` 将 `layer_panel.rs`、`layer_tests.rs` 格式化，已回灌云端仓库并重新核对 manifest。

原生 GUI 复核（第二个 RCam 实例，`gui_primitives` / `region` / `rectangular_draw` 三层）已逐项确认：
全显/全隐按钮与图层菜单项均为一次 workspace revision 且制造版本不变（“所有图层已隐藏”提示、全显恢复）；
色盘取色器弹出，拖动色相后松开指针提交一次（工作区 revision +1，制造版本不变）；
Settings 不再出现 SHA-256，仅保留来源文件名；双击层名进入 Solo，再次双击取消；点击短名称“region”右侧空白即可激活该层；
色板 4×4 紧凑排列。截图未存为证据文件。已知边界：默认面板宽度下长名称/带 S 标记的行会被截断为“…”，可拖宽面板（240–480 px）。

## §90 Exit Gate 逐项状态

| 项 | 状态 |
|---|---|
| Gate 0 两项修复 + 0.3 状态收口 | 实现并测试通过；ZIP 实际生成**未执行** |
| New Empty Layer / Import Gerber Layer(s)（多选、原子） | 服务/状态测试通过；**原生**多选导入通过；Finder 拖放未验证 |
| Import 后与外部文件解耦；Save/Save As 不指向 Gerber；Export 不清 dirty/不建 link | 服务测试通过；原生菜单与导出后脏标记已核对 |
| 空层直接删 / 非空强确认 / Undo 同 id z-order style | 服务 + GUI 状态测试通过；原生已核对空层与非空层（**dirty/generated 更强确认、删最后一层未原生核对**） |
| 命名空间隔离、Layer exposure 隔离 | 通过 |
| visible/selectable/locked/reorder/active/solo | 服务通过；原生仅抽查 selectable 与选择；lock/solo/拖拽排序未原生核对 |
| 每层颜色 + 自动配色、按类别着色、分类 V/S/L | core/service/GUI 状态通过；原生已核对自动配色与层颜色；**类别颜色/类别设置对话框未原生核对** |
| Filled/Outline/ZeroWidth；View style 不改 writer bytes | 状态 + bytes 断言通过；原生 Outline/ZeroWidth 已渲染；Metal parity 尚未含 Outline/ZeroWidth/类别色专项用例 |
| hidden layer/class 从 render index 与 hit 候选过滤 | 自动测试通过；原生隐藏图层已复核（最小面板宽度下点击 👁 生效） |
| Text active-layer target | 服务 + GUI 状态测试通过 |
| Reference/Production parity | Metal 288 例精确一致（见上） |
| 每层导出、precision roundtrip、headless workflow | 通过 |
| 性能（10 层 × 1000 对象） | 仅 debug 下自动测试；release/native 数据**未采集** |
| 面板宽度/窄屏（§100–107） | 实现为 `SidePanel` 240–480 px + `truncate()` + tooltip；原生做了最小宽度检查、修复与复核，**§107 十四项未逐项执行** |
| native Mac evidence、package sidecar、fresh extract | 构建/测试/Metal/GUI 冒烟已有；固定 ZIP、SHA256SUMS、fresh extract **未执行**（需要干净提交，未提交） |

§59/§78/§85 的最低自动测试在云端与 Mac 上基本都有对应用例，已知缺口如实列出：
(a) §59-14 ApertureBlock：parser 仍拒绝 `%AB`，没有对象能产生该类，只测了保留类的颜色/默认样式与分类顺序；
(b) §78-20 浮动放置目标层被删除：`text_panel` 的预览校验会在目标层消失或 revision 变化时取消浮动并回到选择工具，
但这是 `EditorApp` 内的 egui 路径，没有无窗口的专项测试；
(c) §107 面板宽度 14 项属原生检查，仅部分执行。

## 设计决定与已知边界

- 面板顺序 TOP-first，渲染快照 BOTTOM-first；命中沿面板 top-first。
- Solo 只是临时可见性覆盖，不会显示用户已隐藏的层，也不写入 `visible`。
- Outline/ZeroWidth 是诊断显示：跳过 Clear 对象，Outline 用边缘测试而不是合成 Clear；不代表制造结果。
- Locked 层仍可删除（结构事务，可 Undo）。删除不触碰磁盘文件。
- 旧 `document.open` 仍保留无前缀 ID（测试/单文件 headless）；GUI 使用 NewWorkspace + ImportGerbers。
- Undo 历史按字节预算按整事务淘汰；被淘汰的 Remove 事务同步丢弃其侧数据。
- `crates/editor-app/Cargo.toml` 的 `objc2-foundation` features 增加 `NSArray`（多选文件面板返回 URL 数组），Mac 上 `--locked` 编译通过。
- 选择光晕的探针距离比 1.5 px 大 1.23 %（见上），属显示细节，不影响命中、测距或导出。
- 占位类型没有产品入口，不属于 “已支持” 能力。

## 仍需完成（未做）

1. 复核 Settings 中修复后的哈希行显示（r4 构建后只复核了最小宽度、隐藏点击与快捷键提示）。
2. 原生 GUI 其余项：类别设置对话框、lock/solo/拖拽排序、dirty/generated 更强确认、删最后一层、Finder 拖放、§107 逐项截图并存证。
3. Parity 新增 Outline/ZeroWidth/类别色/层色专项用例；10 层 × 1000 对象 release 性能数据。
4. 干净提交后：`test_audit_core10.py`、`test_package_source.py`、固定 ZIP 与 `SHA256SUMS.txt`、fresh extract。

## 下一阶段

S4-B2（`.rcam` Native Project Format + Block Core）待本阶段审查通过后另行启动；本阶段没有冻结任何 `.rcam` 字段。
