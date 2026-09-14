# S0 基线登记

日期：2026-09-14。此文件记录实际环境和未完成门禁，不是验收通过报告。

| 字段 | Windows A | macOS B（本机） |
|---|---|---|
| 硬件 | 阻塞：未提供 | Mac mini Macmini9,1，Apple M1 8核 CPU/8核 GPU，16 GB |
| OS | 阻塞：未提供 | macOS 26.5.1，25F80，arm64 |
| 编译基础 | 未执行 | CommandLineTools，项目 .tools 安装 Rust 1.89.0（29483883e），锁定文件已创建 |
| GPU | 未执行 | 系统报告支持 Metal；应用实际显示 Apple M1 / Metal / IntegratedGpu，已原生启动 |
| 显示器/DPI/刷新率 | 未登记 | 系统探测未返回显示器，待 GUI 实测 |
| 性能画布 | 1600×900，未执行 | 1600×900，未执行 |
| 基线 OS 覆盖 | Windows 10/11 原生仍需实测 | 当前 OS 不替代设计要求的 macOS 15 证据 |
| 独立查看器 | 未准备 | 未验证 |

原始探测：sw_vers、uname -m、xcode-select -p、system_profiler SPHardwareDataType SPDisplaysDataType（剔除序列号等身份信息）。首次检查 PATH 无 cargo/rustc/rustup；不是已有 Rust 工程。依赖精确版本以 Cargo.lock、rust-toolchain.toml 和 DEPENDENCIES.md 为准。

## 数值及资源策略

设计规定 f64 毫米、核心比较容差 1e-6 mm、拟定输出 q=1e-6 mm、文字误差1e-3 mm、显示弦高0.35物理像素继续有效；S0 不实现 writer，输出整数位及范围须在 S1 通过规范/量化验证后冻结。S0 自身资源限制必须记录实际代码值，不能把设计的未来预算说成已实施。

性能门槛完整继承 DESIGN_V1 第14节；尚未生成 P10K/P100K/PMIX/PPOL/PSTRESS、未执行性能采样。测量将固定 release、机器、1600×900、10秒预热、60秒轨迹、3次重复和原始日志。

## 规范依据与差异审核

官方入口 https://www.ucamco.com/en/gerber/downloads 于本次读取同时列出2026.05和2024.05规范。2026.05第11.1节概述文字/错误修订，不足以证明 parser 完整兼容；2026.05第4.2节要求MO/FS在首条操作前且各一次，固定格式小数位为6；旧格式导入须单独按废弃选项规则处理，不得猜位数。

规范链接：
- https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf
- https://www.ucamco.com/files/downloads/file_en/456/gerber-layer-format-specification-revision-2024-05_en.pdf

已核读修订说明和坐标格式相关段落；完整相关差异表、规范文件哈希、Region/圆弧/旧格式逐项审查仍未完成，AT-003不得标通过。

## 样本

授权目录 /Volumes/硬盘盒/0727SMT；只读盘点记录置于 evidence/review-s0-20260914-2232/inventory.json。词法特征只是风险线索，不能替代真实 parser/语义/编辑往返。样本生成来源与年代不能根据路径日期臆造；缺失即登记未知。私有 manifest 不提交，不上传源文件。

2026-09-14 S0实际资源预算：parser输入2MiB/对象100,000；GPU固定容量4层/16对象，超限拒绝。均是原型边界，不能按V1通用输入能力宣传。主代理已执行35项独立语义/拒绝/只读接口检查，最终原始日志 audit-sixth.log，退出0；早期失败日志保留。
