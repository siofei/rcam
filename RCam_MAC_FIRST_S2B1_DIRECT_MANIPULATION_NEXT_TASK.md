# RCam 下一阶段 Codex 任务：Mac-first S2-B1 单对象直接操作闭环

> 基线：S2-A.3 `025e3a3` Mac GUI 基础闭环已阶段性通过。  
> 当前平台：macOS Apple Silicon。  
> Windows：deferred / not executed，不作为本轮门禁；最终 V1 双平台门槛不删除。  
> 本轮目标：**在现有单选 GUI 上增加直接拖动物体、原位复制和删除，使日常 Gerber 编辑从“数值操作”进入直接 CAD 操作，同时完成 VectorScene 长期架构文档的正式合并。**

---

## 1. 开始前必读

先阅读：

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/S2_A2_REVIEW.md`
- `docs/S2_A3_REVIEW.md`
- `docs/adr/0014-s2a2-hit-test.md`
- `docs/adr/0016-s2a3-gui.md`
- 本次外部复审 `RCam_S2A3_025e3a3_Review.md`

保留现有 Parser、Semantic、Writer、Hit Test、Workspace 和 service 契约。不要为了拖拽在 UI 内直接修改 `SemanticDocument`。

---

## 2. 先完成文档收尾：VectorScene ADR 改为 0017

当前仓库已有：

```text
docs/adr/0016-s2a3-gui.md
```

所以之前准备的 VectorScene ADR **不得再使用 0016**。

本轮新增：

```text
docs/adr/0017-vector-scene-and-format-interchange.md
```

同步更新：

```text
AGENTS.md
docs/DESIGN_V1.md
docs/IMPLEMENTATION_PLAN.md
```

固定长期架构：

```text
Gerber
  ↓
gerber-parser / gerber-io
  ↓
Manufacturing Model (SemanticDocument)

DXF ─┐
SVG ─┼→ VectorScene
PLT ─┘       ↓
      Manufacturing Conversion
              ↓
      Manufacturing Model
```

并冻结：

- `VectorScene` 是通用矢量交换模型，不替代 Gerber Manufacturing Model；
- Gerber 导入不强制经过 VectorScene；
- DXF/SVG/HPGL/PLT 等未来统一先进入 VectorScene；
- 禁止 GPU Mesh / pixel / display tessellation 反向生成 Gerber、DXF、SVG、PDF；
- Bezier/Spline 等 Gerber 无原生表达的曲线，未来必须经过显式、可验证的制造转换；
- 本轮**不实现** DXF/SVG/PLT 导入导出；
- 不修改 `ACCEPTANCE_V1.md` 和 `acceptance_cases.json` 的 96 个 V1 用例；
- 不把计划能力写进 `CAPABILITIES.md` 作为已支持。

同时修正阶段文档漂移：

- `IMPLEMENTATION_PLAN.md` 不再把 S1-A 写成当前任务；
- 当前活动阶段改为 S2-B1；
- `AGENTS.md` / `DESIGN_V1.md` 明确“当前开发 Mac-first、Windows 延后”，但保留最终双平台 V1 要求。

修改完成后重新生成 Source/Package Manifest。

---

## 3. 本轮固定范围

### 必须完成

```text
单对象直接拖动
拖动预览
Esc / capture loss 取消
release 一次 Move transaction
Undo / Redo
原位 Duplicate
Delete
对应 GUI 菜单/按钮/快捷键
Mac 原生交互证据
```

### 本轮明确不做

```text
多选
框选
跨层 selection set
Grid / Snap
测距
Rotate / Mirror GUI
文字
Panelize
R-tree / production renderer
Windows
```

不要把 S2-B 全部功能一次堆进这一轮。

---

## 4. 单对象拖动模型

当前 GUI 仍保持：

```text
selected: Option<ObjectId>
```

S2-B1 **不要**先改成多选结构。

新增 UI-only drag state，例如：

```text
DragState
├─ object_id
├─ layer_id
├─ start_world_mm
├─ current_world_mm
├─ preview_dx_mm
├─ preview_dy_mm
└─ armed / dragging
```

这只是 Workspace/UI 临时状态，不能进入 Manufacturing Document，也不能形成 dirty / revision / Undo。

---

## 5. 拖动启动规则

第一版规则冻结为：

1. 普通单击仍使用现有 exact `objects.hit_test`；
2. **只有已经 selected 的对象**才能在当前手势中进入 drag；
3. 用户在 selected object 上按下并超过固定 physical pixel drag threshold 后进入预览；
4. 在未选对象上按下，只执行现有选择逻辑，不要求同一手势完成“异步选择 + 拖动”；
5. locked layer 不进入 drag；
6. busy / blocked / display_error 时不能进入制造 drag。

这样避免为了“按住未选对象立即拖”在 UI 线程重新实现同步 hit-test。

drag threshold 使用 physical pixel 冻结，例如 3–5 px，并有 Retina 自动测试。

---

## 6. Drag Preview：不得每帧写 Manufacturing Model

错误实现：

```text
pointer move
→ objects.move
→ revision+1
→ pointer move
→ objects.move
→ revision+1
...
```

禁止。

正确实现：

```text
pointer down
→ local preview state

