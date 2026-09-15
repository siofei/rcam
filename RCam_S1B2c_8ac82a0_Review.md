# RCam S1-B2c `8ac82a0` 复审报告

> 审查对象：`RCam_S1B2c_8ac82a0_source_with_evidence.zip`  
> 阶段：Mac-first S1-B2c / pre-GUI 模型收口  
> 结论：**阶段性通过，可以进入 S2-A Mac GUI 基础闭环。**  
> 本结论不是完整 V1、CORE10、Windows 或生产加工验收通过。

## 1. 本次实际复核

本次解包并静态检查全部源码、设计/ADR、测试和 public evidence，并实际执行：

- `sha256sum -c PACKAGE_MANIFEST.sha256`：1038 项 OK；
- `python3 scripts/source_manifest.py --check`：PASS，当前包 1036 项；
- `python3 scripts/test_audit_core10.py`：7/7 通过；
- 对照 `tested-source-hashes.json`、`environment.json`、final gate 原始日志和 `PACKAGE_INFO.json` 检查源码/证据绑定；
- 审查 `editor-core`、`editor-service` 的 manufacturing/workspace 状态、dirty、lock、Undo/Redo、export、close 与 JSON DTO；
- 审查 `s1b2c_workspace_workflow` 17 项专项测试源码及证据输出。

当前审查容器没有 Rust/Cargo，因此没有再次执行 Cargo。随包 macOS arm64 原始证据记录：

- workspace：181 passed / 0 failed / 2 ignored；
- S1-B1：17/17；
- S1-B2a：27/27；
- S1-B2b：31/31；
- S1-B2c：17/17；
- automation：4/4；
- headless：2/2；
- release `editor-app` 构建成功。

本轮证据身份较上一阶段明显改善：`environment.json` 的 Git HEAD 与 `tested_code_commit=1b3b2a5...` 一致，且 clean Git 状态下执行 final gates；`PACKAGE_INFO.json` 又单独声明 packaging/evidence commit `8ac82a0...`。

## 2. S1-B2c 目标完成情况

### 2.1 Manufacturing 与 Workspace 已真正分离

`SemanticLayer` 当前只有：

```rust
pub struct SemanticLayer {
    pub id: String,
    pub objects: Vec<SemanticObject>,
}
```

`display_name / visible / locked` 已移动到 `editor-service::LayerWorkspaceState`，按稳定 `LayerId` 保存。真正来自 Gerber 的 LN/image 等信息仍保留于 `SourceMetadata`。

**判定：通过。**

### 2.2 Dirty 只反映制造内容

`content_hash()` 仅序列化 `SemanticDocument`；Workspace 不再进入 dirty hash。

专项测试实际覆盖：

- lock 不产生 dirty；
- visible 不产生 dirty；
- display_name 不产生 dirty；
- workspace-only close 不要求制造修改确认；
- workspace-only 修改前后导出字节完全一致；
- 制造编辑仍会产生 dirty；
- 成功 export 后制造 dirty 清零。

**判定：通过。**

### 2.3 双 revision 语义成立

当前公开：

```text
revision            manufacturing revision
workspace_revision  workspace/editor revision
```

`layer.update` 只有实际值改变时推进 `workspace_revision`；no-op 不推进任何 revision、不建立制造 Undo、不清 Redo、不改变 dirty。

当前 `layer.update` 同时要求调用方持有最新 manufacturing revision 和 workspace revision。这个策略比完全独立的 workspace CAS 更严格，但已经在 ADR 0012 中明确冻结，GUI 只要从最新 `DocumentInfo` 提交即可，不构成本阶段阻塞。

**判定：通过。**

### 2.4 Lock 责任边界正确

新制造编辑：

```text
Move / Duplicate / Delete / Rotate / Mirror
```

统一在 service/application 边界调用 `check_workspace_edit()`；锁定返回 `LAYER_LOCKED`。

Undo/Redo 不再读取当前 lock，因此：

```text
Move -> Lock -> Undo
Move -> Undo -> Lock -> Redo
```

均可恢复已有制造事务。

**判定：通过。**

### 2.5 Workspace 不进入 Gerber

导出使用 manufacturing document + source metadata，不使用 Workspace。专项用例验证修改名称/显隐/锁定后导出字节与修改前一致；重新打开恢复默认 visible/unlocked，不把临时 Workspace 写进加工文件。

**判定：通过。**

### 2.6 服务依赖仍与 GUI/GPU 解耦

public dependency audit 显示 `editor-service` 正常依赖树未出现：

```text
egui / eframe / egui-wgpu / wgpu / winit
```

后续脚本自动化仍可复用同一 ApplicationService。

**判定：通过。**

## 3. 没有发现需要在进入 GUI 前返工的制造核心问题

本次没有发现会要求重新设计 Move/Duplicate/Delete/Rotate/Mirror、Undo/Redo、Writer 或 Workspace 分层的问题。

以下属于后续阶段任务，不是 S1-B2c 缺陷：

1. `objects.hit_test` 尚未实现；
2. layer/document f64 bounds 尚未作为公开服务能力实现；
3. GUI 仍是 S0 fixed demo；
4. Production Renderer 尚未实现；
5. selection/hover/pan/zoom 等真正 app state 尚未建立；
6. Workspace 当前是会话态，关闭重开不恢复，这是冻结范围内的预期行为；
7. 大文件结构历史仍有 O(N) order guard 技术债；
8. CORE-03/07/08、文字和 Windows 仍 deferred。

## 4. S2-A 前应保持的架构约束

进入 GUI 后不能倒退已有边界：

```text
ApplicationService
  ├─ manufacturing revision / history / export
  └─ workspace layer state

App/View State
  ├─ current document/session
  ├─ selected ObjectId
  ├─ hover ObjectId
  ├─ active tool
  ├─ pan/zoom
  └─ panel state
```

GUI 不得直接取得 `&mut SemanticDocument` 修改制造数据。数值移动、Undo/Redo、锁定、保存都继续走 ApplicationService。

点选最终真值必须使用 f64 manufacturing geometry。GPU Mesh、像素颜色和 AABB 只能做显示或候选筛选，不能成为最终命中判定。

## 5. S2-A 建议范围

下一阶段应正式做 Mac GUI 的第一个可用闭环：

```text
Open / drag-drop real Gerber
-> layer list
-> fit / pan / zoom
-> exact point hit-test
-> single selection + highlight
-> property panel
-> numeric Move
-> Undo / Redo
-> Save As / Export
```

这一轮只做单文件/单选/数值移动闭环。暂不加入：

- 鼠标拖动物体；
- 多选/框选；
- Grid/Snap/测距；
- 文字；
- Production Renderer 大规模性能优化；
- Windows。

## 6. 阶段判定

**S1-B2c：PASS（Mac-first 阶段门禁）。**

可以停止继续扩展 headless 基础能力，开发重心转到 S2-A Mac GUI。

Windows 继续 `deferred / not executed`，不阻塞当前 Mac 开发，也不得写为 passed。
