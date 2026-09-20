# S2-C1 Grid / Snap / Measure — Mac-first

2026-09-20；范围决策见 [ADR 0022](adr/0022-s2c1-grid-snap-measure.md)。
本轮承接用户提供的 S2C1 下一任务说明与 9779a45 复审报告；上一阶段结论不代替本轮证据。

阶段 S2-C1；需求 R08/R10/R11/R13/R16/R17/R18/R19/R21/R22。
局部 AT-023/024/025/031/032/039/040/044/045/062/075。
允许模块：editor-core/grid（纯数值）、editor-app（workspace/tools/drag/native evidence）、文档与验收脚本。
没有修改 Manufacturing Model、Gerber writer、服务协议、GPU manufacturing renderer 或依赖。

## 可执行闭环

- 工具栏显示/隐藏网格，输入有限正数步长 mm，显式 Grid Snap ON/OFF。
- 网格覆盖层按缩放抽稀；Snap 保留明确步长，不随显示密度改变。
- 鼠标拖动按抓取点吸附，整个选择集用唯一 f64 delta；preview 不提交，release 一个 Move。
- 属性栏数值 Move 不吸附。Esc/失焦取消沿用原手势契约。
- “测距”：点击 A、移动预览 B、点击固定 B；第三次点击重新开始；Esc 清除。
- 测距为两点直线，显示 A/B、ΔX、ΔY、Distance；Grid Snap 状态显式可见。
- 光标在画布内显示 f64 制造毫米坐标，离开画布隐藏；读数不用于重新量化编辑。
- Grid/Measure 只在 app，切换文档清除测量。Overlay 不参与制造、命中、导出或 Undo。

## 验证与重跑

使用项目锁定工具链：PATH=.tools/cargo/bin，CARGO_HOME=.tools/cargo，
RUSTUP_HOME=.tools/rustup，CARGO_TARGET_DIR=.tools/target（全部解析为绝对路径）。

`python3 scripts/run_s2c1_gates.py --out <新的运行目录>/gates`
记录 fmt/check/clippy/workspace tests、两个 service 集成入口、normal dependency tree、release build、Metal parity。
GPU 命令需在允许访问本机 Metal 的环境执行；沙箱中无 adapter 不计为 GPU 通过。

最终原生运行（以新的输出目录，禁止覆盖旧运行）：

```sh
RCAM_NATIVE_BENCH=s2b32 RCAM_S2C1_GATE=1 RCAM_BENCH_OUT=<绝对路径>/native .tools/target/release/editor-app
python3 scripts/validate_s2c1_native.py <绝对路径>/native
```

复用原 native gate：1600×900 physical、ppp=2、10 秒预热、30 秒 Pan/Zoom、
1000 selected Drag 10 秒×3；p95 ≤50ms、release ≤300ms，不删慢帧。
Grid ON；第一轮 Snap OFF，后两轮 ON。随后真实 egui Pointer/Key 事件驱动测距、Esc、单选吸附拖动、Undo、保存重开。
新增 text-focused Escape 保留测量的真实键盘事件回归。
工具栏状态由 harness 设置，不把它声称为人工点击工具栏；另做真实控件 smoke。
所有输出使用公开 synthetic P1K；不读取/上传私有 Gerber。

## 保留边界

Windows deferred / not executed。Object Snap、英寸显示、P100K renderer、PMIX 完整 AT-075、
CORE10 全编辑往返、文字和多格式未纳入本轮。96 个有效用例与最终双平台 V1 门槛不变。
最终判定与日志位置见 S2_C1_REVIEW.md。
