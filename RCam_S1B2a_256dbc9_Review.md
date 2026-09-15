# RCam S1-B2a `256dbc9` 源码与证据复审

> 审查对象：`RCam_S1B2a_256dbc9_source_with_evidence.zip`  
> ZIP SHA-256：`5e496361a4457d2d150570429c5a600fe9d3ca1ff324a0d6cb9616ae41362dcd`  
> 当前策略：Mac-first；Windows 延后，不作为本阶段门禁。  
> 审查结论：**S1-B2a 可以判定为阶段性通过，可以进入 S1-B2b Rotate/Mirror。完整 V1 仍未完成。**

## 1. 本次实际复核范围

本次不是只读项目自带的 Review，而是重新检查了当前归档、源码、测试和公共 evidence。

实际完成：

- 解包并审查 `editor-core`、`editor-service`、`gerber-io` 及 S1-B2a 专项测试。
- 与上一版 S1-B1 源码做差异核对。
- `PACKAGE_MANIFEST.sha256`：完整校验通过。
- `python3 scripts/source_manifest.py --check`：通过，当前清单 **502 files**。
- `python3 scripts/test_audit_core10.py`：**7/7 通过**。
- `evidence-public/s1-b2a/.../tested-source-hashes.json`：30 个源码／Cargo 文件与当前归档 **30/30 匹配**。
- 原验收数据仍为 **96 个有效用例 + 1 个退役用例**，规范状态没有被篡改为“已通过”。
- 检查 S1-B2a Cargo 门禁的原始日志及 `commands.json`；10 个命令均记录 exit code 0。

当前审查环境没有 Rust/Cargo，因此没有重新执行 Rust 编译。以下 Rust 结果属于**随包原始 macOS 日志，且已经通过源码哈希与当前归档绑定**：

- workspace：132 passed / 0 failed / 2 ignored
- S1-B1 workflow：17/17
- S1-B2a workflow：27/27
- release build：exit 0
- editor-service 依赖树日志未包含 egui/eframe/egui-wgpu/wgpu/winit

release 二进制没有放入交付 ZIP，因此本次不能独立重新计算 `c6f1eacc...` 的二进制 SHA；这是证据完整度限制，不是源码缺陷。

## 2. 上一轮要求完成情况

### 2.1 S1-B1.1 收口

**通过。**

- `Move(0,0)` 返回错误，不推进 revision，不增加 Undo，不清除 Redo。
- 非零位移但 f64 最终无变化同样不制造假事务。
- `NOT_FOUND.details` 已使用 `{entity,id}`，document/layer/object 不再混用。
- `source_path` 与 `last_saved_path` 已分开；新路径 export 不会谎称源文件已改写。
- 旧任务已归档，README 已修订。
- 已补交 `evidence-public/s1-b1/`。

### 2.2 ObjectOrigin / 当前曝光顺序

**通过。**

模型已从：

```text
source_command: usize
```

改为：

```text
ObjectOrigin::Imported { command_index }
ObjectOrigin::Generated { operation_id }
```

当前曝光顺序由 `layer.objects` 决定；Writer 不依赖 provenance 排序。

这为 Duplicate、未来文字和新建几何留下了正确扩展边界。

### 2.3 Duplicate

**通过当前阶段目标。**

确认：

- `objects.duplicate` 已进入真实服务能力。
- 新对象使用单调、会话级稳定 ID。
- 失败不消耗 generated ID。
- Undo/Delete 不回收 ID；Redo 恢复原 ID。
- source request 顺序不会控制绘制顺序，实际按当前 exposure order 复制。
- 副本插在各自源对象之后，Dark/Clear 顺序有专项验证。
- geometry、exposure、aperture reference、origin 均按契约处理。
- Zero-offset duplicate 被明确允许。
- Export → Reopen 有专项测试。

### 2.4 Delete

**通过当前阶段目标。**

确认：

- 多对象删除为单事务。
- Undo 恢复 exact ObjectId、geometry、exposure、origin 和原 index/order。
- Redo 删除同一批身份。
- Delete All 后可以导出和重新解析合法空图层。
- malformed / unknown command 并未因“允许空图层”而被放宽。

### 2.5 历史模型

**通过。**

当前显式区分：

```text
Modify
Insert
Delete
```

并且 Undo/Redo 保存的是真实前后状态，而不是靠反向移动／重新生成 ID 恢复。

结构编辑有 order guard、history count 和 history byte budget；失败事务不提交部分结果。

