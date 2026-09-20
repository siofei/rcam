# S3-FINAL Requirement / AT / Evidence Coverage

基线：DESIGN_V1 / ACCEPTANCE_V1 1.1，schema_version=2。平台结论仅为
`S3 = PASS（Mac-first）`；Windows 为 `deferred / not executed`。完整 AT 若同时要求 S4 文字、
异步 jobs、覆盖保存或双平台，本表把 S3 子项与完整用例状态分开，不把局部通过写成完整 PASS。

证据根为 `evidence/s3-final-20260920/public/`。`gates/03.log` 是 workspace suite；专项原始日志为
`gates/04.log` 至 `gates/12.log`，Metal parity 为 `gates/18.log`；原生窗口/P1K 数据在 `native/`，
其独立校验为 `native-validation.json`。最终 commit、clean status、环境和二进制哈希均在同一证据根。

状态：PASS 表示 Mac-first S3 所需范围有同一 clean commit 证据；PARTIAL 表示完整 AT 还含后续阶段
或另一平台；NOT TESTABLE 表示当前自动化无法制造指定原生 OS 事件，并不伪造人工结论。

| 功能 | Requirement | AT | 自动测试 | Mac Native Evidence | 当前状态 | 缺口 |
|---|---|---|---|---|---|---|
| 数值 Move / 非法数值 | R10 | AT-031 | `s1b_edit_workflow`、app tests | 同 build GUI service path | PASS（Mac S3） | Windows 未执行 |
| Drag preview / cancel | R10/R11 | AT-032 | app tests 覆盖 Esc、PointerGone、revision/Undo 不变 | `native/s2c1-tools.json` 的真实 egui drag；Esc 测距/focus 事件 | PARTIAL | 原生 window blur/lost capture 无可靠注入，NOT TESTABLE；自动测试保留 |
| 任意角 Rotate / Pivot | R10/R14 | AT-033 | `s1b2b_transform_workflow`、app tests | `native/` phase `rotate` | PASS（Mac S3） | Windows 未执行 |
| Mirror / Arc / 重复变换 | R10/R11/R14 | AT-034 | `s1b2b_transform_workflow` | `native/` phase `mirror`；Metal parity | PASS（Mac S3） | Windows 未执行 |
| Duplicate / 独立 ID | R10/R11 | AT-035 | `s1b2_edit_workflow`、headless closeout | `native/` phase `duplicate` | PASS（Mac S3） | 系统图形剪贴板仍非 service API；Windows 未执行 |
| Dark/Clear/Dark 复制顺序 | R05/R10/R14 | AT-036 | `s3_edit_closeout::ordered_dark_clear_dark...` writer/reopen/coverage | macOS headless real parser/writer | PASS（Mac S3） | Windows、独立生产查看器未执行 |
| Flash aperture COW / DCode | R04/R10/R14 | AT-037 | service + app COW/Undo/writer tests | `native/` phase `flash_size_cow` 与 `s3_save_reopen` | PASS（Mac S3） | Macro 尺寸编辑按设计拒绝；Windows 未执行 |
| Delete / Undo 完整恢复 | R10/R11 | AT-038 | `s1b2_edit_workflow`、headless closeout | `native/` delete/undo/redo phases | PASS（Mac S3） | Windows 未执行 |
| 多对象原子操作 | R10/R11/R15 | AT-039 | transform/edit/headless suites | P1K 三轮每次一个 Undo | PASS（Mac S3） | Windows 未执行 |
| Undo/Redo 分支 | R11 | AT-040 | `s1b_edit_workflow`、`s1b2_edit_workflow` | native delete Undo/Redo + drag Undo | PASS（Mac S3） | Windows 未执行 |
| dirty 与保存基线 | R11/R15 | AT-041 | headless、workspace、app Save As tests | native Save As/Reopen；`after-tools.gbr == baseline.gbr` | PASS（Mac S3） | 覆盖保存冲突属 AT-093 后续缺口 |
| Undo 内存预算 | R11/R16 | AT-042 | `s3_edit_closeout::history_evicts...` | macOS test process | PASS（Mac S3） | 结构编辑 order guard 仍 O(N)，记录为性能风险而非 correctness failure |
| 输入焦点保护 | R09/R10/R12/R18 | AT-043 | app shortcut/focus tests | native phase 221：文本焦点 Esc 不清测量 | PASS（Mac S3） | Windows 快捷键未执行 |
| Grid + endpoint/center Snap | R13 | AT-044 | Grid、object priority、8 logical-point radius、Alt disable、unit invariance tests | native grid snap + Retina ppp/canvas record | PASS（Mac S3） | Angular Snap 不属于 S3；Windows 未执行 |
| 多测量 / 坐标 / mm-in | R08/R13 | AT-045 | 3-4-5、角度、多条、Esc、inch display tests | native phases 20/21/22/221/23 | PASS（Mac S3） | Windows 未执行 |
| 完整保存闭环 | R10/R12/R14 | AT-054 | S3 headless全命令 + app save/reopen | native `s3-final.gbr` 重开 | PARTIAL（S3 编辑子集 PASS） | 完整 AT 还要求中英文文字和独立查看器；S4/独立工具未执行 |
| 显示状态不影响导出 | R08/R14 | AT-062 | app Grid/Measure writer identity tests | native baseline/after-navigation/after-tools 字节一致 | PASS（Mac S3） | Windows 未执行 |
| P1K 1000 selected drag | R10/R11/R17 | AT-075 | viewport budgets + validator tests | `native-drag-3x10s.json`、`release-latency.json`，p95≤50ms、release≤300ms | PASS（Mac） | P100K 与 Windows deferred |
| Headless no GUI/GPU dependency | R02/R10/R14/R18/R21 | AT-086 | automation/headless/S3 closeout；`cargo tree` | Mac 原生 headless；`gates/13.log` 无 egui/wgpu/winit | PARTIAL | 完整 AT 要 Windows；异步终态在 S4 jobs 前不适用 |
| GUI/service 同业务实现 | R10/R12/R14/R21 | AT-087 | app integration + boundary audit + headless geometry compare | same-build native full S3 flow | PARTIAL（S3 编辑子集 PASS） | 完整 AT 的中文文字/固定字体和 Windows 未执行 |
| DTO / JSON 严格校验 | R03/R21/R22 | AT-088 | `automation_contract` + edit suites | macOS headless | PARTIAL | Windows locale 未执行；非有限 JSON 由 serde 边界拒绝 |
| Query / stable IDs / cursor | R09/R21/R22 | AT-089 | S1/S2 query/select suites | macOS headless | PARTIAL | 完整双平台未执行 |
| 接口失败零修改 | R06/R10/R15/R22 | AT-090 | edit/transform/workspace/S3 failure snapshots | macOS headless | PASS（Mac S3） | Windows 未执行 |
| revision conflict / Undo 单调 | R11/R16/R22 | AT-091 | automation/edit/workspace suites | macOS headless | PARTIAL | 后台耗时写入属于未实现 jobs；Windows 未执行 |
| `edit.batch` 原子性 | R10/R11/R15/R22 | AT-092 | S3 success + mid-step failure + external-I/O reject | macOS headless真实模型/writer | PARTIAL | 取消、预算失败、batch Duplicate/Delete 尚未开放；Windows 未执行 |
| 无头保存授权/冲突 | R14/R15/R20/R22 | AT-093 | headless export、权限与 fail-closed tests | macOS headless | PARTIAL | `replace_if_unchanged` 覆盖流程未实现；只支持新路径/deny |
| jobs / cancel / stale snapshot | R15/R16/R22 | AT-094 | 无 | 无 | 未执行 | S4/S5 工作；S3 不新增 jobs |
| capability / unknown version/op | R06/R21/R22 | AT-095 | `automation_contract` | macOS headless | PARTIAL | Windows 未执行；只公布实际 batch 子集 |
| host 权限 / 无脚本运行时 | R01/R20/R22 | AT-096 | automation/headless permission tests + dependency audit | macOS headless，无监听服务 | PARTIAL | Windows reparse-point 未执行 |
| 双平台可追溯流程 | R12/R14/R18/R19/R21/R22 | AT-097 | Mac S3 evidence/package gates | Mac 同 commit 可追溯 | PARTIAL / BLOCKED | Windows、文字、全部 AT-086—097 未完成，不能宣称完整 V1 |

## S3 退出判断

Mac-first S3 的制造编辑、原子历史、Grid/Snap/Measure、metrics 回归和 P1K 门禁均有证据，
且没有 S3 范围 B0/B1 残留。AT-032 的原生 blur/lost-capture 为明确 NOT TESTABLE，自动取消链通过；
不将其改写为人工 PASS。AT-054/086—097 的后续阶段或双平台部分仍保持 PARTIAL/BLOCKED。

因此本轮只签署 `S3 = PASS（Mac-first）`。这不代表完整 V1、Windows、双平台、CORE10 10/10
或 96 个 AT 全通过；S4-A1 只能在本交付复审后另开任务。
