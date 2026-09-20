# ADR 0021：Viewport preparation 与 native performance evidence

日期：2026-09-20；状态：已采纳（测试结果由独立 evidence 确认）。
范围：Mac-first S2-B3.2，R/AT 与允许模块见 ../S2_B3_2_PLAN.md。

保留 camera-independent global RenderIndex 和全局 GPU buffer。camera 的 f64 world bounds 扩展 2 physical px，覆盖 1.5px selection edge 与 AA；cell 查询再加一格保守 halo。CSR union 排序去重后按原 object/exposure 顺序计算预算，不按 center 排除跨界对象。全局 bounds 加 f32 运算保护；shader 在整个场景外返回透明，避免 clamp 边缘 cell 污染。

Selection flags 在 worker View 变化时计算、以 Arc 身份缓存 GPU 上传。普通导航只遍历当前 candidate IDs；不重建基础 index。非零 preview 仍同步重建全局 index，单独测 P1K/P10K/P100K 各60次。保持原 2e9 work 限额；P100K 固定视口若拒绝则记录 BLOCKED 和独立同输入 index build 时间，不把提前拒绝当完整渲染性能。若扩展样本造成超过50ms CPU stall，下阶段必须 versioned background/incremental 或 selected-only overlay，不提高门槛。

RCAM_NATIVE_BENCH=s2b32 仅 release 显式启用，RCAM_BENCH_OUT 必须指向新的输出目录。使用同一个 eframe window/device/production callback，RawInput 注入真实 gesture 路径，制造操作经过原 ApplicationService。异步尺寸校准后实际 canvas 固定1600×900；预热10秒，活动导航30秒，连续drag10秒×3。活动边界在输入状态机确定；不事后过滤慢帧。下一帧开始等待 surface GPU fence，结合 production callback frame stamp 验证画过目标 revision；release latency 是提交/present 后 GPU 完成的保守上界，不声称显示器扫描时间。benchmark 额外同步会影响吞吐，正常产品不启用该 poll。

所有活动帧保留 frame ID/focus/canvas/CPU/candidates/fence 数据；缺帧、无 callback、失焦、尺寸偏差、history/坐标/metrics/writer 不变性失败均 FAIL。Python 独立重算 p95 并核对结果。截图来自 egui surface screenshot event，不以离屏截图代替。

P1K native 通过仅能关闭本轮所定义 renderer/performance 阻塞；不等于 PMIX 完整 AT-075、CORE10 完整流程、Windows 或双平台 V1 通过。没有新增产品依赖、公开业务 API 或制造模型变更。
