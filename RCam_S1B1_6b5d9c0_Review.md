# RCam S1-B1 `6b5d9c0` 源码复审

日期：2026-09-15  
对象：`RCam_S1B1_6b5d9c0_source.zip`  
阶段策略：Mac-first；Windows 延后，不作为当前阶段门禁。

## 1. 结论

**结论：S1-B1 的实现目标基本成立，可以结束“只读/写回原型”阶段，继续向编辑功能推进。**

当前已经存在真实业务链路：

```text
Open
→ Query
→ Move
→ Undo
→ Redo
→ Validate
→ Export new path
→ Reopen
→ Manufacturing geometry compare
```

不是空接口：`editor-core` 有原子 Move 与历史事务，`editor-service` 暴露 `objects.move`、`history.undo`、`history.redo`、`document.close`，并有独立的 `s1b_edit_workflow.rs`。

本次审查环境没有 Rust/Cargo，因此没有重新执行 Rust workspace 测试；实际执行：

- `python3 scripts/source_manifest.py --check`：PASS，306 files；
- `python3 -m unittest discover -s scripts -p test_audit_core10.py`：7/7 PASS；
- 静态核对 Move、Undo/Redo、dirty/revision、safe export、GUI service lifetime、S1-B1 tests 与文档。

项目自带 `docs/S1_B1_REVIEW.md` 声明 macOS arm64 上 workspace 105 passed / 0 failed / 2 ignored，S1-B1 专项 17/17；但本次源码 ZIP 没有打包 `evidence/s1b1-*` 原始日志，只包含历史 `evidence-public/s1-a1`，所以这些 Cargo/Metal 结果本次无法独立复核。

## 2. 已确认实现正确的主干

### 2.1 原子 Move

`EditHistory::move_objects`：

- 空对象集、重复 ID、未知 ID、锁定层、超预算拒绝；
- 先在克隆几何上完成全部平移与验证，再统一 apply；
- 成功只形成一个 Transaction；
- 失败不会产生部分几何修改；
- Redo 只在新编辑成功后清空。

覆盖：Flash、Line、RectangularSweep、Arc、Region。

Arc 同时平移 `start/end/center`，并检查平移后 full/zero identity 与半径关系；Region 对所有 Line/Arc edge 整体平移。

### 2.2 Undo / Redo

历史保存 before/after 几何，不通过“反向执行 Move”实现 Undo。Undo/Redo 前检查对象身份、索引、当前几何是否与事务预期一致；成功后 revision 单调递增。

### 2.3 保存基线与 dirty

当前使用制造模型内容 SHA-256 作为保存基线，而不是用 revision 或 Undo 栈长度判断 dirty。Undo 回保存状态时可恢复 clean；Redo 回编辑状态可恢复 dirty。

### 2.4 Safe export

当前只允许新目标路径；导出经过：

- revision 检查；
- host path policy；
- metadata policy；
- 模型 validate；
- writer；
- sync；
- writer round-trip 验证；
- no-clobber publish。

源路径不能直接作为当前 export target。

### 2.5 GUI 没有绕过服务

`editor-app` 已长期持有 `ApplicationService`，为后续 GUI 编辑共用同一业务入口做好准备。当前窗口仍是只读示例，这是符合 S1-B1 任务边界的。

## 3. 进入下一阶段前应修的确定问题

### F-01：0 位移会制造虚假编辑事务

严重度：**中高，建议在任何 GUI 拖动/数值编辑接入前修复。**

`move_objects` 只检查 `(dx,dy)` 是否为合法有限坐标，没有拒绝 `(0,0)`。因此：

```text
objects.move(dx=0, dy=0)
```

会：

- 克隆出与 before 完全相同的 after；
- push Undo transaction；
- 清空 Redo；
- revision +1；
- 实际制造内容完全没变。

这与“revision 对内容修改单调推进”的语义不一致，也会导致 GUI 点击/拖动未产生位移时污染历史。

建议契约：

- 精确 `(0,0)`：`INVALID_ARGUMENT`，revision/history/content 全不变；
- 若非零输入因 f64 可表示性导致所有目标坐标实际不变，也应拒绝为 no-op；
- 增加 `zero_move_is_noop_or_rejected_without_history` 回归测试。

### F-02：`NOT_FOUND` 的结构化错误类型标错实体

严重度：**中，建议 S1-B2 前修。**

当前 `ServiceError::not_found()` 固定返回：

```text
message: document X is not open
details: { document_id: X }
```

但它同时被用于：

- document 不存在；
- layer 不存在；
- object 不存在；
- aperture/查询实体不存在等。

因此缺失 object 时机器码虽然是 `NOT_FOUND`，结构化 details 却把 object ID 标成 document ID。

建议拆分：

```text
not_found_document(id)
not_found_layer(id)
not_found_object(id)
not_found_entity(kind,id)
```

统一：

```json
{"entity":"object","id":"object-123"}
```

不要让未来脚本/GUI解析 message 文本。

### F-03：新路径导出后“保存路径身份”不明确

严重度：**中，S2 GUI 前必须定稿。**

成功 `gerber.export_layer` 会更新 `saved_content_hash`，使 document 变为 clean；但：

