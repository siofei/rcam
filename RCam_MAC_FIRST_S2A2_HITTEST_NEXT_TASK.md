# RCam 下一阶段 Codex 任务：Mac-first S2-A.2 精确 Hit Test

> 基线：S2-A.1 Bounds 已阶段性通过。  
> 当前平台：macOS Apple Silicon。  
> Windows：deferred / not executed，不作为本轮门禁。  
> 本轮目标：**完成从 f64 制造坐标点到稳定 ObjectId 列表的精确对象命中服务，为下一轮真实 Mac GUI 单选建立唯一几何真值。**

## 1. 开始前必读

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/S1_B2C_REVIEW.md`
- `docs/S2_A1_REVIEW.md`
- `docs/adr/0010-s1-b2b-transform-representation.md`
- `docs/adr/0012-s1-b2c-workspace-state.md`
- `docs/adr/0013-s2a-bounds.md`
- 外部复审：`RCam_S2A1_20260916_Review.md`

保留现有 Parser/Semantic/Writer/Edit/Workspace/Bounds 回归。不得为了点选方便直接用 renderer pixel、GPU mesh、AABB 或粗 bounds 冒充最终几何结果。

## 2. 本轮固定范围

只完成：

```text
point_mm + layer_id + tolerance_mm
        -> f64 precise geometry hit-test
        -> stable ordered ObjectId list
        -> editor-service / JSON contract
        -> edit/history regression
```

本轮**不完成完整 GUI**。

可以补纯算法辅助模块，但不要扩张到：

- File Dialog；
- Finder drag-drop；
- 新 renderer；
- layer panel；
- selection overlay；
- 鼠标拖动物体；
- 多选/框选；
- Grid/Snap；
- 文字；
- Windows。

这些放到 S2-A.3。

## 3. 新增服务操作

建议：

```text
objects.hit_test
```

请求：

```json
{
  "layer_id": "layer-...",
  "point": {
    "x_mm": 12.34,
    "y_mm": 56.78
  },
  "tolerance_mm": 0.05
}
```

结果：

```json
{
  "document_id": "doc-...",
  "revision": "12",
  "layer_id": "layer-...",
  "object_ids": ["obj-a", "obj-b"]
}
```

要求：

- read-only；
- 不接受 `expected_revision`；
- 不改变 manufacturing/workspace revision；
- 不改变 dirty/history；
- unknown document/layer 使用结构化 `NOT_FOUND`；
- NaN/Inf/负 tolerance/超范围坐标明确 `INVALID_ARGUMENT`；
- 未知字段严格拒绝；
- `system.capabilities` 只有真实实现后才报告 `objects.hit_test`。

## 4. 命中语义必须先冻结

### 4.1 这是“对象几何命中”，不是最终可见像素

每个 SemanticObject 独立判断其几何。

因此：

- Dark 对象可以命中；
- Clear 对象也可以命中；
- 后来的 Clear 是否擦掉前面的 Dark，不改变前一个对象自身是否在该点有几何；
- GUI 如果以后需要“最终可见对象”语义，另设独立 API/策略，不与本轮混淆。

### 4.2 Workspace 不是隐式输入

`visible/locked/display_name` 不进入 core/service manufacturing hit-test。

以后 GUI：

- hidden layer：不调用该层 hit-test；
- locked layer：可以显示/选择，但编辑提交由 service 拒绝。

不要把 workspace 状态偷偷读入 `objects.hit_test`。

### 4.3 顺序

结果必须保持当前 `layer.objects` / exposure order。

禁止依赖：

```text
HashMap iteration
R-tree traversal order
GPU order
```

空间索引以后只能筛候选，最终结果按稳定文档顺序输出。

## 5. tolerance 语义

`tolerance_mm >= 0`。

建议固定为：

> 在保持对象原始填充/孔洞拓扑的前提下，允许距对象材料边界 `<= tolerance_mm` 的点作为选择命中。

必须写测试明确边界行为，而不是把所有尺寸直接随意增大后复用制造验证。

特别注意：

- 大孔中心在 tolerance 明显小于到孔边界距离时必须仍 MISS；
- 接近 hole 边缘可按 tolerance HIT；
- tolerance 不能改变 document geometry；
- tolerance 不参与 Writer。

如果实现中采用“距离到几何”的算法，优先保持这一语义。

## 6. Flash 命中

必须覆盖：

```text
Circle
Rectangle
Obround
Polygon
Macro
```

### 6.1 LocalTransform

使用当前同一套：

```text
mirror
rotation
scale
```

优先将 world point 通过 inverse LocalTransform 转回 aperture local space，再做 aperture 精确判断。

不要从 transformed AABB 判断最终 HIT。

### 6.2 Hole

Circle/Rectangle/Obround/Polygon 已支持的 hole 必须扣除。

至少测试：

```text
solid interior      HIT
outside             MISS
hole center         MISS
hole boundary ± tol
rotated/mirrored/scaled flash
```

### 6.3 Macro

不能复用 S2-A.1 的 conservative dark envelope 当最终命中。

必须按当前已支持 Macro primitives 顺序/曝光语义计算对象局部 material：

```text
primitive 1  Circle
primitive 4  Outline
primitive 21 CenterLine
```

尊重 primitive rotation、offset、Dark/Clear。

若存在尚不能证明的 Macro 组合，fail-closed，而不是 bounds HIT。

## 7. Line

当前圆形 aperture stroke 语义：

```text
capsule(start, end, width/2)
```

点到中心线段的最短距离：

```text
<= width/2 + tolerance
```

零长度 stroke 按圆形 dot 处理。

保持此前 CPU/GPU 退化边界测试，不重新引入固定长度平方 clamp。

## 8. RectangularSweep

使用当前模型的真实轴向矩形 Minkowski sweep，不得仅检查：

```text
expanded AABB
```

因为以后斜向 start/end 的 expanded AABB 会包含并非实际材料的角落。

实现精确 point-in-sweep / distance-to-sweep 判断，并覆盖：

```text
horizontal
vertical
diagonal
zero-length
90° rotate 后 width/height 交换
mirror
```

## 9. Arc

必须完全复用 S1-A.1 的圆弧制造语义：

- declared start/end/center；
- CW / CCW；
- full circle；
- zero sweep；
- average-radius interpretation；
- non-zero arc deviation；
- endpoint radial joins；
- round end caps / stroke width；
- mirror 后方向翻转。

禁止重新写一个“严格等半径圆”的简化 hit-test。

至少测试：

```text
point on valid sweep         HIT
same radius outside sweep    MISS
inside stroke band           HIT
outside stroke band          MISS
start/end cap
full circle
zero sweep dot
non-zero deviation
CW/CCW
move/rotate/mirror 后
```

## 10. Region

使用现有 canonical contour / topology 语义。

至少支持：

- solid contour inside/outside；
- hole；
- cut-in；
- line edge；
- arc edge；
- boundary tolerance。

不要把 Region AABB/bounds 作为最终结果。

对于现有 fail-closed Region 情况继续 fail-closed；不能为了 hit-test 放松制造输入合法性。

## 11. 编辑与历史联动测试

Hit Test 必须读取当前 revision 的真实几何，因此至少验证：

```text
Open
 -> hit old position
 -> Move
 -> old position MISS
 -> new position HIT
 -> Undo
 -> old position HIT
 -> Redo
 -> new position HIT
