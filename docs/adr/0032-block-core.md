# ADR 0032 — Block Core 正式实现

状态：Accepted（S4-B2，2026-09-22）。取代 ADR 0030 中 Block 部分的占位描述；ADR 0030 继续作为其余长期预留
（Object Snap、Command/Shortcut、Board Coordinate、Drill）的记录。实现证据见 [S4_B2_REVIEW](../S4_B2_REVIEW.md)。

## 背景

S4-B1 只在 `editor-core::block` 留了 `BlockDefinition`/`BlockInstance` 占位类型（无产品入口，仅类型 + 一个单元测试）。
钢网设计大量出现重复开口（BGA/QFN 阵列），`.rcam v1` 必须从第一天认识 Block，否则 v2 需要一次昂贵的数据迁移
才能把重复几何从“物理复制”改成“引用共享”。

## 决定

1. **`BlockDefinition` 存在 `SemanticDocument.block_definitions`，不属于任何一个 Layer。**
   `SemanticDocument`（`crates/editor-core/src/lib.rs`）新增 `block_definitions: Vec<BlockDefinition>`
   字段（`#[serde(default)]`，Gerber 导入产生的文档永远是空 Vec）。这是侵入性最小的选择：bounds/hit-test/metrics/
   export 已有的每一个函数都已经拿着完整 `&SemanticDocument`，解析 `BlockInstance` 时不需要给几十个调用点
   新增一个 context 参数。

2. **`BlockInstance` 是 `SemanticGeometry` 的一个新变体（`SemanticGeometry::BlockInstance { definition_id,
   transform }`），不是包一层新枚举。** 曾经考虑把 `SemanticObject.geometry` 包成
   `ObjectGeometry::{Primitive(SemanticGeometry), Block(..)}`，但 `SemanticGeometry::` 在代码库里有约 400 处
   构造点（主要是测试夹具），包一层会强迫全部重写；直接加变体只强制编译器审查真正**穷尽匹配**
   `SemanticGeometry` 的约 20 个调用点（bounds.rs、hit_test.rs 及其 select_rect 子模块、metrics.rs、
   transform.rs 的 `WorldTransform::apply`、workspace.rs 的 `classify_object`/`geometry_fingerprint`、
   gerber-io 的 writer/precision、editor-app 的渲染器），其余构造点完全不受影响。实例的 `object_id` 直接复用
   已有的 `SemanticObject.object_id`（不是 `block.rs` 里独立的 `BlockInstance.id` 字段——那个结构体保留作为
   service 层的便利视图，见下）。

3. **不允许嵌套（Block 不能引用 Block）由类型系统强制，不是运行时检查。** `BlockObject.geometry` 的类型是
   `BlockObjectGeometry`——一个只镜像 `SemanticGeometry` 五个非 Block 变体的独立枚举，没有 `BlockInstance`
   分支。`impl From<BlockObjectGeometry> for SemanticGeometry`（解析时用）与
   `impl TryFrom<SemanticGeometry> for BlockObjectGeometry`（`blocks.create_definition_from_objects` 捕获现有
   对象时用，遇到 `SemanticGeometry::BlockInstance` 返回 `BlockError::NestedBlock`）是两者之间唯一的桥梁。

4. **`BlockTransform { translation, rotation_deg, mirror: bool }` 复用既有 `CoordinateTransform2D`
   （`board.rs`）的语义（`reflect_x` 后旋转、无 scale/shear）。** `mirror: bool` 就是 `reflect_x`；
   Mirror X + Mirror Y 的等价 canonicalization 不需要新代码——`reflect_x=true, rotation=180` 已经数学上等于
   `CoordinateTransform2D` 的 Mirror Y（`block.rs` 的
   `mirror_x_then_mirror_y_canonicalizes_to_rotate_180` 测试核对）。

5. **Instance 的 Move/Rotate/Mirror/Duplicate 直接复用既有 `objects.move/rotate/mirror/duplicate` 服务，
   没有新增专门的 delta 编辑方法。** `editor-core::edit::translate()`（Move）与
   `transform::WorldTransform::apply()`（Rotate/Mirror，含 `edit_batch`）现在对
   `SemanticGeometry::BlockInstance` 多一个 match 分支：Move 直接平移 `transform.translation`；
   Rotate/Mirror 把世界变换表示成 `CoordinateTransform2D` 后调用已经过测试的 `CoordinateTransform2D::after()`
   与旧 transform 复合，再拆回 `BlockTransform`。这些函数原本就是所有对象编辑走的唯一路径，加一个分支即得到
   §23 要求的“Instance 编辑只改自己”，不需要重新实现一遍。只有绝对设置 transform（`blocks.
   update_instance_transform`）、创建/重命名/删除 Definition、创建新实例（无来源实例）、Explode 是真正的
   新操作。

6. **Definition 编辑没有专门的服务方法。** S4-B2 不做 GUI Block Editor（§27/§67），本阶段没有产品路径去修改
   已存在 Definition 的内部几何；`definition_edit_affects_all_instances_but_instance_edit_affects_one`
   （`editor-core/tests/block_core.rs`）直接改 `document.block_definitions[i]` 并手动 bump `revision`，
   证明“共享引用，多个 Instance 立刻看到新几何”这个架构不变量成立，而不是先造一个之后要删除的临时 API。

7. **RectangularSweep 在非 90° 合成旋转下 fail-closed，不静默产生错误矩形。** `block::resolve_geometry`
   复用 `WorldTransform::apply` 对 `RectangularSweep` 已有的 `rotation_deg % 90 != 0` 拒绝逻辑（这条规则
   S4-B1 就存在，用于普通对象的 Rotate），Explode / Export flatten / `blocks.create_instance` 校验都会撞到
   同一处保护。

8. **Hit test 返回整个 Instance 的身份，不需要专门代码。** `hit_test` 的主循环已经 `result.push(object.
   object_id.clone())`（不管 geometry 是什么），只要 `SemanticGeometry::BlockInstance` 的解析结果（取最近的
   已解析图元）被路由回同一个循环，§30 的“点击选中整个实例”就是既有结构的自然结果。

9. **`DisplayClass::BlockInstance` 是新增分类，不是复用 `ApertureBlock`。** `ApertureBlock` 保留给 Gerber
   `%AB`（parser 仍拒绝，无产品入口）；RCam Block 是完全不同的概念。

10. **显示时共享 Definition 派生结果，不写回制造模型。** `BlockDisplayCache` 的 key 为 definition id /
    revision / rotation / mirror；instance 只追加 translation。renderer 中每个 resolved primitive 仍携带同一
    instance ObjectId，因此选择 halo 覆盖整实例。Filled/Outline/ZeroWidth 与 Layer/Category 样式均只改显示。

11. **Export 先 flatten，再按当前 ManufacturingPrecision 归一化。** Definition 内部坐标/线宽/光圈尺寸与
    instance transform 都必须服从导出时 policy；粗量化破坏 Arc/Region 拓扑时 fail-closed，working project 不变。

## 不做（本阶段）

Nested blocks；任意角 scale/shear；完整 GUI Block Editor（创建/浏览/编辑 Definition 的界面）；Block 专属
Object Snap（复用既有 `SnapFeatureProvider`）；跨项目 Block Library。

## 不变量

`BlockDefinition` 永远是项目级、被引用、不被复制；一次 `blocks.create_definition_from_objects` /
`blocks.explode_instance` 是一个事务、一次 Undo；`.rcam` 存储的 Instance 永远是 `{definition_id, transform}`，
不展平；Gerber Export 展平是允许的（导出是 exchange output，不是工程真值）。
