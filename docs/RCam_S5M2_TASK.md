# RCam S5-K1 `c6f9920` 验收报告与下一阶段任务规划

> 验收对象  
> - `RCam-S5K1-c6f9920-Source.zip`  
> - `RCam-S5K1-c6f9920-Evidence-NoMedia.zip`
>
> 验收日期：2026-10-02  
> 项目方向：Mac-first；CircuitCAM 4.4 完整兼容放到最后阶段。

---

## 1. 最终结论

### S5-K1 实现状态

```text
S5-K1 Implementation
= PASS / FROZEN
```

当前 `c6f9920` 可以作为 S5-K1 的冻结版本，不建议继续在快捷键设置与迁移功能上扩展开发。

### 原生完整验收状态

```text
Native Full Closeout
= DEFERRED BY USER / NATIVE PENDING
```

原生 GUI 的剩余补验以后单独执行，不阻塞进入下一阶段。

### 下一阶段

```text
S5-M2
Mixed Workload / Stress / Recovery Closeout
```

重点进入高负载、长时间运行、异步任务生命周期和恢复能力验证。

---

# 2. 本次验收范围

本次针对 Source 和 Evidence 分开核查：

1. 源码包身份与完整性；
2. Evidence 包完整性；
3. 最终 commit 与审计记录一致性；
4. 快捷键配置、迁移、持久化和冲突处理实现；
5. Release / Test / Automation 证据；
6. RC1 问题关闭情况；
7. Native 验收缺口；
8. 是否存在阻断进入下一阶段的问题。

---

# 3. Source 身份

源码提交：

```text
c6f9920cd25190cd9c45a6c55c0e8c31c7a57938
```

Source 包内 manifest 已核对。

结果：

```text
MANIFEST.sha256
PASS
```

未发现源码包内部文件哈希不一致。

---

# 4. Evidence 完整性

Evidence 包：

```text
RCam-S5K1-c6f9920-Evidence-NoMedia.zip
```

包内 Evidence 文件哈希检查：

```text
EVIDENCE_FILES.sha256
PASS
```

最终 clean audit 与 RC1 production source 的代码身份能够对应当前提交。

未发现：

```text
Evidence 对应其他 commit
Evidence 与 Source 代码版本漂移
Source 被重新打包后内容变化
```

等阻断问题。

---

# 5. 自动化与构建证据

Evidence 中记录的最终测试状态：

```text
Workspace Tests
806 passed
0 failed
47 ignored
```

Automation：

```text
automation_contract
4 passed
0 failed

headless_workflow
2 passed
0 failed
```

同时已有：

```text
service dependency boundary PASS
public release build exit 0
internal release build exit 0
fresh archive identity PASS
dirty/mutation archive rejection PASS
```

这些证据共同支持 S5-K1 的代码冻结。

---

# 6. 本次环境限制

当前验收环境没有 Rust/Cargo 工具链，因此本轮无法重新执行：

```text
cargo fmt
cargo check
cargo clippy
cargo test
cargo build --release
```

所以：

> 本报告中的编译与测试结果来自上传 Evidence 中经过哈希验证的原始结果，不应表述为本轮环境重新执行得到。

这不影响源码静态检查和 Evidence 一致性结论，但后续如果进行正式发布签字，可在具备 Rust 工具链的干净机器上再做一次 fresh build。

---

# 7. S5-K1 实现评价

S5-K1 已经不是简单的快捷键绑定，而是形成了较完整的配置系统。

主要组成：

```text
Command Registry
        ↓
Shortcut Config Schema
        ↓
Decode / Validate
        ↓
Conflict Preflight
        ↓
Shortcut Settings UI
        ↓
Atomic shortcuts.json Store
        ↓
Runtime Keymap
        ↓
Command Dispatcher
        ↓
Business Handler
```

重点代码包括：

```text
shortcut_config.rs
shortcut_settings.rs
shortcut_store.rs
command.rs
```

整体职责划分合理。

---

# 8. Shortcut Config

当前配置层已经具备：

