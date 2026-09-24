# S4-C1 Full Object Snap 复审（Mac-first）

## 结论

**PASS（Mac-first bounded）。** 先前 `28e5f50` 的 PASS 结论被 closeout 复核降为 ALMOST PASS：runtime 在 feature generation 阶段仍以 8 px 截断，导致 resolver 无法在 8–11 px 保留上一候选。本轮已把候选查询扩至 11 px，同时保持新候选只能在 8 px acquire；静态、Nearest、线线/线弧 Intersection、双候选切换、Retina 和无 previous 的 9 px 拒绝均从真实 `editor-app::object_snap::Runtime::resolve` 验证。

统一 resolver 已接入 Direct Drag、Text Placement、Measure 与 Pick Base Point；F3 的菜单、工具栏和快捷键都经 Command/Keymap/Dispatcher。项目重开后的 generated ID allocator 也会从持久化对象、光圈和 Block ID 续接，避免拾取基点后的文字预览与已有 ID 冲突。本轮停在 S4-C1 复审边界，不启动 INFRA1，也不开始 Grip Editing / S4-C2。

此结论不表示 Windows、完整 V1、CORE10 10/10 或 P100K 通过。

## 实施范围

- 关联需求：R08、R09、R13、R17、R18。
- 关联用例：AT-025、AT-030、AT-032、AT-044、AT-045、AT-062、AT-074、AT-075。
- 统一 `editor-core::snap`：Endpoint / Vertex / Midpoint / Center / Quadrant / Intersection / Nearest，稳定 feature identity，Object > Grid，候选查询 11 physical px、8 px acquire、11 px release。
- 默认以 Manufacturing Boundary 为真值；Original Path 是默认关闭的高级来源。标准光圈孔不作为制造材料边界，Region 孔与 macro clear 保留。
- Rectangle / Circle / Polygon / Obround / Line / RectangularSweep / Arc / Region / CompatibilitySolid / BlockInstance 使用解析几何，不从 mesh、GPU 或像素反推。
- `WorldIndex` 先限定附近对象，再按需生成 feature；无全局 snap-point database。BlockDefinition 局部缓存按 definition revision 失效，instance 只应用 rigid transform。
- effective visible/selectable 及 class filter 生效；locked 但 selectable 的图层可作参考；拖动时排除自身 selection set。
- F3 经 Command/Keymap 切换；Alt 临时关闭 Object + Grid；marker 用物理像素，状态坐标复用 DisplayUnit formatter。
- `.rcam v1` 持久化 snap kinds/radius/source，旧 JSON 通过 serde defaults 兼容。

## 门禁结果

| 命令 / 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets --locked` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked --no-fail-fast` | PASS，全工作区 0 failed；需要私有样本/原生环境的用例保持 ignored |
| `cargo build --release --locked -p editor-app` | PASS |
| `s4c1_release_snap_performance --release --ignored` | PASS |
| `native_metal_block_instance_parity --release --ignored` | PASS，Apple M1 / Metal，54 组 exact-RGBA |
| Runtime hysteresis matrix | PASS，真实 Runtime 覆盖 7.5/9/10.9/11.1 px、无 previous 9 px、Nearest、两类 Intersection、双候选与 Retina |
| Mac 原生 Object Snap 交互 | PASS，ppp=2，Apple M1 / Metal；F3 ON→OFF→ON；Measure Vertex→Center；Pick Base Point Center→preview→一次 commit |
| Project reopen generated IDs | PASS，保存/重开后由 `generated-object-0` 继续生成 `generated-object-1`；原生恢复工程文字生成继续为 11–13 |

注：Metal 用例在受限沙箱首次无法枚举 adapter（`NotFound`）；同一命令在 macOS 原生权限下通过，原始日志已保留。

## 性能证据（release）

| 场景 | 索引构建 | 查询 | 邻近对象 / 特征 |
|---|---:|---:|---:|
| 10 图层 × 1,000 对象 | 1,539 µs | 87 µs（candidate 87 / resolver 0） | 1 / 1 |
| 100,000 对象 | 14,391 µs | 10 µs（candidate 9 / resolver 0） | 1 / 1 |
| 2,000 边 Region | — | 424 µs（candidate 424 / resolver 0） | 1 / 1 |

这些数据是当次 Apple M1 release 运行结果，不是通用硬件承诺。

## 原生证据

- `evidence/s4c1/20260925-run1/native-s4c1/native_observations.jsonl`：Center / Quadrant / Vertex / Midpoint / Intersection / Nearest、locked reference、grid fallback、Direct Drag。
- `evidence/s4c1/20260925-run1/native-s4c1-v2/native_observations.jsonl`：Text Placement 在独立文字网格复选框关闭时仍吸附到 Circle Center，一次提交一次 Undo。
- `evidence/s4c1/20260925-run1/native-summary.json`：上述操作的 schema_version=2 汇总。
- `evidence/s4c1/20260925-run1/native-s4c1-v2/screens/s4c1-text-center-snap.ppm`：原生帧截图。
- `evidence/s4c1/20260925-run1/native_metal_block_instance_parity.log`：Apple M1 / Metal exact-RGBA 原始输出。
- `evidence/s4c1/20260925-run1/snap_performance.json`：有界性能原始数据。
- `evidence/s4c1/20260925-hysteresis-closeout-run1/native_hysteresis_observations_final.json`：最终 release 的原生 8→11 px runtime matrix。
- `evidence/s4c1/20260925-hysteresis-closeout-run1/native-measure-final/native_observations.jsonl`：Vertex→Center 测量、µm/mm 格式化，revision/dirty/Undo 不变。
- `evidence/s4c1/20260925-hysteresis-closeout-run1/native-ui-final2/native_observations.jsonl`：F3 ON→OFF→ON 与 Pick Base Point Center→preview→commit。
- `evidence/s4c1/20260925-hysteresis-closeout-run1/native_closeout_summary.json`：schema_version=2 closeout 汇总。

## 交付与剩余边界

最终交付由已测试的 clean commit 生成 `RCam_S4C1_<shortsha>_source.zip`、`RCam_S4C1_<shortsha>_public_evidence.zip`、`SHA256SUMS.txt` 与 `source_fresh_extract_report.json`；哈希由交付目录的 sidecar 绑定。

- Windows：deferred / not executed。
- 完整 V1 / CORE10 10/10 / P100K：本轮未执行。
- INFRA1 Runtime Diagnostics Foundation：下一阶段，未启动。
- S4-C2 Grip Editing：未开始。
