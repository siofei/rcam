# RCam 下一轮 Codex 任务：S4-D1 PCB / PnP / RefDes Foundation（Mac-first）

> 前置基线：S4-C5 Array / Panelization v1 已通过 Mac-first bounded 复审。下一阶段建议名称为 **S4-D1**；若现有路线文档采用不同编号，先以 ADR 对齐，不要悄悄覆盖旧里程碑。
>
> **本轮只做 PnP/RefDes 的可靠基础能力，不做元件到开孔的自动图形匹配、自动库替换、自动拼板配对、Drill、完整 VectorScene 或生产装配文件导出。**

## 1. 最终产品闭环

用户可执行以下真实工作流：

```text
当前 .rcam 工程（一个或多个 Gerber Layer）
 → Import PnP CSV/TSV
 → 明确列映射 / 单位 / 坐标和旋转约定 / Side
 → 预览+诊断并确认导入
 → Board→Manufacturing World 坐标配准
 → 搜索 R123 / C15 等 RefDes
 → 列表选择 → Canvas 定位/高亮元件中心与方向
 → Save .rcam / Open / Recovery
```

所有 PnP 元数据与制造对象保持**独立**。Gerber Import/Export 继续只处理制造几何；**添加或定位组件绝不更改 Gerber 输出**。

## 2. 现有预留必须复用

优先复用 `editor-core/src/board.rs`：

```text
CoordinateSpace = Source / Board / ManufacturingWorld
CoordinateTransform2D = reflect_x → rotation_deg → translation（无 scale/shear）
BoardPoint / BoardSide
ComponentId / ComponentPlacement
```

`ComponentPlacement` 独立于 `SemanticObject`。RefDes 是组件身份/查询字段，不是一个闪光点、Region、文字对象或 BlockDefinition 的附加属性。保留 `f64 mm` 制造世界坐标。

`rcam-project` 当前只有 `board: Option<BoardProjectState>` 的**空占位**；开始编码前必须独立写 ADR，决定真实 board 数据怎样序列化、迁移、限制大小和处理旧 v1。不能仅为了快就把未知字段写进 v1 导致既有 reader 读不出且不说明兼容策略。

## 3. PnP 输入范围（v1 明确有边界）

1. CSV、TSV（UTF-8/UTF-8 BOM）；列映射由用户确认：`RefDes / X / Y / Rotation / Side`，`Footprint / Value` 可选。允许常见不同列名，但无法确定时绝不自动猜测。
2. 单位必须显式为 mm/inch（可提供可靠预识别和确认步骤）；存储统一转 `f64 mm`。拒绝 NaN、Infinity、溢出和越界值。
3. 旋转角定义为 Board 平面 CCW 度；若源文件约定不同，提供明确输入方向/符号选项。不推断不同厂商的 Bottom 镜像规则。
4. Side 只接受明确 Top/Bottom 映射；缺失、未知或歧义给出逐行 Diagnostic，允许用户修改映射后再重试。
5. 同一个 `(side, RefDes)` 重复、空 RefDes、错误数值与无效 UTF-8 应返回明确行号/字段的错误；v1 推荐**整批不提交**，不静默丢失异常行。
6. 限定最大文件字节、行数、字段长度、组件数和解析工作量；必须检查整数乘法/JSON/ZIP 预算。先以真实样本和合成 benchmark 决定默认值，不要无限读入。
7. 导入源文件**完全脱离工程**：可保留 basename、SHA-256 和导入时间等 provenance，但不能依赖外部 PnP 文件长期存在，也不允许隐式热重载。

## 4. Board 世界配准是硬门禁

- 原始 PnP 点处于 **Board** 空间，不能直接假设等于 Gerber 世界坐标。提供 `CoordinateTransform2D`（反射、旋转、平移），不引入 scale/shear。
- v1 可以通过“Board 两个已知点 ↔ Canvas 两个目标点”求一套 rigid transform；两个点不能重合或太近，镜像/Side 解释必须由用户显式选择，不能静默猜测。
- 提供变换前/后的点距离及配准残差；距离不一致代表源文件尺度/轴或单位可能错误，**拒绝配准而不是偷偷缩放**。
- 允许 Identity/手工旋转+平移作为简化路径，但 UI 必须展示“未校准/已校准”状态。
- 读取已有 Object Snap/Grid 基点时复用 S4-C1 Resolver；不建立第二套 PnP snap。
- 所有转换必须支持 inverse/roundtrip 数值回归；Top/Bottom/反射的坐标和角度变化必须用解析式验收。

