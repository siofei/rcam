# RCam 下一阶段 Codex 任务：Mac-first S1-B2c / pre-GUI 模型收口

> 基线：S1-B2b package commit `23046c0`。  
> 当前开发/阶段验收平台：macOS Apple Silicon。  
> Windows 继续 deferred / not executed，不作为本轮门禁。  
> 本轮目的：**在正式 GUI 接入前清理 manufacturing / workspace 状态边界。**  
> 本轮不实现文字、不实现 Production Renderer、不扩 Gerber parser 范围、不做完整 GUI。

## 1. 开始前必读

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/AUTOMATION_API.md`
- `docs/S1_B2B_REVIEW.md`
- `docs/adr/0009-s1-b2a-object-transactions.md`
- `docs/adr/0010-s1-b2b-transform-representation.md`
- `docs/adr/0011-pre-gui-model-hygiene.md`
- 外部复审：`RCam_S1B2b_23046c0_Review.md`

保留所有 S1-B1/B2a/B2b 已通过能力和回归，不重新设计制造几何。

## 2. 本轮冻结目标

当前 `SemanticLayer.locked/name` 混在制造模型中，而且 dirty hash 序列化整个 `SemanticDocument`。

GUI 前必须变为：

```text
Manufacturing Document
├─ LayerId
├─ ordered objects
├─ apertures
├─ source manufacturing metadata
└─ 只包含影响 Gerber 输出/验证的状态

Workspace / Editor State
├─ layer display name
├─ visible
├─ locked
├─ 当前 GUI 选择（可留 app 层）
└─ 其他不进入 Gerber 的 UI 状态
```

核心规则：

> **修改 workspace 状态不能让 Gerber 变 dirty；Undo/Redo manufacturing history 不能被当前 workspace lock 阻止；但新的编辑命令必须尊重 lock。**

## 3. Manufacturing 模型清理

### 3.1 `SemanticLayer`

目标建议：

```rust
pub struct SemanticLayer {
    pub id: String,
    pub objects: Vec<SemanticObject>,
}
```

如果因兼容迁移暂时不能一次移除字段，也必须让 manufacturing hash、writer、validation 与 edit history 不再依赖 UI-only 字段，并写 ADR 说明过渡期限。

原 source 中的 Gerber `%LN` / image metadata 继续留在 `SourceMetadata`，不要误删真正的源文件元数据。

### 3.2 Workspace 状态

在 `editor-service` 每份打开文档中增加独立状态，例如：

```rust
LayerWorkspaceState {
    display_name: String,
    visible: bool,
    locked: bool,
}
```

按稳定 LayerId 索引。

默认：

- visible = true
- locked = false
- display_name 由导入层默认名/文件名生成，但不进入制造 hash

不得让 renderer/app 直接改 `SemanticDocument` 来实现这些功能。

## 4. Revision / Dirty 语义

保留现有 manufacturing `revision` 语义：

- Move/Duplicate/Delete/Rotate/Mirror/Undo/Redo 成功 → manufacturing revision +1
- 失败/no-op → 不推进

新增明确的 workspace revision（建议）：

```text
workspace_revision
```

`layer.update` 成功只推进 workspace revision，不推进 manufacturing revision，不创建 manufacturing Undo entry，不改变 dirty。

如果采用另一方案，必须达到同样的可观察语义并在 ADR 中冻结，不能让 UI 状态和 manufacturing revision 混在一起。

`DocumentInfo` 建议返回：

- revision
- workspace_revision
- dirty
- undo_entries / redo_entries

`LayerInfo` 返回：

- layer_id
- display_name
- visible
- locked
- object_count

## 5. `layer.update` 最小接口

新增真实服务操作：

```text
layer.update
```

最小允许更新：

- `display_name`
- `visible`
- `locked`

要求：

- layer 必须显式指定；
- 空/未知 layer → 结构化 NOT_FOUND/INVALID_ARGUMENT；
- display_name 有长度预算；
- 全部字段均未改变 → no-op，不推进 workspace revision；
- 不允许借 layer.update 改 manufacturing object/order/exposure；
- API 错误继续使用统一 response envelope。

## 6. Lock 语义必须调整

### 6.1 新编辑

以下新制造编辑必须检查 workspace lock：

- objects.move
- objects.duplicate
- objects.delete
- objects.rotate
- objects.mirror
- 未来 text.create / property edit

locked → `LAYER_LOCKED`，文档、revision、history、Redo 全部不变。

锁检查放在 service/application command 边界，不让 GUI 自己决定。

### 6.2 Undo / Redo

Undo/Redo 是恢复既有 manufacturing transaction：

```text
当前 layer.locked == true
```

**不得阻止 Undo/Redo。**

否则用户执行：

```text
Move
→ Lock layer
→ Undo
```

会被卡死。

从 `editor-core::EditHistory::check_transaction()` 去除 workspace lock 依赖；core history 应只验证制造对象身份、顺序和 before/after 状态。

## 7. Dirty / Export / Close

必须新增测试证明：

```text
Open
→ layer.visible=false
→ dirty == false

Open
→ layer.locked=true
→ dirty == false

