# RCam S1-A 源码复审报告

> 审查对象：`rcam-s1a-20260915-f6da9a4.zip`
> 审查目标：判断 S1-A 是否达到进入 S1-B 编辑闭环的条件。
> 结论：**S1-A 主体实现成立，但建议先完成一个很短的 S1-A.1 圆弧/旧文件兼容修正，再进入 S1-B。**

## 1. 本次可独立确认的内容

本次解包后实际检查了工作区源码、公开 fixtures、设计/验收文档和 Python 检查工具。

实际执行：

- `python3 scripts/source_manifest.py --check`：通过，当前源码清单 **136 files**。
- `python3 scripts/test_audit_core10.py`：**7/7 通过**。
- `python3 -m py_compile scripts/*.py`：通过。
- `fixtures/synthetic/s0c/manifest.json`：18/18 文件 SHA-256 匹配。
- `fixtures/synthetic/s1a/manifest.json`：50/50 文件 SHA-256 匹配，其中 15 正例、35 负例。
- `docs/acceptance_cases.json`：仍为 **96 个有效用例 + 1 个退役记录**。
- 源码中可见 **82 个 Rust test 标记**。

当前审查环境没有 `cargo` / `rustc`，因此没有重新编译 Rust，也不能独立复核仓库文档中声明的 80 passed、release build、Metal 或 CORE10 原始 Cargo 日志。

此外，本次 ZIP **没有包含 `evidence/` 目录**。因此 `docs/S1_A_REVIEW.md` 中引用的 `final-gates-v3/`、`core10-scan-final-v2/`、reference-point 日志等只能确认“报告中有记录”，不能从这份交付包独立重新审计。

## 2. 已经做到的部分

### 2.1 架构方向正确

当前已有：

- `editor-core`：自有 f64/mm 几何和语义模型。
- `gerber-io`：独立的 S1 parser 适配、语义解释、Writer、round-trip 校验。
- `editor-service`：文件授权边界、Open/Get/List/Query/Validate/Export。
- GUI/GPU 没有进入 `editor-service` 正常依赖闭包的设计方向仍保持。

第三方 parser AST 没有直接作为编辑器公共模型，这是正确的长期方向。

### 2.2 Writer 已形成真实闭环

`write_s1` / `export_s1_new_path` 已经具备：

- 从自有语义模型生成规范化 Gerber；
- 毫米绝对坐标输出；
- 动态 stroke aperture；
- Arc/Region/极性输出；
- 内存 round-trip；
- 临时文件写入；
- 重新读取并再次 round-trip；
- new-path/no-clobber 发布。

`headless_workflow.rs` 也已经覆盖：

`document.open -> objects.query/get -> document.validate -> gerber.export_layer -> reopen -> validate`

这说明后续脚本自动化接口不是空预留。

## 3. S1-B 前必须处理的关键问题

### B0-1：G75 圆弧被错误要求“起终半径几乎完全相等”

位置：

- `crates/editor-core/src/lib.rs:332-349`
- `crates/gerber-io/src/s1.rs:2916-2930`

当前 `ArcGeometry::is_valid()` 要求：

```rust
(radius - end_radius).abs() <= EPSILON_MM
```

其中 `EPSILON_MM = 1e-6 mm`。

这会把具有正常坐标量化误差的 Gerber 圆弧直接判为非法。

Gerber 规范明确允许 **non-zero arc deviation**：由于文件分辨率和生成软件舍入，中心到起点和终点的半径在真实文件中通常不完全相等。输入软件必须能解释这种合法的非零 deviation，而不是要求 1 µm 内完全等半径。

这和当前 CORE10 结果高度吻合：

- CORE-06：G75 radius/sweep consistency failure
- CORE-09：G75 radius/sweep consistency failure

**建议：不要简单把 1e-6 改成一个更大的拍脑袋容差。**

需要把 Gerber Arc 的“输入语义合法性”和编辑器内部“规范化圆/曲线表示”分离：

1. 保留 start / end / declared center / direction / source resolution；
2. 计算并记录 arc deviation；
3. 按规范验证中心位置和 sweep；
4. 为显示/命中选择一个明确、可重复的规范化解释；
5. Writer 对未编辑 fuzzy arc 优先保持制造语义，而不是先强制投影成一个错误的理想圆。

### B0-2：G74 圆心候选选择算法与规范不一致

位置：`crates/gerber-io/src/s1.rs:2933-2965`

当前逻辑：

- 枚举 4 个中心；
- 只保留严格等半径且 <=90° 的候选；
- **只有候选数量恰好为 1 才接受**；
- 多于 1 个直接报 `G74 center is ambiguous`。

但 G74 规范的规则不是“必须只有一个满足候选”。

单象限模式的 I/J 是无符号距离，确实会形成 4 个中心候选；正确规则应考虑方向、<=90°，然后选择 **deviation 最小** 的候选。

项目现有测试：

`crates/editor-service/tests/s1a_semantic_truth.rs:158`

```text
g74_ambiguous_centers_and_full_circle_are_rejected
```

把“多个候选直接拒绝”固化成了测试，因此这里需要**先纠正测试真值，再改实现**，不能为了保持旧测试而继续错误规则。

这很可能直接关联 CORE-08：

`G74 has no valid single-quadrant center`。

### B0-3：G74 起点=终点的零度圆弧处理错误

