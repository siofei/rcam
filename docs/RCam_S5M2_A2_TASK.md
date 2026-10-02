# RCam S5-M2-A2 Cancellation Closeout 开发与验收计划

> 适用基线：`RCam-S5M2-A-eb4ecb67` checkpoint  
> 基线状态：Architecture PASS / Implementation PASS / Automated Gates PASS / Final Acceptance PARTIAL  
> 目标：关闭 S5-M2-A 的取消、过期结果、Native 时延和 clean-freeze 缺口，并为 S5-M2-B 批量拖动阶段建立稳定基线。  
> 项目方向：Mac-first；CircuitCAM 4.4 完整兼容继续放到最后阶段。

---

# 1. 阶段定位

S5-M2-A 当前已经完成：

```text
Task identity
Revision fencing
Generation fencing
Rule revision fencing
Geometry policy fencing
Cooperative cancellation 基础
Stale result rejection
Import transactional commit
Selection cancellable path
Scene build cancellable path
```

当前不足：

```text
未完成完整 Native Cancel latency 验收
存在部分不可取消长段
Evidence verifier 不可移植
当前为 dirty / uncommitted checkpoint
尚未形成 clean frozen commit
```

因此下一阶段定义为：

# S5-M2-A2 — Cancellation / Native / Clean-Freeze Closeout

本阶段不新增业务功能，只关闭 S5-M2-A 的剩余验收债。

---

# 2. 最终目标

S5-M2-A2 完成后必须可以正式记录：

```text
S5-M2-A

Architecture     = PASS
Implementation   = PASS
Automated Gates  = PASS
Native Cancel    = PASS
Evidence         = PASS
Clean Package    = PASS
Final Acceptance = PASS
Code             = FROZEN
```

然后才进入：

```text
S5-M2-B
Batch Drag / 100–5000 Objects
```

---

# 3. 本阶段不做的内容

明确不做：

```text
新 CAD 编辑工具
新的 Pattern Engine
Rule Library
Manufacturing Geometry 大改
CircuitCAM 4.4 CAM/CAT/GMC 完整兼容
Windows 正式移植
PMIX
PPOL
PSTRESS
Device Recovery
```

这些不属于 S5-M2-A2。

---

# 4. P0：取消时延闭环

## 4.1 验收门槛

冻结目标：

```text
Cancel visible feedback <= 500 ms
Controlled task termination <= 2 s
```

这里区分两个指标：

### UI Feedback

用户点击 Cancel 后：

```text
<= 500 ms
```

必须出现明确状态：

```text
Cancelling...
Cancel requested
```

不能等 worker 完全结束后才告诉用户。

### Worker Termination

对于被定义为可取消的任务：

```text
<= 2 s
```

必须进入：

```text
Cancelled
```

或者其他明确 terminal state。

---

# 5. P0：统一 Task Lifecycle 约束

保留当前状态机：

```text
Queued
  ↓
Running
  ↓
CancelRequested
  ↓
Cancelled
```

另一条合法路径：

```text
Running
  ↓
Committing
  ↓
Completed / Failed
```

关键规则：

```text
Cancel 与 begin_commit 使用原子竞争
```

### Cancel 赢

```text
Running
→ CancelRequested

begin_commit
→ Reject
```

不得进入写事务。

### Commit 赢

```text
Running
→ Committing

Cancel
→ TooLate
```

UI 必须显示：

```text
Too late to cancel
```

或等价状态。

绝对禁止：

```text
已经 Commit
但 UI 显示 Cancelled
```

---

# 6. P0：TaskVersion 保持完整

当前任务身份模型必须继续保留：

```text
document_id
document_revision
workspace_revision
generation
rule_revision
geometry_policy_hash
```

不得为了简化代码退化成只比较：

```text
document_revision
```

验证点：

```text
Document 修改
Undo
Redo
Project replace
Project close
Workspace state change
Rule revision change
Geometry policy change
```

任意一项发生，都必须能够让旧任务进入：

```text
STALE_TASK
```

---

# 7. P0：清理不可取消长段

当前重点审计以下路径：

```text
edges_for
Block expansion
World index build
Render index build
accelerate_polygons
metrics
Gerber read
Gerber parse
```

原则：

```text
先测时延
再决定是否插 checkpoint
```

不要因为“理论上可能慢”就重写 parser。

---

# 8. Geometry / edges_for

当前常见形式：

```text
for object:
    cancel.checkpoint()
    edges_for(object)
```

