# RCam 下一阶段 Codex 任务：Mac-first S1-B2 编辑能力扩展

> 执行记录（2026-09-15）：S1-B1.1 与 S1-B2a 已完成本机门禁，见 [S1_B2_REVIEW](docs/S1_B2_REVIEW.md)。
> 本文原任务正文保留；后续仅按 ADR 0010 进入 S1-B2b，不自动进入 GUI。

> 基线：S1-B1 `6b5d9c0`。  
> 当前平台：macOS Apple Silicon。Windows 延后且保持 not executed/deferred；不新增 Linux/WSL2 产品支持。  
> 本任务不进入完整 GUI、文字、Production Renderer 或兼容性扩张。

## 1. 先完成 S1-B1.1 收口

在新增编辑类型前先修以下契约：

1. `objects.move(dx=0,dy=0)` 不得推进 revision、不得新增 Undo、不得清除 Redo。推荐返回 `INVALID_ARGUMENT`；对非零输入但最终所有制造坐标不发生可表示变化的情况同样不得制造假事务。
2. 将 `NOT_FOUND` 改成带实体类型的结构化错误；document/layer/object/aperture 等不得都返回 `{document_id: ...}`。
3. 冻结新路径导出的保存身份：保留 `source_path` 时增加 `last_saved_path`（或等价字段）；GUI 后续必须能区分“打开来源”和“最后保存目标”。
4. 把根目录过期 `RCam_CODEX_NEXT_TASK.md` 归档；README 删除“headless editing 尚未实现”的过期描述。
5. 为本阶段生成脱敏 `evidence-public/s1-b1/`，至少包含环境、Cargo 门禁摘要、源码哈希、release SHA 与专项结果；不包含私有 Gerber。

新增回归：

```text
zero_move_does_not_create_history
missing_object_has_object_not_found_details
missing_layer_has_layer_not_found_details
export_reports_saved_target_without_claiming_source_was_modified
```

## 2. 在 Duplicate/Delete 前拆分“当前顺序”和“来源 provenance”

当前 `SemanticObject.source_command: usize` 不能诚实表示 Duplicate/文字等新建对象。

建立明确模型，例如：

```rust
pub enum ObjectOrigin {
    Imported { command_index: usize },
    Generated { operation_id: String },
}
```

约束：

- 当前曝光顺序以 `layer.objects` 顺序（或独立稳定 order key）为准；
- Writer 输出顺序只依据当前文档顺序；
- imported command index 只用于诊断/provenance，不作为新对象的伪造排序值；
- 原始对象导入后 provenance 保持；
- Duplicate 产生 Generated origin；
- Undo 恢复原对象和原顺序；
- 不允许使用 `usize::MAX` 等哨兵冒充真实源命令。

## 3. S1-B2a：Duplicate

新增：

```text
objects.duplicate
```

最少参数：

```json
{
  "layer_id":"layer-1",
  "object_ids":["object-1"],
  "dx_mm":5.0,
  "dy_mm":-3.0
}
```

要求：

- 一次请求一个原子事务；
- 新对象获得全新稳定 ObjectId；
- 不复用被删除过的 ID；
- exposure/geometry/aperture reference 保持；
- duplicate 后再应用明确偏移；
- 插入顺序必须冻结并测试，推荐在对应源对象之后、保持请求对象的当前曝光顺序；
- Clear/Dark 相对次序不可因为 duplicate 被任意重排；
- 新对象 origin 为 Generated；
- Undo 删除本次全部新对象，Redo 以相同 ObjectId/顺序恢复；
- 失败不得留下部分新对象或消耗 ID 到不明确状态（若 ID 单调预留后不回收，需明确文档契约）。

## 4. S1-B2a：Delete

新增：

```text
objects.delete
```

要求：