当前 `ArcGeometry::is_valid()` 对非 full-circle 强制：

```rust
start.distance_mm(end) > EPSILON_MM
```

而 G74 分支始终：

```rust
full_circle: false
```

所以 G74 `start == end` 会被拒绝。

规范中 G74 与 G75 在这里恰好不同：

- G75：起点=终点 -> 360° full circle；
- G74：起点=终点 -> **0° arc**，结果相当于当前位置的一次 aperture image/dot。

现有测试也把 G74 start=end 当失败用例，需要修正。

建议在语义层显式处理零度 draw，不要把它误转成 G75 full circle。可选择规范化为等价的零长度 stroke/flash，但必须用独立覆盖点测试证明制造结果相同。

### B1-1：Capabilities 当前对 G74/G75 的宣称过宽

`ApplicationService::capabilities()` 当前直接声明：

```text
G74/G75 circular interpolation
```

但项目自己的 CORE10 扫描只有 5/10 语义成功，且 3 份失败与 arc 直接相关。

在上述圆弧问题修复前，建议临时改成更精确的能力描述，例如：

```text
G75 exact-radius subset
G74 validated subset (legacy compatibility incomplete)
```

或者不对外宣称完整 G74/G75，直到对应测试通过。

## 4. CORE10 当前真正意味着什么

`docs/S1_A_REVIEW.md` 当前报告：

- CORE-01：通过
- CORE-02：通过
- CORE-03：CreationDate 属性语法问题
- CORE-04：通过
- CORE-05：通过
- CORE-06：G75 arc failure
- CORE-07：FS 2.5 坐标宽度问题
- CORE-08：G74 center failure
- CORE-09：G75 arc failure
- CORE-10：通过

当前是 **5/10 语义成功**。

这里建议把剩余问题分成两类：

### 必须修的制造几何问题

- CORE-06 / 08 / 09 圆弧。

这三项和 S1-A 冻结能力本身直接相关，应该在 S1-B 前处理。

### 旧文件兼容策略问题

- CORE-03：无效/非标准 CreationDate 属性。
- CORE-07：坐标字段超过声明 FS 宽度。

这两类不要通过“全局放宽语法”解决。

推荐增加明确的 `LegacyImportPolicy`：

- **图像无关 metadata**：可以在独立参考工具确认图形成像不受影响后，以 warning 捕获原始字段并继续；Writer 默认不复制无效 metadata。
- **影响坐标解释的 FS 不一致**：继续 fail-closed，除非通过独立查看器/业务真值建立唯一、确定的兼容解释，并把该规则做成受限、可诊断的 legacy adapter。

## 5. 其他需要保持的边界

### 5.1 不要现在开始大规模 GUI

当前最有价值的是先让核心制造几何正确。

### 5.2 Reference Renderer 继续保留

S0 的小型 reference renderer 可以继续作为正确性对照，不要直接把 16-object 路径扩成生产 renderer。

### 5.3 Writer 自身重开不是唯一真值

当前 Writer -> 自己 parser -> semantic equal 是必要门禁，但 parser 和 writer 共用同一语义实现，存在 common-mode bug 风险。

项目已经设计 gerbv 固定点对照，这是正确方向。后续建议增加 Ucamco 官方测试文件的固定回归。

## 6. Windows 与证据状态

当前源码文档仍明确写明：

**Windows 原生证据缺失，S0-C platform gate blocked。**

因此不能声明双平台阶段完成。

另外本 ZIP 未带 `evidence/`。后续交付审查包建议包含脱敏证据：

```text
evidence/
  environment.json
  cargo-check.log
  cargo-test.log
  clippy.log
  release-build.log
  core10-summary.json        # 不含私有路径/内容
  reference-check.json
  hashes.json
```

私有 Gerber 本身仍不应打包。

## 7. 阶段判定

### S1-A 主体：基本成立

已经有真实 semantic model、Writer、无界面 service 和规范化往返。

### 是否马上进入 S1-B：不建议

先做一个很短的 **S1-A.1 Arc & Legacy Compatibility Gate**。

退出条件建议：

1. G75 接受规范允许的 non-zero arc deviation；
2. G74 按 least-deviation 规则选择中心；
3. G74 start=end 按 0° semantics 正确处理；
4. 加入“nonsensical center”对应负例；
5. 修正错误的 G74 旧测试；
6. CORE-06/08/09 重新扫描并记录结果；
7. 对 CORE-03/07 给出明确 LegacyImportPolicy，不允许静默猜测；
8. capabilities 只公布真实通过能力；
9. 所有公开正/负 fixture 不回归；
10. Windows 证据若仍缺失继续标 blocked，但不阻止无 UI 的核心算法开发。

完成后再进入 S1-B：

```text
Open
 -> Query Object
 -> Move(+5,-3mm)
 -> Undo
 -> Redo
 -> Export(new path)
 -> Reopen
 -> Independent Validate
```

## 8. 总结

**这版值得继续，不需要推倒重来。**

S1-A 的主要工程结构已经形成；现在发现的问题不是“没做”，而是一个典型的 CAM/Gerber 兼容性边界：圆弧不能按普通 CAD 的“严格等半径圆”去验证真实 Gerber。

先把 G74/G75 语义按规范修正，再开始 Move/Undo/Redo，会比先进入编辑功能更稳。
