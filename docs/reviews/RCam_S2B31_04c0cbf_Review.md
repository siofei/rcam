# RCam S2-B3.1 `04c0cbf` 复审报告

> 审查对象：`RCam_S2B31_04c0cbf_source.zip` + `RCam_S2B31_04c0cbf_public_evidence.zip`  
> 受测提交：`04c0cbf13d803d9fffa1a2ebcc1b612386c55d5b`  
> 平台策略：Mac-first，Windows deferred  
> 结论：**生产 renderer 的主要结构性瓶颈已经修复；S2-B3.1 实现可以继续保留，但阶段仍不宜签署完整通过。下一步应补真正的 Viewport Culling，并用应用内原生 benchmark 闭合固定画布 Pan/Zoom 与 10 秒 ×3 拖动门禁。**

## 1. 本次实际复核

- Source ZIP SHA-256：`dd6b0ea8c0ae7304ff330d11337013d8425c70601c70e7b8a0e05f8e5bf30ad3`，与 public evidence 的 `package-verification.json` 一致。
- Public evidence ZIP SHA-256：`2116a5f04ef73527b5793c3175f1dece6a6af130d4e73b8e8c6d77e328530cf4`。
- `python3 scripts/source_manifest.py --check`：`PASS current source manifest: 1539 files`。
- `python3 scripts/test_package_source.py`：PASS。
- `python3 scripts/test_audit_core10.py`：7/7 PASS；这只是审计脚本，不等于 CORE10 全流程通过。
- `EVIDENCE.sha256` 校验通过。
- `source-before.json`、`source-after-gates.json`、`source-after-native.json`：三者均为同一 HEAD，1540 个跟踪文件 hash 完全一致；与本次 Source ZIP 对应文件 1540/1540 一致。
- 当前环境没有 Rust / Metal 运行条件，因此 evidence 中的 Cargo / native GPU 结果属于“原始日志 + commit/hash 身份复核”，不是本次重新执行。

## 2. Renderer 结构修复评价

### 2.1 已完成的关键改造

当前生产 renderer 已不再是“每个 pixel × 全部 objects”。新增 `RenderIndex`：

```text
Scene Objects
    ↓
World-space grid
    ↓
CSR ordered candidate lists
    ↓
Fragment 只读取当前 world-cell 候选
```

其正确性约束做得合理：

- 200000 display object 上限；
- 16384 grid cell 上限；
- 1000000 cell reference 上限；
- 16384 objects/cell 上限；
- candidate list 按原 scene/exposure 顺序插入，不按 aperture / polarity / layer 重排；
- Dark/Clear 和 layer isolation 仍由原有顺序语义控制；
- 一格 halo 用于保护 CPU/GPU f32 grid 边界；
- 超限明确返回 `RESOURCE_LIMIT`，没有通过漏画降成本。

这已经解决上一阶段 P1K 因 `pixels × all objects` 被直接 budget 拒绝的问题。

### 2.2 Reference Renderer 保留正确

`reference.wgsl` 没有被删除，production / reference 通过独立 Metal 测试做完整 RGBA 对照。

Public evidence 记录：

- 15 个公开场景；
- 3 种 selection；
- 2 种 preview delta；
- 共 90 组；
- 每组 16384 像素；
- RGBA 字节零容差相等。

覆盖 P1K、Dark/Clear、跨层 Clear、孔洞、Macro、Arc、Region 等场景。

这对后续继续优化 renderer 很重要，应继续保留为正确性 oracle。

## 3. P1K 结果

固定样本：1000 个直径 0.5 mm 圆，40×25。

已证明：

- 1000 对象可框选；
- `objects.metrics` = 1000 exact / 0 unsupported；
- 面积和周长与独立公式相符；
- Pan/Zoom/preview 不改变 metrics；
- writer bytes 不因导航/preview 改变；
- drag release 只产生一个制造事务；
- Undo 恢复原坐标。

1600×900 release 离屏、全 1000 selected preview 三轮各 10 秒：

```text
round 0 p95 = 43.459208 ms
round 1 p95 = 40.989208 ms
round 2 p95 = 40.292917 ms
```

三轮均低于 50 ms 门槛；但它是 CPU prepare + GPU fence 的离屏补充数据，不等于 native GUI AT-075。

## 4. 仍存在的关键问题

### D-01：目前还不是真正的 Viewport Culling

当前 `RenderIndex` 是整个场景的 world-space index，camera 改变时复用同一个 index。这一点有优点：Pan/Zoom 不重建索引。

但它意味着：

```text
屏幕外对象
→ 仍存在 scene.objects
→ 仍进入全局 RenderIndex
→ 仍占 GPU object/primitive/index buffer
```

Shader 对屏幕当前 pixel 只查询对应 cell，所以**视口外对象通常不会参与 pixel 的 material evaluation**；但 CPU 端仍没有完全做到“视口外对象不参与本帧准备”。

`gpu::prepare` 目前仍：

```rust
for object in scene.objects.iter() {
    ...
}
```

