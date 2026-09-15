# RCam S1-B2b `23046c0` 源码与证据复审

> 审查对象：`RCam_S1B2b_23046c0_source_with_evidence(3).zip`  
> 当前策略：macOS Apple Silicon 优先；Windows 延后，不作为当前阶段门禁。  
> 审查结论：**S1-B2b 阶段性通过，可以进入 pre-GUI 模型收口；不需要返工 Rotate/Mirror。**  
> 注意：这不是完整 V1、生产加工资格、CORE10 10/10、GUI 或性能验收通过。

## 1. 本次实际复核

本次审查环境没有 Rust/Cargo，因此没有重新执行 Rust 编译和 `cargo test`；对交付包进行了归档、源码、证据和可执行 Python 检查。

实际结果：

- ZIP SHA-256：`c634895971ff48d747737a3649d95cf618f300d848cacb8d969837b7871166e1`
- `PACKAGE_MANIFEST.sha256`：通过。
- `python3 scripts/source_manifest.py --check`：通过，报告 928 个受清单管理文件。
- `python3 scripts/test_audit_core10.py`：7/7 通过；该结果只验证审计脚本，不等于 CORE10 产品验收通过。
- Rust 源码中发现 166 个 `#[test]` 标记；数量本身不等于运行结果。
- `evidence-public/s1-b2b/s1b2b-20260915-125200/tested-source-hashes.json`：33 个受测源码/manifest/toolchain 文件与当前归档逐字节哈希一致，33/33 匹配。
- 公共证据记录 workspace：164 passed / 0 failed / 2 ignored；S1-B1 17/17、S1-B2a 27/27、S1-B2b 31/31、automation_contract 4/4、headless_workflow 2/2。此组 Cargo 结果来自随包原始日志，本次没有重新运行。

与此前版本相比，这次证据链完整度已经足以支持阶段复审：源码哈希、命令日志、合成 Gerber、JSON 请求/响应、失败开发记录和 release 哈希均在包内。

## 2. S1-B2b 实现判断

### 2.1 Rotate/Mirror 已形成真实服务能力

`ApplicationService` 已正式暴露：

- `objects.rotate`
- `objects.mirror`

并进入 capabilities。请求包含显式 `layer_id`、`object_ids`、pivot/axis 和 expected revision，复用原子 `Operation::Modify` 历史，不是 GUI 专用捷径。

### 2.2 Flash 方向变换实现正确

`editor-core/src/transform.rs` 没有只变换 Flash 中心，而是：

1. 世界刚性变换作用于中心；
2. 既有 `LocalTransform` 转为 2×2 正交矩阵；
3. 世界旋转/镜像矩阵左乘；
4. 再确定性分解为现有 `mirror + rotation_deg`；
5. 重新计算矩阵核对分解结果。

uniform scale 保留。这个方向适合 Rectangle/Obround/Polygon/AM 等非圆 Flash，避免“位置变了但 aperture orientation 没变”的错误。

### 2.3 Line / RectangularSweep 边界正确

- Circular Line：start/end 一起变换，width 保持。
- RectangularSweep：只接受整数 90° 旋转；90°/270° 时交换 width/height；水平/垂直镜像保持尺寸。
- 任意 37° 等不能被当前模型精确表达的矩形扫掠会返回 `UNSUPPORTED_FEATURE`，不会只旋转中心线后伪装成合法结果。
- 90° 使用精确正交矩阵，不依赖 `sin(90°)` 造成微小漂移。

### 2.4 Arc 镜像和语义保存正确

Arc 对：

- start
- end
- center

统一进行世界变换。旋转保持 CW/CCW；镜像明确翻转 CW ↔ CCW。

同时检查并保留：

- full circle / start=end 身份
- zero sweep 身份
- start radius / end radius
- arc deviation
- source 语义

没有用 tessellation 或重新拟合圆心反推制造圆弧。

### 2.5 Region 保持完整对象变换

Region 的所有 contour/edge 在克隆候选模型上一次性变换：

- Line edge 两端一起变换；
- Arc edge 复用 Arc 刚性变换；
- 镜像时 Arc direction 翻转；
- edge order 不变；
- 最终重新执行制造几何验证。

任何 edge 失败时，`modify_objects` 尚未 commit，所以不会留下半个已变换 Region。

### 2.6 Undo/Redo 原子性方向正确

Rotate/Mirror 继续保存 before/after manufacturing geometry，由 Undo/Redo 恢复真实存储状态，没有通过反向旋转或再次镜像计算撤销。

候选对象全部修改、验证和历史预算检查完成后才 commit；混合选择中一个对象不支持时整批失败。

## 3. 本轮发现的问题和技术债

以下问题不阻塞 S1-B2b 阶段通过，但应在进入 GUI 前处理。

### D-01：制造模型仍混入 workspace/editor 状态 —— pre-GUI 必须修

当前：

```rust
pub struct SemanticLayer {
    pub locked: bool,
    pub id: String,
    pub name: String,
    pub objects: Vec<SemanticObject>,
}
```

同时 `content_hash()` 直接序列化整个 `SemanticDocument`。

未来 GUI 一旦开放图层锁定或重命名，会产生两个错误后果：

1. 纯编辑器状态可能改变 manufacturing dirty；
2. 当前 `EditHistory::targets()` 和 `check_transaction()` 都读取 `layer.locked`，因此用户在编辑后锁定图层，会导致原本合法的 Undo/Redo 被当前 workspace lock 阻断。