- `source_path` 仍然是最初打开的原文件；
- `source_sha256` 仍然是最初原文件；
- 导出的修改版位于另一个路径。

这符合当前“导出后更新保存基线”的局部设计，但进入 GUI 后会出现语义歧义：窗口若显示原路径，同时 dirty=false，用户可能误认为原文件已保存。

建议二选一并写进契约：

1. **Save As 语义**：成功后 current_path 切换到新文件；或
2. **Export 语义**：source_path 保留，同时新增 `last_saved_path/saved_target`，GUI明确显示“来源”和“最后保存位置”。

当前更推荐方案 2，因为 API 名已经是 `gerber.export_layer`。

### F-04：根目录任务文件仍是 S0-B，README 有一句过期说明

严重度：**中（开发流程风险）。**

根目录：

```text
RCam_CODEX_NEXT_TASK.md
```

仍然是“下一轮 Codex 任务：S0-B 修补、回归和范围冻结”。继续让 Codex默认读取它可能导致阶段倒退。

另外 README 在前文已经声明 S1-B1 编辑存在，但后面仍写：

> a V1 headless editing workflow is not yet implemented

这与当前 `s1b_edit_workflow.rs` 矛盾。

建议：

- 把旧任务移到 `docs/archive/tasks/`；
- 根目录只保留当前下一阶段任务；
- README 改成“完整 V1 headless automation 尚未完成，但 S1-B1 Move 编辑闭环已实现”。

### F-05：S1-B1 原始证据没有进入本次源码交付包

严重度：**证据/交付问题，不是代码缺陷。**

`docs/S1_B1_REVIEW.md` 引用：

```text
evidence/s1b1-20260915-final/
evidence/s1b1-precommit-20260915/
```

但 ZIP 中 `evidence/` 被 `.gitignore` 排除，只带了 `evidence-public/s1-a1`。

因此本次无法独立核对 105/0/2、17/17、release build SHA 等原始结果。

建议以后每个阶段生成脱敏：

```text
evidence-public/s1-b1/
  environment.json
  cargo-test-summary.txt
  gates/*.log
  tested-source-hashes.json
  release-sha256.json
  acceptance-results.json
```

私有 Gerber 不进入公开证据包。

## 4. 不阻塞 S1-B1，但下一阶段必须考虑的架构问题

### 4.1 `source_command: usize` 不适合未来新建对象

当前 `SemanticObject`：

```rust
source_command: usize
```

它适合表示“从原 Gerber 哪一条命令来的对象”，但 Duplicate、文字、新建图形并不存在真实 source command。

目前 validation 还用 `source_command` 检查 source order。若未来用伪造 command number 或 `usize::MAX`，会把“来源 provenance”和“当前曝光顺序”混为一谈。

在实现 Duplicate/Text 前应拆分：

```text
当前曝光顺序：由 layer.objects 顺序/独立 order key 表示
来源信息：Option<SourceProvenance>
```

例如：

```rust
enum ObjectOrigin {
    Imported { command_index: usize },
    Generated { operation_id: String },
}
```

Writer 只依赖当前对象顺序，不依赖 imported command index 产生输出顺序。

### 4.2 Arbitrary Rotate 对 RectangularSweep 当前模型不完整

`RectangularSweep` 只有：

```text
start/end/width/height
```

没有 aperture orientation。

任意角旋转后，原来的轴向矩形扫掠一般不能继续用该结构精确表达。因此不要直接对 start/end 做旋转然后假装完成 AT-033。

进入 Rotate 前要么：

- 将 stroke 模型一般化为 `Path + ApertureDefinition + LocalTransform`；
- 要么首阶段只允许模型能无损表示的旋转子集，并明确拒绝其他角度。

Mirror 同样要处理 Arc CW/CCW 翻转和 aperture local transform composition。

## 5. 当前阶段判定

### 可以判定

- S1-B1 代码主干真实存在；
- Move/Undo/Redo/Export/Reopen 架构满足本轮设计方向；
- 可以继续进入后续编辑能力开发；
- Windows 继续 deferred，不阻塞 Mac-first 开发。

### 不能判定

- 不能在本次审查环境重新签署 macOS Cargo/Metal PASS；
- 不能宣称完整 AT-030/039/040/041/054 等已经通过；
- 不能宣称 CORE10/V1/生产兼容性通过；
- 不能宣称 GUI 已具备编辑能力。

## 6. 建议下一步

不要直接一次实现 Duplicate/Delete/Rotate/Mirror 全部。

推荐：

```text
S1-B1.1（很短）
  修 0 位移 / NOT_FOUND / save target contract / 文档任务文件

→ S1-B2a
  Object origin/order 模型
  Duplicate
  Delete
  Undo/Redo for insert/delete
  Export/Reopen

→ S1-B2b
  Transform representation ADR
  Rotate
  Mirror
  Arc direction + aperture transform

→ S2-Mac
  打开文件 / 图层 / 点选 / 高亮 / 属性 / 数值编辑
```

这样可避免在 Rotate 和未来 Text 阶段被 `source_command` 与 RectangularSweep 表示能力反复返工。
