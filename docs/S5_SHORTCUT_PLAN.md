# S5-K1 PLAN — implementing

2026-10-02，唯一正式任务见RCam_S5K1_SHORTCUT_SETTINGS_MIGRATION_NEXT_TASK.md，SHA已核对；基线0f3ec046640c4293b53e402305ad3e7f47000929，独立codex/s5-shortcut-settings。此前草稿稀疏/全局Apply语义全部作废。主任务要求独立阶段复审、先不commit，后由主任务安排阶段commit与clean四件套。

P0现状库存/兼容默认/作用域/契约已冻结，core仅Keymap显式有效bindings构造和strict shortcut字段；新app shortcut_config.rs承担纯schema/validation/补全/canonical/default/format。P1聚焦测试后独立复审，P2 shortcut_store.rs为独立文件OS锁/fingerprint/提交点/后台结果，P3快捷键路由/同handler/enabled/动态hint，P4 shortcut_settings.rs为单项record/edit/清除/default/preview/picker，P5适用门禁与原生/强退出/public smoke后冻结。测试入口各在实际创建时记明。

允许与非目标严格按正式附录D；没有新制造/服务API/schema/依赖。输入派发不永久轮询文件，不破坏D1/D2/S5M1缓存。缺命令默认补充规则在纯模型完成，Windows native仍deferred。证据目录exports/S5K1_<runid>/，每个运行另存。

保留键证据：Apple官方https://support.apple.com/en-us/102650（2026-10-02读取）确认CmdQ/H/M、CmdSpace/Tab、OptionCmdEsc、CtrlSpace、Ctrl方向键、F3/Fn系统条件；Microsoft官方https://support.microsoft.com/en-us/accessibility/windows/keyboard-shortcuts-in-windows与https://learn.microsoft.com/en-us/windows/configuration/keyboard-filter/predefined-key-combinations确认Windows系统键、AltF4/Tab/Space与CtrlAltDelete。不更改系统设置。File OS锁使用锁定Rust1.89新稳定std::fs::File::try_lock，先编译核对，无新增依赖；只验证Mac的replace与锁，Windows原生延期。

## RC1 — 冻结复审整改（2026-10-02）

独立冻结复审 REQUEST CHANGES；禁止commit。关联C15/C16/C17、R09/R10/R12及AT-043/078适用部分，不调整冻结验收或新增业务范围。允许app main/command_widgets/block_ui/layer_panel与对应实际widget/router回归，保留core/service制造边界。先保留v6与用户报告两组手测（无视觉附件），再共用实际快照hint及can_execute、每次路由前后完整context门禁，后台请求失败阻断本帧后续派发。覆盖正常/满/断开队列、Block transient、busy/loading/模态实际工具widget、全部可配置菜单/toolbar/property/context入口重绑/清除提示。适用门禁后生成新候选freeze，交原独立审查复验；旧截图/二进制不重标新源码。
