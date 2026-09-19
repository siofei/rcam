# S2-B1 Mac 单对象直接操作交付记录

Mac-first S2-B1 实现、自动门禁和原生主闭环通过；不等于完整 AT、CORE10 或双平台 V1 通过。
最终受测提交 `aed930fcc2cdbfd729520bf213c3cf2b855a0dca`，运行 ID `s2b1-final2-20260919`。
环境 macOS 26.5.1 (25F80)、arm64、Apple M1 / Metal、锁定 Rust 1.89.0。工作区 273 passed / 0 failed / 4 ignored；
忽略项中的三个 GPU 测试另行原生执行通过，私有 CORE10 工作流未执行。Python 7项审计工具测试不等于 CORE10 通过。

## 范围与实现

阶段 S2-B1（复用现有 S3 编辑和 S4 保存服务）。R07/R08/R09/R10/R11/R14/R15/R18/R19/R21/R22；
局部 AT-022/025/026/030/032/035/036/038/040/041/043/054/086/087/090/091。
修改 editor-app 输入/临时状态/显示 uniform/WGSL、相应自动测试、文档与清单/打包脚本。
core、parser、writer、service 公共契约及制造算法未改，无新依赖。

- 单对象选择保持 Option；已选对象按下时在后台 exact hit_test，锁定/隐藏/busy/blocked/display_error 禁止开始。
- Drag 为 UI 临时值；4 physical px 阈值，f64 mm 位移，GPU uniform 预览保持有序曝光和局部透明；不重建 Scene、不逐帧查询或修改制造数据。
- 使用原始 PointerButton press 坐标；异步命中返回前已释放时保留终点。该修复来自原生快速拖动复测，不以测试通过掩盖真实问题。
- release 一次带按下版本/对象 ID 的 Move；零位移不提交，Esc/失焦/PointerGone/任务或视图切换取消，失败恢复显示且服务原子拒绝。
- Cmd+D 原位 Duplicate 选中新 ID；Delete/Backspace 删除，失败保留 selection。菜单/按钮共用服务；文本焦点隔离。
- VectorScene 正式 ADR 0017；保留用户原有设计修改并修复引用，重建已删除的阶段计划并更新当前 S2-B1。
  非 Gerber 与 Gerber 制造路径边界保持，S2-B2 精确 within/window 缺口已登记。96用例与平台门槛未改。

## 最终命令与退出结果

PATH/CARGO_HOME/RUSTUP_HOME 使用项目 .tools。原始输出位于 [公共证据](../evidence-public/s2b1-final2-20260919/)。

| 实际命令 | exit |
|---|---:|
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --all-targets --locked` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo test --workspace --locked` | 0 |
| `cargo test --locked -p editor-core --test s2a_hit_test` | 0 |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b2b_transform_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b2c_workspace_workflow` | 0 |
| `cargo test --locked -p editor-service --test s2a_hit_test_workflow` | 0 |
| `cargo test --locked -p editor-service --test s2a_bounds_workflow` | 0 |
| `cargo test --locked -p editor-service --test automation_contract` | 0 |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 |
| `cargo tree --locked -p editor-service --edges normal` | 0 |
| `cargo build --release --locked -p editor-app` | 0 |
| `python3 scripts/source_manifest.py --check` | 0 |
| `python3 scripts/test_audit_core10.py` | 0 |
| `cargo test --locked -p editor-app native_metal -- --ignored --nocapture` | 0 |
| `cargo test --locked -p editor-app --example s0-demo native_gpu_coverage_regressions -- --ignored --nocapture` | 0 |

服务正常依赖树无 egui/eframe/egui-wgpu/wgpu/winit/objc2。两个新 renderer Metal 模式分别对14个公开样本检查
静态与单对象预览位移：12594+12586 个覆盖点；旧 S0 GPU 覆盖回归另通过。CPU 制造几何为对照，静态含独立孔洞断言；
这不替代完整外部查看器或性能验收。所有旧 S0/S1/S2-A 回归保留。

## 原生证据

最终候选的 gui/native.log 与 01–11 JPEG 为实际原生操作和原始截图。初始13对象：
移动源圆 (10,20) → (15.022645,23.025691)，revision1/undo1；Undo/Redo保留ID；原位复制创建新ID，
拖动副本到 (18.956041,25.143672)，原圆不变。Delete后13对象/selection清空；Undo恢复14对象。
输入框内 Backspace/Cmd+D/Cmd+Z 保持 revision7；锁定层拖动不改变制造版本。
Save As → Reopen 后14对象及上述坐标一致，另有独立输出坐标断言和源SHA保护。
重开后再次快速拖动成功并Undo，最终日志状态 doc-2 revision2、dirty=false、undo0/redo1。
源码身份前后1209跟踪文件逐字节一致，Git clean；运行包与最终release二进制SHA相同。

**取消手势证据边界：**用户在首候选3457a75上实际执行按住拖动Esc、切换窗口后释放，原文确认“都恢复原位、制造版本不变”。
见 gui/user-confirmation.md 与 prior-candidate。最终 aed930f 的取消自动回归再次通过；不声称用户在最终候选重复做过手势。
原生预览跟随由真实拖动与用户取消手势支持，未录制逐帧视频；无高对象数性能结论。

## 未完成项与剩余风险

- Windows deferred/not executed；不授予完整V1、CORE10或全量AT通过。
- 外部 RCam_S2A3_025e3a3_Review.md 未提供，未读取或引用其结论。
- 多选/框选、Rotate/Mirror GUI、Grid/Snap/测距、文字、production renderer留后续；当前仍有每帧O(N)预算扫描和显示资源上限。
- 原生任务只支持同步服务后台串行执行，未实现完整内部任务取消或脚本运行时；发行包未签名/公证。
- 首次Metal沙箱运行因找不到适配器失败，原生权限重跑通过；开发失败保留在 evidence/s2b1-development-20260919。
- 最终源码/证据交付提交只修改文档、公共证据和manifest，产品Rust/WGSL/Cargo仍精确对应受测提交。

完成本轮后停止，不启动 S2-B2。
