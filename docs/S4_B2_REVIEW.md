# S4-B2 — Block Core + `.rcam` schema v1 final review

2026-09-23。范围见 [S4_B2_PLAN](S4_B2_PLAN.md)，决策见
[ADR 0031](adr/0031-rcam-native-project-format-v1.md) / [ADR 0032](adr/0032-block-core.md)。

## 结论

**S4-B2 = PASS（Mac-first）。** Final Closeout 的 B0/B1、Block display/selection、codec security、
UI Component Foundation、native Metal、原生 GUI smoke、400×100 bounded performance、clean commit 固定交付与
fresh extract 均在同一受测源码身份下通过。

Windows deferred / not executed；不宣称完整 V1、P100K 或完整 CORE10 release。本轮到此停止，**没有启动
S4-B3**，也没有实现 `File → Open/Save .rcam`。

原始证据位于 `evidence/s4b2-final-20260923/`（Git 忽略）：

- `gates/gates.json`、逐命令日志、`tested-source-hashes.txt`、`binary-sha256.txt`；
- `gui/native-gui-smoke.json`、`native_observations.jsonl`、`native_actions.log`、screens；
- `delivery/RCam_S4B2_<shortsha>_{source,public_evidence}.zip`、`SHA256SUMS.txt`；
- `delivery/source_fresh_extract_report.json`。

## Final Closeout 实现

### B0 — Block × Manufacturing Precision

Gerber export 的私有快照现在执行：

```text
resolve / flatten BlockInstance
→ current ManufacturingPrecision normalization
→ semantic validation
→ writer
→ reopen / geometry comparison
```

working project 不被归一化或展开。`block_core_workflow` 固定覆盖 precision A 创建 Definition、切到 precision B
导出，Definition 内 Flash/Line/Arc/Region、光圈尺寸、线宽、arc start/end/center 与 region vertices 全部服从 B；
粗精度破坏拓扑时无 target file、项目/历史/revision 不变。

### Block display / selection

- `BlockDisplayCache` 按 `(definition id, revision, rotation, mirror)` 缓存 Definition 派生几何；instance 只追加
  translation。缓存只属于 display，既不写回 `SemanticDocument`，也不作为 writer 输入。
- `Scene` 对每个 resolved primitive 继续使用 instance ObjectId；`selection_flags` 因而同时标记整实例，不会只亮
  第一个内部 primitive。
- `DisplayClass::BlockInstance` 独立于 Gerber `%AB` 的 `ApertureBlock`；Filled/Outline/ZeroWidth、Layer/Category
  color、Visible、Selected 都按 Block 类别统一生效。
- service `visible_bounds` 同样 resolve Definition；Block fixture 的 Fit/GUI 初始 framing 不再漏掉实例。

### `.rcam` reader security

- `Budget.max_string_len` 对 ProjectId、LayerId、ObjectId、BlockDefinitionId/name、display name、aperture id、
  provenance、ObjectOrigin operation id 等 persisted strings 统一生效；超限为 `RESOURCE_LIMIT/string_len`。
- `Budget.max_json_depth` 在 schema parse 前检查 manifest/project/layer/block JSON；超限为
  `RESOURCE_LIMIT/json_depth`。
- 回归包含 exactly-at-limit、limit+1、depth limit、depth+1、huge ignored optional object 与 shallow unknown
  optional field。未知 optional 仍按 v1 policy 忽略，但不能绕过 archive/string/depth budget。

### UI Component Foundation

`editor-app/src/ui/` 现有轻量 `tokens`、`icons`、`buttons`、`modal_widgets`、`layer_row` 与
`command_widgets`。没有引入自研 UI framework：

- 统一 Primary/Secondary/Destructive/Icon/Toggle/Toolbar/CompactAction 语义；
- `RcamIcon` 隔离当前 glyph；
- LayerRow 保留 240–480、UTF-8 ellipsis、tooltip 与 compact controls；
- File Import/Export、Undo/Redo、Delete/Duplicate、Grid、Measure、Text、Layer add/delete/solo 由
  `CommandId → label/icon/enabled/checked/shortcut hint` descriptor 驱动；
- app/service 回归证明 revision、dirty、Undo、Gerber bytes、Layer workspace behavior 未改变。

## 同轮修复：Lisong/Songti Light `sdf 点` 卡顿

