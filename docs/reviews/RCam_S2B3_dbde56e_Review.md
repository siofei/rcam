# RCam S2-B3 `dbde56e` 源码复审

> 审查对象：`RCam_S2B3_dbde56e_source.zip`  
> 外部阶段报告：`S2_B3_REVIEW.md`  
> 审查日期：2026-09-20  
> 当前策略：Mac-first；Windows deferred / not executed。

## 结论

**S2-B3 的 GeometryMetrics、lazy cache、服务 API、属性面板和 UTF-8 source package 实现可以保留，不需要返工。**

但本轮不建议签署“完整 S2-B3 通过”。外部报告已经把 `S2B3-LARGE-SELECTION` 标为 B1：固定 1000 对象样本会被现有 renderer 的 pixel/object budget 拒绝。源码审查确认这不是测试样本偶发问题，而是当前 renderer 的结构性限制。

因此建议状态为：

```text
S2-B3 GeometryMetrics 功能闭环      PASS
S2-B3 UTF-8 Source Package         PASS
S2-B3 1000 对象原生交互门禁       BLOCKED (renderer)
完整 S2-B3 阶段                    NOT SIGNED
下一阶段                           S2-B3.1 Scalable Renderer
```

在 S2-B3.1 清掉该 B1 之前，不建议继续扩张 Grid/Snap、文字、Final Layer Area 或多格式能力。

---

## 1. 本次独立复核范围

本次实际执行：

```text
python3 scripts/source_manifest.py --check
python3 scripts/test_package_source.py
python3 scripts/test_audit_core10.py
```

结果：

```text
source manifest: 1474 files PASS
source package UTF-8/deterministic roundtrip: PASS
CORE10 audit tool tests: 7/7 PASS
```

原始 ZIP SHA-256：

```text
055445b0a3e20547817f03dc517342199d9bc44f95129edaaa224b8a9ba974d7
```

另外直接检查原始 ZIP：共有 135 个非 ASCII 路径，135/135 均设置 UTF-8 filename flag，前一阶段中文路径打包问题已修复。

当前审查容器没有 Rust/Cargo，因此**没有重新执行**外部报告中的 324 项 Rust 测试、Metal ignored tests 和 Mac GUI 人工流程。该部分只核对源码是否与报告描述一致，不能把报告中的本机成绩描述为本次重新执行结果。

本次 source ZIP 内没有 `evidence-public/s2b3-final-20260920/`；最终 S2-B3 原生 evidence 由用户单独提供的 `S2_B3_REVIEW.md` 描述。后续若希望单 ZIP 可独立复审，建议 source 包之外再交付一个 public-evidence companion ZIP，或把脱敏 public evidence 一起打包。

---

## 2. GeometryMetrics 实现审查

### 2.1 真值来源正确

`crates/editor-core/src/metrics.rs` 直接读取 Manufacturing Geometry：

```text
SemanticGeometry
    -> analytic metrics
    -> GeometryMetrics { area_mm2, perimeter_mm }
```

未从：

```text
GPU Mesh
显示 tessellation
屏幕像素
LOD
```

反推制造面积/周长。

符合 ADR 0018 的边界。

### 2.2 Standard Flash

已实现：

- Circle
- Rectangle
- Obround
- Regular Polygon
- circular hole
- LocalTransform uniform scale

孔洞执行：

```text
Area = outer - hole
Perimeter = outer boundary + hole boundary
```

rotation / mirror 不改变 metrics；scale 按 `s² / |s|` 规则处理。

Macro 明确 `Unsupported`，没有把 Dark/Clear primitive 面积错误相加减。

### 2.3 Line / RectangularSweep

圆形 Line 使用真实 capsule：

```text
A = L*w + πr²
P = 2L + 2πr
```

零长度作为圆点。

RectangularSweep 复用真实制造多边形并用 shoelace + 真边长，不使用 AABB 面积。

### 2.4 Arc

当前采用 fail-closed：

- zero sweep -> dot
- full circle -> annulus / filled disk
- 可证明无自交的 open circular tube -> exact
- arc deviation connector 无法证明 union 边界 -> Unsupported
- `r >= radius`、major arc endpoint caps 可能相交 -> Unsupported

没有重新退化为严格等半径假设，也没有用折线近似冒充 exact。

这一策略适合 S2-B3。

### 2.5 Region

Region 使用 canonical manufacturing edges：

- Line 解析积分
- Arc 解析积分
- 真边界长度
- exact retraced cut-in pair 去除
- winding 判断 outer/hole
- 模糊闭合、分支、touching loop、需要 union proof 的多 contour fail-closed
- `3*N²` 工作量预收费

没有新增一个未经验证的 Boolean Engine。

---

## 3. Cache / Shape Identity

`editor-service/src/metrics.rs` 使用会话内派生 cache：

```text
ObjectId -> shape token
shape token -> Result<GeometryMetrics, MetricsError>
```

确认：

- Move 保留 token；
- rigid Rotate 保留；
- Mirror 保留；
- Duplicate 共享 shape token；
- Delete 从 active 转 historical；
- Undo 可恢复并复用；
- close document 清理；
- 不以 document revision 作为全量 cache invalidation key；
- cache 有 4096 value FIFO；
- identity 有 20000 项 / 2 MiB 上限；
- 超限清派生 cache，只损失复用，不修改制造模型/历史。

