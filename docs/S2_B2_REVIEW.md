# S2-B2 Mac 精确框选与多选验收记录

**实现与自动门禁通过，主要原生交互已验证；修饰键点击及完整原生日志仍有缺口，暂不签署阶段全部通过。**
运行 ID `s2b2-modifiers-20260920`。环境 macOS 26.5.1 (25F80)、arm64、Apple M1 / Metal、Rust 1.89.0。
受测对象是基于仓库 HEAD 的未提交工作树，产品55文件的精确哈希见 source-before.json，
非干净提交验收；不把基础 HEAD 冒称受测提交。release SHA-256：
`7274439feb8646122a3a3d6d1568e88280f62ef84a2b557ee45aa500a671afdc`。

## 范围与实现

阶段 S2-B2，需求 R04/R05/R07/R09/R10/R11/R14/R15/R18/R19/R21/R22。
局部 AT-022/026/027/028/029/030/032/035/036/038/039/040/041/043/054/086/088/089/090/091/095。
修改 core 几何查询、service DTO/分派、app 选择/手势/显示、测试、文档和清单/本机打包脚本。
无新增依赖，Cargo.lock/parser/writer/制造事务算法不变。完整96用例、AT-079退役身份及平台门槛不变。

- objects.select_rect 显式 layer_id/rect_mm/mode，只读且稳定曝光顺序，全有或全无。
  保留 objects.query contains 的原含义。Window 检查实际材料曲线解析极值；Crossing 计算真实边界交点和内部点。
  不是将对象保守 AABB 当最终真值：特别对 Macro 先顺序布尔求材料边界，再判定完整包含。
- C/R/O/P/孔洞/局部旋转镜像缩放、Macro1/4/21、Line、真实矩形Minkowski sweep、Arc及Region。
  Arc复用canonical circle与deviation径向接线、全圆/零扫掠；Region复用canonical contour/cut-in/winding。
  空材料不选；Clear作为独立对象可选；数值歧义/2,000,000工作预算超限整次拒绝。
- SelectionSet 保持确定插入顺序、去重，以最后加入者为primary。普通click单选，Ctrl-click加选，Shift-click减选。
  对象删除/隐藏/Undo/Redo/关闭后清理无效选择；选择不进入制造模型，不改revision/dirty/history。
- 空白按下才框选，左→右Window，右→左Crossing；release后台一次矩形查询，预览不逐帧查询。
  Esc/blur/PointerGone取消，异步命中前已释放仍保留终点；同帧Esc阻止新手势。
- 按下任一已选成员可拖动整组（即使同时命中更上方未选对象），预览只改显示标志/位移。
  同层一次Move/Duplicate/Delete一个事务，失败保留选择；Duplicate选中新集合，Delete成功清空。
  锁定/跨层整组拒绝；当前GUI仍为单文件单层导入，不引入跨层拆事务。
- GPU选中标志缓冲支持多个对象，按原曝光顺序合成，独立轮廓高亮；保留资源预算。
- ADR0018冻结GeometryMetrics派生真值、lazy cache/失效、Object Metrics与最终Layer Area分界。
  本轮没有指标字段/API/算法壳；下一轮为S2-B3 GeometryMetrics + 属性面板。

## 自动验证

原始日志：`evidence/s2b2-modifiers-20260920/`；gates/commands.json、regression/commands.json 与 metal/commands.json记录每条真实命令、退出码、耗时。
PATH/CARGO_HOME/RUSTUP_HOME/CARGO_TARGET_DIR使用项目.tools缓存。