- 多对象删除原子；
- Undo 恢复完全相同的 ObjectId、geometry、exposure、origin 与 layer index/order；
- Redo 再删除同一批对象；
- 新编辑成功后清空 Redo；
- 未知/重复 ID、锁定层、空集、超预算全部拒绝且不变更文档；
- 删除后 query/分页 revision 冲突语义保持正确；
- Writer/Reopen 后制造曝光与删除后的文档一致。

历史层建议从“仅 before/after geometry change”扩展为显式事务操作，而不是给 Duplicate/Delete 硬塞假 geometry：

```text
ModifyObjects
InsertObjects
DeleteObjects
```

每种事务必须有明确内存预算。

## 5. S1-B2b 前置：Rotate/Mirror 表示能力 ADR

本轮不要在模型不够表达时直接实现任意旋转。

先形成 ADR，逐类型说明：

### Flash

- pivot 旋转中心坐标；
- 对非圆 aperture，必须正确组合 `LocalTransform.rotation_deg`；
- Mirror 必须组合 `LocalTransform.mirror`，不能只镜像中心。

### Circular Line

- 旋转/镜像 start/end；宽度不变。

### Arc

- start/end/center 一起变换；
- Rotate 保持 CW/CCW；
- Mirror 必须 CW↔CCW；
- full_circle/zero_sweep/source resolution/deviation 语义保持。

### Region

- 所有边整体变换；
- Mirror 内所有 Arc 方向翻转；
- contour edge order 与拓扑保持。

### RectangularSweep

当前结构缺少 aperture orientation。任意角旋转不能只旋转 start/end。

必须二选一：

1. 一般化为 `Path + Aperture + LocalTransform`；或
2. 明确限制为当前模型可无损表示的旋转集合并 fail-closed。

在 ADR 通过前，不得把 arbitrary rotate capability 暴露给 `supported_operations`。

## 6. 必须新增的测试

至少新增：

```text
duplicate_flash_has_new_stable_id
duplicate_preserves_exposure_order
duplicate_clear_object_preserves_composition
duplicate_undo_redo_restores_same_ids
delete_multi_object_is_atomic
delete_undo_restores_exact_indices_and_ids
delete_redo_removes_same_objects
failed_duplicate_preserves_document_history_and_id_contract
failed_delete_preserves_document_history
query_cursor_conflicts_after_insert_delete
duplicate_export_reopen_matches_document
delete_export_reopen_matches_document
```

并保留 S1-B1 所有测试通过。

## 7. 当前不要做

- 不做大规模 GUI；
- 不做中文/英文字体；
- 不做脚本解释器/HTTP/RPC；
- 不扩展 CORE-03/07/08；
- 不重写 Production Renderer；
- 不把 Windows 标记通过；
- 不在 RectangularSweep 无法表达的情况下假装支持任意 Rotate。

## 8. Mac 阶段门禁

在 macOS arm64 实际执行并保存原始日志：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1b_edit_workflow
cargo test --locked -p editor-service --test s1b2_edit_workflow
cargo tree --locked -p editor-service --edges normal
cargo build --release --locked -p editor-app
python3 scripts/source_manifest.py --check
```

要求正常 `editor-service` 依赖仍无 egui/eframe/wgpu/winit。

## 9. S1-B2a 退出条件

只有以下全部满足才进入 Rotate/Mirror 或 S2 GUI：

- S1-B1.1 四项契约修正完成；
- origin/order 不再依赖伪造 `source_command`；
- Duplicate/Delete 真实实现并出现在 capabilities；
- Insert/Delete Undo/Redo 恢复 exact ID/order；
- 失败事务原子；
- 编辑后 validate/export/reopen/geometry compare 通过；
- 源文件保护继续通过；
- macOS 全 workspace 门禁通过；
- S1-B1 原测试不回归；
- Windows、CORE10 完整通过仍保持 deferred/not complete。

完成后停止，提交源码、日志、源码哈希、release SHA、脱敏 evidence-public，并明确列出 Rotate/Mirror ADR 结论。不要自动进入 GUI。
