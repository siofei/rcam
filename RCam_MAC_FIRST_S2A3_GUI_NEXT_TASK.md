# RCam 下一阶段 Codex 任务：Mac-first S2-A.3 GUI 基础编辑闭环

> 基线：S2-A.2 精确 Hit Test 已阶段性通过。  
> 当前平台：macOS Apple Silicon。  
> Windows：deferred / not executed，不作为本轮门禁。  
> 本轮目标：**把已有 Parser / Semantic / Edit / Workspace / Bounds / Hit Test / Writer 真实接入一个可操作的 Mac Gerber GUI，形成第一版可见、可选、可数值编辑、可保存的桌面闭环。**

---

## 1. 开始前必读

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/S1_B2C_REVIEW.md`
- `docs/S2_A1_REVIEW.md`
- `docs/S2_A2_REVIEW.md`
- `docs/adr/0012-s1-b2c-workspace-state.md`
- `docs/adr/0013-s2a-bounds.md`
- `docs/adr/0014-s2a2-hit-test.md`
- 外部复审：`RCam_S2A2_c13ff54_Review.md`

保留全部无界面回归和现有 service 契约。GUI 不得绕过 `ApplicationService` 直接修改 `SemanticDocument`。

---

## 2. 本轮固定范围

完成：

```text
真实文件打开
Finder Drag & Drop
图层列表
visible / locked / display name
Fit to Window
Pan / Zoom
屏幕坐标 ↔ f64 mm
单击精确选择
选中高亮
属性面板
数值 Move
Undo / Redo
Save As
导出后状态/路径提示
错误/阻塞提示
Mac 原生交互证据
```

本轮不做：

- 鼠标直接拖动物体；
- 多选；
- 框选；
- Grid / Snap；
- 测距；
- Rotate/Mirror 的 GUI 面板（服务已有，后续接）；
- Duplicate/Delete 的 GUI 工具（服务已有，后续接）；
- 中英文文字；
- Panelize；
- 大规模 R-tree；
- Windows。

不要因为现成 service 已有更多命令，就在这一轮把所有按钮一次性堆到 GUI。

---

## 3. 替换默认 S0 Demo

当前 `editor-app` 默认仍是固定：

```text
include_bytes!(s0_polarity.gbr)
S0App
4 layers / 16 objects
read-only technology demo
```

S2-A.3 默认启动路径必须改为真正的编辑器 App。

S0 correctness renderer/demo 可以：

- 保留到 test/dev feature；或
- 保留成独立测试 harness。

但发行 `editor-app` 默认窗口不得再声称：

```text
S0 read-only technology demo
No editing or export capability
```

---

## 4. GUI 与服务边界

建议 `editor-app` 内状态：

```text
EditorApp
├─ ApplicationService
├─ current_document_id
├─ current DocumentInfo
├─ selected_layer_id
├─ selected_object_id
├─ camera
├─ pending UI numeric values
├─ last error / warning
└─ renderer state
```

所有已提交制造修改必须走已有 service：

```text
objects.move
history.undo
history.redo
gerber.export_layer
layer.update
objects.hit_test
```

GUI 可以维护：

```text
selection
hover
camera
panel width
active field
```

但不得持有可写的第二份制造 Document。

每次成功编辑后刷新必要的：

```text
DocumentInfo
object DTO / layer DTO
render snapshot/change set
selection validity
```

---

## 5. 文件打开

### 5.1 File → Open

Mac 提供用户可见的文件打开操作。

原生文件对话框实现只允许存在 `editor-app`，不能让 `editor-service` 新增 GUI/窗口依赖。

如新增 crate（例如 native dialog），必须：

- 说明用途；
- 锁定版本；
- 检查许可证；
- 更新 THIRD_PARTY_NOTICES；
- 不改变 service dependency boundary。

### 5.2 Finder Drag & Drop

使用 egui/eframe 原生 dropped files 输入，将本地路径交给同一个 `document.open` 路径。

必须验证：

```text
中文路径
空格
#
.gbr
.gbx
```

### 5.3 打开失败

范围外/损坏文件：

- 不显示残缺画面；
- 不替换当前已打开文档；
- 显示结构化错误；
- 当前 dirty 文档不得被静默丢弃。

本轮可以在“打开另一文件时当前文档 dirty”的情况下拒绝并提示先 Save/Discard；不要求完整多文档 UI。

---

## 6. Layer Panel

左侧图层列表使用 service `layers.list` / Workspace state。

至少显示：

```text
visible checkbox
locked toggle
display_name
object count（如已有 DTO 可取得）
```

规则：

- visible=false：renderer 不显示该层，点击也不查询该层；
- locked=true：仍可显示、仍可选择，但编辑按钮/提交会被 service 拒绝；
- display_name 只影响 Workspace；
- 上述操作不得改变 manufacturing dirty；
- 不得产生制造 Undo/Redo。

当前文档只有一层时也使用相同模型，不写单层特例污染后续多层能力。

---

## 7. Camera：Fit / Pan / Zoom

### 7.1 制造坐标

Camera 内部的世界坐标参数优先使用 `f64`：

```text
center_mm: f64
pixels_per_mm: f64
```

GPU 最终 uniform 可以受检转换为 f32，但屏幕→制造坐标不要先降到 f32。

### 7.2 Fit to Window

只调用：

```text
document.bounds / layer.bounds
```

Fit 计算需考虑：

- 可见图层；
- canvas 可用区域；
- 固定边距，例如 5%；
- 空文档/空 visible layer 安全状态。

不得从 renderer mesh 反推制造 bounds。

### 7.3 Pan

Mac 至少支持：

- 鼠标中键/约定拖动；
- Trackpad two-finger scroll 平移。

不能与对象选择点击冲突。

### 7.4 Zoom

滚轮/Trackpad zoom 以鼠标当前位置为中心：

```text
zoom 前 cursor 对应 world point
==
zoom 后 cursor 对应 world point
```

设合理 min/max pixels_per_mm，防止 Inf/NaN。

---

## 8. S2-A.3 Renderer

### 8.1 架构

建立正式语义渲染入口，不再让默认 GUI 依赖 S0 `DocumentSnapshot` 的 4 层/16 对象 ABI。

建议：

```text
Semantic Document / render DTO
        ↓