| 实际命令 | exit |
|---|---:|
| cargo fmt --all -- --check | 0 |
| cargo check --workspace --all-targets --locked | 0 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 0 |
| cargo test --workspace --locked | 0 |
| cargo test --locked -p editor-core --test s2a_hit_test | 0 |
| cargo test --locked -p editor-service --test s1b_edit_workflow | 0 |
| cargo test --locked -p editor-service --test s1b2_edit_workflow | 0 |
| cargo test --locked -p editor-service --test s1b2b_transform_workflow | 0 |
| cargo test --locked -p editor-service --test s1b2c_workspace_workflow | 0 |
| cargo test --locked -p editor-service --test s2a_hit_test_workflow | 0 |
| cargo test --locked -p editor-service --test s2a_bounds_workflow | 0 |
| cargo test --locked -p editor-service --test automation_contract | 0 |
| cargo test --locked -p editor-service --test headless_workflow | 0 |
| cargo tree --locked -p editor-service --edges normal | 0 |
| cargo build --release --locked -p editor-app | 0 |
| python3 scripts/source_manifest.py --check | 0 |
| python3 scripts/test_audit_core10.py | 0 |
| cargo test --locked -p editor-core --test s2b_rect_selection | 0 |
| cargo test --locked -p editor-service --test s2b_select_rect_workflow | 0 |
| cargo test --locked -p editor-app --bin editor-app | 0 |
| cargo test --locked -p editor-app native_metal -- --ignored --nocapture | 0（原生权限） |
| cargo test --locked -p editor-app --example s0-demo native_gpu_coverage_regressions -- --ignored --nocapture | 0（原生权限） |

工作区298 passed / 0 failed / 5 ignored。55个app非GPU测试、12个新增core矩形测试、1个真实JSON/文件专项通过。
四个GPU忽略项已另行原生执行通过；余下私有CORE10工作流未执行。Python7个审计工具测试不等于CORE10通过。
服务正常依赖无egui/eframe/wgpu/winit/objc2。包内二进制与受测release哈希一致。
Metal新renderer三模式共37766个覆盖点，包含独立孔洞真值，实际fragment pipeline也创建；旧S0另通过。
不将这些点覆盖检查当原生鼠标/快捷键/焦点操作证据。

独立几何反例含环内框、斜扫掠AABB空角、弧中心空区、Macro被Clear截断后的Window、Region孔洞/cut-in、
变换标准光圈、相切及1e-7毫米间隙、全圆/零扫掠/CW/CCW/deviation。
另有392次独立材料单元格Window/Crossing对照，不复用产品边界算法。
JSON专项保留实际请求/响应，验证严格字段/版本/显隐独立性、整组编辑/历史/曝光顺序、导出重开及源字节保护。

## 原生交互与证据边界

用户已取消全部 CUA 交给 Luna 的临时安排；后续由主代理继续原生验证。2026-09-20 用户修订：Ctrl-click 加选、Shift-click 减选，覆盖初始任务单的 Shift 切换规则。
原生双向框选、整组拖动/撤销/重做、复制/删除/撤销、锁定、文本焦点、Arc/孔洞空区及Save As/Reopen已完成；16张截图和逐项记录见 gui/observations.md。源哈希未变；独立读取Gerber坐标验证前三对象同时移动，其余Flash中心保持。
Ctrl/Shift-click与拖动中取消的原生输入受CUA接口限制，NOT TESTABLE；完整原生stdout/stderr及逐步history计数未取得。自动选择/手势/事务测试通过不能替代这些证据。
首候选 s2b2-20260920-004005 保留为历史；后续修正重叠选择拖动/Esc守卫，不将旧包记录冒充最终候选。
初次沙箱Metal测试无法枚举适配器，原始失败保留，最终在原生权限重新执行通过。

## 未完成与限制

- Windows deferred / not executed，完整AT/CORE10/双平台V1未通过。
- 当前仅几何选择；最终可见模式/循环重叠选择的完整V1要求仍保留。
- 斜RectangularSweep为算法层精确测试；parser/编辑/输出仍按既有轴向限制，不伪造GUI斜扫掠导入通过。
- 线性扫描、Macro有界二次边界准备、每帧显示预算扫描；未作高对象数性能或R-tree/production renderer承诺。
- 本轮不做Rotate/Mirror GUI、Grid/Snap、测距、文字、最终Layer Boolean Area、多格式交换。
- 外部 RCam_S2B1_source_Review.md 未提供，未引用其结论。
- 应用未签名/公证，使用现有工具链缓存，未做干净机器安装或跨机发行验收。

公开可分发证据：`evidence-public/s2b2-modifiers-20260920/`。受测测试包：`evidence/s2b2-modifiers-20260920/package/RCam.app`。本轮停止在S2-B2，不启动S2-B3。