### 2.6 Rotate/Mirror 前置 ADR

**ADR 0010 可作为下一阶段实现依据。**

主要决策合理：

- Flash 必须变换 center 并组合 LocalTransform。
- Line 可任意角旋转。
- Arc 同时变换 start/end/center；Mirror 翻转 CW/CCW。
- Region 整体变换并保持拓扑。
- RectangularSweep 当前不具备任意 aperture orientation，因此只允许能精确表达的 90° 倍数旋转和水平／垂直镜像；其余 fail-closed。

## 3. 当前没有发现的阻塞级问题

本次没有发现需要推翻以下设计的缺陷：

- ApplicationService 边界；
- Stable ObjectId；
- Insert/Delete Undo/Redo；
- Duplicate/Delete 的原子性；
- Export → Reopen；
- Source file protection；
- ObjectOrigin；
- Mac-first 策略。

因此**不需要再停留在 S1-B2a 修补阶段。**

## 4. 两个需要记录但不阻塞 S1-B2b 的工程债务

### D-01：结构事务的 order guard 为 O(N) 内存／时间

当前每一次 Duplicate/Delete 都保存整层：

```text
before_order: Vec<String>
after_order: Vec<String>
```

预算公式还按整层对象 ID 做保守计费。

这在小到中等 Gerber 上正确且安全，但在大图层上扩展性有限。仅按代码中的最低 ID 计费项估算，约 10~15 万对象就会接近 64 MiB transaction budget；50 万对象必然不能使用当前结构编辑事务。

这与 `max_document_objects = 500000` 不矛盾，因为服务允许打开大文件但可以对编辑返回 RESOURCE_LIMIT；不过它与未来“大文件也能顺畅 Duplicate/Delete”的产品目标有距离。

**建议：**不阻塞 S1-B2b；在 Production Renderer / 大文件性能阶段前，将整层 ID 快照替换成更紧凑的结构版本号、固定大小 order hash、局部 predecessor/successor anchors 或 range transaction。

不要为了省内存而取消原子性或顺序校验。

### D-02：GUI 加入 Layer Lock 前必须重新定义 Dirty/Undo 语义

`SemanticLayer.locked` 当前属于 `SemanticDocument`，而 `content_hash()` 会序列化整份文档。

目前没有公开 `layer.update`，所以不存在实际错误；但未来 GUI 一旦允许“锁定图层”：

1. 单纯锁定可能把文档标成 dirty，虽然 Gerber 制造内容没有变化；
2. 当前 `check_transaction()` 在 layer.locked 时拒绝 Undo/Redo，用户可能在锁定后无法撤销之前的几何编辑。

同样，未来 layer name / visibility 等纯编辑器状态不应混入 Gerber 制造内容基线。

**建议：**在进入 S2 GUI、实现 `layer.update` 之前，拆分：

```text
Manufacturing content state
Editor / workspace state
View state
```

并让 dirty 基线依据真正会改变导出 Gerber 的 manufacturing state，而不是整个 serde 文档。

## 5. 单层／多层边界仍需在 GUI 前冻结

当前服务实际是单文件、单层导入文档；这使当前：

```text
Export one layer → saved_content_hash 更新
```

是成立的。

V1 设计以后需要多个 Gerber 图层同时显示。进入 S2 前必须明确选择：

### 方案 A（推荐 Mac V1）

```text
1 Gerber file = 1 service document
GUI Workspace = 多个 service documents 的聚合
```

优点：保存边界天然对应一个文件，不需要现在重构 writer 和 per-layer baseline。

### 方案 B

真正实现：

```text
document.import → 一个 service document 中多个 Gerber layers
```

那么必须同步实现逐层 save baseline / dirty、跨层 ID 规则和 writer/export 约束。

在这个决策冻结前，不要直接把当前单层 record 当成完整多层工作区。

## 6. 阶段判定

### S1-B2a

**PASS（macOS 本阶段实现门禁）**。

### 完整 V1

仍然未通过，以下保持 deferred / incomplete：

- macOS 真正 GUI 编辑；
- Metal GUI 编辑实测；
- IME；
- Rotate/Mirror；
- 文字；
- Production Renderer；
- CORE10 10/10；
- 性能／长期稳定性；
- Windows。

## 7. 下一步

进入 **S1-B2b：Rotate/Mirror**。

这一阶段继续只做 core/service/headless，不立即进入 GUI；完成后再进行一次很短的 pre-GUI model hygiene，然后进入 S2 Mac GUI。