这与当前仅有刚性/结构编辑的能力边界一致。

未来若新增：

```text
Aperture 尺寸修改
孔径修改
Region node edit
非均匀 scale
```

必须同步扩展 shape identity / history，不可沿用当前 token。

---

## 4. `objects.metrics` API

接口行为基本符合任务要求：

- 独立只读 API；
- 不偷塞进 `objects.get`；
- 最多 10000 Object IDs；
- duplicate ID 拒绝；
- unknown object -> structured NOT_FOUND；
- 请求顺序稳定；
- Unsupported 是 item 状态，不伪造 0；
- summary 仅合计 exact 项；
- `metrics_work` 超限整次失败，不发布 staged cache；
- 返回当前 document revision，但查询不推进 revision/dirty/history。

GUI 单选显示：

```text
面积：... mm²
周长：... mm
```

多选明确显示：

```text
对象面积合计
对象周长合计
```

并注明：

```text
对象独立指标合计，未进行图层曝光布尔去重
```

没有误称“最终开口面积”。

---

## 5. UI 后台模型

`editor-app` 仍使用串行后台 `Model/ApplicationService` worker。

Metrics 查询触发条件由 document/revision/selection identity 控制；UI 帧只读结果，不每帧重新计算制造 metrics。

这条边界正确。

---

## 6. Source ZIP UTF-8 修复

`scripts/package_source.py` 现在：

- 固定 entry 顺序；
- 固定时间戳；
- 固定权限；
- 只允许 source manifest 内容；
- 排除 `.git/.tools/target/private`；
- 输出 `MANIFEST.sha256` + `PACKAGE_MANIFEST.sha256`；
- 非 ASCII path 由 Python zipfile 以 UTF-8 filename flag 写入。

本次直接检查原始 ZIP：

```text
non-ASCII entries = 135
missing UTF-8 bit 11 = 0
```

前一阶段 mojibake 路径问题可以关闭。

---

# 7. 当前真正的 B1：Renderer 架构

这是本轮最重要的结论。

当前 `editor-app/src/gpu.rs` 中：

```rust
let mut work = pixels * scene.objects.len() as f64;
...
if work * 8. > 2_000_000_000. {
    return Err("RESOURCE_LIMIT: display pixel/object budget ...")
}
```

而 `editor.wgsl` 的 `sample_scene()` 对每个 fragment sample：

```text
for i in 0 .. object_count
    AABB reject
    object_material(...)
```

每个像素 4 个 coverage samples，selection edge 又有额外采样。

因此当前主 renderer 的基本复杂度仍接近：

```text
O(screen_pixels * visible_objects)
```

在 1000 个对象时，单是初始预算就要求：

```text
pixels * 1000 * 8 <= 2,000,000,000
=> pixels <= 250,000
```

也就是说正常 1600×900 物理画布约 1,440,000 pixels 时，**1000 对象必然在预算检查阶段被拒绝**，甚至还没考虑真实 object bounds/material cost。

因此外部报告的 `S2B3-LARGE-SELECTION` 不是偶发性能波动，而是当前渲染算法决定的必然结果。

不能用以下方式“修复”：

```text
把 2,000,000,000 调大
删除 1000 对象样本
缩小验收窗口
关闭 AA
降低几何精度
```

这些只会隐藏结构问题。

---

## 8. 建议的下一步

下一阶段定义为：

```text
S2-B3.1 Scalable Renderer / Large Selection Unblock
```

目标不是做完整 P100K 最终优化，而是：

1. 主应用不再对每个像素扫描全部对象；
2. 保留当前 renderer 为 small-scene/reference oracle；
3. 新 renderer 继续严格保持：
   - layer isolation；
   - Dark/Clear exposure order；
   - aperture local hole / local Dark/Clear；
   - Arc/Region/Macro display semantics；
   - selected preview；
4. 固定 1000 对象样本可以正常打开；
5. 1000 selection metrics 能在原生 GUI 完成；
6. AT-075 的 1000 对象 drag 性能开始具备可测条件；
7. 不改 parser/writer/metrics 真值。

推荐增量路线见配套下一任务。

---

## 9. 文档 carry-over

当前源码包仍有阶段描述滞后：

- `docs/CAPABILITIES.md` 仍把 S2-B3 写成“开发候选”；
- `docs/IMPLEMENTATION_PLAN.md` 仍写“当前任务 S2-B3”；
- `AGENTS.md` 当前活动阶段仍为 S2-B3；
- README 中部分段落仍引用 S2-B2 范围/报告。

下一阶段应在不改 96 个验收用例身份的前提下同步状态。

---

## 10. 最终判定

| 项目 | 判定 |
|---|---|
| GeometryMetrics core | PASS |
| Standard Flash / hole | PASS |
| Line / RectangularSweep | PASS |
| Safe Arc subset | PASS / fail-closed |
| Proven Region subset | PASS / fail-closed |
| Macro metrics | Explicit Unsupported，符合本轮边界 |
| Lazy cache / shape token | PASS |
| `objects.metrics` | PASS |
| 单选/多选属性文案 | PASS |
| Metrics 不参与 writer | 设计/代码边界正确 |
| UTF-8 source package | PASS |
| 1000 对象 native metrics | BLOCKED by renderer |
| 完整 S2-B3 | **NOT SIGNED** |
| 下一阶段 | **S2-B3.1 Scalable Renderer** |