每帧扫描全部对象做 projected bounds / primitive cost / budget 计算。

因此 50 万 / 20 万对象的大文件即使只看局部，CPU prepare 仍然是 O(total objects)。

此外 budget 使用：

```text
pixels × index.max_candidates
```

这里 `max_candidates` 是**整个场景所有 cell 的全局最大值**。如果视口外存在一个极高密度 cell，而当前视口非常稀疏，当前 viewport 仍可能被这个屏幕外密集区拖累并触发 `RESOURCE_LIMIT`。

这与期望的“可视区域外不渲染”仍有差距。

### 建议修法

保留全局、camera-independent RenderIndex，但增加一个轻量 `ViewportRenderSet`：

```text
Camera + Physical Canvas
        ↓
Viewport world bounds + AA/selection margin
        ↓
RenderIndex visible cell range
        ↓
有序 union / dedup candidate ids
        ↓
visible_max_candidates
visible material budget
```

这样：

- 不必每次 Pan/Zoom 重建全局 index；
- 预算只看 viewport 相关 cell；
- CPU projected/material cost 只遍历 visible candidate ids；
- dense offscreen cluster 不再导致当前 sparse viewport 被拒绝；
- candidate ids 仍保持原 scene/exposure order。

首版不强制重打包 GPU object buffer；保留全局 object buffer 也可以。重点先消除每帧 CPU 全对象扫描和全局 max-cell budget。

### D-02：Preview index 仍同步重建全场景

当前非零 drag delta 时：

```rust
RenderIndex::build(&scene.objects, &selected_flags, delta)
```

也就是每个 preview frame 都重新遍历整个 scene 构建索引。

P1K 的 CPU prepare 数据很低，因此 1000 对象可接受；但代码自己也正确记录了：200000 对象时可能卡 UI。

暂不建议立即实现复杂的双 index shader，但下一阶段至少需要：

1. 对 P10K / P100K 采集 preview-index build 时间；
2. 明确阈值；
3. 超过阈值后迁移到 versioned background / incremental preview index，或 selected-only preview overlay index；
4. 不允许把 pointer move 直接变成同步 O(N) UI stall。

### D-03：Native 性能门禁尚未闭合

Public evidence 本身正确保留了两项 B1：

- 原生 10 秒持续 drag ×3 未执行；
- 原生固定 1600×900 的 Fit/Pan/Zoom 未完整执行。

原因是外部 CUA 输入能力不足，而不是产品代码无法做到。

下一轮不要继续依赖人工 CUA 来闭合这两个性能门禁，建议增加**仅测试/benchmark 模式的应用内原生驱动器**：

```text
RCAM_NATIVE_BENCH=1
→ 固定 window/canvas
→ 自动 Fit
→ 自动 Pan/Zoom 轨迹
→ 自动持续 10 秒 preview drag ×3
→ release
→ 等待最终 frame
→ 输出 frame interval / prepare / GPU / release latency JSON
```

要求仍运行真实 `editor-app` + Metal + production renderer，不得用 headless service 或离屏 test 冒充 native GUI。

## 5. 非阻塞观察

- `RenderIndex` 当前以 scene bounds 和 object count 自动决定 grid，适合作为第一版通用 spatial bin。
- 一-cell halo 会增加 references，但对正确性更稳；现有 1M references 边界需在更大样本上观察。
- Region 采用保守 bounds 会增加 candidate 数，这是正确的 fail-safe；以后只能优化 bounds，不能缩小到可能漏图。
- `max_candidates` 的全局保守估计是目前最值得优先做 viewport 化的部分。
- 当前没有理由回退 GeometryMetrics 或 Selection 实现。

## 6. 阶段判断

### 可以确认

- S2B3-LARGE-SELECTION 的“1000 对象 renderer 直接预算拒绝”结构性阻塞已经解除。
- Production renderer 的 world-bin 方向正确。
- 对 reference 的零容差 parity 证据很强。
- P1K 全可见场景不再每 pixel 遍历 1000 objects。
- 当前实现值得继续沿用，不需要推倒重写成传统 mesh renderer。

### 不能确认

- S2-B3.1 完整通过；
- AT-075 完整通过；
- P100K 性能；
- 真正 viewport-only CPU render preparation；
- Windows；
- CORE10 完整流程。

## 7. 下一步

建议下一阶段命名：

**S2-B3.2 Viewport Culling + Native Performance Gate Closure**

只做：

1. Viewport world bounds / margin；
2. visible cells / visible ordered candidates；
3. viewport-local candidate max / budget；
4. 消除 `gpu::prepare` 每帧全 scene object 扫描；
5. dense-offscreen-cluster 回归；
6. 应用内 native benchmark harness；
7. 固定 1600×900 Pan/Zoom；
8. 原生 10 秒 drag ×3；
9. release→final-visible latency；
10. preview full-index build 的 P10K/P100K 数据与下一步策略。

本轮不要加入 Grid/Snap、文字、Final Layer Area、DXF/SVG/PLT 或 Windows。