风险：

```text
单个复杂 object 的 edges_for 本身耗时过长
```

这种情况下：

```text
cancel.checkpoint()
```

只存在于 object 之间还不够。

## 处理策略

如果真实 benchmark 显示：

```text
single edges_for <= 100 ms
```

可以不改。

如果出现：

```text
single edges_for >= 500 ms
```

则需要把 checkpoint 下沉到：

```text
curve iteration
polygon iteration
block child iteration
path segment iteration
```

---

# 9. Block Expansion

对于：

```text
BlockInstance
Nested Block
Large repeated geometry
```

建议允许：

```text
expand_block_cancellable(...)
```

并在：

```text
每 N 个 child
```

检查一次。

建议第一版：

```text
N = 64
```

但不要硬编码为产品语义。

应以：

```text
响应时延
```

为准调整。

---

# 10. accelerate_polygons

当前如果：

```text
scene build
↓
collect polygons
↓
accelerate_polygons()
```

而 `accelerate_polygons()` 为整块不可取消，就要测实际耗时。

若真实最大 fixture：

```text
< 200 ms
```

可以保留。

若：

```text
> 500 ms
```

则需要：

```text
chunked acceleration build
```

例如：

```text
for polygon_chunk in chunks:
    cancel.checkpoint()
    build_chunk()
```

---

# 11. Metrics

需要检查：

```text
area
bbox
centroid
perimeter
edge count
geometry statistics
```

是否存在：

```text
整层扫描
大 polygon 扫描
大 block 展开
```

如果 metrics 已经小于时延门槛：

```text
无需重构
```

否则增加 cancellable variant。

---

# 12. Gerber Read / Parse

不要直接把 parser 重构为异步流式。

先做真实 benchmark。

至少选择：

```text
small fixture
medium fixture
large real Gerber
largest known production Gerber
```

记录：

```text
file read duration
parse duration
semantic build duration
total import duration
```

判断：

### 情况 A

```text
read + parse < 2 s
```

可以视为受控不可取消段。

### 情况 B

```text
read + parse >= 2 s
```

需要进一步拆分：

```text
read
parse
semantic conversion
index build
```

优先把 checkpoint 加在：

```text
semantic conversion
index build
```

如果第三方 parser 本身不可取消，只需要明确记录：

```text
parser uninterruptible window
```

并判断是否满足 2 秒 SLA。

---

# 13. P0：Import Transaction Safety

现有设计必须保持：

```text
Read
↓
Parse
↓
Build temporary result
↓
Cancel checkpoint
↓
begin_commit
↓
history.add_layers
↓
Single transaction
```

取消发生在 Commit 前：

```text
Document unchanged
Undo unchanged
No partial layers
No partial objects
```

进入 Commit 后：

```text
Cancel = TooLate
```

不得尝试中途中断：

```text
history.add_layers
```

否则容易制造半事务。

---

# 14. P0：Selection Cancel

对：

```text
objects_select_rect_with_cancel
```

必须保证：

```text
all results
or
CANCELLED
```

不得返回：

```text
partial selection
```

取消后：

```text
current selection unchanged
```

除非 UI 有明确的“渐进选择”设计；当前阶段不采用渐进选择。

---

# 15. P0：Scene Build Cancel

Scene build 必须继续保持：

```text
build temporary scene
↓
complete
↓
publish
```

取消发生：

```text
drop temporary scene
keep old scene
```

不得：

```text
publish half scene
```

UI 也不能进入：

```text
half new / half old
```

混合状态。

---

# 16. P0：Stale Result Matrix

必须补齐正式测试矩阵。

## Case 1 — Document Changed

```text
Task Start
revision = 100

Modify document
revision = 101

Task Result revision = 100

Reject
```

---

## Case 2 — Undo

```text
Task Start
↓
Undo
↓
revision changes
↓
Reject old result
```

---

## Case 3 — Redo

同上。

---

## Case 4 — Project Close

```text
Task Start
↓
Close project
↓
Cancel / invalidate generation
↓
Task returns
↓
Reject
```

---

## Case 5 — Project Replace

```text
Project A
Task A start

Open Project B

Task A returns

Reject
```

---

## Case 6 — Rule Revision Change

```text
Task rule_revision = 5
Current rule_revision = 6

Reject
```

---

## Case 7 — Geometry Policy Change

```text
Task geometry_policy_hash = A
Current = B

Reject
```

---

# 17. P0：Native Cancel 验收

必须使用：

```text
Release build
```

