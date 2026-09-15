# ADR 0012：S1-B2c workspace 与制造状态分离

日期：2026-09-15；阶段 S1-B2c，macOS arm64 优先；Windows deferred / not executed。
需求 R07/R10/R11/R14/R15/R19/R21/R22；局部用例
AT-022/039/040/041/060/062/086/088/090/091/095/097。
允许修改 editor-core 模型/历史、editor-service DTO/入口、gerber-io 图层构造适配、
相关测试、docs/README、源码/证据清单及归档工具。无新依赖，不改变 parser 支持范围。

## 冻结契约

- SemanticLayer 仅有 id、ordered objects；真正的 LN/image 信息保留 SourceMetadata。
- 每个服务文档拥有按稳定 LayerId 索引的 LayerWorkspaceState：display_name/visible/locked。
  初始名优先源 LN、image name，随后文件名；visible=true、locked=false。工作区设置不持久化为 Gerber。
- 制造 revision、dirty、保存路径和 Undo/Redo 维持原契约。新增 workspace_revision 十进制字符串，初始0。
- layer.update 顶层 expected_revision 必须匹配制造版本；params.expected_workspace_revision 必须匹配工作区版本。
  两项比较均在服务串行可变借用内执行。旧请求返回 REVISION_CONFLICT，不自动覆盖最新设置。
- params 严格自有 DTO：layer_id、expected_workspace_revision 必填，display_name/visible/locked 可选；
  省略或 null 表示不更新。display_name 非空白且最多1024 UTF-8字节，不截断；空 layer_id INVALID_ARGUMENT，
  未知层 NOT_FOUND.details={entity:layer,id:...}。非法参数、溢出、版本冲突均零修改。
- 全部设置相同（包括空 patch）成功 no-op。实际变化只推进 workspace_revision 一次，返回 DocumentInfo；
  统一 response.revision 仍为制造 revision。没有制造 Undo entry，不清 Redo、不改变 dirty/last_saved_path。
- layers.list 返回 layer_id/display_name/visible/locked/object_count。开发期 API v1 的 name 字段改为 display_name；
  没有已发布客户端或持久工程格式兼容承诺。
- 新 Move/Duplicate/Delete/Rotate/Mirror 在共用服务锁检查后调用既有 core 命令。锁定返回 LAYER_LOCKED。
  隐藏不禁止显式几何查询或制造编辑。未来 property/text 命令必须复用此服务检查。
- Undo/Redo 不读当前 workspace lock；恢复已记录的制造状态，仍检查对象身份、顺序和 before/after。
  原 core 锁断言迁移到新公共 JSON 专项测试；预算、错序守卫、几何回归继续保留。
- 导出仅使用制造快照和真正的源元数据。纯工作区修改关闭不需制造未保存确认；制造修改仍需确认。
- GUI selection、hover、工具、pan/zoom、面板状态留在 app；不进入制造历史。保留 S0 GUI 为参考演示。

## S2 边界设计（本轮不开放能力）

拟议 objects.hit_test：显式 layer_id、point_mm:{x_mm,y_mm}、tolerance_mm，返回绑定制造 revision 的
稳定 object IDs，按原曝光/对象顺序。默认查询单个对象制造几何，Clear 对象也可被识别；最终可见曝光
点选需另外命名并实施有序合成，不能混用两种语义。GUI 可从稳定列表实现重叠循环。
拟议 layer.bounds/document.bounds 返回 f64 mm 的 min/max，空内容返回 null；不随显隐改变，GUI 可显式聚合可见层。
Flash 必须使用孔洞及 LocalTransform；Arc 使用真实圆弧与扫掠语义；Region 使用冻结 contour 规则。
索引只提供候选，最终必须精确几何判定；Mesh、光栅像素、AABB 不得冒充命中真值。
未完成全几何验收前不列入 capabilities。S2另行实现小闭环，本轮完成后停止。

## 证据与交付

先形成源码候选 commit，再在相同源码、干净 Git 状态上执行 final gates，保留执行前后 HEAD、
tracked hashes、真实退出码和环境。公共证据另 commit；PACKAGE_INFO 明确 tested_code_commit 与
packaging_evidence_commit。归档清单按实际文件生成，不猜测未提交删除。
进入任务时四份旧根目录交接文档已经删除；保持删除并从源码清单移除，Git历史保留原件。
本轮提供的任务/复审原文存入 docs/handoffs，不覆盖旧阶段证据。
完整96用例及 required_platforms 不变；专项通过不等于 GUI、CORE10、生产资格或双平台 V1 通过。
