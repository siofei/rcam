# ADR 0014：S2-A.2 f64 对象几何点选

阶段 S2-A.2（Mac-first）。需求 R04/R05/R09/R10/R11/R19/R21/R22；关联
AT-010/011/012/013/014/015/016/026/027/028/030/039/040/088/089/090/095 的局部步骤。
允许修改 editor-core 几何查询辅助模块、editor-service DTO/分派/能力表、相应测试、
设计/API/验收文档和源码证据清单。保留 S2-A.1；不新增依赖、不改 parser/writer 支持范围，
不实现 GUI。使用 ponytail 原则复用既有圆弧 canonical-circle、Region canonical-contour 和历史。

## 公共契约

`objects.hit_test` params 为 `{layer_id, point:{x_mm,y_mm}, tolerance_mm}`。
point 不接受额外字段。请求使用既有 api_version=1/request_id/document_id 信封，拒绝
expected_revision。结果 `{document_id,revision,layer_id,object_ids}`，与信封 revision 一致。
只读、不改变制造/工作区版本、dirty、历史或路径；按当前 layer.objects 顺序返回稳定 ID。
未知文档/图层 NOT_FOUND；空层返回空列表。几何查询不读 workspace，不做最终可见曝光合成，
Dark/Clear 独立可命中。GUI 将来自行决定是否调用隐藏层，不改变服务语义。

point 各轴绝对值不超过 1e9 mm；tolerance 有限且位于 [0,1e9] mm，否则 INVALID_ARGUMENT。
命中定义为到对象材料闭包的最短距离 <= tolerance；材料内部和边界命中。
只允许 f64 运算量级的舍入误差，不额外添加固定 1e-6 选择带；计算不确定性超过原
EPSILON_MM 时拒绝 UNSUPPORTED_FEATURE。tolerance 不修改制造模型，不进入 writer。

## 几何实现

- Flash 逆变换 point 与 tolerance 到 aperture local space，距离回到世界毫米比较。
  标准孔严格位于外形内部，使用真实外形距离/圆孔边界距离。R/O/P 不用 AABB 代替。
- Macro 先解析成旋转/偏移后的圆和多边形。各边界按解析交点切分，区间两侧分别按
  primitive 顺序计算 Dark/Clear；保留至少一侧有最终材料的边界区间（内部区间本身也属于
  材料，不会扩大材料闭包），再计算点到该线段/圆弧
  的最短距离。不能复用保守 Dark 包络，也不能把被后续 Clear 擦除的内部边界算入 tolerance。
  数值不可分辨的接近/近重合情况 fail-closed，不猜拓扑。只做每次查询内的共享光圈准备，
  不维护跨 revision 缓存。查询工作预算 2,000,000，超限 RESOURCE_LIMIT，整次不返回部分 ID。
- 圆形 Line 用无固定长度阈值的点到线段距离，零长度才按圆点。
- RectangularSweep 是两端矩形的凸包（精确 Minkowski sweep），支持算法层面的斜向；
  parser/制造编辑仍保持已冻结的轴向支持范围，不借查询放宽导入/输出。
- Arc 复用平均半径圆弧与原端点径向接线，距离包含 sweep/方向、圆端帽、整圆和零扫掠。
- Region 复用 canonical contour 的原有非零 winding/cut-in 语义，聚合填充与真实边距。
  保持原无效 Region 拒绝边界，不引入 SVG 规则或显示细分。

## 验证和身份

独立数值断言覆盖全部几何、孔洞、Macro 有序布尔边界、欧氏 tolerance 和退化线段；
真实 JSON/文件回归覆盖 Move/Duplicate/Delete/Rotate/Mirror/Undo/Redo 和 workspace 独立性。
冻结已有 S2-A.1 与本轮代码后提交，在 clean tracked/untracked 状态执行 Mac final gates，
保留 commit、源码哈希、原始日志与 schema_version=2 局部结果。原 96 用例、阈值、
required_platforms 不变；Windows deferred / not executed，本轮通过不等于完整 GUI/V1。
外部 RCam_S2A1_20260916_Review.md 当前未提供；不引用或伪造其结论。