editor-render（可新 crate，也可先为明确模块）
        ↓
correctness-first display geometry
        ↓
wgpu callback / buffers
        ↓
canvas
```

Renderer 只读，不修改制造模型。

### 8.2 正确性优先

当前目标不是十万对象性能，而是当前 Editable 子集**不漏画、不假画**。

至少处理当前实际可编辑对象：

```text
C/R/O/P Flash + hole + LocalTransform
Macro 1/4/21
Line
RectangularSweep
Arc
Region
Dark/Clear 顺序
```

如果某输入的完整显示尚不能证明：

> 整个 canvas/document 进入明确“无法安全显示/编辑”状态。

禁止只跳过未知对象继续展示剩余图形。

### 8.3 Dark/Clear 与局部孔洞

必须区分：

```text
Aperture/宏内部 Clear
    = 对象局部材料孔洞

SemanticObject Exposure::Clear
    = 对 layer 之前材料的擦除
```

不能把 Macro clear primitive 直接当作 layer global clear，否则会擦掉之前对象。

不同 layer 仍独立合成。

### 8.4 显示细分

Arc/曲线可以按 zoom 自适应 tessellation，但：

- tessellation 只属于显示；
- Writer/Hit Test/Bounds 不读取 Mesh；
- zoom 增大后不能因为固定 32 段产生明显折线；
- GPU 缓存优化延后，先记录 dirty rebuild 次数。

### 8.5 Selection Overlay

选中高亮作为独立 overlay/pass：

```text
normal layer render
        +
selection overlay
```

不要为了改变选中颜色而修改制造对象或重新生成 Gerber。

---

## 9. 单击选择

### 9.1 坐标转换

点击：

```text
screen logical/physical position
        ↓ camera
