# S5-M2 Mixed Workload / Stress / Recovery

用户于 2026-10-02 正式启动。基线 c6f9920cd25190cd9c45a6c55c0e8c31c7a57938；任务来源见 RCAM_S5M2_TASK（实际文件名 RCam_S5M2_TASK.md）。S5-K1 implementation frozen；Native Full Closeout deferred by user，附件中的历史验收结论不是本轮重新执行的结果。

## 范围与顺序

S5，R05/R09/R10/R11/R16/R17/R18/R21/R22；局部关联 AT-017/018/030/032/039/040/063–075/086/087/090–094。冻结的 96 个用例、schema_version=2、required_platforms 及性能阈值保持不变。

本次按仓库“小闭环”规则先交付 M2-A Task Identity / Revision Fence / Cancel。允许 editor-service 的任务上下文与 import、editor-app 的 worker/状态/显示取消、必要测试、docs/scripts/manifest；不重构 shortcut system，不改 writer/制造精度/工程 schema，不新增依赖。先对实际队列接入任务身份、工作区和规则版本、多 generation 及安全取消；不得仅用没有调用者的框架宣称完成。

后续顺序 M2-B 100/500/1000/5000 objects drag → M2-C PMIX → M2-D PPOL → M2-E 30min/1h/2h PSTRESS → M2-F device recovery → native closeout。每步单独记录真实结果。完整 PMIX 保持设计 40k Flash+30k Line+20k Arc+10k 16-edge Region；Text/Block/多层为额外混合工作流，不替代原性能样本。PPOL 保持 4 层共 20k 有序操作。不能把短循环当 2h 长稳，不能把重建缓存当真实 device lost 恢复。

## M2-A 验收

- 排队任务捕获 task_id/document_id/revision/workspace revision/generation/rule revision/geometry policy identity；执行前拒绝已关闭、替换、修改、Undo/Redo 或 policy 失效的任务。
- 取消与提交通过一个原子状态决定先后；取消成功必须零提交，进入不可取消提交段后明确 too_late。不能仅丢弃 GUI 回包掩盖后台已修改。
- 读任务取消不发布部分 scene/selection；导入取消在原子提交前保留原工程。耗时阶段的检查点及尚不可中断部分明确报告。
- GUI 最新请求接收，旧完成不得清除较新任务 busy；队列失败不撤销已接受任务的身份。
- 无界面真实服务与 GUI worker 路径回归；fmt/check/clippy/workspace tests/automation_contract/headless_workflow/release/service dependency tree。原始命令、退出码、基线+diff 哈希、环境按 run-id 保存。

状态：IN_PROGRESS。Windows、全 V1、CORE10 和全部 M2 native 未执行，不标 PASS。

M2-A 受控检查点补充：允许 editor-core/hit_test/select_rect 的只读取消回调，不更改几何算法、阈值或支持范围；GUI 大框选经 service 复用该入口。固定每对象检查、整次无部分结果。单个复杂几何、parser/OS read、索引 build 和 metrics 内部仍需后续细化与 2s 实测，不将 post-result 拒收称为及时终止。
