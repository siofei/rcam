# S0-A 独立审核与交付记录

日期：2026-09-14；运行ID：review-s0-20260914-2232；设计基线1.1，验收schema_version=2。

**结论：S0-A 的本机构建、有限语义与只读接口检查通过，macOS Metal 静态画布可运行；S0 整体仍阻塞，不能进入 S1 或标记 V1 通过。** 96个有效AT用例尚无完整通过项，AT-079只保留退役记录。

## 实施和职责

Luna编写产品代码及修复；主代理制定范围、只读盘点真实样本、独立代码审查、编写独立检查程序并运行验证。

关联 S0；R01/R03/R04/R05/R06/R08/R17/R18/R19/R20/R21/R22；AT-001–004、006–011、017–019、085–086、088 的前置/局部检查。代码改动限 core/io/service/app、Cargo/toolchain、合成fixture、CI和配套说明；完整设计/验收要求与门槛未降低。

交付：Rust工作区；真实gerber_parser的有限圆Flash适配器；f64毫米制造模型与顺序曝光/局部孔洞/跨层隔离；自有只读JSON服务；从服务快照传入对象数据的eframe/wgpu Callback演示。无编辑/撤销/文字/导出，无生产文件输出。普通输入与内建演示几何分开，不能相互污染。

## 环境及构建身份

Mac mini Apple M1（8核CPU/8核GPU）、16GB、macOS 26.5.1 (25F80)、arm64，CLT，项目隔离Rust 1.89.0。实际应用显示 Apple M1 / Metal / IntegratedGpu。当前26.5.1不能替代设计指定的macOS15证据。
工具链/缓存/target在项目 .tools；未修改用户全局Rust环境。当前目录尚无Git仓库，不能提供commit SHA；以 build-hashes.json 的逐文件SHA256冻结本次源代码、Cargo.lock与二进制。

最终release可执行文件SHA256：`0b3402292032daf9001e7bddb1168743bfdf5021fd2e3689981b8a4c6ed472d9`。
本地测试包：`.tools/RCam S0.app`；仅为原生启动/复核包装，未签署正式发行/公证验收，不是V1发行包。

## 主代理实际执行结果

下列命令在最终标题/字符修复之后执行，见 approved-commands.json、approved-01.log 至 approved-05.log：

| 命令 | 退出码 | 判定 |
|---|---:|---|
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --all-targets --locked | 0 | 通过 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 0 | 通过 |
| cargo test --workspace --locked | 0 | 13项测试通过，0忽略；含2项S0 automation_contract |
| cargo build --release --locked -p editor-app | 0 | 通过 |

单独执行：

| 命令 | 退出码 | 判定/证据 |
|---|---:|---|
| cargo test --locked -p editor-service --test automation_contract | 0 | S0真实parser/只读JSON契约通过；final-06.log，不等于完整AT-086–097 |
| cargo test --locked -p editor-service --test headless_workflow | 101 | 尚无此test target；未实现真实编辑/导出/重开，final-07.log |
| cargo tree --locked -p editor-service --edges normal | 0 | 正常依赖无egui/eframe/egui-wgpu/wgpu/winit；final-08.log |
| cargo run --offline --locked --manifest-path evidence/review-s0-20260914-2232/audit-harness/Cargo.toml | 0 | 主代理独立35项检查全通过；audit-approved.log |

独立35项覆盖合法2.6格式、平移坐标真值、英寸中心/半径、模态Y、未知/畸形输入、重复定义/坐标/文档保护、输入几何不被演示污染、JSON错误码与请求关联。独立程序在 evidence 下，不修改Luna产品测试以获得通过。

原生GUI：首次沙箱启动无法连接macOS窗口服务，日志 gui-launch.log 保留并终止该失败进程；原生启动成功，最终日志 gui-approved-launch.log 与 ../evidence/review-s0-20260914-2232/gui-approved.png。截图显示同一快照的7个对象/4层、D/C/D环、穿心线与局部孔洞、跨层Clear不擦除下层实心圆。截图为静态补充，不能替代制造几何/独立Gerber查看器验证。

拖动/缩放/动态裁剪：**阻塞**。Computer Use返回 `Computer Use permissions are not granted`，不使用其他输入注入机制绕过；未执行交互、不声称通过。截图权限可用，不代表输入控制权限已授予。真实Windows DX12、DPI/IME/设备恢复/性能均未执行。

## 审核发现与修复

保留早期失败日志；未删除失败输入。已修复：环孔测试未实际叠加图元；上游M前缀宽松解析；初始坐标默认为零；行号过滤偏移；重复/反序坐标及定义顺序；错误拒绝正常光圈/FS声明；科学计数法光圈；重复文档覆盖；普通导入追加演示图形；空GPU回调/背景色模拟孔洞；shader硬编码几何与位置选层；GPU超限静默漏画；默认字体导致关键说明方框。

S0采用可读英文控件，系统窗口标题中文。完整中文UI/IME/矢量字体仍未实现，不以此降低V1中文要求。

## 真实样本与后续门禁

用户授权目录 /Volumes/硬盘盒/0727SMT 只读本地使用。盘点2113个普通文件，识别971个Gerber候选，920个唯一SHA256；未解压档案。预先固定REAL30/CORE10身份并二次核对30份源哈希。私有manifest在 fixtures/private/manifest-s0-20260914-2232.json，未复制/修改原文件或上传。

词法初筛发现AM/旧图像命令/G74/SR，且核心集合包含这些风险；统计不等于语义判定。具体风险见 docs/adr/0002-real-sample-scope-risk.md。不能为了10/10改选简单样本；下一步必须先完成核心命令使用审计、明确范围扩展，并同步设计/用例/ADR，然后才能进入S1。

仍未完成：完整规范2024.05/2026.05相关差异表与独立工具核对；所有传递依赖/默认内嵌字体/分发文件许可归档；来源年代/独立几何真值；Windows及macOS15证据；交互权限；真实编辑保存闭环和CORE10 10/10；性能与发行。AT-001/002/003/004等不得由局部证据升级通过。

完整逐平台状态见 acceptance-results.json（185个平台结果槽、全部96有效用例保留）。本报告只批准当前有限技术原型可继续研究，不批准生产加工用途。
