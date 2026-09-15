# RCam 下一轮 Codex 任务：S1-A.1 圆弧与旧文件兼容门禁

> 前置：S1-A 已实现 semantic model、safe writer、headless Open/Query/Validate/Export。
> 本轮只修正进入 S1-B 前的制造几何语义和兼容边界；完成后再做 Move/Undo/Redo。

## 1. 先读

- `AGENTS.md`
- `docs/DESIGN_V1.md`
- `docs/CAPABILITIES.md`
- `docs/S1_A_REVIEW.md`
- `RCam_S1A_NEXT_TASK.md`
- 本轮审查报告 `RCam_S1A_f6da9a4_Review.md`
- Ucamco 当前 Gerber Layer Format Specification 中 Valid Arcs / G74 章节

不要执行根目录旧 `RCam_CODEX_NEXT_TASK.md` 的 S0-B 任务。

## 2. 本轮目标

修正 G74/G75 圆弧语义，使旧 Gerber 兼容策略与规范分离，并重新扫描 CORE10 中的圆弧失败项。

本轮不实现：

- Move / Duplicate / Delete
- Undo / Redo
- 中文文字
- 完整 GUI 文件流程
- Production renderer
- Python/Lua/JS runtime
- HTTP/RPC
- Linux/WSL2

## 3. G75：实现 arc deviation 语义

当前禁止继续使用：

```rust
(radius_start - radius_end).abs() <= 1e-6
```

作为 Gerber arc 合法性的必要条件。

要求：

1. 从输入中保留 start/end/declared center/direction/source resolution。
2. 计算 `start_radius`、`end_radius`、`arc_deviation`。
3. 按规范允许 non-zero deviation。
4. 加入规范禁止的 nonsensical center 检查。
5. 明确内部 canonical rendering/coverage 策略；不得悄悄把 fuzzy arc 当成错误的理想圆。
6. Writer 对未编辑 arc 必须保持允许范围内的制造语义。
7. round-trip comparator 要比较制造语义，不要求输入先变成严格等半径圆。

新增公开 fixture：

- exact radius G75
- small non-zero deviation G75
- larger but still valid demonstrative deviation
- nonsensical center negative case
- full circle
- near-full arc

## 4. G74：按规范选择中心

当前“候选数量不是 1 就拒绝”的策略需要删除。

要求：

1. I/J 按 unsigned distance。
2. 枚举 4 个中心候选。
3. 按方向过滤。
4. sweep 必须 <= 90°。
5. 计算每个候选 arc deviation。
6. 按规范选择 deviation 最小的候选。
7. 只有在规范规则仍无法给出确定解释时 fail-closed。

修正现有测试：

`g74_ambiguous_centers_and_full_circle_are_rejected`

不能继续把“多个候选”本身视为错误真值。

新增至少：

- CW quarter arc
- CCW quarter arc
- 多候选但 least-deviation 可唯一选择
- 真正不可判定的负例（如确实存在）

## 5. G74 start == end

Gerber 单象限模式中 start=end 是 0° arc，不是 G75 的 360° full circle。

要求：

- 不能报普通 arc invalid；
- 不能输出成 G75 full circle；
- 规范化后的制造覆盖必须等价于当前位置 aperture image/dot；
- 加独立物理点覆盖测试；
- Writer/reopen 后结果不改变。

## 6. Arc model

推荐从当前仅适合 exact circle 的 `ArcGeometry` 中分离：

```text
GerberArcInput
  start
  end
  declared_center
  direction
  source_quadrant_mode
  deviation

CanonicalArcGeometry / RenderCurve
```

也可采用其他设计，但必须满足：

- 原始制造语义可追溯；
- hit-test/render 不依赖错误等半径假设；
- Writer 不从 GPU mesh 反推；
- 将来 Move/Rotate/Mirror 能正确变换。

不要简单把 EPSILON 从 1e-6 改成 0.01 来“通过真实文件”。

## 7. CORE10 重新扫描

保持原 CORE10 身份和哈希，不换样本。

至少重新检查：

- CORE-06
- CORE-08
- CORE-09

记录：

- source hash before/after
- 首个原始失败原因
- 新结果
- arc deviation 统计（脱敏）
- 是否和独立查看器几何一致

不泄露私有路径、文件名和 Gerber 内容。

## 8. CORE-03 / CORE-07 兼容策略 ADR

本轮不要求强行把它们变成成功，但必须冻结策略。

### CORE-03 非法/非标准 metadata

允许提出 `LegacyImportPolicy::MetadataWarning`：

- 仅对不影响 image geometry 的已知属性；
- 保留原始字段和 warning；
- 不静默声称属性有效；
- Writer 默认不复制无效 metadata；
- 必须有严格模式。

### CORE-07 FS 宽度不一致

继续 fail-closed，除非：

- 独立参考工具给出唯一几何解释；
- 兼容规则只针对明确模式；
- 有正/负 fixture；
- 诊断中明确标记 legacy recovery；
- 不改变规范模式默认行为。

形成 ADR，不允许全局“自动扩位”。

## 9. Capabilities

在 arc 修正完成前，不得笼统宣称完整：

```text
G74/G75 circular interpolation
```

完成后 capability 必须由真实测试/实现状态生成或同步验证。

## 10. 验证

目标机执行：

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p editor-service --test s1a_semantic_truth
cargo test --locked -p editor-service --test s1a_independent
cargo test --locked -p editor-service --test headless_workflow
cargo build --release --locked -p editor-app
```

新增 Arc 专项必须可以单独运行。

继续用独立工具验证公开 arc fixtures；优先加入 Ucamco 官方 Gerber test files 中对应 arc 样例作为外部回归来源。

## 11. evidence

本次交付 ZIP 至少包含脱敏日志/摘要，不再只在本机 Git ignored 目录：

```text
evidence-public/s1-a1/
  environment.json
  source-hashes.json
  cargo-test-summary.txt
  arc-fixture-results.json
  core10-redacted-summary.json
  reference-check-summary.json
```

私有 Gerber 不进入包。

## 12. 退出条件

满足全部条件后才进入 S1-B：

- G75 non-zero deviation 支持；
- G74 least-deviation center selection 正确；
- G74 0° arc 正确；
- nonsensical arc 负例正确拒绝；
- 旧错误 G74 测试已修正；
- 公开 fixtures 全部通过且旧 fixtures 不回归；
- CORE-06/08/09 已重新扫描；
- CORE-03/07 LegacyImportPolicy ADR 已冻结；
- capabilities 与实际能力一致；
- source manifest 更新且通过；
- Windows/macOS 状态分别记录，Windows 若仍缺证据继续标 blocked。

完成后停止，不自动扩展 GUI。

下一任务才是 S1-B：

```text
Open
→ Query Object
→ Move(+5mm,-3mm)
→ Undo
→ Redo
→ Export(new path)
→ Reopen
→ Independent Validate
```
