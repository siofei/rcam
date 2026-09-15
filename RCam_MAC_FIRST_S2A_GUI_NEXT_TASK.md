# RCam 下一阶段 Codex 任务：Mac-first S2-A GUI 基础编辑闭环

> 基线：S1-B2c tested code commit `1b3b2a5ac4b312d1ced3b94393fd92ba73e914ac`，交付包 commit `8ac82a0...`。  
> 当前开发/阶段验收平台：macOS Apple Silicon。  
> Windows：deferred / not executed，不作为本轮门禁。  
> 本轮目标：**第一次把已经通过的制造编辑核心接入真正可用的 Mac Gerber GUI。**

## 1. 开始前必读

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/S1_B2C_REVIEW.md`
- `docs/adr/0010-s1-b2b-transform-representation.md`
- `docs/adr/0012-s1-b2c-workspace-state.md`
- 外部复审：`RCam_S1B2c_8ac82a0_Review.md`

保留全部 S1-A/B 已通过能力和回归。不要为了 GUI 方便绕过 ApplicationService、修改 SemanticDocument 字段或重新实现一套编辑逻辑。

## 2. 本轮固定闭环

S2-A 只需要完成：

```text
File Open / drag-drop
    -> real ApplicationService document
    -> layer list
    -> fit-to-window + pan/zoom
    -> exact point hit-test
    -> single selection
    -> selection highlight
    -> property panel
    -> numeric Move
    -> Undo / Redo
    -> Save As / Export
    -> Reopen / verify
