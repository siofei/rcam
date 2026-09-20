# S2-C2 验收报告（Mac-first，阶段性通过）

2026-09-20。范围见 [S2_C2_PLAN.md](S2_C2_PLAN.md)，决策见
[ADR 0023](adr/0023-s2c2-transform-gui.md)。基线 commit 为
`577b670aa888e0d543ce6c1ff024ce0b08c18ea6`；本轮位于
`codex/s2c2-transform-gui`，交付时仍为未提交工作树，因此不把该基线写成“已包含 S2-C2”的 commit。

## 结论

Mac-first S2-C2 Rotate / Mirror GUI 小闭环阶段性通过。属性面板经既有 `ApplicationService`
接入 `objects.rotate` / `objects.mirror`；选择集默认 Pivot 与镜像轴来自所选对象制造 bounds union
center，而不是对象中心平均值、renderer mesh 或屏幕像素。任意有限角、快捷 ±90°、世界原点与自定义
Pivot、Horizontal / Vertical 镜像均已接入。多选一次操作只提交一个有序 SelectionSet 请求、一个
revision 与一个 Undo；Grid Snap 不读取或改写 Transform 参数。

用户追加的测距需求也在同一闭环完成：完成的两点测量可同时保留多条，每条线段中部显示
毫米距离与相对世界 +X 轴的逆时针角度（归一化为 `[0°, 360°)`）；Esc 一次清除全部。
它仍是 app-only Overlay，不改变 revision / dirty / history / writer。

本轮采用任务允许的无 preview 方案：只有 Apply 或镜像按钮会提交服务请求。不新增依赖，不修改
制造变换、writer、Renderer 或 96 个 acceptance case identity；AT-045 内容已同步新的多标注/角度要求。

Windows deferred / not executed。因此这里只能标记 Mac-first S2-C2 阶段性通过，不能标记完整 AT、
双平台 V1 或 Windows 通过。

## 自动验证

最终自动门禁在 macOS 26.5.1、Apple M1、arm64、Metal 上执行，证据根目录为
`evidence/s2c2-20260920-measure-final/gates/`：

- `cargo fmt --all -- --check`：exit 0；
- `cargo check --workspace --all-targets --locked`：exit 0；
- `cargo clippy --workspace --all-targets --locked -- -D warnings`：exit 0；
- `cargo test --workspace --locked`：353 passed / 0 failed / 8 ignored；其中 editor-app 为
  84 passed / 0 failed / 6 ignored；
- `s1b2b_transform_workflow`：31 passed / 0 failed；
- `automation_contract`：4 passed / 0 failed；
- `headless_workflow`：2 passed / 0 failed；
- `cargo tree --locked -p editor-service -e normal`：exit 0，正常依赖树不含
  egui / eframe / wgpu / winit / rfd；
- `cargo build --release --locked -p editor-app`：exit 0；
- 原生 Apple M1 / Metal production-reference exact RGBA parity：1 test passed，288 个组合逐像素 PASS。

第一次 sandbox 运行保存在 `evidence/s2c2-20260920-run1/`，其 Metal adapter 因运行环境不可用而失败；
它不计通过。`run2-native` 是 Transform GUI 首次完整原生通过；追加 Measure 要求后在
`measure-final` 上再次完整重跑，并以后者作为最终自动门禁证据。

新增 editor-app 纵向测试覆盖：制造 bounds union center、Flash、Arc、Region、RectangularSweep、
双轴镜像、多选单事务、锁定/跨层拒绝、Grid 分离、metrics 刚性不变量、Undo/Redo、Save As/Reopen、
文本焦点、0°与非有限输入。RectangularSweep 仅允许整数 90°旋转，37°保持原子失败。
测距测试另断言 3-4-5 的 `53.130102°`、四个主轴方向的 `0/90/180/270°`、两条保留及 Esc 清空。

同一最终 `.app` 另以 `RCAM_NATIVE_BENCH=s2b32 RCAM_S2C1_GATE=1` 运行原生窗口 harness，
输出到 `evidence/s2c2-20260920-measure-final/native-final/`，exit 0；
`python3 scripts/validate_s2c1_native.py .../native-final` 同样 exit 0。独立校验结果为 2286 个活动帧、
7 条工具记录、距离 `[3,4,5]` mm、角度 `53.13010235415598°`。该运行还断言
Retina ppp=2 下画布 1600×900、Esc/重开清空、工具操作前后制造字节不变。
一次预跑保留在同级 `native/`；它暴露长工具栏文本换行将画布压到 1600×870，因旧尺寸断言失败，
不计最终通过。压缩为单行提示后在新运行 ID 完整重跑通过，旧记录未覆盖。

## Mac 原生 GUI 证据

原生未签名测试包：`evidence/s2c2-20260920-measure-final/package/RCam.app`。使用公开 synthetic
`fixtures/synthetic/s2a3/gui_primitives.gbr`，未读取或复制 private 样本。详细观察与截图索引见
`evidence/s2c2-20260920-run2-native/gates/gui/observations.md` 与
`evidence/s2c2-20260920-measure-final/native-gui/observations.md`。

已实际操作并保存证据：

- Region +90°，随后一次 Undo 与一次 Redo；
- Region Horizontal 与 Vertical mirror，轴坐标明确显示为选择集中心；
- 框选 3 个对象，Grid Snap ON 时输入 37°，状态栏记录共同 Pivot `(20.5, 20)`，revision 只增加 1；
- 一次 Undo 同时恢复 3 个对象；
- Flash 在自定义世界原点 `(0, 0)` 上旋转 37°；
- RectangularSweep 输入 37°返回 `UNSUPPORTED_FEATURE`，revision 保持 0、无 dirty 标记、几何不变；
- 另存为 `s2c2-transform-saved.gbr` 后重新打开 13 个对象成功；
- Grid Snap ON 下原 Measure 两点固定为 A `(20,20)`、B `(29.9,20)`、距离 9.9 mm；
- 新原生包中连续建立 2 条测量，线中同时显示 `9.984777 mm ∠ 0.000000°` 与
  `11.230977 mm ∠ 63.434945°`，工具栏计数为 2；Esc 后计数为 0 且两条均消失；
- Region 属性中的面积 36 mm²、周长 30 mm 在旋转/镜像前后保持不变。

## 哈希与剩余边界

- release binary / `.app` 内 executable：
  `ef417f519eb8f53ed6a7e8e57829721af732ebe6fd563a408ee8601023c492f5`；
- public input：
  `eca20f1392c007b97edfbc669aa0a6250118cc960aa5efb70df6ade49b8c2991`；
- GUI Save As output：
  `2a39126bd14e5b10b43f76d2a2719ac35cc92eb62c57fa937171f2a6a7cc2110`。

未实现且仍在范围外：Transform preview、Rotate Handle、Angular/Object Snap、自定义镜像轴、
Scale/Skew/Group、文字、P100K 优化及 Post-V1 格式。Windows 原生、Windows DX12 与最终双平台证据
均未执行；进入 S4-A 前应先独立复审本轮，不自动开始下一阶段。