```

还要覆盖：

```text
Duplicate -> 原/新对象都可命中且顺序稳定
Delete -> deleted ID 消失
Undo Delete -> 同一 ObjectId 恢复
Rotate -> 新几何位置命中
Mirror -> 新几何位置命中
```

不允许维护一个与 SemanticDocument 脱节的第二份选择几何缓存。

## 12. 最低测试清单

至少新增 core/service：

```text
hit_test_flash_circle_hole
hit_test_rectangle_obround_polygon_local_transform
hit_test_macro_dark_clear_primitives
hit_test_line_width_and_tolerance
hit_test_zero_length_line
hit_test_rectangular_sweep_exact_not_aabb
hit_test_arc_direction_and_sweep
hit_test_arc_full_zero_and_deviation
hit_test_region_inside_outside_hole_cutin
hit_test_clear_object_returns_object_geometry
hit_test_order_is_exposure_order
hit_test_invalid_params_are_atomic
hit_test_after_move_undo_redo
hit_test_after_duplicate_delete
hit_test_after_rotate_mirror
```

建议加真实 parser fixtures，不只手工构造 SemanticGeometry。

## 13. 性能边界

S2-A.2 可以线性扫描：

```text
O(N)
```

暂时不要引入 R-tree/复杂缓存。

但测试/代码结构要保证未来可以：

```text
Bounds / R-tree candidate filter
        -> exact f64 hit-test
```

并保持最终顺序稳定。

不要现在宣传十万/百万对象实时点选性能。

## 14. Evidence 与代码身份

上一轮是 uncommitted working tree + exact hashes，可复核但说明成本高。

本轮建议：

```text
1. 完成实现和测试
2. commit tested code
3. git status clean
4. 执行 final gates
5. 保存 HEAD / source hashes / raw logs
6. 如需补文档包，再做 packaging-only commit
```

私有 CORE10 不进入 ZIP。

## 15. Mac final gates

执行并保留原始日志：

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

当前仍不要求 Windows。

## 16. S2-A.2 退出条件

全部满足才进入 S2-A.3 GUI：

- [ ] `objects.hit_test` 真实实现并进入 capabilities；
- [ ] API/DTO/错误契约固定；
- [ ] f64 mm + tolerance 语义固定；
- [ ] C/R/O/P/Macro Flash + hole + LocalTransform 精确；
- [ ] Line 精确；
- [ ] RectangularSweep 精确，不用 expanded AABB 冒充；
- [ ] Arc 遵循 S1-A.1 全部语义；
- [ ] Region inside/outside/hole/cut-in 精确；
- [ ] Clear 独立对象几何可返回；
- [ ] 结果顺序稳定；
- [ ] Move/Duplicate/Delete/Rotate/Mirror/Undo/Redo 后结果一致；
- [ ] Workspace state 不隐式改变 service hit-test；
- [ ] 不从 GPU/f32 反推；
- [ ] 原有回归全部通过；
- [ ] editor-service 仍无 GUI/GPU 正常依赖；
- [ ] Mac evidence 完整且绑定 clean tested commit；
- [ ] Windows 保持 deferred / not executed。

完成后停止，不自动扩张到完整 GUI。

---

**本轮目标只有一个：为 S2-A.3 GUI 提供可信、稳定、可自动化复用的制造几何点选真值。**