```

目标是“一份受支持真实 Gerber，可以在 Mac GUI 中真正打开、看见、选中一个对象、数值移动、撤销重做并另存”。

## 3. 不要继续扩展旧 `S0App`

旧窗口作为 reference/correctness demo 保留或放到 dev/demo feature。

建立新的 S2 app shell，例如：

```text
editor-app
├─ AppSession
│  ├─ ApplicationService
│  ├─ current DocumentId
│  ├─ selected ObjectId
│  ├─ hovered ObjectId
│  ├─ camera/view state
│  └─ dialog/panel state
├─ layer panel
├─ property panel
├─ status bar
└─ canvas
```

App state 中的 selection/hover/pan/zoom 不进入 manufacturing document，也不进入 manufacturing Undo/Redo。

## 4. 文件打开

至少支持：

1. 菜单 File -> Open；
2. macOS Finder 拖放 Gerber 到窗口。

文件对话框依赖只能放在 `editor-app`/platform 层，不得进入 `editor-service` 的正常依赖闭包。新增依赖前记录版本、用途、许可证和替代方案。

打开必须调用真实 `ApplicationService::open` / 对应 JSON 服务入口，不再使用 `include_bytes!` 作为主界面数据源。

错误时显示结构化诊断，不能用部分画面冒充成功打开。

本轮 GUI 可只维护一个 current document；多文档/工程工作区以后再做。

## 5. 首先补齐 S2 查询能力

### 5.1 Bounds

在 core/service 增加并测试：

```text
layer.bounds
document.bounds
```

返回 f64 mm：

```json
{"min_x_mm":...,"min_y_mm":...,"max_x_mm":...,"max_y_mm":...}
```

要求：

- Flash 尊重 LocalTransform、孔洞不影响外包围盒；
- Line/RectangularSweep 使用真实宽度；
- Arc bounds 必须包含 sweep 实际经过的极值角，不得简单用整圆 AABB；
- Region 聚合真实 edge bounds；
- 空层/空文档返回 null；
- manufacturing bounds 不随 visible/locked/selection 改变。

GUI 的 fit-to-window 使用此 f64 bounds，不扫描 GPU mesh 反推。

### 5.2 `objects.hit_test`

实现最小、完整的 point hit-test：

输入：

```text
layer_id
point_mm {x_mm,y_mm}
tolerance_mm
```

输出：

```text
revision
stable object IDs
```

结果顺序固定为该层原 object/exposure order，便于以后实现重叠循环选择。

精确规则：

- Circle/Rectangle/Obround/Polygon/Macro Flash：尊重 Aperture shape + LocalTransform + hole；
- Line：真实扫掠宽度；
- RectangularSweep：真实矩形扫掠；
- Arc：真实中心、半径、方向、sweep 和宽度；
- Region：真实 contour/edge/fill 语义；
- Clear 对象也可以作为“对象几何命中”返回；最终可见像素点选以后另设语义，不混用。

`tolerance_mm` 是选择容差，不得改制造几何。

空间索引本轮不是必须；如果使用，只能筛候选，最终必须做精确 f64 几何判断。

## 6. GUI Layout

第一版保持简单：

```text
┌────────────────────────────────────────┐
│ File  Edit  View                       │
├──────────┬─────────────────┬───────────┤
│ Layers   │                 │ Properties│
│ visible  │  Gerber Canvas  │ Object ID │
│ locked   │                 │ Type      │
│ name     │                 │ Position  │
├──────────┴─────────────────┴───────────┤
│ x/y mm | zoom | revision | dirty       │
└────────────────────────────────────────┘
```

本轮不追求视觉装修，优先正确性、稳定 ID 和状态同步。

## 7. Layer panel

显示 `layers.list` 的：

- display_name；
- visible；
- locked；
- object_count。

修改名称/显隐/锁定必须调用 `layer.update`，并携带最新 manufacturing revision + workspace_revision。

隐藏层：

- 不绘制；
- 默认不参加 GUI hit-test；
- 不改变 manufacturing dirty。

锁定层：

- 仍可显示和选择；
- 数值 Move 提交时由 service 返回 `LAYER_LOCKED`；GUI 不伪造本地成功。

## 8. Camera / Fit / Pan / Zoom

制造坐标始终 f64 mm。

Camera/view state 可用 f64 保持世界坐标，再在 GPU 附近采用 floating origin/f32 局部坐标；不能把 f32 renderer 值写回制造模型。

最低要求：

- 打开文件自动 fit-to-window；
- 滚轮/触控板平滑 zoom；
- zoom 以鼠标位置为中心；
- 中键/约定手势 pan；
- resize 后坐标转换正确；
- 状态栏显示当前鼠标 manufacturing X/Y mm。

## 9. Renderer：本轮做“正确的小型语义 Renderer”，不做最终性能优化

旧 S0 16-object per-pixel shader 不作为真实 GUI 主 Renderer。

建议新增独立 `editor-render` 模块/crate，依赖 `editor-core` + egui-wgpu/wgpu；`editor-service` 仍不得依赖 GPU。

本轮 Renderer 必须能正确显示当前 Editable 子集：

- Flash（C/R/O/P 及当前已验证 Macro 表达）；
- Line；
- RectangularSweep；
- Arc；
- Region；
- Dark / Clear 有序曝光；
- layer visible；
- selected object overlay。

可以使用 CPU tessellation + GPU buffers/cache；**不要**为了性能重排 Dark/Clear 对象。

暂不要求百万对象性能、R-tree、indirect draw、compute shader。先保证与 f64 semantic model 的小样结果一致。

Renderer 是只读消费者：

```text
Semantic snapshot/change set -> renderer
```

不得从 GPU mesh 反推制造几何。

## 10. Single selection

只实现单选。

流程：

```text
screen point
-> camera inverse transform
-> manufacturing point mm
-> service/core hit_test
-> stable ObjectId
-> app.selected_object_id
-> renderer highlight
```

点击空白清除选择。

多个对象重叠时，本轮默认选择返回列表中的最后一个/最上层对象，并把完整命中列表保留给后续循环选择；选择规则写 ADR 和测试，不能依赖 HashMap 顺序。

对象被 Delete/Undo/Close 后，GUI 必须重新校验 selection；不存在的 ID 自动清除。

## 11. Properties + Numeric Move

属性栏至少显示：

- ObjectId；
- geometry type；
- exposure；
- 基础坐标/中心；
- 当前 layer；
- origin（Imported/Generated，可只读）。

第一版编辑仅提供数值位移：

```text
dx_mm
dy_mm
Apply
```

调用现有 `objects.move`。成功后：

- 更新 manufacturing revision；
- selection 保持同一 ObjectId；
- renderer 刷新；
- dirty 正确更新。

失败（locked、revision conflict、invalid value）不得先本地移动画面再回滚。

## 12. Undo / Redo

菜单/快捷键接入现有：

```text
history.undo
history.redo
```

macOS：Cmd+Z / Cmd+Shift+Z。

成功后刷新 DocumentInfo、selected object、renderer snapshot。

当前 lock 不阻止 Undo/Redo 的契约必须继续保持。

文本输入框获得焦点时，不得因为 Cmd+Z 等快捷键误操作画布/制造历史；按现有验收 AT-043 设计焦点路由。

## 13. Save As / Export

File -> Save As 使用现有安全导出服务：

- 默认新路径；
- GUI 负责选择目标；
- overwrite/metadata confirmation 必须显式处理；
- 成功后展示 `last_saved_path`；
- source_path 与 last_saved_path 不混淆；
- 原文件不被导入/编辑过程修改。

本轮 Save As 后至少用服务重新打开一次输出文件进行回归测试；GUI 不必自动打开副本。

## 14. GUI 状态错误处理

所有 service error 使用统一 code/details 映射到 GUI：

- INVALID_ARGUMENT
- NOT_FOUND
- LAYER_LOCKED
- REVISION_CONFLICT
- UNSUPPORTED_FEATURE
- RESOURCE_LIMIT
- CONFIRMATION_REQUIRED
- FILE_CONFLICT
- IO_ERROR
- VALIDATION_FAILED

不要让 UI 根据中文/英文 message 字符串判断逻辑。

## 15. 本轮必须新增测试

至少新增 headless/core/service：

```text
layer_bounds_flash_line_arc_region
arc_bounds_only_include_sweep_extrema
empty_layer_bounds_is_null
workspace_visibility_does_not_change_manufacturing_bounds
hit_test_flash_respects_hole_and_local_transform
hit_test_line_uses_width_and_tolerance
hit_test_arc_respects_direction_and_sweep
hit_test_region_inside_outside_and_hole
hit_test_clear_object_returns_object_geometry
hit_test_order_is_stable
hit_test_after_move_uses_new_geometry
hit_test_after_undo_redo_tracks_geometry
```

GUI/app 层至少自动测试纯状态/坐标转换：

```text
screen_world_roundtrip
zoom_about_cursor_preserves_world_point
fit_to_window_contains_bounds
selection_clears_when_object_disappears
workspace_update_does_not_set_dirty_in_app
service_revision_conflict_does_not_apply_local_move
```

另做 Mac 原生人工/录像证据：

```text
Open real/synthetic supported Gerber
-> layer visible toggle
-> lock toggle
-> pan/zoom/fit
-> click select
-> numeric move
-> undo
-> redo
-> save as
-> reopen output
```

## 16. 本轮明确不做

- Windows；
- 多选/框选；
- 鼠标直接拖动物体；
- Grid/Snap；
- 测距；
- 中英文矢量文字；
- 自动拼板；
- Production Renderer 百万对象优化；
- CORE10 新兼容；
- Python/Lua/JS 引擎；
- HTTP/RPC。

## 17. Mac final gates

继续执行并保留原始日志：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo test --locked -p editor-service --test s1b2_edit_workflow
cargo test --locked -p editor-service --test s1b2b_transform_workflow
cargo test --locked -p editor-service --test s1b2c_workspace_workflow
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
python3 scripts/source_manifest.py --check
```