## 5. Component 读写、搜索和 UI

建议正式 Service API：

```text
components.import_pnp
components.list
components.search
components.get
board.get_registration
board.set_registration
view.focus_component（GUI/View 操作，不是制造编辑）
```

- `components.*` 返回类型化 DTO、稳定 `ComponentId`、RefDes、Side、位置/旋转等；对 Automation 不依赖 GUI 当前选择。
- RefDes 搜索支持精确、前缀和受控子串查询；大量元件列表使用 virtualized table，用户可按 Side/Footprint 过滤。
- 点击结果只在 Canvas 显示**非制造 overlay**：中心 marker、可选方向箭头、RefDes 标签、相机 focus/highlight；不能制造新 Gerber Flash 或修改原 Layer。
- 组件世界定位应通过唯一 Board→World transform 计算；没有有效配准时显式提示，不能装作绝对位置准确。
- View focus/search/highlight 不改变 manufacturing revision、project dirty 或 Undo。
- PnP Import、Registration 作为工程状态的原子更改；必须建立与现有项目 Dirty/Undo/Redo/Recovery 一致的事务边界。若现有 `EditHistory` 只处理制造对象，**不要伪造 Layer 事务**，先增加适当的项目级事务抽象。
- Layer visibility/lock 不应静默修改组件信息；组件 overlay 需独立启用/隐藏，仍与当前 Manufacturing View 坐标一致。

## 6. `.rcam` 持久化与兼容

- 完整保存组件表、Board 单位/源格式映射、导入来源摘要和 Board→World 注册结果；保存前后确定性编码。
- 旧项目 `board=None` 必须可打开；旧 `.rcam` 内没有组件时应保持原样，不虚构 PnP 数据。
- 如果必须调整 schema/version：新增明确兼容/迁移测试、旧 reader 边界说明和资源预算。不得仅更新 `BoardProjectState` 而不更新 `rcam-project` codec、manifest、Reader 与实际 Save/Open/Recovery。
- 任何组件视图的修改、搜索/聚焦、单位显示切换都不能改变 Gerber Writer 字节；工程 Save/Open/Recovery 后组件坐标与注册状态保持一致。
- 完整路径、客户私有 PnP 源内容不得进入 Diagnostics ZIP。

## 7. 重要非目标

本轮明确不做：

```text
自动按组件匹配 Gerber 开孔组
通过位号自动替换 GMC / BlockDefinition
同一 RefDes 的跨 Layer 几何归属推断
自动确定贴装机/EDA 坐标系
多板自动拼版/组件阵列同步
Drill、DXF、Gerber 新增属性输出
PnP 制造/装配文件导出
Windows 阶段签署
```

未来“按位号选取对应钢网开孔”和“RCam 自主识别/匹配/替换”应在 **S4-D2 Component↔Opening Association** 单独设计：先建立可解释、可确认、可撤销的关联，不把最近邻误当生产真值。

## 8. 核心专项测试

至少建立以下合成 fixtures / 测试集：

- CSV/TSV、BOM、不同列顺序、明确单位、非 ASCII RefDes/Footprint、Top/Bottom、多种 rotation convention。
- Missing/duplicate field、错误单位、非法数值、重复 RefDes/Side、资源边界；所有导入失败必须**零项目更改**。
- 板坐标到世界：Identity、平移、90°/任意角、反射、inverse，配准两点成功与退化、距离失配拒绝；不能出现 scale/shear。
- Component list/search/get 返回稳定 ID，RefDes 精确/前缀/子串一致；100k 合成元件的搜索与列表内存/耗时有实测预算，不能每帧全量构建 UI row。
- Import/Registration 单事务、Undo/Redo、Dirty/Recovery、Save/Open、deterministic bytes、非法项目/ZIP fail-closed。
- Gerber Export 前后字节或等价 writer snapshot 不变；没有 PnP 操作生成制造对象。
- 坐标叠加与 Camera focus：同一 Component 在不同 zoom/ppp 下 marker 位置正确；没有注册时给 warning。
- INFRA1 operation log 有成功/失败/行数/诊断类别、没有原始私有源内容或用户绝对路径。

