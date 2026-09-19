# S2-B3 GeometryMetrics 实施与验收报告

**代码、自动门禁及主要Mac原生闭环通过；1000对象原生指标验收受现有renderer预算阻塞，S2-B3暂不签署全部完成。**
Windows deferred / not executed；完整AT、CORE10和双平台V1均未通过。

## 身份与范围

运行ID：s2b3-final-20260920。macOS26.5.1(25F80)、arm64、Apple M1/Metal、Rust1.89.0。
受测提交：`dbde56e777a8b5176a552b84739efb3db3aa032b`，隔离检出 `/private/tmp/rcam-s2b3-final-20260920`，
分支 `codex/s2b3-geometry-metrics`。它包含启动时未提交的S2-B2基线和本轮实现。
原工作区原有改动保留，未重置或切换当前分支；已把隔离受测提交取回为本地分支codex/s2b3-geometry-metrics，未更改当前索引/工作树。原始diff/status见开发evidence。
1475个跟踪文件在final gates和native验证前后逐字节一致，隔离Git状态始终clean。
源码包按这个精确受测提交生成；最终报告/本轮证据单独交付，不冒称包内已有后写的报告。

阶段S2-B3，关联R04/R05/R09/R10/R11/R14/R16/R18/R19/R21/R22；
局部AT-010/011/012/013/014/015/016/026/030/033/034/035/038/039/040/041/054/088/089/090/091/095/097。
改动仅core指标与几何helper可见性、service会话派生缓存/API、app后台属性、对应测试、打包脚本/文档。
未改parser/writer/制造事务算法、Cargo.lock、依赖、96个用例身份、阈值或required_platforms。

## 实现与边界

- C/R/O/P Flash与中心圆孔解析公式，孔边计周长；局部刚性不变、正均匀scale按平方/一次方缩放。
- 圆形Line为真实capsule；零长为圆。RectangularSweep取真实解析凸多边形，非AABB。
- Arc零扫掠dot、全圆annulus/filled disk和可证明无重叠open tube精确；偏差连接、厚弧内偏移和major端帽重叠明确unsupported。
- Region复用canonical制造曲线，Green积分和真边长；移除精确反向cut-in连接，按既有winding识别材料边界。
  模糊闭合/相交或需union证明的多轮廓拒绝；3*N²工作量预收费。不是通用Boolean Engine。
- Macro首版明确unsupported；不复用含内部线的命中边界算周长，不对原语面积做错误加减。
- 专用objects.metrics只读DTO，保持请求顺序，未知ID结构化NOT_FOUND、重复ID拒绝、资源超限整次失败。
  每次最多10000对象/2000000解析工作；summary仅合计exact项，unsupported无面积/周长字段。
- 会话非序列化shape token，Move/Rotate/Mirror保留，Duplicate共享，Delete转history引用，Undo/Redo恢复。
  4096项FIFO结果与20000项/2MiB身份预算，关闭清理；未将revision用作全量cache失效键。
  现有mutation入口仅刚性/结构，未来尺寸/光圈/节点命令必须扩展前后shape identity历史，不宣称这些命令已实现。
- GUI现有串行worker按document/revision/selection变化查询，UI帧只显示结果；任务序号校验后发布。
  单选显示面积/周长，多选明确“对象面积合计/对象周长合计”，部分支持注明已精确项，不宣称最终开口面积。
- Python标准库确定性source ZIP，固定entry顺序/时间/权限；Unicode filename bit11，source/package双manifest。
  真正extractall后中文路径及source_manifest.py --check通过；不含.git/target/.tools/private。

## 执行结果

工作区 **324 passed / 0 failed / 5 ignored**。4个GPU忽略项另行原生执行通过；私有CORE10工作流忽略项未执行。
新增21个core公式/边界测试、2个service cache测试、1个真实JSON/IO工作流、2个app面板/worker测试。
7个Python审计工具测试和1个ZIP测试通过，不能等同CORE10通过。
服务正常依赖树无egui/eframe/wgpu/winit/objc2。日志保留每条实际命令、退出码、耗时。

| 实际命令 | exit |
|---|---:|
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --all-targets --locked` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo test --workspace --locked` | 0 |
| `cargo test --locked -p editor-core --test s2a_hit_test` | 0 |
| `cargo test --locked -p editor-core --test s2b_rect_selection` | 0 |
| `cargo test --locked -p editor-service --test s2b_select_rect_workflow` | 0 |
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
| `cargo test --locked -p editor-core --test geometry_metrics` | 0 |
| `cargo test --locked -p editor-service --test metrics_workflow` | 0 |
| `cargo test --locked -p editor-app --bin editor-app` | 0 |
| `python3 scripts/test_package_source.py` | 0 |
| `cargo test --locked -p editor-app native_metal -- --ignored --nocapture` | 0 |
| `cargo test --locked -p editor-app --example s0-demo native_gpu_coverage_regressions -- --ignored --nocapture` | 0 |

## 原生结果与证据

公开证据：`evidence-public/s2b3-final-20260920/`；完整本机包与日志：`evidence/s2b3-final-20260920/`。
详见gui/observations.md与21张原始截图。九种几何数值符合独立公式；Macro明确不可计算。
Move、Duplicate、Delete/Undo、完整/部分多选合计、Save As/Reopen均已通过原生操作。
Rotate/Mirror按任务允许使用真实service专项证明；未实现或模拟GUI按钮。
源Gerber hash保持；原生输出两次包含移动后的圆坐标(12.844148,23.388770)，服务专项另证明查询前后writer bytes相同。
源码/二进制/输入输出hash及环境见source-*.json、artifact-hashes.json、environment.json。

## 未完成与风险

- **B1 / S2B3-LARGE-SELECTION：**固定1000圆样本被既有renderer像素×对象预算拒绝；见20-large-open.jpg。
  大selection指标交互/响应尚无原生通过证据。保持原样本、原门槛，未扩展生产renderer或伪造性能通过。
- S2-B2修饰键点击、held drag Esc/blur/PointerGone补证NOT TESTABLE：CUA无modifier-click与可保持按下的输入接口。
- Windows deferred / not executed；完整CORE10、全套AT、双平台V1、跨机安装/签名/公证未完成。
- 外部RCam_S2B2_01b4751_Review.md未提供，没有引用其结论。
- Macro与不确定Region/Arc返回unsupported是公开功能边界；不新增最终Layer Area、Grid/Snap、文字或多格式实现。
- 现有native启动日志保留旧“S2-B2”静态标签，不能拿标签作为构建身份；身份以冻结commit和SHA-256为准。

本轮停止在上述S2-B3范围；不把局部通过改写成阶段全部通过。

交付源码包：`exports/RCam_S2B3_dbde56e_source.zip`，解包后的source/package manifest均PASS，SHA见package-verification.json。
