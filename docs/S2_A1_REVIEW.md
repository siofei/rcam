# S2-A.1 制造边界查询交付记录

**本轮小闭环 PASS；S2-A 整体未完成，不能进入 S2-B。**

## 范围与实现

阶段 S2-A.1（Mac-first），需求 R04/R07/R08/R19/R21/R22；关联 AT-010/012/013/014/015/016/022/023/039/054/088/089/095 的局部前置步骤。
修改 editor-core 边界计算、editor-service DTO/查询、对应测试、设计/API/验收说明及源码清单。无新依赖。

- 新增 document.bounds/layer.bounds：f64 mm、revision 绑定、严格参数、空内容 null 和结构化 NOT_FOUND。
- C/R/O/P 和 Macro Flash 组合局部变换；线段/矩形扫掠含真实宽高；圆弧只含 sweep 极值和原端点接线；Region 复用既有 canonical contour。
- 图层边界包括 Dark/Clear，不依赖显隐/锁定，不改变制造 revision/dirty/历史。
- Macro 为全部 Dark primitive 的保守包络；局部 Clear 可能使框内留白，不能当成最终可见几何或点选真值。

## 实际验证

环境：macOS 26.5.1（25F80）、Apple Silicon arm64、Rust/Cargo 1.89.0；复用项目现有工具链缓存，非干净发行环境。
运行 ID：`s2a1-20260915-235810`；原始证据：`evidence/s2a1-20260915-235810`。
受测的是未提交工作区，不能冒称 clean commit；environment.json 记录基线 HEAD 与状态，tested-code-hashes.json / code-identity-after.json 核对全部 Rust 源码、Cargo 文件及工具链声明前后一致。

- 工作区：**200 passed / 0 failed / 2 ignored**。
- 新增专项：核心 12/12、服务 7/7。独立数值真值覆盖标准 Flash、旋转长圆/多边形、偏心 Macro、圆弧方向/整圆/零扫掠/半径偏差、Region、多层和空层。
- 服务经真实 JSON/本地文件打开、查询、移动、Undo、锁定后 Redo、导出重开并核对边界，源字节保持；读取前后 DocumentInfo 不变。
- 6 份既有公开合成文件经真实 parser 的边界核对通过（Macro、矩形扫掠、Region、cut-in、精确/偏差圆弧）。未将它们宣称为 CORE10 真实样本编辑验收。
- 服务正常依赖树审查通过：无 egui/eframe/egui-wgpu/wgpu/winit；release 构建通过。

| 实际命令 | 退出码 | 原始日志（证据目录内） |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | `gates/01.log` |
| `cargo check --workspace --all-targets --locked` | 0 | `gates/02.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `gates/03.log` |
| `cargo test --workspace --locked` | 0 | `gates/04.log` |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 | `gates/05.log` |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 | `gates/06.log` |
| `cargo test --locked -p editor-service --test s1b2b_transform_workflow` | 0 | `gates/07.log` |
| `cargo test --locked -p editor-service --test s1b2c_workspace_workflow` | 0 | `gates/08.log` |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | `gates/09.log` |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 | `gates/10.log` |
| `cargo test --locked -p editor-service --test s2a_bounds_workflow` | 0 | `gates/11.log` |
| `cargo test --locked -p editor-core --test s2a_bounds` | 0 | `gates/12.log` |
| `cargo tree --locked -p editor-service --edges normal` | 0 | `gates/13.log` |
| `cargo build --release --locked -p editor-app` | 0 | `gates/14.log` |
| `python3 scripts/source_manifest.py --check` | 0 | `gates/15.log` |

构建时 PATH 指向项目 `.tools/cargo/bin`，CARGO_HOME / RUSTUP_HOME 分别使用现有 `.tools/cargo` / `.tools/rustup`。专项通过 RCAM_S2A_EVIDENCE 保存实际请求、输入/输出与 SHA-256。

## 证据索引

- `gates.json` 与 `gates/*.log`：完整命令、退出码、耗时、原始输出。
- `workflow/*/requests.json`：实际 JSON 请求/响应；`files-sha256.json`：本轮生成/复制样本和输出摘要。
- `tested-code-hashes.json`、`code-identity-after.json`：受测代码身份；`release-sha256.txt`：release 二进制摘要。
- `results.json`：schema_version=2 的局部检查记录，不授予完整 AT 通过。

## 未完成与下一闭环

1. 下一项是 S2-A 的 objects.hit_test：精确 f64 几何、局部孔洞/变换、明确 tolerance、Clear 可选及稳定曝光顺序，并验证编辑/历史后的命中。
2. 新 GUI、真实路径打开/拖放、图层面板、相机、单选高亮、属性/数值移动/历史/另存及后台任务仍未完成；现有应用仍是 S0 demo，本轮未做 GUI 实机验收。
3. Windows deferred / not executed；Mac Metal GUI、完整 CORE10 往返、性能和双平台 V1 仍未完成。
4. 包络查询按引用几何量线性扫描，无新增缓存/空间索引；不作百万图元性能声明。
5. 原有 96 个有效用例、AT-079 退役身份、required_platforms、阈值和失败历史保持。未自动提交、打包或上传。
