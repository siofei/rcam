# ADR 0019：Mac-first S2-B2 精确框选与多选

阶段 S2-B2，需求 R04/R05/R07/R09/R10/R11/R14/R15/R18/R19/R21/R22。
局部 AT-022/026/027/028/029/030/032/035/036/038/039/040/041/043/054/086/088/089/090/091/095。
允许修改 editor-core 几何查询、editor-service DTO/分派、editor-app 选择/手势/显示、对应测试与文档/清单。
不改 parser/writer 支持范围、制造容差、96 个有效用例或平台门槛。Windows deferred / not executed。

新增 objects.select_rect：单个显式 layer_id、rect_mm（min_x_mm/min_y_mm/max_x_mm/max_y_mm）、mode window/crossing。
只读，不接收 expected_revision，返回 document_id/revision/layer_id/object_ids；按曝光顺序，全有或全无。
不读取 workspace；GUI 在后台按可见层调用，全部成功后才替换选择集。跨层选择仅供查看，制造操作整批拒绝。
Window 要求非空对象材料完整位于闭矩形内；Crossing 要求对象材料闭包与闭矩形相交。
采用制造材料解析边界的线段/圆弧交点、极值与内部点判断；不以保守 AABB 作最终关系。
Macro 复用有序局部布尔边界；Region 复用 canonical contour/cut-in；圆弧保留 deviation 径向接线。
不确定/超预算整次拒绝，不输出部分 ID。query contains 仍表示对象包含矩形。

SelectionSet 按确定的插入顺序去重，以 LayerId/ObjectId 标识，最后加入者为 primary。
普通 click 单选，空白 click 清空；Ctrl-click 加选，Shift-click 减选。locked 可查看但整组禁止制造编辑。
空白按下才框选；左至右 Window，反向 Crossing；release 一次查询，Esc/blur/PointerGone 取消。
按下已选对象可拖动整组；未拖动的 release 恢复普通 click 单选语义。
所有预览仅 UI，单次 Move/Duplicate/Delete 使用服务一个事务，失败保留选择。
对象失效/隐藏/文档关闭清理选择；Undo/Redo 不自动重选已清理 ID。
当前 GUI 单文件单层导入边界不变，不为测试新增产品多层导入。

2026-09-20 用户修订覆盖原始任务单：Ctrl-click 只加选（已选对象不重复、不改变顺序），Shift-click 只减选（未选对象不加入），二者同时按下时减选优先。捕获鼠标按下事件的修饰键，修饰键点击不启动制造拖动。