Open
→ rename display name
→ dirty == false
```

并且：

- Export 输出字节不因 workspace visible/locked/display_name 改变；
- Close 不应因为只有 workspace 改动而要求“未保存制造修改”确认；
- last_saved_path 仍只对应制造文件保存；
- manufacturing edit 后 dirty 仍正常；
- export 成功后 manufacturing dirty 仍清零。

## 8. 不要把 GUI 选择塞进 manufacturing document

本轮只冻结原则：

- selected object IDs
- active tool
- hover
- pan/zoom
- active panel

属于 app/view state，不属于 SemanticDocument，也不进入脚本制造历史。

选择集未来可以由 GUI 持有稳定 ObjectId；对象被 Delete/Undo/Close 后 GUI 负责按服务结果修剪，不把 Selection 变成制造模型字段。

## 9. 为 S2 点选预留 core/service 接口边界

本轮至少形成 ADR/接口设计，不要求一次完成全部 CAD 选择算法。

S2 需要：

```text
objects.hit_test(point_mm, tolerance_mm, layer_id)
layer/document bounds
```

要求冻结：

- 使用 f64 manufacturing geometry；
- Flash 要尊重 hole + LocalTransform；
- Arc 使用真实 arc；
- Region 使用真实 contour 语义；
- GPU Mesh / raster pixel 不能作为最终命中真值；
- 空间索引未来只能提供 candidates，最终还要精确几何判断；
- 结果保持稳定 exposure/object order，支持之后的重叠循环选择。

如果本轮实现 hit-test，只实现小而完整的 point hit-test；不要顺手扩成完整框选系统。

## 10. 当前 S0 GUI 的处理

不要把旧 `S0App` 继续堆成功能完整 GUI。

它可以：

- 保留为 reference/correctness demo；或
- 移到 dev/demo feature。

S2 将建立新的 app shell：

```text
ApplicationService
   ↓
real document session
   ↓
layer/workspace state
   ↓
read-only render snapshot/change set
   ↓
renderer
```

本轮只准备边界，不实现 Production Renderer。

## 11. 证据流程修复

上一轮 public evidence 的源码哈希与最终包一致，但 `environment.json` 的 Git HEAD 仍是起点 `256dbc9`，而包声明 commit `23046c0`。

本轮 final run 必须做到：

1. 候选源码 commit 已形成；
2. `git status --porcelain` 为空，或明确记录仅 evidence 输出目录是 ignored；
3. final gates 后记录 `git rev-parse HEAD`；
4. `tested-source-hashes.json` 与该 commit 内容一致；
5. PACKAGE_INFO 区分：
   - tested code commit
   - packaging/evidence commit（如不同）
6. 不再出现 `uncommitted_deletions_excluded` 与 ZIP 实际内容矛盾。

同时修 README 中 S1-B2b 阶段描述：Duplicate/Delete 属于 S1-B2a，Rotate/Mirror 属于 S1-B2b。

## 12. 必须新增的测试

至少包含：

```text
workspace_lock_does_not_change_manufacturing_dirty
workspace_visibility_does_not_change_manufacturing_dirty
workspace_rename_does_not_change_manufacturing_dirty
workspace_update_increments_only_workspace_revision
workspace_noop_does_not_increment_revision
locked_layer_rejects_new_move_atomically
locked_layer_rejects_duplicate_delete_rotate_mirror
lock_after_move_does_not_block_undo
lock_after_undo_does_not_block_redo
workspace_state_not_written_to_export
workspace_only_change_does_not_require_close_confirmation
manufacturing_edit_still_changes_dirty
export_after_manufacturing_edit_clears_dirty
layer_update_unknown_layer_is_typed_not_found
layer_update_strict_json_fields
```

原 S1-B1/B2a/B2b 测试必须全部继续运行。

## 13. 本轮明确不做

- Windows/DX12；
- Python/Lua/JS 脚本引擎；
- HTTP/RPC；
- Production Renderer；
- 大文件结构历史优化；
- 中英文文字；
- 自动拼板；
- 完整框选；
- CORE10 兼容扩展；
- 新 Gerber parser 特性。

## 14. Mac 本轮门禁

实际执行并保留原始日志：

```bash
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

另增加本轮 workspace/model-hygiene 专项测试入口。

## 15. S1-B2c 退出条件

全部满足才进入 S2-A：

- [ ] workspace/editor state 与 manufacturing document 分离；
- [ ] layer lock/name/visible 不影响制造 dirty；
- [ ] 新编辑尊重 lock；
- [ ] Undo/Redo 不被当前 lock 阻塞；
- [ ] layer.update 有稳定 DTO、错误码和 revision 语义；
- [ ] Export/Reopen 不包含 workspace-only state；
- [ ] S1-B1/B2a/B2b 全回归；
- [ ] 服务正常依赖仍无 egui/wgpu/winit；
- [ ] final run 的 commit/source hash/package 身份一致；
- [ ] Windows 保持 deferred，而不是标记 passed。

完成后停止，不自动开展 S2 GUI。

## 16. S2-A 紧接着要做什么（不是本轮范围）

S1-B2c 通过后，下一轮立即进入 Mac GUI 基础：

```text
真实文件 Open / drag-drop
→ 图层列表（visible / lock）
→ fit-to-window + pan/zoom
→ point hit-test
→ selection highlight
→ 属性栏显示
→ 数值 Move
→ Undo/Redo
→ Save As / Export
```

第一版 GUI 先做单选和数值移动；鼠标拖动、多选/框选、Grid/Snap/测距放后续小闭环。
