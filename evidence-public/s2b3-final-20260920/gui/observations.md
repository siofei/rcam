# S2-B3 原生观察（macOS arm64 / Metal）

受测提交 dbde56e777a8b5176a552b84739efb3db3aa032b。通过CUA操作最终package中的RCam.app。
21张截图为工具原始截图字节，无重绘。native.log由冻结release进程stdout/stderr记录。
初次CUA管道断开，reset后以完整应用路径重新连接成功；中间一次窗口状态变化重新查询后继续。
日志首行仍为旧阶段标签“S2-B2 native GPU”，这是继承的静态日志文本；受测身份按二进制hash判定。

- 01：原生Open打开公开gui_primitives.gbr，13对象。
- 02–10：C、带孔C、R、O、P、Line、Arc、RectangularSweep、Region的面积/周长与independent-expected.json相符（显示6位小数）。
- 11：Macro显示“面积/周长：暂不可精确计算”与原因，没有伪造0。
- 12：圆从(10,20)拖动至(12.844148,23.388770)，revision1/undo1，指标仍π/2π。
- 13：Cmd+D创建新ObjectId，14对象/revision2/undo2，副本指标同源圆。
- 14：Backspace删除副本，13对象/revision3/undo3，选择清空且指标隐藏。
- 15：Cmd+Z后14对象/revision4/undo2/redo1，重新点选恢复的副本，指标π/2π。
- 16：框选底部6对象，已精确6/6，对象面积合计45.241861mm²、对象周长合计69.699112mm。
- 17：框选全部14对象，已精确13/14；面积合计（已精确项）174.995395mm²、周长合计（已精确项）212.300876mm，明确1对象不可计算。
- 18：原生另存为metrics_edited.gbr成功，dirty清除、revision4不变、指标不变。
- 19：原生重新打开输出并框选，14对象/13exact，两个合计与保存前相同。独立文本检查输出包含两次移动后圆心，源文件SHA不变。
- 20：固定1000个直径0.5mm圆样本打开25ms（worker日志），触发现有display pixel/object budget，未能框选或计算面板；大selection原生验收 BLOCKED，不以快速拒绝冒充指标性能通过。
- 21：重新打开已保存的编辑结果并选择圆环；指标3π/6π恢复，应用保持可查看。

Rotate/Mirror尚无GUI按钮，本任务允许通过真实service/test验证：metrics cache专项已执行，刚性修改及Delete/Undo只发生1次calculator计算、5次cache hit。
Ctrl-click、Shift-click、Ctrl+Shift Remove priority、held drag+Esc、held drag+blur/PointerGone：NOT TESTABLE。
当前CUA click无modifier参数，drag为按下到释放的单次调用，无hold/mouseDown/keyDown接口；自动测试不改写为native PASS。
1000对象受现有renderer预算阻塞，保留原样本和截图；未改预算、未替换小样本或扩展生产renderer。
本记录不授予完整V1性能、Windows、CORE10或全部AT用例通过。
