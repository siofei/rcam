# RCam 下一阶段 Codex 任务：Mac-first S1-B2b Rotate / Mirror

> 基线：S1-B2a `256dbc9`。  
> 当前平台：macOS Apple Silicon。Windows 继续 deferred / not executed。  
> 本轮只完成可验证的 Rotate/Mirror 制造几何编辑，不进入完整 GUI、文字或 Production Renderer。

## 1. 先读

开始前阅读：

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/S1_B2_REVIEW.md`
- `docs/adr/0009-s1-b2a-object-transactions.md`
- `docs/adr/0010-s1-b2b-transform-representation.md`
- 本轮外部复审 `RCam_S1B2a_256dbc9_Review.md`

保留 S1-B2a 所有通过能力和测试。

## 2. 本轮目标

实现真实服务操作：

```text
objects.rotate
objects.mirror
```

形成：

```text
Open
 → Query
 → Rotate / Mirror
 → Undo
 → Redo
 → Validate
 → Export
 → Reopen
 → independent geometry assertions
```

所有操作必须是：

- 明确 object_ids；
- 单一 layer；
- expected_revision；
- 一次请求一个原子 Undo transaction；
- 失败整体不修改；
- Undo 使用存储的 before/after manufacturing geometry，禁止靠逆变换恢复。

## 3. API 契约

### 3.1 Rotate

建议 DTO：

```json
{
  "layer_id": "layer-1",
  "object_ids": ["object-1"],
  "angle_deg": 37.0,
  "pivot_mm": {"x_mm": 10.0, "y_mm": 20.0}
}
```

要求：

- `angle_deg` / pivot 全部必须 finite；
- 角度规范化，但 0°、±360° 等纯 no-op 不得制造历史；
- 如果全部对象最终状态完全不变，也不得推进 revision / history；
- 旋转结果超坐标范围或数值不可靠，整体拒绝。

### 3.2 Mirror

Mac V1 第一版固定支持水平／垂直世界轴：

```json
{
  "layer_id": "layer-1",
  "object_ids": ["object-1"],
  "axis": {
    "kind": "horizontal",
    "coordinate_mm": 0.0
  }
}
```

或：

```text
vertical: x = coordinate_mm
horizontal: y = coordinate_mm
```

不在本轮开放任意斜轴镜像。

## 4. 几何实现要求

### 4.1 Flash

必须同时处理：

```text
center
LocalTransform.mirror
LocalTransform.rotation_deg
LocalTransform.scale
```

不能只移动 Flash 中心。

对 R/O/P/AM 等非圆 aperture，旋转和镜像必须真正改变 aperture orientation。

实现时使用明确的 2×2 正交变换组合，再确定性分解回 `Mirror + rotation_deg`；必须按当前 `apply_inverse_transform()` 和 Gerber LM/LR/LS 语义验证组合顺序，不能凭直觉相加角度。

测试至少覆盖：

- Rectangle Flash 37°；
- Obround；
- Polygon；
- Macro primitive / hole；
- 已存在 mirror + rotation 的 Flash 再旋转／镜像。

### 4.2 Circular Line

变换 start/end；width 不变。

任意有限角度 rotate；horizontal / vertical mirror。

### 4.3 RectangularSweep

当前结构没有 aperture orientation。

因此：

- Rotate 仅允许精确表示的 90° 整数倍；
- 奇数 quarter-turn 必须交换 width/height；
- 对 quarter-turn 使用精确正交坐标公式，不使用普通 sin/cos 产生 `6e-17` 类漂移；
- Mirror 仅 horizontal / vertical；width/height 不变；
- 选择集中只要包含 RectangularSweep，而 angle 不是允许值，整个请求返回 `UNSUPPORTED_FEATURE`（或冻结的专用错误）且零修改。

不得只旋转 start/end 后继续宣称 aperture 仍正确。

### 4.4 Arc

Rotate：

```text
start
end
center
```

全部变换；CW/CCW 不变。

Mirror：上述三点全部变换，同时：

```text
CW ↔ CCW
```

必须保持：

```text
full_circle
zero_sweep
source.resolution_mm
source.single_quadrant
arc deviation 语义
```

不能重新拟合圆心，也不能用 tessellation 结果反推制造圆弧。

### 4.5 Region

全部 contour / edge 一次整体变换。

- Line edge：两端点变换；
- Arc edge：点变换 + mirror 方向翻转；
- edge order 不变；
- contour role / topology 不变；
- 变换完成后重新做 Region semantic validation。

任何一个 edge 失败，整个对象集合不提交。

## 5. 原子事务与 History

复用 `Operation::Modify`，不要为 Rotate/Mirror 制造伪 Insert/Delete。

要求：

- 一次批量 Rotate = 1 Undo entry；
- 一次批量 Mirror = 1 Undo entry；
- Undo 精确恢复 bit-for-bit 存储状态（允许序列化外部格式量化另行比较）；
- Redo 恢复完全相同 after state；
- 新成功编辑继续清空 Redo；
- 失败不清 Redo；
- locked layer、未知 ID、重复 ID、空 ID、stale revision、资源超限全部原子拒绝。

## 6. Writer / Reopen

Rotate/Mirror 后必须通过：

```text
document.validate
Gerber writer
重新 parse
制造几何独立断言
```

重点检查：

- Rectangle/Obround/Polygon Flash orientation；
- G74/G75 Arc；
- full circle；
- G74 zero-sweep；
- Region Arc；
- Clear object；
- Macro aperture；
- 四次 90° rotation；
- 两次相同 mirror。

不要只验证“自己的 writer 能被自己的 parser 打开”。至少对已知点用独立矩阵公式断言。

## 7. Capabilities

只有实际实现和专项测试全部通过后，才将：

```text
objects.rotate
objects.mirror
```

移入 `supported_operations`。

同时 capabilities 必须明确 RectangularSweep 的限制；不能写成“所有对象支持 arbitrary rotate”。

## 8. 必须新增的测试

至少新增：

```text
rotate_point_90_about_origin
rotate_point_37_about_nonzero_pivot
rotate_flash_updates_local_transform
rotate_existing_mirrored_flash_composes_correctly
rotate_line_roundtrip
rotate_arc_preserves_direction_and_source_semantics
rotate_full_circle_remains_full_circle
rotate_g74_zero_sweep_remains_zero_sweep
rotate_region_preserves_edge_order_and_topology
rectangular_sweep_rotates_exact_90_and_swaps_dimensions
rectangular_sweep_rejects_37_deg_atomically
mirror_arc_flips_direction
mirror_region_flips_all_arc_directions
mirror_flash_composes_local_transform
mirror_twice_restores_exact_state
four_quarter_turns_restore_exact_or_frozen_canonical_state
rotate_undo_redo_restores_exact_state
mirror_undo_redo_restores_exact_state
mixed_selection_failure_is_atomic
rotate_mirror_export_reopen_matches_independent_geometry
```

同时保持：

```text
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo test --locked -p editor-service --test s1b2_edit_workflow
```

全部不回归。

## 9. 本轮顺便记录但不要扩张实现的技术债

### 9.1 Structural order guard scalability

当前 Duplicate/Delete 保存整层 `before_order/after_order`，是 O(N) memory/time。

本轮只增加 ADR / TODO 和一个可测量基线，不要边写 Rotate/Mirror 边重构历史系统。

在 Production Renderer / 大文件性能阶段前必须改为紧凑结构验证，否则十万级以上图层的结构编辑会被 64 MiB 历史预算限制。

### 9.2 GUI Layer Lock / dirty

本轮不要实现 `layer.update`。

在 S2 GUI 前必须先决定：

- layer lock/name/visibility 是否属于 editor/workspace state；
- manufacturing dirty hash 必须忽略不影响 Gerber 输出的 UI 元数据；
- Undo/Redo 是否绕过当前 lock，避免“锁定后 Ctrl+Z 失效”。

### 9.3 多层模型

本轮继续单 Gerber / 单服务文档。

S2 前建立 ADR，优先考虑：

```text
1 Gerber file = 1 service document
GUI workspace 聚合多个 documents
```

不要无设计地启用 `document.import` 多层模式。

## 10. Mac 门禁

macOS arm64 实际执行并保留原始日志：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo test --locked -p editor-service --test s1b2_edit_workflow
cargo test --locked -p editor-service --test s1b2b_transform_workflow
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test headless_workflow
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
python3 scripts/source_manifest.py --check
```

继续生成独立 `evidence-public/s1-b2b/<run-id>/`：环境、commands、原始日志、源码哈希、合成输入输出、请求序列、release SHA、阶段 acceptance-results。

## 11. S1-B2b 退出条件

全部满足才进入 S2 Mac GUI：

- Rotate / Mirror 真实服务实现；
- Flash LocalTransform 组合正确；
- Arc mirror 方向正确；
- Region 拓扑不坏；
- RectangularSweep 限制 fail-closed；
- Undo/Redo exact state；
- export/reopen/independent geometry assertions 通过；
- capabilities 不夸大；
- S1-B1 / S1-B2a 全部不回归；
- macOS 全 workspace 门禁通过；
- evidence-public 可复核；
- Windows 继续 deferred，不标记通过。

完成后停止，不自动开始完整 GUI。