```text
MAX_BYTES = 64 KiB
MAX_KEYS  = 4
```

同时覆盖：

- unknown command 检查；
- duplicate command 检查；
- shortcut conflict 检查；
- platform reserved shortcut 检查；
- schema version 检查；
- 完整配置快照检查；
- 恢复默认快捷键；
- alias；
- Mac / Windows logical modifier；
- context overlap 判断。

这比简单保存字符串键位更安全。

---

# 9. Shortcut Store

`shortcut_store` 的保存流程已经具备生产级基本边界：

```text
Read previous fingerprint
        ↓
Acquire OS advisory lock
        ↓
Re-check fingerprint
        ↓
Write same-directory temporary file
        ↓
Flush / Sync
        ↓
Atomic replace
        ↓
Publish new keymap
```

这能避免大量常见问题：

```text
写一半进程崩溃
两个实例同时保存
保存过程中源文件被外部修改
UI 已切换但磁盘未完成
旧配置被静默覆盖
```

Evidence 中也包含多个强退出阶段验证。

---

# 10. RC1 问题关闭

之前候选版本存在：

```text
B1-01
B2-01
B2-02
```

最终 RC1 已完成针对性修复。

主要涉及：

```text
command_enabled
command_context_blocked
background commit failure sequence
button → real command handler
current shortcut hint
menu / block / layer entry
```

最终独立审计已经将这些问题标记为关闭。

因此：

```text
S5-K1 不需要重新开启开发
```

---

# 11. Evidence 小问题

发现一个非阻断问题：

```text
SOURCE_FRESH_SHA256SUMS.txt
```

部分条目如果直接在错误工作目录执行：

```bash
shasum -a 256 -c ...
```

可能出现：

```text
No such file or directory
```

但按报告记录的真实路径重新定位文件后，对应 SHA 实际匹配。

因此该问题归类：

```text
B2
Evidence helper / path documentation issue
```

不是：

```text
Source corruption
Binary mismatch
Package identity failure
```

后续正式交付包修正脚本工作目录即可。

---

# 12. Native Pending

S5-K1 代码可以冻结，但以下 Native 测试暂时保留：

- 剩余强退出点；
- 完整 `.rcam` 工程编辑组合；
- Undo / Redo / Export / Reopen 真人流程；
- 默认快捷键完整矩阵；
- 旧用户自定义配置升级；
- Dynamic menu hints；
- Disabled command matrix；
- 慢 I/O；
- 快速连续 UI 操作；
- Public build settings smoke；
- 第二台 Mac；
- Windows。

这些属于：

```text
Acceptance Debt
```

不是当前代码开发阻塞。

以后可以单独创建：

```text
S5-K1 Native Closeout
```

进行补验。

---

# 13. S5-K1 冻结要求

建议以：

```text
c6f9920cd25190cd9c45a6c55c0e8c31c7a57938
```

作为 S5-K1 baseline。

后续原则：

```text
不得为了 S5-M2 压力测试随意重构 S5-K1 shortcut system
```

除非新阶段确实发现：

```text
B0 crash/data-loss
B1 production blocker
```

否则快捷键模块只允许 bug fix。

---

# 14. 下一阶段：S5-M2

正式建议：

# S5-M2 — Mixed Workload / Stress / Recovery Closeout

目标不是继续增加编辑功能，而是证明 RCAM 在真实高负载使用下仍然：

```text
快
稳定
可取消
不会安装旧结果
不会泄漏资源
发生设备错误后能够恢复
```

---

# 15. S5-M2-1：异步任务生命周期

优先级：

```text
P0
```

需要建立统一异步任务状态模型。

至少包含：

```text
task_id
document_id
document_revision
generation
cancel_token
result_revision
```

典型流程：

```text
Task A Start
        ↓
Document Revision = 100
        ↓
用户修改工程
        ↓
Document Revision = 101
        ↓
Task A Finish
        ↓
发现 result_revision = 100
        ↓
Reject Stale Result
```

禁止：

```text
旧分析结果写回新工程
```

---

## 必测场景

### A. Document Changed

