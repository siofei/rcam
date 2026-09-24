# ADR 0036：制造几何 Object Snap

- 状态：Accepted（S4-C1，Mac-first）
- 日期：2026-09-25
- 关联：R08、R09、R13、R17、R18；AT-025、AT-030、AT-032、AT-044、AT-045、AT-062、AT-074、AT-075

## 背景

S4-B1 只预留了 Snap 类型。S4-C1 需要让拖动、文字浮动放置、测距和相对基点共享一套接近 CAD 的对象捕捉，同时保持制造几何、显示网格和 GPU 数据之间的边界。

## 决策

1. `editor-core::snap` 是唯一捕捉语义：`SnapKind`、`SnapFeatureId`、`SnapFeature`、`SnapQuery`、`SnapCandidate`、`SnapFeatureProvider` 和 `SnapResolver` 不按工具复制。
2. 默认来源是 Manufacturing Boundary；Original Path 是显式高级选项。边界复用 hit-test 的 f64 Line/Arc 解析边，不读取 renderer mesh、tessellation 或像素。
3. 正式类型为 Endpoint/Vertex、Midpoint、Center、Quadrant、Intersection、Nearest；Nearest 默认关闭，Tangent/Perpendicular 仅保留类型。
4. 半径按 physical px 保存，默认 8、允许 4–20。查询以 `camera points/mm × pixels_per_point` 换算成世界半径。
5. 查询顺序为 11 physical px candidate radius → `WorldIndex` 附近对象 → lazy feature generation → 附近边对解析交点 → Grid candidate → 单一 Resolver。Resolver 仍只允许 8 px 内的新候选 acquire，并允许上一候选在 11 px 内 retain；禁止全工程 snap-point 数据库。
6. 只接受 effective visible 且 selectable 的层/分类。Locked 不影响捕捉；编辑权限仍由 ApplicationService 拒绝。
7. Object candidate 在半径内时高于 Grid；候选以屏幕距离为主、类型为小幅稳定权重。已捕获候选在 11 physical px 内保持，避免 8 px 边界抖动。
8. `SnapFeatureId` 在未修改几何内稳定。BlockDefinition 只缓存 `(definition_id, revision)` 的局部解析特征；每个 BlockInstance 只施加 rigid transform，revision 改变即失效。
9. Alt 临时禁用 Object 与 Grid；F3 通过 Command/Keymap 切换 Object Snap。当前候选和 hysteresis 不持久化。
10. `.rcam v1` 的 Snap settings 增加带 serde 默认值的 `manufacturing_boundary` / `original_path` 字段，旧 v1 缺字段时读取为 `true/false`，不改变 format_version。

## 取舍

- 单个对象内部目前用边 bounds 过滤，但没有额外树索引；1000-edge dense Region 以基准数据决定是否继续细分。
- 标准 aperture hole 在 S4-C1 不提供边界捕捉；Region hole 与 AM 中已经归一化、仍构成材料边界的 contour 参与。该限制必须在 UI/能力文档中可见。
- Automation 不模拟鼠标，也不新增公共操作；未来如需 headless 查询，可在同一 provider 上增加 `geometry.snap_features`。

## 后果

制造修改仍只在释放/确认时通过 ApplicationService 形成一个事务；逐帧捕捉是只读 UI/runtime 计算。Grip Editing、Block Editor、Alignment、Drill、PnP/RefDes 和完整 Tangent/Perpendicular UX 留给后续阶段。