f64 manufacturing mm
```

明确 Retina `pixels_per_point`，不能把 logical point 当 physical pixel 混用。

### 9.2 tolerance

GUI selection tolerance 采用“屏幕像素 → mm”转换，例如：

```text
tolerance_mm = 6 physical_px / physical_pixels_per_mm
```

具体像素门槛在 S2-A.3 freeze，并写自动测试。

要求：

- zoom 越大，制造 tolerance 越小；
- 同一物理点击范围视觉感受稳定；
- tolerance 不写回 Document。

### 9.3 查询策略

仅在用户明确 click 时调用 `objects.hit_test`。

**不要在每个 pointer move/hover frame 调用**，当前算法仍是 O(N)，Macro 有每-query 准备成本。

可见图层的默认单选策略冻结为：

```text
从最上方可见图层向下
→ 调 objects.hit_test
→ 第一层存在 hits 时停止
→ hits.last() 为当前 selected ObjectId
```

理由：service 结果保持层内曝光顺序，最后一个是后曝光对象。

Clear 对象允许被选中。

locked layer 允许选中，但不能提交 Move。

### 9.4 点击空白

清除当前 selection。

### 9.5 编辑后

Move / Undo / Redo 后：

- selection ID 若仍存在，保持；
- 对象删除（虽然本轮 UI 不提供 Delete）时若以后发生，必须安全清除；
- 不能依赖旧 hit cache。

---

## 10. Properties Panel

右侧至少显示：

```text
ObjectId
Layer
Exposure
Geometry type
Origin (Imported/Generated)
关键制造参数（只读）
```

本轮唯一可提交编辑项：

```text
Move ΔX mm
Move ΔY mm
```

输入：

- 使用文本字段；
- 支持负数与小数；
- 非有限/非法输入不得提交；
- 0,0 不提交假事务；
- Enter/Apply 形成一次 service Move；
- 成功后 revision 更新；
- 失败不丢 selection。

不要让 UI 直接改对象坐标字段。

---

## 11. Undo / Redo

至少提供：

```text
Cmd+Z      Undo
Cmd+Shift+Z / Cmd+Y（按 Mac 约定冻结） Redo
```

并有菜单/按钮可发现。

规则：

- 文本框正在编辑时，普通文字编辑撤销不能误触制造 Undo；
- service history 是制造真值；
- Undo/Redo 成功后刷新属性和 renderer；
- lock 不阻止历史恢复（沿用 S1-B2c）。

---

## 12. Save As

本轮必须有用户可见：

```text
File → Save As…
```

调用已有 `gerber.export_layer`。

规则：

- 明确当前导出 layer；
- 不暗中合并层；
- 默认不覆盖源文件；
- 覆盖需要现有 service 安全策略；
- 写失败不改变 dirty；
- 成功后显示 `last_saved_path`；
- `source_path` 与 `last_saved_path` 语义继续区分；
- 导出后能由本软件重新打开。

当前多层文档如尚无“工程保存”能力，不要做假的 Save Project；只做 layer Gerber Save As。

---

## 13. 窗口标题和状态栏

窗口标题建议：

```text
RCam — filename.gbr *
```

`*` 仅表示 manufacturing dirty。

Workspace visible/locked/name 改变不得出现 `*`。

状态栏至少显示：

```text
cursor X/Y mm
zoom
selected ObjectId（截短显示可）
manufacturing revision
workspace revision
last error/warning
```

---

## 14. 错误与安全状态

GUI 不得吞 service 错误。

至少区分显示：

```text
UNSUPPORTED_FEATURE
VALIDATION_FAILED
RESOURCE_LIMIT
LAYER_LOCKED
REVISION_CONFLICT
FILE_CONFLICT
IO_ERROR
NOT_FOUND
```

任何 renderer fail-closed 状态必须明显阻止编辑/保存误操作，不能只在 console 打 log。

---

## 15. S2-A.2 遗留小修

### 15.1 ResourceLimit actual

Hit Test 当前 ResourceLimit 的 `actual` 固定写成 `limit+1`。

本轮顺手调整 `Budget` 错误信息，使 details 至少准确表达：

```text
resource
limit
attempted/actual
```

不要用字符串解析错误。

### 15.2 选择真值入口

GUI Selection 唯一入口：

```text
objects.hit_test
```

明确禁止使用：

```text
layer_coverage_at
Bounds
GPU pixel
renderer mesh AABB
```

作为最终对象选择判定。

---

## 16. 自动测试最低清单

新增 core/app/service/GUI-state 可自动检查的测试，至少：

```text
camera_fit_uses_f64_bounds
camera_zoom_keeps_cursor_world_point
screen_world_roundtrip_retina_scale
selection_tolerance_is_pixel_stable
selection_uses_hit_test_order_topmost_layer
selection_skips_hidden_layers
selection_allows_locked_but_move_rejected
selection_clear_on_empty_click
selection_survives_move_undo_redo
workspace_changes_do_not_mark_dirty
numeric_move_is_one_history_entry
numeric_zero_move_not_submitted
save_as_keeps_source_and_last_saved_identity
open_failure_keeps_current_document
renderer_refuses_partial_unsupported_document
```

如新增 renderer 的纯几何转换函数，应单独测试 Flash/Arc/Region/Macro 显示数据生成。

---

## 17. Mac 人工 GUI 验收

这是第一轮必须真正保留**原生交互证据**的阶段。

至少准备一份公开小样和若干已有 synthetic fixture，在 Mac 实机执行：

1. 冷启动 editor-app；
2. File → Open 一个受支持 `.gbr`；
3. Finder 拖入另一个文件；
4. Fit；
5. Trackpad Pan/Zoom；
6. 点击明显 Flash；
7. 点击 hole 中心，应不选该 Flash；
8. 点击重叠对象，确认默认选取顺序；
9. 隐藏 layer，无法再从该层点击；
10. 锁定 layer，仍可选但 Move 被拒绝；
11. 属性栏 `dx=5, dy=-3`；
12. Cmd+Z；
13. Redo；
14. Save As 新路径；
15. 重新打开输出，确认对象位置；
16. Workspace 改名/显隐不会出现 manufacturing dirty `*`；
17. 中文/空格/# 文件路径正常。

保留：

- App 版本/commit；
- macOS / GPU / Metal adapter；
- 屏幕 scale；
- 原始 GUI 日志；
- 关键截图或短录像；
- 输入/输出 SHA-256；
- 操作清单及结果。

截图不能替代自动几何测试，但本阶段也不能只靠无窗口测试冒充 GUI 已验收。

---

## 18. 性能边界

S2-A.3 不承诺十万对象流畅。

但对于公开小样/中小 Gerber：

- Pan/Zoom 不应每帧重新 Parse；
- 不应每 mouse move 做 Hit Test；
- camera 变化不修改制造 revision；
- renderer rebuild 原因可记录；
- UI 主线程不能做文件解析的大型同步阻塞，如当前 service 尚同步，应至少显示 busy 并记录后续异步化任务，不能伪称 AT-063 已通过。

Production Renderer 的大规模缓存/instancing/R-tree 留到后续性能阶段。

---

## 19. Evidence 与代码身份

继续使用 S2-A.2 已经规范化的流程：

```text
实现完成
→ commit tested code
→ git status clean
→ final gates
→ HEAD + full tracked hashes before/after
→ GUI native evidence
→ packaging/report commit（如需要）
```

公共 evidence 不包含私有 CORE10。

---

## 20. Mac final gates

至少执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-core --test s2a_hit_test
cargo test --locked -p editor-service --test s2a_hit_test_workflow
cargo test --locked -p editor-service --test s2a_bounds_workflow
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo test --locked -p editor-service --test s1b2_edit_workflow
cargo test --locked -p editor-service --test s1b2b_transform_workflow
cargo test --locked -p editor-service --test s1b2c_workspace_workflow
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
python3 scripts/source_manifest.py --check
python3 scripts/test_audit_core10.py
```

