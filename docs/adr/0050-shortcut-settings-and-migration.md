# ADR0050 — 快捷键完整快照与迁移

Accepted for implementation，2026-10-02；不是PASS。以正式整合任务为唯一契约。复用stable CommandId/Keymap/Resolver/Dispatcher，schema1格式rcam-shortcuts/logical-v1/source_platform/bindings，全部41可配置项与空列表禁用。缺项补本机默认；补充默认遇用户键冲突则新增项未绑定并提示；用户明确冲突/未知/字段/版本/超限整份拒绝。grip.cancel固定Esc不在文件。

单项确认/清除/恢复走同一完整校验→预编译→OS锁内fingerprint→唯一临时文件sync→原子替换→runtime安装。没有全局Apply草稿。导入预览immutable候选与独立generation，确认一次replace；导出已提交完整快照，拒绝写当前配置目标。提交前失败旧内存/字节不变；提交后目录sync警告保持新状态而不谎报回滚。坏启动文件保留，需明确重新建立确认。

六项原未接GUI默认不激活，保留PrimaryY/PrimaryShiftZ和Delete/Backspace。41动作复用原handler与enabled，固定输入/取消协议，record独占与release屏障，IME/事件时text owner优先。独立shortcuts文件不改revision/dirty/Undo/selection/camera/工程与其它prefs。Windows native未验；不扩Palette/chord或其它S5功能。
