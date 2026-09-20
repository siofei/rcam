# S3-FINAL GUI / ApplicationService Boundary Audit

审计目标：`editor-app` 不直接修改 `SemanticDocument`、`layer.objects`、制造 geometry 字段或 writer AST。
GUI 持有的是只读 `RenderSnapshot`/`ObjectInfo` 和 app-only camera/selection/tool state；所有已提交制造修改
由后台串行 `Model` 调用同一个 `ApplicationService`。

| GUI 动作 | app 分派 | ApplicationService | 制造事务 |
|---|---|---|---|
| 数值 Move | `Action::Move` | `objects_move` / `objects.move` | 1 revision + 1 Undo |
| Direct Drag release | `Action::DragMove` | `objects_move` | preview 仅 UI；release 1 transaction |
| Duplicate | `Action::Duplicate` | `objects_duplicate` | 1 transaction，新稳定 ID |
| Delete | `Action::Delete` | `objects_delete` | 1 transaction |
| Rotate | `Action::Rotate` | `objects_rotate` | 选择集 1 transaction |
| Mirror | `Action::Mirror` | `objects_mirror` | 选择集 1 transaction |
| Flash width/height | `Action::SetFlashSize` | `objects_set_properties` | Aperture COW，1 transaction |
| Undo / Redo | `Action::History` | `history_undo` / `history_redo` | revision 递增，内容基线独立 |
| Save As | `Action::Save` | `grant_file_access` + `export_layer` | service writer；GUI 只提供路径/确认 |
| Open / Close | `Action::Open/Close` | `open` / `close` | service 生命周期 |

只读/工作区状态映射：点选调用 `objects_hit_test`；Window/Crossing 调用 `objects_select_rect`；属性读取
调用 `objects_get`/`objects_metrics`；图层显隐/锁定/名称调用 `layer_update`。Camera、selection、Grid、
Snap candidates、Measure overlay、display unit 和 drag preview 不进入制造 revision/Undo/writer。

静态搜索仅在 app 的 display/GPU/test 构造代码发现 `SemanticDocument` 或对测试 Scene 的 geometry 赋值；
生产 `state.rs` 对 `layer.objects` 的读取位于只读 snapshot → snap candidate 转换，没有写入。正常依赖树
`cargo tree --locked -p editor-service -e normal` 不含 egui/eframe/wgpu/winit；原始结果在
`evidence/s3-final-20260920/public/gates/13.log`。

结论：S3 GUI 的 Move/Duplicate/Delete/Rotate/Mirror/Flash size/Undo/Redo/Save As 均没有第二套制造实现。
`edit.batch` 目前是 service/automation 能力；GUI 仍按一个用户动作发一个原子 service 请求。
