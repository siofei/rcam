# S5-M2-C PMIX acceptance addendum

2026-10-04 resumed on accepted 34ccaa42. Every resumed-source item is NOT EXECUTED until attached evidence proves it. Original failures remain historical.
Stage S5, requirements and permitted modules are frozen in S5_M2_C_PLAN.md.
ADR 0053 defines the workload. This addendum does not change schema_version=2,
the 96 active acceptance cases, required_platforms, AT-079 retirement or V1 gates.
Mac-first bounded only; Windows, full CORE10, K1 native debt, CircuitCAM 4.4,
PPOL, PSTRESS and DeviceRecovery are not closed here.

## Local executable matrix

| Local case | Required evidence | Gate |
|---|---|---|
| C01 | Source SHA; independent full 100k object coordinates/types/IDs/order; 160k Region edges; arc direction; aperture parameters; bounds | Exact counts/identity; f64 coordinate tolerance 1e-9 mm |
| C02 | Analytic material/empty witnesses; convex and concave Region area; aperture and local-hole truth | All frozen witnesses correct |
| C03 | Actual native Metal production/reference RGBA at fixed mixed crops, zooms and preview selections | Existing zero-mismatch rule; independent ordered reference |
| C04 | Real mixed 1000 selection/move/Undo/Redo plus interruption; independent translated expectation | One transaction; unchanged other objects/definitions; exact Undo |
| C05 | 200 frozen point queries and full-box source-order result | CPU p95 <=20ms; visible highlight p95 <=100ms; box ready <=300ms |
| C06 | Small fixed project functional matrix, including layer/Solo/lock and complete text/block selection | All actions observed; view-only state leaves manufacturing/export unchanged |
| C07 | Real project save/reopen and safe per-layer Gerber export/reopen | IDs/styles/precision retained in project; correct flattened manufacture; no source overwrite |
| C08 | Existing lifecycle/drag regressions and scene/selection/project fences | No stale or partial application |
| C09 | Three native 10s warm +60s navigation runs, 1600x900 physical, actual 60Hz | p95 <=50ms, p99 <=100ms, max <=200ms; input-GPU p95 <=100ms |
| C10 | Three native 10s warm +10s 1000-object drag runs | Same frame gates; release/Undo/Redo visible completion <=300ms each |
| C11 | Peak process RSS and custom GPU buffers, raw observations | <=1GiB RSS; <=512MiB custom buffers |
| C12 | Portable verifier and semantic rehashed negative cases | Authentic evidence passes; each isolated corruption fails; restored evidence passes |
| C13 | All mandated build/service/Python/native gates and independent review | No missing mandatory result; final source/binary/commit/archive binding verified |

Import completion duration is report-only, with no invented PMIX load SLA.
The local query budget borrows unchanged P100K numbers but does not relabel AT-074.
The analytic renderer has zero object triangles and one fullscreen triangle per draw;
report actual primitives/points/bin data instead of inventing tessellation counts.
Small native reference crops do not replace the full 100k performance workload.
Limited Dark/Clear/layer checks do not satisfy PPOL dense-overlap performance.

## Existing-case traceability

The titles below are read from the current machine-readable baseline. Each mapping
means regression relevance or bounded local coverage, never full acceptance of the
entire case. In particular AT-025 monitor switching, AT-015 all hole forms, AT-047/050
all fonts and AT-073 PPOL remain outside this local fixture's coverage.

| Existing case | Current title |
|---|---|
| AT-001 | 从干净环境构建并运行 |
| AT-003 | 规范、硬件和能力矩阵冻结 |
| AT-010 | C／R／O／P 标准 Flash |
| AT-011 | 标准光圈孔洞的局部透明语义 |
| AT-012 | 线段扫掠、端帽和边界 |
| AT-013 | 顺逆圆弧、跨象限与边界 |
| AT-015 | Region 的凹轮廓、多轮廓和合法孔洞 |
| AT-017 | Dark→Clear→Dark 顺序合成 |
| AT-018 | 图层之间的 Clear 不相互擦除 |
| AT-021 | 同类型图层独立存在 |
| AT-022 | 显隐和锁定 |
| AT-023 | 平移与适合窗口 |
| AT-024 | 以鼠标位置为中心缩放 |
| AT-025 | DPI、面板裁剪与显示器切换 |
| AT-026 | 精确点选不只看 AABB |
| AT-027 | 清除对象与遮挡后的选择语义 |
| AT-028 | 重叠对象选择顺序稳定 |
| AT-029 | 左向右包含框选与右向左交叉框选 |
| AT-030 | 编辑后空间索引不滞后 |
| AT-031 | 数值移动及异常数值拒绝 |
| AT-032 | 拖动预览、Esc 取消与失去捕获 |
| AT-033 | 任意角旋转和旋转中心 |
| AT-034 | 镜像圆弧及重复变换 |
| AT-039 | 批量操作的原子性 |
| AT-040 | Undo/Redo 分支 |
| AT-041 | 保存基线与脏标记 |
| AT-047 | ASCII 字符生成并写入 Gerber |
| AT-050 | 字高、对齐和制造曲线误差 |
| AT-052 | 文字分组与未包含能力说明 |
| AT-054 | 编辑后的完整保存闭环 |
| AT-055 | 量化后的圆弧和坐标边界检查 |
| AT-056 | Writer 的定义、状态和结束标记 |
| AT-061 | 一个图层一个文件，不暗中合并 |
| AT-062 | 不同缩放与显示状态不影响导出 |
| AT-063 | 加载和几何生成不阻塞 UI |
| AT-065 | 旧异步结果不会污染新文档 |
| AT-067 | 缓存失效范围与曝光顺序 |
| AT-073 | 混合图元和有序极性性能 |
| AT-075 | 一千对象拖动与提交性能 |
| AT-083 | 验收报告不伪造、不跳过门禁 |
| AT-086 | 服务无窗口／GPU依赖且完成真实编辑往返 |
| AT-087 | GUI与服务共用业务实现并产生一致几何 |
| AT-090 | 接口校验与GUI一致且失败零修改 |
| AT-091 | 修订冲突与Undo后的旧请求不会重新有效 |
| AT-092 | 多命令批次原子提交、回滚与单次撤销 |

## Evidence and closeout

Retain raw failed runs. Do not trim frame tails, drop transition frames, replace
fixtures, lower thresholds or silently switch display conditions. Raw input/camera,
worker shared clock, current scene identity and actual GPU-completed callbacks must
support the causal chain. Frame deletion followed by renumbering and rehashing must
still fail verification. Every run binds the exact fixture, source manifest and
binary hash; semantic snapshots must permit independent geometry checks.

Only the parent independent reviewer may approve the dirty candidate for a stage
commit. No push. Rebuild committed clean source in a fresh target, produce matching
Source/Evidence archives, verify fresh extraction and obtain final package review.
Until then stage status remains IN PROGRESS, even if selected local tests pass.
