# S2-B3.2 Viewport + Native 门禁

Mac-first S2，关联R04/R05/R08/R09/R10/R11/R16/R17/R19/R21；局部AT-017/018/023/024/030/032/039/040/062/067/070/075/081/086/087。
允许修改editor-app显示/benchmark/测试、公开synthetic、脚本/文档；core/service/gerber-io与制造真值不改。
保留零容差reference parity，原50ms/300ms门槛和96个用例不变；P1K不能代替PMIX完整AT-075。
读取用户补充RCam_S2B31_04c0cbf_Review.md：按D-01视口局部化，测量D-02全量preview成本，以真实窗口harness处理D-03。

冻结native轨迹：真实eframe/Metal surface；测得画布1600×900；Fit，10秒预热，30秒连续sinusoidal Pan/Zoom；
全选1000圆，三轮每轮至少10秒通过egui RawInput注入Pointer事件（经过原Gesture/ProbeDrag/DragMove），释放、等待最终surface帧GPU完成，再Undo。
活动序列边界由harness状态机定义；第一帧进入时无上一活动帧，不统计进入前空闲间隔，之后每帧全部保留。
GPU完成采用下一原生帧开始时device.poll Wait，记录等待和前一update到完成耗时；不是GPU timestamp或显示器扫描时刻。
最终可见口径为目标revision的production callback真实绘制，surface提交/present返回后GPU完成的保守上界；保留surface截图。
不把离屏数据当原生。若任一帧无法绘制、尺寸不符、历史或指标不符，保留FAIL；不删慢帧。
preview CPU采样P1K/P10K/P100K，各60次冻结delta；超过现有50ms帧预算意味着下一阶段必须后台/增量或selected-only索引。
不扩展后续功能；Windows deferred / not executed。
