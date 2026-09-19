# ADR 0020：有序 world-space 显示索引

2026-09-20，S2-B3.1。仅 editor-app 派生显示数据，不改制造、命中、选择服务、Metrics 或 writer。

`reference.wgsl` 保留此前逐像素全对象解释器，继续执行独立制造覆盖探针；生产使用 `editor.wgsl`。
两者使用相同显示 primitive、相机、四样本 AA、颜色/层合成及 1.5px 选择边缘。
新增原生 Metal 测试比较完整 RGBA 字节，容差为零；覆盖圆、孔洞、Macro、Arc、Region、
曝光顺序、上层 Clear、单选/全选、3/-2mm preview，并含原冻结 P1K 样本。

RenderIndex 按场景边界、长宽比和对象数选择网格，无固定毫米格距。每格 CSR 候选按 scene
顺序插入；全局 Dark/Clear 和层顺序不重排。对象 AABB 仍作二级早退，局部材料函数保持。
最大 16384 格、1000000 引用、每格 16384 对象；引用超限逐步折半，最终不能有界则 RESOURCE_LIMIT。
索引输入最多 200000 对象，所有计数在分配前受控。原显示 primitive/points 200000 上限仍在。
错误包含 resource/limit/actual；候选采样工作上限保持 2000000000，不再乘全场景对象数。
估计采用物理像素×最大单格候选×20，加裁剪后对象材料预算；该保守估计仍可能拒绝密集重叠。
索引缓冲最大约 4.1MB，已有几何缓冲受 200000 display items 限制；不新增无界 GPU 缓冲。

基础索引跟随不可变 Scene，由原后台 worker 构建。相机改变不参与索引身份；跨 LOD 的 Region
使用 canonical edge 的保守、固定包络（圆弧取全圆包络），相同 bounds/对象顺序/显隐/anchor
复用原 Arc<RenderIndex>。LOD 细分仍可在原 worker 更新，不重解析、不修改制造几何。
preview 按已选对象平移后的 bounds 重建临时显示索引，不修改制造坐标/历史；释放仍复用原单事务。
首版 preview 构建在回调准备前同步执行，200000对象/1000000引用是已知上限；P1K实际耗时
单独采集，后续大样本可把 preview index 转为有版本取消的后台/增量更新，不宣称P100K已通过。

选择边缘从四个采样点查询候选，用四路有序归并去重，不重新全扫描 selection。
最初 helper 提前返回及重复循环在 Metal 精确边缘存在像素差异，改成同序单次材料计算后通过
完整 RGBA 对照；保留零容差门槛，未把差异掩盖成抗锯齿容差。

本阶段无新依赖。未来 renderer 可升级层 tile/实例化，但须继续通过 reference 和独立几何真值。
P1K 离屏 CPU+GPU fence 计时仅性能补充，不等于 native GUI frame interval 或 AT-075。
Windows deferred / not executed，完整 V1/CORE10 和 P100K 门槛不变。