而不是 debug build。

至少连续执行：

```text
3 runs
```

---

# 18. Native Test A — Import Cancel Before Commit

流程：

```text
Open RCAM
↓
Start large Gerber import
↓
Click Cancel while Running
```

记录：

```text
T0 = click Cancel
T1 = UI shows Cancelling
T2 = worker terminal
```

要求：

```text
T1 - T0 <= 500 ms
T2 - T0 <= 2 s
```

最终：

```text
Document object count unchanged
Layer count unchanged
Undo count unchanged
Dirty state unchanged
```

---

# 19. Native Test B — Cancel Too Late

流程：

```text
Start task
↓
wait until Committing
↓
Click Cancel
```

必须：

```text
Cancel result = TooLate
```

最终：

```text
operation completes normally
Document correct
Undo entry correct
UI does not claim Cancelled
```

---

# 20. Native Test C — Project Close During Task

```text
Start task
↓
Close project
```

必须：

```text
task cancelled or generation invalidated
```

旧 result 后续返回：

```text
must not install
```

---

# 21. Native Test D — Project Switch

```text
Project A
↓
start task
↓
Open Project B
↓
A result returns
```

必须：

```text
Project B unchanged
```

---

# 22. Native Test E — Undo / Redo During Task

```text
Start background task
↓
Undo
↓
old result rejected

Start another task
↓
Redo
↓
old result rejected
```

---

# 23. P1：Evidence Verifier 可移植性

当前：

```text
verify_checkpoint.py
```

存在开发机绝对路径依赖。

例如：

```text
/Users/.../.codex/worktrees/...
/Volumes/.../.tools/...
```

最终包不允许保留这种依赖。

---

# 24. Evidence Verifier 新规则

脚本必须基于：

```text
script directory
package root
relative paths
```

解析。

推荐：

```python
ROOT = Path(__file__).resolve().parent
```

或者：

```text
Evidence Root
../Source Root
```

由 CLI 显式参数传入。

---

# 25. 推荐 CLI

```bash
python3 verify_evidence.py \
  --source /path/to/source \
  --evidence /path/to/evidence
```

输出：

```text
Source manifest: PASS
Evidence manifest: PASS
Commit identity: PASS
Test logs: PASS
Release identity: PASS
Native matrix: PASS
```

退出码：

```text
0 = all pass
1 = evidence mismatch
2 = usage/configuration error
```

---

# 26. P1：Evidence 必须记录环境

每个 Native / benchmark 项目至少记录：

```text
commit
build profile
macOS version
CPU
RAM
GPU
display resolution
display refresh rate
fixture hash
start time
duration
result
```

性能数据不能脱离机器环境解释。

---

# 27. P1：正式 Commit

当前 checkpoint：

```text
dirty / uncommitted
```

不能作为最终冻结基线。

完成 A2 后必须：

```text
git status
```

确认准备提交。

然后：

```text
git commit
```

形成独立 commit。

例如：

```text
S5-M2-A Cancellation Closeout
```

---

# 28. P1：Clean Worktree

正式 package 前：

```text
git status --porcelain
```

必须为空。

记录：

```text
clean_worktree = true
```

最终 package 不接受：

```text
untracked source
modified tracked file
staged-but-uncommitted source
```

---

# 29. P1：Final Package Identity

建议：

```text
RCam-S5M2-A-<git-short-sha>-Source.zip
RCam-S5M2-A-<git-short-sha>-Evidence.zip
```

不要再使用：

```text
manifest hash prefix
```

伪装成 git commit。

包名中的 ID 必须明确就是：

```text
git short SHA
```

---

# 30. Final Automated Gates

正式冻结前重新执行：

```text
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets
cargo clippy -p internal-evidence --all-targets
cargo test --workspace
automation_contract
headless_workflow
service dependency boundary
cargo build --release
```

要求：

```text
all exit 0
```

---

# 31. Task Lifecycle Tests

至少独立覆盖：

```text
cancel_before_commit
cancel_after_commit
stale_document_revision
stale_workspace_revision
stale_generation
stale_rule_revision
stale_geometry_policy
project_close
project_switch
```

不能只依赖 UI 测试间接覆盖。

---

# 32. Import Tests

至少：

```text
cancel import before commit
successful import one undo
failed parse zero mutation
stale import result rejected
project closed result rejected
```

---

# 33. Selection Tests

至少：

```text
selection complete
selection cancelled
selection stale
selection no partial publish
```

---