```text
Start task
Modify document
Task finishes
Reject
```

### B. Project Closed

```text
Start task
Close project
Cancel task
No result installation
```

### C. Project Replaced

```text
Project A
Start task

Open Project B

Task A returns

Reject
```

### D. Undo / Redo

```text
Start background analysis
Undo or Redo
Revision changes
Old result rejected
```

### E. Rule / Geometry Policy Changed

后续 Pattern Engine 会使用：

```text
rule_revision
geometry_policy_hash
```

因此现在异步框架就应该支持多 generation token，而不能只比较一个 document revision。

---

# 16. S5-M2-2：1000 Object Drag

目标：

```text
1000 objects selected
        ↓
continuous drag preview
        ↓
mouse release
        ↓
single transaction
```

必须保证：

```text
1 drag
=
1 transaction
=
1 undo entry
```

拖动过程中禁止：

```text
每个 object 单独创建 Undo
每帧重建完整 Document
每帧重建完整 spatial index
每帧重新创建大量 GPU resource
```

建议模型：

```text
DragStart
    snapshot original transforms

DragUpdate
    preview_delta only

DragCommit
    one batch command
```

---

## 验收指标

记录：

```text
100 objects
500 objects
1000 objects
5000 objects
```

统计：

```text
input latency
frame time
CPU
RSS
GPU upload
allocation count
commit duration
undo duration
redo duration
```

---

# 17. S5-M2-3：PMIX

构造真实混合制造对象，而不是纯 Circle Flash benchmark。

建议包含：

```text
Flash
Rectangle
Obround
Polygon
Region
Arc
Stroke
Text
BlockInstance
Multi Layer
Dark / Clear
```

操作矩阵：

```text
Pan
Zoom
Fit
Select
Box Select
Move
Rotate
Layer Toggle
Solo
Visibility
Lock
Undo
Redo
```

---

## PMIX 重点

需要测：

```text
scene rebuild
spatial index rebuild
GPU upload
selection update
hover update
dirty region
RSS
frame time
```

目标是发现：

```text
只有纯 Flash 快
混合 Region / Arc 后性能突然坍塌
```

这一类隐藏问题。

---

# 18. S5-M2-4：PPOL

构造复杂 Dark / Clear 工程：

```text
Large Dark Region
        +
Thousands of Clear Holes
        +
Dark Flashes
        +
Overlapping Regions
```

同时测试：

```text
Zoom
Pan
Select
Layer Toggle
Edit
Undo
Redo
```

这里不仅测性能。

还必须证明：

```text
优化后的 renderer
```

不会改变：

```text
Manufacturing Polarity Semantics
```

建议增加 Reference Renderer / CPU Geometry Truth 对照。

---

# 19. S5-M2-5：PSTRESS

建立长时间运行测试。

循环：

```text
Open
Pan
Zoom
Select
Drag
Undo
Redo
Layer Toggle
Fit
Close
Open
```

至少记录：

```text
30 min
1 h
2 h
```

可以自动化的部分尽量自动化。

观察：

```text
RSS
GPU memory
worker count
pending task count
scene cache
spatial index cache
file handle count
temporary files
```

重点发现：

```text
RSS linear growth
GPU resource leak
worker leak
task queue accumulation
cache never reclaimed
```

---

# 20. S5-M2-6：Cancel

所有长期任务都需要：

```text
CancellationToken
```

至少覆盖：

```text
import
index build
geometry analysis
large selection computation
future pattern matching
future area computation
```

要求：

```text
Cancel is cooperative
```

不能简单杀 worker 导致：

```text
half committed document
broken cache
dangling UI state
```

取消后的状态必须等价于：

```text
任务从未 Commit
```

---

# 21. S5-M2-7：Device Recovery

重点针对 GPU / renderer。

需要模拟或构造：

```text
surface lost
surface outdated
device lost
swapchain recreation
window resize during recovery
```

目标：

```text
Document remains alive
        ↓
Renderer reconstructed
        ↓
Scene rebuilt
        ↓
Continue editing
```

不允许：