pointer move
→ 只改变 selected object 的显示 offset
→ manufacturing revision 不变
→ dirty 不变
→ Undo 不变

pointer release
→ 计算最终 f64 dx/dy
→ 一次 objects.move
→ 一个 Undo entry
```

拖动 preview 不能通过修改 SemanticObject 或临时 Writer 输出实现。

建议 renderer 给 selected object 增加 preview translation uniform/overlay；不要为了预览重建整份制造 Scene。

---

## 7. 拖动提交和取消

### Release

release 时：

```text
abs(dx) / abs(dy) 有真实变化
→ objects.move
```

要求：

- 只提交一次；
- 使用 f64 mm；
- 事务完成后清空 preview；
- selection 保留同一 ObjectId；
- renderer 使用最新 snapshot；
- Undo entry +1；
- revision +1。

### 零位移

如果最终 manufacturing dx/dy 均为 0：

```text
不调用 service
不产生 Undo
不改变 revision
```

### Cancel

以下情况全部取消 preview，不修改文档：

```text
Esc
pointer capture lost
窗口失焦（无法证明 release）
当前文档切换/关闭
service 进入 busy 前的异常中止
```

如果 release 提交后 service 返回错误，必须恢复正常 renderer、清除 preview，并保留原始制造数据。

---

## 8. Duplicate GUI

已有 `objects.duplicate` 真值继续复用，不重新实现复制算法。

本轮 GUI 采用简单、明确的第一版语义：

```text
Cmd+D / 菜单 Duplicate
→ 当前 selected object 原位复制 (dx=0, dy=0)
→ service 返回新 ObjectId
→ GUI 自动选中新副本
→ message 提示“已原位复制，可拖动副本”
```

这与现有 S1-B2a 已测试的原位 duplicate 语义一致。

要求：

- locked layer 禁止 Duplicate；
- 一次 Duplicate 一个 Undo entry；
- Undo 删除副本；Redo 恢复相同 ID；
- source object 不变；
- 曝光顺序继续使用 service/current layer order 真值；
- 不通过复制 renderer primitive 生成制造对象。

---

## 9. Delete GUI

提供：

```text
Delete / Backspace（按 macOS 最终键位冻结）
菜单“删除对象”
```

调用已有：

```text
objects.delete
```

要求：

- 文本输入框有焦点时，Delete 只能编辑文本，不能删 Gerber 对象；
- locked layer 禁止制造 Delete；
- 删除成功后 selection 清空；
- Undo 恢复对象和原曝光位置；
- Redo 再删除；
- Delete 失败不清 selection。

---

## 10. 文本焦点与快捷键

继续沿用 S2-A.3 原则：

```text
ctx.wants_keyboard_input() == true
```

时制造快捷键不得抢占文本编辑。

至少测试：

- 在 ΔX / ΔY 输入框里 Cmd+Z 不执行制造 Undo；
- Delete 不删除对象；
- Cmd+D 不 Duplicate；
- Esc 如用于退出文本编辑，不能意外提交 drag。

---

## 11. 本轮不要实现框选：先记录精确语义缺口

当前 `objects.query` 的：

```text
relation = contains
```

已经冻结为：

> **对象包含查询矩形**

它不是 CAD 左→右窗口框选需要的：

> **查询矩形完整包含对象**

并且当前 rectangle relation 对 Arc / Region、部分非圆 Flash、斜 RectangularSweep 仍可能 fail-closed / unsupported。

所以本轮明确禁止：

- 用现有 `contains` 反向当 window select；
- 用 Bounds/AABB 冒充最终框选；
- 遇到 Arc/Region 就跳过并返回部分结果。

在 `IMPLEMENTATION_PLAN.md` 为 S2-B2 登记：

```text
新增精确 within/window relation（或专用 selection query）
+ exact intersects
→ multi-select
→ 左→右 contains-window
→ 右→左 crossing-intersects
```

---

## 12. 本轮性能约束

不要提前做 production renderer，但保持：

- drag pointer move 不调用 parser；
- drag pointer move 不调用 service hit_test；
- drag pointer move 不重建制造 document；
- preview 尽量不完整 `Scene::build`；
- release 才触发一次 snapshot/rebuild；
- camera 操作不改变制造 revision。

当前 `gpu::uniforms()` 的每帧 O(N) 预算扫描可以继续记录为后续性能债，不要通过删除 fail-closed 预算换取帧率。

Workspace display-name/lock 若能低风险避免完整 scene rebuild，可以优化，但不是本轮必须改变制造架构的理由。

---

## 13. 自动测试最低清单

新增至少：

```text
drag_preview_does_not_change_revision_dirty_or_history
drag_release_commits_exactly_one_move_transaction
drag_zero_delta_is_noop
drag_escape_cancels_without_side_effects
drag_capture_loss_cancels_without_side_effects
locked_layer_cannot_drag
service_failure_clears_preview_and_preserves_geometry
undo_redo_after_drag_preserves_selection
retina_drag_threshold_is_physical_pixel_stable
cmd_d_duplicates_in_place_and_selects_new_object
duplicate_undo_redo_preserves_generated_id
delete_selected_clears_selection_and_undo_restores
text_focus_blocks_manufacturing_delete_duplicate_undo
```

现有全部 S0/S1/S2-A 回归继续通过。

---

## 14. Mac 原生人工验收

至少执行：

1. 打开 `gui_primitives.gbr`；
2. 选中一个明显 Flash；
3. 鼠标/触控板直接拖动该 selected Flash；
4. 拖动期间位置预览跟手，revision/dirty 不应每帧变化；
5. release 后 revision 只增加一次；
6. Cmd+Z 恢复；
7. Redo 恢复拖动后位置；
8. Esc 取消一次拖动；
9. 锁定图层后尝试拖动，制造几何不变；
10. Cmd+D 原位复制，selection 切换到新 ID；
11. 拖动副本离开原对象；
12. Undo/Redo 验证复制+拖动历史；
13. Delete 删除副本；Undo 恢复；
14. 在文本输入框按 Delete/Cmd+D/Cmd+Z，不触发制造操作；
15. Save As → Reopen，最终对象位置和数量一致。

保留：

- tested commit；
- git clean；
- Mac/GPU/Metal；
- 原始 native log；
- 截图或短录像；
- 输入/输出 SHA-256；
- final revision / undo / redo 状态。

---

## 15. Final gates

至少继续执行 S2-A.3 全部门禁，并新增本轮 app/service 专项：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo test --locked -p editor-service --test s1b2_edit_workflow
cargo test --locked -p editor-service --test s1b2b_transform_workflow
cargo test --locked -p editor-service --test s1b2c_workspace_workflow
cargo test --locked -p editor-service --test s2a_hit_test_workflow
cargo test --locked -p editor-service --test s2a_bounds_workflow
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
python3 scripts/source_manifest.py --check
python3 scripts/test_audit_core10.py
```

