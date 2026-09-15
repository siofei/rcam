# S1-B2c pre-GUI 模型收口交付与验收

**PASS：macOS 本阶段实现门禁。** 停止于 S1-B2c，下一轮可进入 S2-A；本轮没有实现 S2 GUI。

源码受测提交：`1b3b2a5ac4b312d1ced3b94393fd92ba73e914ac`。最终运行：`s1b2c-20260915-232401`。
包装/证据提交与源码提交分开；最终 ZIP 内 PACKAGE_INFO.json 记录两者完整SHA。

## 范围和改动

需求 R07/R10/R11/R14/R15/R19/R21/R22；关联 AT-022/039/040/041/060/062/086/088/090/091/095/097 的局部步骤。
改动 editor-core 制造模型/历史、editor-service 状态/DTO/入口、gerber-io 图层构造、相关回归与文档/证据工具。
无新增依赖；未修改 parser 支持范围、writer算法、渲染或制造容差。

- SemanticLayer 仅保留稳定 id 和有序对象；名称、visible、locked 移至每文档独立 workspace。
  真正的 SourceMetadata（包括 LN/image 信息）保留。
- layer.update 实现严格JSON字段、显式图层、1024 UTF-8字节名称预算、制造/workspace双版本检查。
  实际变化仅推进 workspace_revision；no-op 不推进版本，不产生制造历史、不清Redo、不改变dirty。
- 五种新制造编辑均由服务检查锁定；Undo/Redo 不读取当前锁，仍核对制造身份、顺序和 before/after。
- 纯工作区修改不触发制造保存/关闭确认，导出字节不变；重开恢复默认工作区设置。
- ADR 0012 冻结 S2 f64 精确 hit-test/bounds 边界；没有发布未实现能力。S0窗口保留为参考演示。
- README纠正 Duplicate/Delete=S1-B2a、Rotate/Mirror=S1-B2b；源码清单反映进入任务前已有的四份旧交接文档删除。
  旧文档仍在Git历史；本轮两份外部文档原文归档 docs/handoffs。

## 实际执行及结果

环境：macOS 26.5.1 arm64、Rust/Cargo 1.89.0，使用现有锁定工具链及缓存；非干净发行环境。
原始证据：`evidence/s1b2c-20260915-232401`；公共副本：`evidence-public/s1-b2c/s1b2c-20260915-232401`。

| 实际命令 | 退出码 | 日志（公共证据根下） |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | gates/01.log |
| `cargo check --workspace --all-targets --locked` | 0 | gates/02.log |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | gates/03.log |
| `cargo test --workspace --locked` | 0 | gates/04.log |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 | gates/05.log |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 | gates/06.log |
| `cargo test --locked -p editor-service --test s1b2b_transform_workflow` | 0 | gates/07.log |
| `cargo test --locked -p editor-service --test s1b2c_workspace_workflow` | 0 | gates/08.log |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | gates/09.log |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 | gates/10.log |
| `cargo tree --locked -p editor-service --edges normal` | 0 | gates/11.log |
| `cargo build --release --locked -p editor-app` | 0 | gates/12.log |
| `python3 scripts/source_manifest.py --check` | 0 | gates/13.log |

- 工作区 **181 passed / 0 failed / 2 ignored**。两个原有ignored入口分别为原生GPU和显式私有CORE审计，未计通过。
- 专项 S1-B1 **17/17**，S1-B2a **27/27**，S1-B2b **31/31**，S1-B2c **17/17**；接口契约 **4/4**，无界面流程 **2/2**。
- 正常服务依赖树没有 egui/eframe/egui-wgpu/wgpu/winit，见 dependency-audit.json。
- 96个有效用例、全部 required_platforms 和退役记录与23046c0逐项一致，见 spec-integrity.json。
- release构建成功，SHA-256：`21942cedbdec8e72228202de17e10958837c8e13006540532b7330fd3b4211af`。该程序仍是S0参考演示，未声称具有新GUI操作。

## 验证要点与证据身份

新增17项真实JSON测试包含要求的15项及版本冲突、能力/DTO往返。公开workflow记录请求、响应、输入、导出。
锁定失败测试在已有Redo分支上检查完整DocumentInfo与对象内容不变，随后Redo成功；锁定后Undo恢复原对象存储。
导出对照逐字节一致，并独立断言重开Flash中心为(2,3) mm；源文件字节保持，重开状态恢复visible/unlocked默认值。
制造编辑仍变dirty，隐藏状态不阻止显式编辑，成功导出后恢复clean并更新last_saved_path。

旧测试中的core锁断言迁到服务专项，符合新责任边界；原预算、错序守卫和制造几何回归保留，未删失败用例获取通过。
开发过程有一处新测试Flash字段名编译错误、一处clippy可折叠条件警告，修正后门禁全过；原始失败日志在development。

final gates开始前已形成源码候选，前后Git状态为空，HEAD一致；全部930个受管理文件逐字节匹配该commit。
两份source identity JSON相等；后续只增加公共证据、交付说明与清单，不改受测产品代码。
最终PACKAGE_INFO分别记录tested_code_commit、packaging_evidence_commit和实际包文件列表；包级SHA清单另行逐项核验。

## 未执行、未完成与风险

Windows/DX12 **deferred / 未执行**；macOS GUI编辑/Metal/IME/干净发行 **未执行**。
S2 GUI、文字、Production Renderer、精确点选实现、完整框选、edit.batch、跨文档编辑、脚本引擎 **未实现**。
CORE10完整编辑往返、独立外部工具本轮复核、release性能及长期稳定性 **未完成/未执行**；历史CORE-03/07/08与独立圆弧差异保留。
结构历史O(N)顺序守卫和64MiB预算保持，为后续大文件阶段技术债。
工作区设置目前仅会话内有效；关闭重开不恢复名称/显隐/锁定，符合本阶段边界。
不得据此宣称完整AT、双平台V1、生产加工资格或正式发行通过。
