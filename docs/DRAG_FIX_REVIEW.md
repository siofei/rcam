# S0 画布拖动修复复核

运行 ID：drag-fix-20260915-004722。阶段 S0；R08 / R18 / R19，构建关联 R01；AT-023 部分验证。允许修改范围：editor-app 导航及其回归测试、验证文档、源文件清单。没有改变制造几何、服务、文件读写和私有样本。

## 修复

S0-B-GUI-001 的原因是 egui 0.33.3 在松开鼠标的帧中将 drag_delta() 置零，同时 dragged() 返回 false。如果该帧还有最后一段移动，旧实现会遗漏它。现在仅在画布拥有拖动或刚结束拖动时读取该帧 pointer.delta()，一次计入完整位移。没有新增依赖。

新增 navigation_tests.rs，直接调用生产导航函数，通过真实 egui Context 输入帧覆盖：同帧移动并释放、分段移动、正反方向、释放后的空帧和悬停、画布外按下后释放及静止点击。

## 实际验证

环境：macOS 26.5.1 (25F80)，Apple M1 / Metal / IntegratedGpu；Rust 1.89.0，锁定 Cargo.lock。

修复前回归退出 101：期望 [107, 59]，实际 [7, 9]。修复后两项导航测试通过，四种方向/分帧组合位移均精确相等，且没有重复累加。

以下命令全部退出 0；原始输出位于本次 evidence 目录的 checks/01.log 至 09.log，完整命令和耗时见 checks/commands.json：

```
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-app navigation_tests -- --nocapture
cargo test --locked -p editor-service --test automation_contract
cargo test --locked -p editor-service --test dependency_boundary
cargo build --release --locked -p editor-app
```

工作区测试：31 passed，0 failed，1 ignored（原生 GPU 专项，本次未重跑）。headless_workflow 尚未实现，本次未执行，不能声明编辑导出往返通过。

实际运行了更新后的 `.tools/RCam S0.app`。CUA 拖动 [450,250] → [500,300] 后，中间圆心从截图约 [703,438] 移至 [753,488]，正向位移完整，缩放保持 18，revision 保持 0。截图为 01-initial.jpg、02-forward.jpg。之后工具反复返回 `noWindowsAvailable`，重新连接、Raise 和重置工具会话仍未恢复坐标操作；03-final-state.jpg 确認窗口仍显示修复版。原生反向拖动验证阻塞，不能把自动化事件测试当作反向 GUI 实测通过。

安装二进制 SHA-256：`ad7176665a65f8a1311615af58b90d8be060968cfb4a108abacd14f361e752ad`。旧二进制保存在本次 evidence/previous-app-binary，历史报告未覆盖。

## 结论与边界

拖动丢失释放帧位移已修复，回归通过；macOS 原生正向拖动通过。Windows 原生及 macOS 反向 GUI 操作未完成。AT-023 仍为部分验证，Fit 等完整导航要求尚未实现，不标记完整用例或双平台 V1 通过。

证据：`evidence/drag-fix-20260915-004722/`。