另执行本轮 direct manipulation / native Metal 专项。

`editor-service` 正常依赖树继续不得出现 GUI/GPU 依赖。

---

## 16. S2-B1 退出条件

全部满足才可以标记本阶段通过：

- [ ] VectorScene ADR 以 **0017** 正式加入，所有引用无 0015/0016 冲突；
- [ ] `IMPLEMENTATION_PLAN.md` 当前阶段不再停留在 S1-A；
- [ ] 当前 Mac-first、Windows deferred 策略文档一致；
- [ ] selected object 可直接鼠标拖动；
- [ ] preview 不修改 Manufacturing Model；
- [ ] release 只产生一个 Move transaction；
- [ ] Esc / capture loss 可无副作用取消；
- [ ] locked layer 不可拖动；
- [ ] Cmd+D 原位 Duplicate 并选中新 ID；
- [ ] Delete GUI 调用真实 service；
- [ ] Duplicate/Delete 均有 Undo/Redo；
- [ ] 文本焦点不误触制造快捷键；
- [ ] Save As / Reopen 保持最终几何；
- [ ] 原有 S2-A3 Open/Layer/Camera/Hit Test/Save 回归全部通过；
- [ ] Mac 原生直接操作证据齐全；
- [ ] Windows 继续 deferred / not executed。

完成后停止。

下一阶段 S2-B2 再处理：

```text
exact within/window rectangle query
multi-select
左→右窗口框选 / 右→左交叉框选
Rotate/Mirror GUI
Grid / Snap
测距
```
