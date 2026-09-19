# S2-A.3 Mac GUI 实现与验收记录

## 结论

**Mac-first S2-A.3 本轮基础编辑闭环通过；不代表 Windows 或双平台 V1 通过。**
运行ID `s2a3-final-20260919`，2026-09-19，macOS26.5.1(25F80)、arm64、Apple M1 / Metal。
17项自动门禁通过；工作区256通过、0失败、3忽略。两个Metal专项另行真机执行通过。
原生Open、选择、移动、撤销/重做、Save As与重开由CUA操作并记录；Finder拖放、真实双指平移和
捏合缩放由用户明确回复“三项操作均正常”。随后窗口实际显示“中文 # 输入.gbx”，日志对应doc-3。
两类证据分开记录，用户手势确认不冒充自动化物理事件生成。

## 实现与范围

真实编辑器替换默认S0演示。原生Open与eframe dropped files走同一后台ApplicationService；
左侧图层、中央语义画布、右侧对象属性与数值Move、历史、当前图层新路径Save As。
使用自有只读render.snapshot；正常服务依赖树无egui/eframe/wgpu/winit/objc2。
相机f64毫米和logical points/mm，选择容差6 physical px；只在点击时查询exact hit_test。
最上可见层首个非空结果的最后ID；Clear与锁定可选，锁定禁止新Move，历史仍可恢复。

显示支持当前C/R/O/P孔洞/局部变换、Macro1/4/21、Line/矩形Sweep/Arc/Region；
局部孔洞与图层Clear独立且保持曝光顺序。Region自适应细分仅用于显示，不参与制造/命中/导出。
超限/精度不足整幅拒绝并禁编辑和保存。ResourceLimit.actual现在报告真实累计attempted，有精确值回归。

阶段S2（接入既有S3 Move/历史及S4导出）。Rxx/局部AT编号和允许修改模块见
[ADR0016](adr/0016-s2a3-gui.md)。96用例schema_version=2、AT-079退役、required_platforms保持原样；
下表是本任务17步原生清单，不是96个完整V1用例的通过声明。

## 代码身份与交付

实现提交 `bbc2523`；冻结清单后的测试候选：
**`728e31be719c294b42666811a4cd2c0c44bae5ab`**。

原工作区另有AGENTS.md修改和DESIGN_V1.md/IMPLEMENTATION_PLAN.md删除，均保留且未纳入GUI提交。
在 `/private/tmp/rcam-s2a3-acceptance` 隔离干净检出执行最终门禁。source-before、
source-after-gates、source-after-gui记录相同1157个跟踪文件哈希，Git状态为空。
源码包使用提交中原始设计文档，不含用户尚未提交的文档调整。
后续交付提交仅增加报告/公共证据和源码清单对JPEG/GBX的收录，不修改产品Rust/WGSL/Cargo源码。

证据：[evidence-public/s2a3-final-20260919](../evidence-public/s2a3-final-20260919/)。
应用二进制SHA256：`052bd36bf80b7233c4599d96b318f6bd7b2a66f7db0fa207261ac8b80d8a4260`。

## 实际命令与退出结果

使用仓库已有Rust1.89.0工具链与Cargo.lock。gates/和metal/保存逐命令原始输出及commands.json。

| 命令 | exit |
|---|---:|
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --all-targets --locked` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo test --workspace --locked` | 0 |
| `cargo test --locked -p editor-core --test s2a_hit_test` | 0 |
| `cargo test --locked -p editor-service --test s2a_hit_test_workflow` | 0 |
| `cargo test --locked -p editor-service --test s2a_bounds_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b2b_transform_workflow` | 0 |
| `cargo test --locked -p editor-service --test s1b2c_workspace_workflow` | 0 |
| `cargo test --locked -p editor-service --test automation_contract` | 0 |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 |
| `cargo tree --locked -p editor-service --edges normal` | 0 |
| `cargo build --release --locked -p editor-app` | 0 |
| `python3 scripts/source_manifest.py --check` | 0 |
| `python3 scripts/test_audit_core10.py` | 0 |
| `cargo test --locked -p editor-app native_metal_semantic_renderer -- --ignored --nocapture` | 0 |
| `cargo test --locked -p editor-app --example s0-demo native_gpu_coverage_regressions -- --ignored --nocapture` | 0 |

