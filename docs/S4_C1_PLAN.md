# S4-C1 Full Object Snap 计划（Mac-first）

## 范围

本阶段只交付制造几何 Object Snap。关联 R08/R09/R13/R17/R18；局部覆盖 AT-025/030/032/044/045/062/074/075。允许修改 `editor-core` 捕捉/命中几何、`editor-app` 工具与 UI、`rcam-project` Snap settings、`editor-service` 项目默认值及对应测试/文档。

不做 Grip Editing、Block Editor/library、Array/Panelization、Alignment command、Drill、PnP/RefDes、Windows、P100K，也不启动 S4-C2。

## 可运行闭环

```text
raw pointer
→ physical-pixel radius
→ WorldIndex nearby objects
→ analytic manufacturing features / bounded intersections
→ Object + Grid resolver
→ stable world point + marker/status
→ Direct Drag / Text / Measure / Pick Base Point
→ one existing ApplicationService transaction on commit
```

## 验收切片

1. Rectangle/Circle/Polygon/Obround/Line/Arc/Region/AM/CompatibilitySolid/BlockInstance 解析特征。
2. Endpoint/Vertex、Midpoint、Center、Quadrant、Intersection、Nearest；稳定 ID、距离优先与 hysteresis。候选生成半径固定覆盖 11 physical px；resolver 仅允许 8 px 内 acquire，并允许同一候选在 11 px 内 retain。
3. visible/selectable/class 过滤；locked selectable 可捕捉；Object 高于 Grid；Alt 临时禁用；F3 开关。
4. 8 physical px 默认半径与 Retina ppp；状态坐标复用 mm/in/mil/µm formatter。
5. Direct Drag、多选共同位移、文字浮动放置、测距、Pick Base Point 共用 resolver；预览与提交同一点。
6. `.rcam v1` 设置往返和旧字段缺省兼容；不保存当前候选/hysteresis。
7. 10×1K 邻域、100K 空间索引与 1000-edge Region 的有界性能证据。
8. Mac 原生 UI/Metal、机器可读捕捉观察、release build 和固定 source/evidence ZIP。
9. Runtime 回归必须覆盖静态特征、Nearest、线线/线弧 Intersection、无 previous 的 9 px 拒绝、双候选切换与 Retina；不得只测 core resolver。

## 阻断规则

任何制造语义、B0 数据安全、Object/Grid 解析、原生验证或 clean-package 门禁失败都不得标记 PASS。Windows 始终为 deferred / not executed，且不能由 macOS 或 headless 测试替代。
