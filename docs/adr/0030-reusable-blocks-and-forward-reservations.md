# ADR 0030 — Reusable Block、Object Snap、Command/Shortcut 与 Board Coordinate 长期预留

状态：Accepted（长期方向，S4-B1 只做类型占位）。来源：RCam 长期架构指导（钢网设计方向）与 AGENTS Addendum，
已合并到 `AGENTS.md` “Forward Architecture Reservations” 与 `DESIGN_V1.md` “长期架构方向”。

## 产品定位

RCam 的主要用途是 PCB 钢网/Stencil 制造图形设计、编辑、检查与工程管理，因此优先级高于普通 Gerber Viewer：
精确制造几何、多 Layer、开口编辑、可复用 Block、对象捕捉、Grid/Snap/Measure、`.rcam` 工程、Board Coordinate/PnP/RefDes、
大量重复开口性能、Gerber/Drill 导入导出。

## 决定

1. **Reusable Block** = `BlockDefinition`（项目级可复用制造几何）+ `BlockInstance`（属于某 Layer，`definition_id` +
   translation/rotation/mirror）。禁止 non-uniform scale/shear；**第一版禁止 nested block**（避免循环引用与迁移复杂度）。
   修改 Definition 更新全部 Instance；修改 Instance 只改 transform。Gerber Export **flatten** 实例；RCam Block 不等同于 Gerber `%AB`。
   `.rcam v1` 冻结前必须已有 Block core（S4-B2）。
2. **Object Snap** 统一为 `SnapKind/SnapFeature/SnapFeatureId/SnapQuery/SnapCandidate/SnapFeatureProvider/SnapResolver`（snap.rs 已有类型与测试，无产品入口）。
   目标：Endpoint/Vertex、Midpoint、Center、Quadrant、Intersection、Nearest，后续 Tangent/Perpendicular。特征来源是制造边界，
   不是 GPU/显示几何。采用“屏幕半径 → 空间索引取附近对象 → lazy 生成 features”，**禁止全局预生成 snap 点库**。
   Grid 与 Object Snap 由同一 Resolver 处理。
3. **Grip Editing 不等于 Object Snap。** 现阶段只保证稳定 feature identity（`SnapFeatureId`/`GripFeatureId`）。
4. **Command/Shortcut**：`CommandRegistry/CommandId/Keymap/ShortcutContext/ShortcutResolver/CommandDispatcher`；
   Menu/Toolbar/Context Menu/Shortcut 调用同一 CommandId。上下文优先级 `IME/TextInput > Modal > Tool > Canvas > Global`；
   逻辑修饰键 `Primary/Secondary/Shift/Alt`（macOS Cmd / Windows Ctrl）；用户 keymap override 属于 AppPreferences，不属于 `.rcam`。
   Automation API 不模拟快捷键，直接调用 ApplicationService。
5. **Board Coordinate**：Manufacturing World 仍是 f64 mm；预留 Source → Board → World，`CoordinateTransform2D`
   只含 translation/rotation/reflection。
6. **Component Placement / RefDes**：独立模型 `ComponentPlacement{ComponentId, refdes, BoardPoint, rotation, side, footprint, value}`，
   不塞进普通 Gerber SemanticObject；未来 `R123 → ComponentPlacement → Board→World → Camera Focus/Highlight`。
7. **Drill/Excellon**：`LayerKind::Drill`，每个 import 独立 Tool namespace，DrillHit/DrillSlot/Route；同样只 Import/Export，不 live-link。
8. **Gerber 兼容边界**：Extended Gerber/RS-274X 由 FS/MO/AD 自动确定；Legacy/Hybrid 在无歧义时规范化；纯 RS-274-D 或有歧义
   永不猜测，进入 Legacy Import Modal 由用户给格式/单位/零压缩/光圈表。

## 阶段归属

| 阶段 | 内容 |
|---|---|
| S4-B1 | Multi-Layer Workspace、Layer UI/style/filter、add/delete/order、Gerber Import 语义；Drill/Board/Block/Snap/Command 仅占位类型 |
| S4-B2 | Block Core、`.rcam` schema v1、Workspace state 与 Snap settings 持久化 |
| S4-B3 | `.rcam` New/Open/Save/Save As、Migration、Recovery、Recent Projects |
| S4-C | 完整 Object Snap、Grip、Block Editor、Explode、Array/Panelization、Alignment、PnP/RefDes、Component Search、Shortcut Settings/Command Palette |

## 不变量（无论未来增加什么）

Manufacturing geometry = f64 mm；GPU/display = 局部/camera-relative f32；Renderer mesh/像素永不反推制造几何；
View/Workspace state 不改变 Gerber 输出；Gerber Import 与源文件解耦；Gerber 是 Export 而非 Project Save；
ApplicationService 是制造修改边界；GUI/Shortcut/Automation 共享业务逻辑。
