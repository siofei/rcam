# S2-C1 验收报告（Mac-first，局部阶段通过）

2026-09-20。基于 9779a45，范围 [S2_C1_PLAN.md](S2_C1_PLAN.md)，决策 ADR0022。
需求 R08/R10/R11/R13/R16/R17/R18/R19/R21/R22；局部 AT-023/024/025/031/032/039/040/044/045/062/075。

## 交付与结果

实现有界自适应视觉网格、明确步长的 f64 Grid Snap、鼠标制造毫米坐标、两点直线测距。
Grid/Measure 留在 app；制造模型、writer、服务协议和 manufacturing renderer 未改，无新依赖。
单选/多选以抓取点得到一个共同 snapped delta，preview 不修改模型，release 只产生一个 Move/Undo。
数值 Move 不受 Snap 影响。测距、网格显隐、步长和吸附开关不改变 revision/dirty/history/writer bytes。

原 S2-C1 闭环测距输出 ΔX=3、ΔY=4、Distance=5 mm；动态 B、固定 B、Esc 清除和文档重开清除均验证。
实际控件检查发现 egui Escape 先释放输入框焦点的问题，已在 raw_input_hook 保留事件时焦点并修复；
最终 native phase 221 真实 Key 事件断言测量保留，phase 23 无文本焦点时 Esc 才清除。
密集图形下的读数重叠也已修复为独立底色，并使用带系统中文 fallback 的字体族。

S2-C2 期间按用户追加需求扩展该 app-only 测距：完成的标注可同时保留多条，每条线中
显示距离与相对世界 +X 轴的逆时针角度，Esc 清除全部。原 S2-C1 运行记录保留为历史证据；
追加实现、测试与原生截图见 `S2_C2_REVIEW.md` 及其独立运行目录。

## 实际执行

运行根目录：`evidence/s2c1-20260920-final3/`（本机证据，不上传）。
macOS 26.5.1 / arm64 / Apple M1 / Metal；Retina ppp=2；physical canvas=1600×900。
最终二进制 SHA-256：`274cd30ba64ff1619a73773e9b857ed3d7ff13b331d0f43a11033a6daba35304`。
测试包：`exports/S2C1-macos-final3-20260920/RCam.app`，内容与受测 release 二进制一致；未签名/未公证测试包。

以下最终命令全部 exit 0，原始输出依次为 `gates/00.log`—`08.log`：

1. `cargo fmt --all -- --check`
2. `cargo check --workspace --all-targets --locked`
3. `cargo clippy --workspace --all-targets --locked -- -D warnings`
4. `cargo test --workspace --locked`：339 passed，0 failed，8 ignored。
5. `cargo test --locked -p editor-service --test automation_contract`
6. `cargo test --locked -p editor-service --test headless_workflow`
7. `cargo tree --locked -p editor-service -e normal`：正常依赖无 egui/eframe/wgpu/winit/rfd。
8. `cargo build --release --locked -p editor-app`
9. `cargo test --release --locked -p editor-app native_metal_reference_production_pixel_parity -- --ignored --nocapture`：288 组 ×16384 pixels，RGBA 零差异。

环境变量与可复跑脚本见计划。普通 suite 的 ignored 不计通过；本轮所需 Metal parity 和原生窗口另行显式执行。

从最终 `.app/Contents/MacOS/editor-app` 运行 `RCAM_NATIVE_BENCH=s2b32 RCAM_S2C1_GATE=1`，
`RCAM_BENCH_OUT` 指向本轮 native 目录，exit 0。
`python3 scripts/validate_s2c1_native.py evidence/s2c1-20260920-final3/native`，exit 0；2501 个活动帧，7 条工具记录。

## P1K 原生性能回归

10 秒预热，30 秒 Pan/Zoom；Grid ON；1000 selected Drag ≥10 秒×3；首轮 Snap OFF，后两轮 ON。

| 轮次 | 活动帧 p95 ms | release→目标 revision surface GPU complete ms |
|---|---:|---:|
| 1 | 27.329000 | 50.698958 |
| 2 | 27.566875 | 73.681000 |
| 3 | 27.464125 | 74.208958 |

阈值仍为 50ms / 300ms；未删慢帧、未改阈值。所有坐标、单事务、Undo、Metrics 和 writer 不变量通过。
时延是 surface 提交/present 返回后的 GPU 完成保守上界，不是显示器扫描时刻。

## 证据与可追溯性

- `gates/gates.json`：命令、exit、环境与 binary hash；`tested-code-hashes.json`：66 个编译输入哈希。
- `native/native-pan-zoom.json`、`native/native-drag-3x10s.json`、`native/release-latency.json`：原始帧数据。
- `native/s2c1-tools.json`：Measure/焦点/单选 Snap/重开的制造状态与断言。
- `native/baseline.gbr`、`after-navigation.gbr`、`after-tools.gbr`、`after-undo-*.gbr` 全字节一致；
  `single-snapped.gbr` 仅含实际 Move，重开中心 (21,14) mm。
- `native/surface-*.ppm` 保留原始 surface 截图，PNG 为无损格式转换；共 19 张。
- `native-validation.json`、`source-identity.json` 与 `EVIDENCE.sha256` 绑定最终结果、clean commit、源码和文件。
- CUA 真实工具栏 smoke 记录见 `controls-observations.md`；它用于发现焦点缺陷，最终修复由同二进制 native Key 事件回归确认。

开发期 run1/final/final2 保留不覆盖：初次测试因保存后 Undo 的脏基线而误用直接重开，修正为明确关闭；
沙箱内无 Metal adapter 的尝试记录为环境受限，最终在原生环境执行。旧结果不充当最终二进制证据。

## 未完成与剩余边界

Windows deferred / not executed；不是双平台 V1 通过。Object Snap、英寸显示、临时吸附修饰键、
完整 AT-044/045、PMIX 完整 AT-075、P100K renderer、CORE10 全编辑往返、文字、多格式和发行签名未在本轮完成。
视觉网格可抽稀，实际 Snap 步长不变；吸附的是抓取点，不宣称任意对象中心都会落格。
自动原生 harness 设置工具栏状态并注入真实 egui Pointer/Key 事件；不冒充人手/触控板/DPI 多显示器全覆盖。
完成 S2-C1 后停止，不自动进入后续阶段。