```text
GPU error
→ 整个工程必须退出
```

---

# 22. 建议开发顺序

S5-M2 不建议从 benchmark 开始。

建议顺序：

```text
M2-A
Task Identity / Revision Fence / Cancel

        ↓

M2-B
1000 Object Drag / Batch Transaction

        ↓

M2-C
PMIX

        ↓

M2-D
PPOL

        ↓

M2-E
PSTRESS

        ↓

M2-F
Device Recovery

        ↓

M2 Native Closeout
```

原因：

异步生命周期和批量事务属于架构能力。

PMIX/PPOL/PSTRESS 主要是用来发现前面架构里的真实问题。

---

# 23. S5-M2 建议验收门禁

## Gate A — Static / Build

```text
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets
cargo test --workspace
release build
```

要求：

```text
exit 0
```

---

## Gate B — Async

必须证明：

```text
stale result rejected
cancelled result rejected
closed document result rejected
new project cannot receive old result
```

---

## Gate C — Transaction

1000-object move：

```text
one operation
one Undo
one Redo
```

结果完全恢复。

---

## Gate D — PMIX

至少一份固定混合工程 fixture。

必须：

```text
open
render
select
move
undo
redo
save
reopen
```

均通过。

---

## Gate E — PPOL

必须验证：

```text
Dark/Clear output
```

与 reference truth 一致。

---

## Gate F — PSTRESS

长时间测试不允许：

```text
linear RSS growth
worker growth
pending task accumulation
GPU resource accumulation
```

---

## Gate G — Recovery

至少验证：

```text
surface recreation
renderer reinit
document preserved
editing continues
```

---

# 24. 建议 Evidence 目录

```text
evidence/
├── package/
├── build/
├── tests/
├── async/
│   ├── stale-result/
│   ├── cancel/
│   └── project-switch/
├── drag1000/
├── pmix/
├── ppol/
├── pstress/
├── recovery/
├── native/
└── audit/
```

每项必须保存：

```text
command
exit code
stdout/stderr
input fixture hash
output hash
environment
commit
```

不要只保存截图。

---

# 25. 下一阶段不做的内容

S5-M2 不应扩展为新功能大杂烩。

明确不做：

```text
CircuitCAM 4.4 CAM Writer
CircuitCAM full roundtrip
GMC full migration
Windows full port
大量新的 CAD 编辑工具
新的 project format 大改
```

这些都不属于 S5-M2。

---

# 26. CircuitCAM 4.4 顺序

项目路线保持：

```text
RCAM 自身核心能力
        ↓
生产稳定性
        ↓
Manufacturing Geometry
        ↓
Pattern / Rule
        ↓
跨平台完善
        ↓
CircuitCAM 4.4 完整兼容
```

CircuitCAM 4.4 完整兼容继续放在最后。

---

# 27. 推荐主路线

当前建议：

```text
S5-K1
Shortcut Configuration / Migration
PASS / FROZEN

        ↓

S5-M2
Mixed / Stress / Recovery
NEXT

        ↓

Mac Production Closeout
Native / IME / DPI / File Safety

        ↓

CORE10 / Real Production Samples

        ↓

Manufacturing Geometry Engine

        ↓

Pattern / Similarity Engine

        ↓

Rule Library / Batch Replacement

        ↓

Windows Production Support

        ↓

CircuitCAM 4.4 Full Compatibility
LAST
```

---

# 28. 最终决定

本轮建议正式记录：

```text
S5-K1 c6f9920
Implementation = PASS
Code = FROZEN
Native Full Closeout = DEFERRED_BY_USER
```

下一阶段正式启动：

```text
S5-M2
Mixed Workload / Stress / Recovery Closeout
```

第一优先级：

```text
Task cancellation
Revision fencing
Stale-result rejection
```

第二优先级：

```text
1000-object interactive drag
Batch transaction
Single Undo
```

之后：

```text
PMIX
PPOL
PSTRESS
Device Recovery
```

完成 S5-M2 后，再决定进入 Mac Production Closeout 或 Manufacturing Geometry 主线。