并新增 S2-A bounds/hit-test/app-state 专项测试入口。

必须启动 release `editor-app` 在 Apple Silicon Mac 上实际完成 GUI 闭环；保存环境、Metal adapter/backend、窗口截图/短录像、输入/输出 Gerber SHA-256、请求日志和 release 二进制 SHA-256。

## 18. S2-A 退出条件

全部满足才进入鼠标拖动/多选阶段：

- [ ] 主 GUI 打开真实路径，不再依赖固定 include sample；
- [ ] Finder drag-drop 可打开；
- [ ] layer list visible/lock/name 正确；
- [ ] f64 layer/document bounds 正确；
- [ ] point hit-test 覆盖当前核心几何；
- [ ] 单选、高亮稳定；
- [ ] pan/zoom/fit 和坐标转换正确；
- [ ] properties 显示真实对象；
- [ ] 数值 Move 走 ApplicationService；
- [ ] Undo/Redo GUI 可用且 lock 契约不回归；
- [ ] Save As 走安全 writer，source 不损坏；
- [ ] Export/Reopen 几何检查通过；
- [ ] editor-service 正常依赖仍无 egui/wgpu/winit；
- [ ] Mac release GUI 实机闭环有证据；
- [ ] Windows 保持 deferred / not executed。

完成后停止，不自动扩张到 S2-B。