## 9. Native Mac 门禁

最终 clean release binary，在真实 Apple Silicon/Metal 下执行：

```text
Import synthetic PnP
→ Column mapping / Unit / Rotation / Side 确认
→ 错误行预览与无提交
→ 合法 Import
→ 双点 Board↔World 配准（对象捕捉）
→ 搜索 RefDes + focus/highlight + overlay
→ 切换 Top/Bottom 和显示单位
→ Undo/Redo Registration
→ Save .rcam / Open / Recovery
→ Export Gerber 并验证无组件副作用
→ Help > Export Diagnostic Package
```

机器可读原生 observations 至少包含：最终 binary/commit SHA、Component 数量、Side、源输入 hash（只公开 synthetic）、注册矩阵/残差、搜索结果、focus 的世界坐标、各步骤 revision/dirty/Undo、Save/Open 摘要和 Gerber output hash。CUA 无法稳定模拟操作时可用受控 instrumented native path，但必须走真实 EditorApp/Service；明确哪些是真实 CUA、哪些是注入输入，不可混报。

## 10. 交付/文档/安全

新增：`docs/adr/0042-pnp-refdes-foundation.md`、`docs/S4_D1_PLAN.md`、`docs/S4_D1_ACCEPTANCE_ADDENDUM.md`、`docs/S4_D1_REVIEW.md`，更新 `README/CAPABILITIES/IMPLEMENTATION_PLAN/AUTOMATION_API/AGENTS/CHANGELOG`。**冻结原 96 个 AT 身份和既有 expected；只新增本阶段 addendum**。阶段结论只能在测试/Native/归档均结束后更新，不要先写 PASS。

必须交付完整四件套：

```text
RCam_S4D1_<shortsha>_source.zip
RCam_S4D1_<shortsha>_public_evidence.zip
SHA256SUMS.txt
source_fresh_extract_report.json
```

- 最终 clean tested commit，固定 ZIP entry 顺序/时间/权限/UTF-8；Source 包含 `PACKAGE_INFO.json`、`MANIFEST.sha256`、`PACKAGE_MANIFEST.sha256`。
- final gates：fmt、check、clippy、workspace、上述 core/service/project/app 专项、automation/headless、Mac Metal/真实 PnP GUI、压缩/Recovery/Gerber/S4-C5 回归、release build、包自测。
- 从**完整 Source ZIP 真正全新解包**后执行 manifest/package self-test，并逐文件比较 tested-source payload 的哈希；Evidence 含 final gates、native observations、关键 stdout/stderr 的脱敏摘要、编译二进制/源码包 hash、`EVIDENCE.sha256`、source/delivery audit。
- 对外 Evidence **严格脱敏本机用户目录/盘符路径**（沿用本轮 B1 改进建议）；未经脱敏的原日志可私有保留但不可进入 Public Evidence。不得打包私有 Gerber/`.rcam`/PnP、字体、客户路径。
- 如果沿用 S4-C2 的 active RawInput 控制 driver，优先使用 `internal-evidence` Cargo feature 从正式公开发行构建中移除，不扩权到任意工程。

## 11. Exit Gate 与停止点

只有 Import/Mapping、Board→World、RefDes 定位、`.rcam` roundtrip/Recovery、零制造副作用、原子错误处理、100k 查询边界、Native/Diagnostics、4 文件完整交付全部闭合后，才写：

**S4-D1 PCB / PnP / RefDes Foundation = PASS（Mac-first bounded）**。

阶段完成后**停止并提交复审**。不要自动开始元件与 Gerber 开孔自动匹配、替换库、Drill 或其它阶段。
