# S2-A.2 精确 Hit Test 交付与验收

**PASS：Mac-first S2-A.2 阶段门禁。可进入 S2-A.3；本轮在无 GUI 点选服务完成后停止。**

受测代码提交：`a1aa073a63328fa2ac9eec5f6eb5a55af072560e`。运行 ID：`s2a2-20260916-005047`。最终门禁前后 Git 状态均 clean，完整 tracked hashes 一致。

## 改动与范围

需求 R04/R05/R09/R10/R11/R19/R21/R22；关联 AT-010/011/012/013/014/015/016/026/027/028/030/039/040/088/089/090/095 的局部步骤。
修改 editor-core 点选/材料边界模块、editor-service DTO/入口/能力表、专项测试、文档/源码清单。无新增依赖，无 parser/writer/制造历史语义变更。受测提交同时冻结此前尚未提交的 S2-A.1 基线及用户提供的交接文档；其功能回归保留。

- `objects.hit_test` 真实 JSON 入口；严格嵌套 point DTO，显式 layer_id/tolerance，结果绑定 revision 并保留原对象曝光顺序。
- C/R/O/P Flash 真实外形/孔洞及镜像、旋转、缩放；Line/零长度/极短非零段；真实矩形 Minkowski sweep；Arc 平均半径、端点接线、方向、全圆/零扫掠；canonical Region/cut-in/弧边。
- Macro 按解析交点切分真实边界，两侧有序曝光判断保留有材料的区间；内部区间本身属于材料闭包，可作为距离证据，被擦除的边界不能成为 tolerance 命中证据。未使用保守包围盒或像素判断。
- Dark/Clear 均可返回。显隐/锁定/名称不是查询输入；查询不改变制造或工作区 revision/dirty/history。
- 参数范围/非有限值/未知字段严格拒绝；未知实体结构化 NOT_FOUND；数值不确定和资源预算超限整次拒绝，不返回部分 IDs。

## 实际测试

环境：macOS 26.5.1（25F80）、Apple Silicon arm64，Rust/Cargo 1.89.0，复用项目现有工具链缓存，非干净机器安装验证。

| 范围 | 结果 |
|---|---|
| 工作区回归 | 229 passed / 0 failed / 2 ignored |
| s2a_hit_test | 18/18 |
| s2a_hit_test_workflow | 11/11 |
| s2a_bounds_workflow | 7/7 |
| S1-B1 / B2a / B2b / B2c | 17/17、27/27、31/31、17/17 |
| automation_contract / headless_workflow | 4/4、2/2 |
| test_audit_core10.py | 7/7（审计脚本单测） |
| 服务依赖审查 | 无 egui/eframe/egui-wgpu/wgpu/winit |
| release editor-app | 构建通过，仍为 S0 GUI |

独立真值包括 24 组有序矩形 Macro、每组 12 个确定性查询点、4 档 tolerance（1152 次对照），使用材料单元格的欧氏距离，独立于产品的边界算法。另有圆形局部 Clear、完全擦除、孔边界、退化线段、圆弧 cap/半径偏差及 Region cut-in 已知点断言。
真实服务测试完成打开→命中→移动→旧位置 MISS/新位置 HIT→Undo/Redo→导出重开；另验证 Duplicate/Delete/Undo Delete 恢复 ID/顺序、Rotate/Mirror、矩形90°宽高交换和圆弧镜像方向。输入源字节保持，查询前后 DocumentInfo 相同，资源失败不返回部分对象。

### 实际命令与退出结果

PATH 使用项目 `.tools/cargo/bin`；CARGO_HOME / RUSTUP_HOME 指向现有 `.tools/cargo` / `.tools/rustup`。RCAM_S2A2_EVIDENCE 保存专项实际请求/响应和受控输入输出。

| 实际命令 | 退出码 | 公共证据根下日志 |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | `gates/01.log` |
| `cargo check --workspace --all-targets --locked` | 0 | `gates/02.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `gates/03.log` |
| `cargo test --workspace --locked` | 0 | `gates/04.log` |
| `cargo test --locked -p editor-core --test s2a_hit_test` | 0 | `gates/05.log` |
| `cargo test --locked -p editor-service --test s2a_hit_test_workflow` | 0 | `gates/06.log` |
| `cargo test --locked -p editor-service --test s2a_bounds_workflow` | 0 | `gates/07.log` |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 | `gates/08.log` |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 | `gates/09.log` |
| `cargo test --locked -p editor-service --test s1b2b_transform_workflow` | 0 | `gates/10.log` |
| `cargo test --locked -p editor-service --test s1b2c_workspace_workflow` | 0 | `gates/11.log` |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | `gates/12.log` |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 | `gates/13.log` |
| `cargo tree --locked -p editor-service --edges normal` | 0 | `gates/14.log` |
| `cargo build --release --locked -p editor-app` | 0 | `gates/15.log` |
| `python3 scripts/source_manifest.py --check` | 0 | `gates/16.log` |
| `python3 scripts/test_audit_core10.py` | 0 | `gates/17.log` |

## 证据和代码身份

- 本地原始证据：`evidence/s2a2-20260916-005047`。
- 随源码归档的公共副本：`evidence-public/s2-a2/s2a2-20260916-005047`。
- environment.json：实际 OS/架构、锁定工具链、tested_code_commit、clean 状态。
- source-identity-before.json / source-identity-after.json：完整受测提交与 tracked 文件哈希，前后一致。
- gates.json / gates/*.log：全部命令、真实退出码、原始输出。
- workflow/*/requests.json、*.gbr、files-sha256.json：仅由测试生成或复制的公开合成样本、真实请求/响应、输入输出哈希，无私有 CORE10。
- release-sha256.txt：受测 release 二进制摘要；results.json：schema_version=2 局部结果。
- 本报告/公共证据在受测提交之后另作文档证据提交，不把后续包装提交冒称为受测代码提交。

## 明确限制和剩余工作

1. 数值近重合/不能可靠分辨的 Macro 组合返回 UNSUPPORTED_FEATURE；工作预算 2,000,000，超限 RESOURCE_LIMIT。上述边界有测试，不通过粗框猜 HIT。
2. 对象线性扫描；Macro 每次查询按共享光圈准备，边界组合有二次复杂度和预算，没有跨 revision 缓存。未做十万/百万对象实时性能承诺。
3. RectangularSweep 斜向仅验证精确算法；导入/制造编辑/输出仍保留原轴向限制。
4. GUI、文件对话框/拖放、新 renderer、选择高亮和 Mac Metal 交互本轮未执行；留到 S2-A.3。Windows deferred / not executed。
5. 外部 RCam_S2A1_20260916_Review.md 未提供，本轮不声称已读取其结论。已有代码/原始证据由当前测试重新核验。
6. 原96有效用例、AT-079退役身份、完整步骤、容差及 required_platforms 保持。S2-A.2 PASS 不等于完整 AT/CORE10/双平台 V1 通过。