工作区256 passed / 0 failed / 3 ignored；忽略项不计通过。
新renderer及原S0 GPU专项另行显式执行均PASS。私有CORE10流程忽略项未执行。
新renderer对14个公开fixture执行12594个Metal/语义覆盖点比较，其中8个为独立孔洞覆盖断言，
并创建实际fragment pipeline。旧S0专项保留独立局部/跨层曝光真值；不宣称新GUI完整多层实测。
Python审计工具7个测试PASS不等于CORE10样本通过。服务正常依赖边界检查和源码manifest均PASS。

## 原生操作证据

所有截图为CUA原始JPEG字节，1182×768归一化截图，无重绘。
display-environment.json原始系统记录：主屏UP27R3，backing3840×2160、logical1920×1080，
144Hz，比例2。相机Retina自动测试覆盖pixels_per_point=2；不从截图尺寸倒推屏幕缩放。

| 步骤 | 结果 | gui/下的证据 |
|---|---|---|
| 01 Cold start | PASS | 01-cold-start.jpg |
| 02 Native Open .gbr | PASS | 02-open-fit.jpg |
| 03 Finder drop | PASS_USER_CONFIRMED | gui/user-confirmation.md, 15-after-user-native-check.jpg, native.log |
| 04 Fit | PASS | 02-open-fit.jpg |
| 05 Physical trackpad Pan/Zoom | PASS_USER_CONFIRMED | gui/user-confirmation.md, 15-after-user-native-check.jpg, native.log |
| 06 Flash selection | PASS | 04-move.jpg |
| 07 Hole center miss | PASS | 03-hole-no-selection.jpg |
| 08 Last exposure hit | PASS | 13-overlap-last-exposure.jpg: object-13 |
| 09 Hidden layer cannot be selected | PASS | 10-hidden-unselectable.jpg |
| 10 Locked selection, Move disabled | PASS | 11-locked-reselect.jpg |
| 11 Move dx=5 dy=-3 | PASS | 04-move.jpg: center10,20 ->15,17; revision1 |
| 12 Cmd+Z | PASS | 05-undo.jpg: revision2, dirty=false |
| 13 Shift+Cmd+Z | PASS | 06-redo.jpg: revision3, dirty=true |
| 14 Native Save As | PASS | 07-saved-source-and-target.jpg: dirty=false |
| 15 Reopen export | PASS | 08-reopened-position.jpg: center15,17 |
| 16 Workspace name/visibility does not dirty | PASS | 12-renamed-clean.jpg: revision0, workspace4 |
| 17 Chinese/space/#/.gbr/.gbx paths | PASS | Chinese/space/# .gbr native Save As/reopen; GBX native Finder drop user-confirmed and observed window title. |

额外：Clear环选中object-12并显示Exposure::Clear，见14-clear-selected.jpg。
GUI导出后原生重开，属性中心为15,17毫米；文件也含对应坐标。
两个输入副本与公开fixture逐字节一致。完整输入/输出/二进制哈希见artifact-hashes.json。
保存后source_path与last_saved_path分开显示，源文件未改变。

## 保留限制与未执行项

- Windows deferred/not executed；完整CORE10、本轮外部几何工具交叉复核和双平台V1未执行/未通过。
- 文本焦点下制造撤销隔离有代码防护，但本轮未单独录制原生焦点操作；IME/无障碍/完整快捷键用例不授予通过。
- eframe未启用AccessKit控件树：原生对话框通过AX操作，画布与属性使用截图坐标；完整辅助技术支持待验收。
- 服务在后台串行运行，有任务ID和busy，尚无内部取消检查点；不授予完整AT-063/064通过。
- 单文件，Workspace不持久化，只允许新路径导出；大规模性能、多选/拖动物体、网格/文字等留待后续。
- correctness-first逐像素renderer受预算限制；日志记录rebuild，不承诺十万对象性能。
- app未签名/公证，也未做跨机器发行验证；本包用于本机阶段测试，无用户字体/私有样本。
- 开发时的路径canonicalization断言、WGSL保留字、clippy告警均修复后重跑；
  开发原始日志保留于原工作区evidence/s2a3-development-*，本报告只声称上述最终候选结果。
