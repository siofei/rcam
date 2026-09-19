# Native observations: s2b2-modifiers-20260920

主代理在用户取消 Luna-only 安排后用 CUA 操作。受测包唯一 ID local.rcam.s2b2.modifiers.20260920；哈希见 binary.json。
全部坐标来自当前窗口截图，保存图片只含 RCam 测试窗口。

|证据|观察|
|---|---|
|01|打开公开 gui_primitives.gbr，13对象，revision 0|
|02|左→右框选底部前三个对象，selection 3，primary object-3，revision 0|
|03|拖动已选圆，三个对象同时移动；revision 1；主对象中心31.996955,21.936443|
|04|一次Cmd+Z整组恢复原位、dirty清除，revision 2|
|05|一次Cmd+Shift+Z整组恢复移动，revision 3|
|06|Cmd+D，13→16对象，3个副本被选中，primary doc-1-generated-object-2，revision 4|
|07|Backspace整组删除，16→13对象、清空选择，revision 5|
|08|一次Cmd+Z恢复16对象，revision 6；随后Redo删除，revision 7|
|09|右→左框选Line/Arc/轴向RectangularSweep，3对象，primary object-8|
|10|锁定后Cmd+D/Backspace/拖动均不改变13对象或revision 7；普通拖动未成立时按click规则变为单选|
|11|解锁、重新框选3对象；数值框输入123后Cmd+D/Backspace，仅文本变12，制造revision仍7|
|12|Arc实际空区内Crossing框，无选择，revision仍7|
|13|已移动圆环孔内Crossing框，无选择，revision仍7|
|14|另存 gui_group_edited.gbr，dirty清除，revision仍7|
|15|完整路径重开输出，13对象、revision0|
|16|普通click单选object-3，中心31.996955,21.936443，与输出文本一致|

第一次从文件列表双击后出现“input is not UTF-8”，不将该次视为成功重开；随后用完整路径定位Gerber并点击Open成功。
源/输出独立核对见 independent-output-check.json。源样本 SHA 保持不变。

NOT TESTABLE：当前CUA click/drag API不支持修饰键参数，pressKey不支持仅按住Shift/Ctrl，因此原生 Ctrl-click/Shift-click 尚未实测。
同样未完成拖动按住期间的Esc/失焦/PointerGone原生输入；相关自动手势测试通过，不替代原生证据。
当前候选由CUA启动，未取得完整stdout/stderr原生状态日志和每步history计数；不拿旧候选日志冒充。
斜矩形sweep仍只在core精确几何测试覆盖，parser原生导入边界未改变。
