# ADR 0039 — Block Editor v1

状态：Accepted for implementation，2026-09-27；S4-C3 Block Editor v1 已完成 Mac-first bounded 验收（PASS）。最终验收摘要与交付身份见 [S4-C3 review](../S4_C3_REVIEW.md)。Windows deferred / not executed；本 ADR 不表示双平台 V1、CORE10 10/10 或 P100K 通过。

Library 为 Layer Panel 同区独立 tab，虚拟列表，creation order；名称重复允许，ID 为唯一身份。显示 object/instance count/revision，支持 Place/Rename/Delete/Select Instances；跨层只读选择沿用现有 SelectionSet。

Create 输入名称后拾取基点，共用 Object/Grid Resolver/Alt。原子 blocks.create_definition_from_objects 保持 aperture 引用和 IDs/Undo。名称 trim，1–128 Unicode scalars，由 Core/Service 最终校验。放置单次，临时轮廓从制造边界在 worker 缓存，鼠标移动只平移显示，点击 blocks.create_instance。Esc、焦点/工程/revision/workspace/工具变化取消；无制造副作用。

实例原子选择；绝对参数用 blocks.update_instance_transform，普通编辑仍用 objects.*。Explode 和非空 Definition Delete 普通确认；被引用 Definition 禁止删除，绝不级联。Rename 维持现有 revision 语义。内部 Definition 几何编辑另行立项。

安全修正：旧 Core 对不连续的选择直接合并到首位置会重排与未选对象的曝光顺序。v1 对不连续曝光序列保守拒绝；连续选择支持全部已有普通形状，Undo 恢复精确顺序。该限制防止 Dark/Clear 覆盖变化，不以截图掩盖制造差异。

Windows deferred；不改变 .rcam schema、Gerber flatten、f64 mm、共享缓存与 ApplicationService 边界。

预算修正：Create 与未引用 Delete 的历史字节估算计入 Definition 的实际 Region edge storage、Flash aperture ID 和名称，防止按每对象固定常量低估。Explode 继续使用现有完整 resolved geometry/history/object-count 预算；失败不创建事务。

通用变换修正：objects.move/rotate/mirror/duplicate 与 edit.batch 最终几何共用只读 Block resolve 校验，事务提交前拒绝内部 RectangularSweep 不可表示角度或成员坐标超界。绝对 blocks.update_instance_transform 的既有校验保持。不得让 GUI 先提交非法 transform 后才在 renderer/export 报错。