另执行本轮新增 app/camera/renderer/state 专项。

`editor-service` 依赖树仍不得出现 GUI/GPU 依赖。

---

## 21. S2-A.3 退出条件

全部满足才标记阶段通过：

- [ ] 默认 App 不再是 S0 fixed demo；
- [ ] File Open 真实可用；
- [ ] Finder drag/drop 真实可用；
- [ ] 中文/空格/# 路径实测；
- [ ] layer panel 使用 Workspace state；
- [ ] visible/locked/name 不污染 manufacturing dirty；
- [ ] f64 Camera / screen-world 转换稳定；
- [ ] document/layer Bounds 驱动 Fit；
- [ ] Pan/Zoom Mac 原生交互通过；
- [ ] renderer 不再受 4 layer/16 object S0 默认限制；
- [ ] 当前声明可编辑输入不会被 renderer 静默漏画；
- [ ] Dark/Clear/局部孔洞显示语义正确；
- [ ] click Selection 只使用 exact `objects.hit_test` 真值；
- [ ] 默认 top-layer + last exposure hit 选择规则冻结；
- [ ] selection overlay 可见；
- [ ] Properties 显示实际对象；
- [ ] 数值 Move 真实提交 service；
- [ ] Undo/Redo GUI 可用；
- [ ] Save As 真实导出并可重开；
- [ ] dirty/source_path/last_saved_path 用户可区分；
- [ ] 错误 fail-closed，不显示残缺文件后继续编辑；
- [ ] Mac GUI 人工证据齐全；
- [ ] 原无界面全部回归继续通过；
- [ ] editor-service 无 GUI/GPU 依赖；
- [ ] Windows 继续 deferred / not executed。

---

完成后停止。下一阶段再做：

```text
S2-B / Mac CAD interaction
鼠标拖动对象
多选
框选
Grid/Snap
测距
Duplicate/Delete/Rotate/Mirror GUI
```

**本轮目标是得到第一版真正“能打开、能看、能点、能数值移动、能撤销、能保存”的 Mac Gerber 编辑器。**