复现参数：macOS `STSongti-SC-Light`（UI 所称 Lisong/Songti Light）、height 3 mm、outline compensation 0、
content `sdf 点`。现象只在文字进入可视区域后发生，与生产 shader 对每个像素扫描每条 Region polygon edge
一致；移出视口后 RenderIndex 排除这些对象，所以其它图形又能继续缩放。

修复仅改变显示加速结构：较大 polygon 保留完整原始 f32 contour 供 reference renderer 使用，同时建立有界、
精确的水平边分箱。每个 sample 只访问 y-range 可能相交的 edge；winding/even-odd 各只执行实际采用的规则。
制造模型、f64 mm、文字轮廓、hit-test、metrics 与 writer 都不读取该缓存。

验证：

- 普通测试使用真实 `STSongti-SC-Light` 与 `sdf 点`，检查 11.5/2/0.5/0.1 mm 可视宽度都不触发原先的
  2B work rollback；
- portable 256-edge polygon 在采样网格上对 full reference winding 完全相同；
- native Metal 18 组（selected/unselected/all、drag delta、3 cameras）optimized vs full-contour reference
  **RGBA 零差异**；
- 1600×900、11.5 mm、selected、60 measured frames 的 release focused run p95 = 41.932 ms，50 ms gate 通过
  （最终原始值以 `gates/14.log` 为准）。

关联 R12/R16/R17；AT-024、AT-050、AT-062、AT-063。它不建立新的 V1 总体性能声明。

## Native Metal / GUI

`native_metal` 同一 release build 覆盖：

- existing 288 reference parity；
- S4-B1 180 view-style parity；
- Block representative matrix：Filled/Outline/ZeroWidth × selected none/one/all × no-drag/drag × 3 cameras，
  rotation/mirror 均在 fixture 内，全部 exact RGBA；
- Lisong/Songti Light full-contour parity 与 timing regression。

原生 GUI 使用 dev-only synthetic autoload（不冒充 Block Editor）：1 Definition、原 instance + 5 added instances，
含 0°/90°/37°/Mirror/Mirror+rotation。computer-use 实际完成 Filled/Outline/ZeroWidth、layer/category color、
点击整 Block selection、Export Gerber、New Workspace、重新导入导出文件；重开后 24 个 flattened primitives 可见，
无 blank canvas、无 `UNSUPPORTED_FEATURE block display`。验证器只接受探针观察/动作/截图，不接受文字声明。

## 400 × 100 bounded performance

Apple M1 release 采集（不定义长期 SLA）：

| 指标 | 结果 |
|---|---:|
| Definition openings / project instances | 400 / 100 |
| `.rcam` bytes | 98,469 |
| encode / decode | 11.321 / 10.620 ms |
| display cache build | 0.154 ms |
| instance display prepare | 9.277 ms |
| export flatten | 16.205 ms |
| cache entries / cached resolved objects | 1 / 400 |
| working project top-level objects | 100 |

Renderer 为实际绘制临时生成 40,000 display objects 是预期的；project 仍存 100 个引用，cache 仍只存一份
400-object Definition，export flatten 也只存在于私有输出快照。

## Final gates

`scripts/run_s4b2_final_gates.py` 只接受 clean commit，记录执行前后状态、全部 source hashes 与 release binary hash。
它实际执行：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
cargo test --locked -p editor-core --test block_core
cargo test --locked -p editor-service --test block_core_workflow
cargo test --locked -p rcam-project
cargo tree --locked -p editor-service -e normal
cargo tree --locked -p rcam-project -e normal
cargo build --release --locked -p editor-app
python3 scripts/source_manifest.py --check
python3 scripts/test_audit_core10.py
python3 scripts/test_package_source.py
python3 scripts/test_package_release.py
cargo test --release --locked -p editor-app native_metal -- --ignored --nocapture --test-threads=1
cargo test --release --locked -p editor-app s4b2_block_release_performance -- --ignored --nocapture
cargo test --release --locked -p rcam-project --test performance_workflow -- --nocapture
```

全部 exit code 为 0，执行前后 worktree clean。source/public-evidence ZIP 的 sidecar 对实际 bytes 复核通过；
fresh extract 内 source manifest、package tests 与 tested-source N/N binding 全部 PASS。

## 保留边界

- 不做 nested blocks、任意 angle scale/shear、完整 Block Editor、Block Library、Drill/PnP/RefDes；
- 不做 `.rcam` File New/Open/Save/Save As、Autosave、Recovery、Recent Projects（全部属于后续单独任务）；
- Windows 未执行；双平台 V1 门槛不变。
