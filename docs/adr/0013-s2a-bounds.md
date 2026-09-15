# ADR 0013：S2-A 第一闭环——制造边界查询

阶段：S2-A.1，Mac-first。需求 R04/R07/R08/R19/R21/R22；关联
AT-010/012/013/014/015/016/022/023/039/054/088/089/095 的局部前置检查。
允许修改：editor-core 的边界计算、editor-service 的只读 DTO/查询分派、对应测试、
设计/API/验收说明、源码清单。无新依赖；不修改 parser、writer、历史语义或 S0 GUI。

## 决策

按一次一个可运行小闭环推进 S2-A。先交付真实 JSON 打开→边界查询→移动→Undo/Redo→
导出重开→边界核对。点选、渲染器和 Mac GUI 完整闭环留在 S2-A 后续工作，不能将本轮
局部通过表述为 S2-A 退出条件通过。Windows deferred / not executed。

- `document.bounds` params 为 `{}`；`layer.bounds` params 为 `{layer_id}`。
  两者仅接受有效服务文档，不接受 expected_revision，未知字段/空层 ID 拒绝。
  返回 `{document_id, revision, bounds}`，bounds 是 null 或 f64 mm 的
  min_x_mm/min_y_mm/max_x_mm/max_y_mm。信封 revision 与结果 revision 一致。
- 边界包围制造对象，不是最终可见像素范围；包含 Dark 和 Clear，不随 visible/locked/name
  改变。空文档/空层返回 null；未知文档/图层返回结构化 NOT_FOUND，不能混同为空。
- C/R/O/P Flash 按实际几何计算轴向支撑点，组合局部镜像、旋转、缩放和中心偏移。
  中心孔不缩小外框。Macro 使用所有 Dark primitive 的解析包络，忽略局部 Clear 对包络的
  裁减；这是明确的保守框（可留白、不可裁掉对象），不宣称是布尔结果的最紧框。
  Macro Circle/CenterLine 的 primitive rotation 同时作用于偏心中心；Outline 转换真实顶点。
- Line/RectangularSweep 扩展真实宽高。Arc 使用已有 canonical-circle 与端点径向接线语义，
  只包含实际 sweep 的轴向极值，加笔宽；原始端点仍包括在内。零扫掠为端点圆点。
  Region 复用已有 canonical contour，再聚合真实直线/圆弧边界，不引入细分或新填充规则。
- 查询不建立历史、不修改 revision/dirty/保存路径。服务使用已验证文档；core 遇到缺光圈
  或非有限边界返回错误，不以丢弃对象恢复“成功”。O(N) 查询足够本闭环，不增缓存或索引。

## 验收

解析真值测试覆盖全部当前几何、局部变换、偏心 Macro、圆弧 sweep/方向/整圆/零扫掠/
非零半径偏差、空内容和多层聚合。真实服务请求验证状态无副作用、错误契约、能力表、
移动/Undo/Redo、导出重开及源文件保持。全部既有构建门禁保留原始日志。
96 个有效用例、AT-079 退役身份、原 required_platforms 与完整步骤不变。