# 34. Scene Tests

至少：

```text
scene complete
scene cancelled
old scene preserved
stale scene rejected
```

---

# 35. 性能记录

本阶段不是完整 performance phase，但需要记录 cancellation-sensitive timings。

至少：

```text
largest edges_for
largest block expansion
largest index build
accelerate_polygons
largest metrics pass
Gerber read
Gerber parse
semantic conversion
```

目的不是追求速度排名，而是证明：

```text
最长不可取消窗口
```

符合 SLA。

---

# 36. 阻断级别

## B0

必须阻止冻结：

```text
Data corruption
Partial commit
Cancelled task still mutates document
Stale result installed into new document
Crash on cancel
Undo stack corrupted
```

---

## B1

必须在 A2 关闭：

```text
Cancel feedback > 500 ms
Controlled task termination > 2 s
TooLate state incorrect
Project switch can receive old task result
Native cancel matrix incomplete
```

---

## B2

允许修复后再冻结：

```text
Evidence script absolute path
README mismatch
Minor log naming issue
Non-blocking diagnostic wording
```

---

# 37. S5-M2-A2 Exit Criteria

只有以下全部满足才可 PASS：

```text
[ ] clean git commit
[ ] clean worktree
[ ] Source manifest PASS
[ ] Evidence manifest PASS
[ ] all automated gates PASS
[ ] Native cancel feedback <= 500 ms
[ ] cancellable task terminal <= 2 s
[ ] cancel-before-commit zero mutation
[ ] cancel-after-commit returns TooLate
[ ] stale document result rejected
[ ] stale workspace result rejected
[ ] stale generation rejected
[ ] stale rule revision rejected
[ ] stale geometry policy rejected
[ ] project close rejects old result
[ ] project switch rejects old result
[ ] selection never publishes partial result
[ ] scene never publishes partial result
[ ] Evidence verifier portable
[ ] final Source/Evidence tied to same Git commit
```

---

# 38. 完成后冻结规则

A2 PASS 后：

```text
S5-M2-A
= FROZEN
```

后续 S5-M2-B 不允许顺手大改 task lifecycle。

只有出现：

```text
B0
B1 production blocker
```

才允许回到 M2-A。

---

# 39. 下一阶段：S5-M2-B

A2 完成后正式进入：

# S5-M2-B — Batch Drag / 100–5000 Objects

---

# 40. M2-B 核心目标

实现：

```text
100 / 500 / 1000 / 5000 selected objects
↓
interactive drag
↓
preview
↓
single commit
```

最终必须：

```text
1 Drag
=
1 Transaction
=
1 Undo
=
1 Redo
```

---

# 41. M2-B 推荐架构

```text
DragStart
    snapshot selected object transforms

DragUpdate
    preview_delta only
    no document commit

DragCommit
    one batch command

DragCancel
    discard preview
```

禁止：

```text
mouse move
→ document mutation
→ undo push
```

---

# 42. M2-B 性能指标

记录：

```text
100 objects
500 objects
1000 objects
5000 objects
```

每档统计：

```text
input latency
frame time
CPU
RSS
GPU upload
allocation
commit duration
undo duration
redo duration
```

---

# 43. 项目总路线

保持：

```text
S5-M2-A2
Cancellation Closeout
        ↓
S5-M2-B
Batch Drag
        ↓
PMIX
        ↓
PPOL
        ↓
PSTRESS
        ↓
Device Recovery
        ↓
Mac Production Closeout
        ↓
Manufacturing Geometry
        ↓
Pattern / Similarity
        ↓
Rule Library / Batch Replacement
        ↓
Windows Production Support
        ↓
CircuitCAM 4.4 Full Compatibility
LAST
```

---

# 44. 最终任务定义

下一次开发任务应明确写成：

```text
任务名称：
RCam S5-M2-A2 Cancellation / Native / Clean-Freeze Closeout

目标：
关闭 S5-M2-A checkpoint 剩余的取消时延、不可取消长段、
Native Cancel、Evidence 可移植性和 clean commit 缺口。

禁止：
新增无关编辑功能；
提前进入 M2-B；
开始 CircuitCAM 4.4 完整兼容。

完成条件：
所有自动化门禁通过；
Native Cancel <=500ms UI feedback；
受控任务 <=2s terminal；
stale result 全矩阵拒绝；
clean commit；
clean worktree；
Source/Evidence 同 commit；
portable verifier；
最终 S5-M2-A = PASS/FROZEN。
```
