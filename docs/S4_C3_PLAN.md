# S4-C3 Block Editor v1 — Mac-first

状态：已完成实现与 Mac-first bounded 验收。最终 fixed package identity 由 [S4_C3_REVIEW](S4_C3_REVIEW.md) 指向包内 PACKAGE_INFO.json、交付 REVIEW.md 与 SHA256SUMS.txt。用户于 2026-09-27 授权开始 S4-C3；输入任务书为 RCam_S4C3_BLOCK_EDITOR_NEXT_TASK.md。前置固定证据为 exports/S4C2_f1293f2/REVIEW.md。

范围：Block Library、同层普通对象创建 Block、Object/Grid Snap 基点、单次浮动放置、实例数值变换、Rename/Delete/Explode、Select Instances。复用 blocks.* 和 objects.*、共享 Definition、现有项目/导出/Recovery。禁止内部 Definition 编辑、Array、Alignment、PnP/RefDes、nested、scale/shear。

关联 R04/R07/R08/R09/R10/R11/R13/R14/R15/R16/R17/R18/R19/R20/R21/R22；AT-025/030/032/033/034/038/039/040/041/043/044/054/055/058/062/063/074/075/086/088/090/091/092/093/095 局部映射。冻结 96 cases/schema_version=2 不变。

允许修改：editor-app 的 Block UI/临时工具/worker/command 路由；editor-core 的 Command Registry 与经证明的 Block 安全缺陷、只读制造边界复用；editor-service 的 Block 权限校验、诊断与专项测试；相关文档、公开 synthetic fixtures、验收与打包脚本。不改 Gerber parser/writer、项目 schema、Snap 算法及既有 Grip 规则。

实施：先冻结 ADR/专项验收，再接入 GUI；目标测试后运行全部 fmt/check/clippy/workspace/service/headless/release 门禁；原生 Metal/CUA、性能及版本化 exports 包绑定固定源码。没有执行的步骤不记 PASS。Windows deferred；完成 C3 后停止复审。

## Status / capabilities closeout（2026-09-27）

基线 `3c6b6dbe475c35f97beb1abd86a8d0bca1018398`；S4、R01/R06/R21/R22、AT-001/086/088/095 局部证据与 C3-12。仅允许 editor-service 的 capability stage metadata、capability tests、状态文档与 manifest/package evidence 变化。制造/UI Block/Grip/Snap/writer/project codec 保持逐文件相同。

重新执行 14 项自动门禁（workspace 加 --no-fail-fast）、真实 JSON capability 调用和 release build。新二进制原生 launch/Metal/Block Library/clean exit；完整 CUA 主链与性能基线从 3c6b6db 继承，前提为 source-diff-audit 通过。固定 clean commit、Source/Evidence ZIP、fresh extract、确定性打包和逐文件哈希闭合后交付；Windows deferred。下一阶段 S4-C4 Alignment / Distribution 本轮不启动。