ADR 0011 已经正确识别这个问题，但尚未实施。

**要求：进入正式图层面板前，把 locked/display name/visible 等迁到 service/workspace state。制造模型只保存会影响 Gerber 输出的状态。Undo/Redo 不受当前 workspace lock 阻断；workspace lock 只限制新编辑命令。**

### D-02：当前 GUI 仍是 S0 技术演示，不能继续“打补丁”当正式 GUI

`editor-app/src/main.rs` 仍然：

- `include_bytes!` 固定加载 `s0_polarity.gbr`；
- 标题写着 `S0 validation`；
- 使用旧 `DocumentSnapshot`；
- GPU preview 只允许 4 layers / 16 objects；
- 只渲染旧 CircleFlash/Line；
- UI 明示 `No editing or export capability`。

这是正常的历史 reference renderer，但它已经和 S1-B2b 服务模型分叉。

**建议保留为开发期 correctness reference，不把它逐项扩展成最终 GUI。S2 应建立新的 S1/S2 GUI shell，通过 ApplicationService 打开真实文档。**

### D-03：正式点选前还缺全几何 hit-test / bounds 服务

当前 `objects.query` 的 rectangle relation 对 Arc/Region、部分 transformed/non-circle Flash 会返回 `UNSUPPORTED_FEATURE`。

这对于脚本的有限查询是安全的，但不够支撑 CAD GUI 的：

- 鼠标点选；
- 重叠对象循环选择；
- 框选；
- fit-to-window；
- 选中高亮。

进入 S2 时需要在 core/service 建立基于真实 f64 manufacturing geometry 的 hit-test 与 bounds；不能让 GUI 用 GPU 像素、Mesh 或 AABB 候选冒充最终几何命中。

### D-04：证据 run 的 Git HEAD 与最终包 commit 不一致

`PACKAGE_INFO.json` 声明最终 commit：

`23046c08093ba55c11bd2482d80e5cd06ce50a4f`

但最终公共 run 的 `environment.json` 中 `git rev-parse HEAD` 为：

`256dbc97cdb99841f4bd32e81819c27f51f52e21`

这是 S1-B2a 起点 commit。

好消息是 `tested-source-hashes.json` 的 33 个关键文件与当前归档 33/33 匹配，因此**源码内容绑定仍然可信**；但“最终 commit 本身经过最终 run”这一表述不够严谨。

下一阶段证据流程应改成：

1. 先形成候选 commit；
2. 确认 `git status --porcelain` 为空；
3. 执行 final gates；
4. 记录该 commit SHA；
5. 如测试后仅修改证据/说明，再单独区分 code commit 与 package commit。

### D-05：`PACKAGE_INFO.json` 的删除说明与实际包不一致

`PACKAGE_INFO.json` 的 `uncommitted_deletions_excluded` 列出：

- `RCam_MAC_FIRST_S1B2_NEXT_TASK.md`
- `RCam_S1B1_6b5d9c0_Review.md`

但两个文件实际上都存在于 ZIP，并且进入 `PACKAGE_MANIFEST.sha256` / `MANIFEST.sha256`。

这是打包元数据错误，不影响产品代码，但应在下一次归档修正。

### D-06：README 有一处阶段描述过时

README 当前仍写到 “S1-B2b Duplicate/Delete workflows are implemented”。Duplicate/Delete 是 S1-B2a，S1-B2b 新增的是 Rotate/Mirror。属于文档小问题。

### D-07：结构编辑 O(N) order guard 继续保留为后期性能债

Duplicate/Delete 仍保存全层 `before_order/after_order`。现有资源预算能安全拒绝大图层结构操作，因此不是正确性缺陷。

在 Production Renderer / 大文件编辑阶段前，再按 ADR 0011 替换为结构版本、固定大小摘要和局部锚点即可。本轮无需返工。

## 4. 是否可以进入下一步

可以。

推荐顺序：

```text
S1-B2b   Rotate/Mirror       ← 本轮通过
   ↓
S1-B2c   pre-GUI model hygiene
   ↓
S2-A Mac GUI foundation
   ↓
Open file / Layer panel / Fit / Pan-Zoom
   ↓
Point select / Highlight / Numeric Move
   ↓
Undo / Redo / Save As
   ↓
S2-B/S3 鼠标拖动、多选、框选、Grid/Snap/测距
```

**不要再继续扩展 Parser/Writer 或无界面几何操作；下一项最高价值工作已经是让现有正确的编辑核心安全进入 GUI。**

## 5. S1-B2c 的退出条件

只有以下条件完成后才进入正式 S2 GUI：

1. manufacturing state 与 workspace state 分离；
2. lock/name/visible 不影响 manufacturing dirty；
3. 当前 lock 不阻断 Undo/Redo；
4. 新编辑仍尊重 lock；
5. `layer.update` 或等价 workspace API 有清晰 revision 语义；
6. 文件导出不序列化 workspace state；
7. S1-B1/B2a/B2b 全部回归保持；
8. final evidence run 的源码/commit/package 身份一致；
9. 更新 README、PACKAGE_INFO 和阶段索引；
10. Windows 继续 deferred，不作为当前退出门禁。

之后即可进入 Mac GUI 基础功能。
