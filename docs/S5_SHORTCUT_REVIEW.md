# S5-K1 快捷键设置 / 迁移 — 实现 checkpoint 与重启交接

本阶段独立于 S5-M2，基线 `0f3ec046640c4293b53e402305ad3e7f47000929`，工作区 `.worktrees/s5-shortcut-settings`，分支 `codex/s5-shortcut-settings`。按用户授权提交 implementation checkpoint（native acceptance pending）；具体提交完整SHA以Git与versioned checkpoint记录为准。本文件不是阶段 PASS 证明。

正式范围和验收以 `RCam_S5K1_SHORTCUT_SETTINGS_MIGRATION_NEXT_TASK.md`（SHA-256 `22d6d76c7549021c6e73883856eafb195dafc0d8f00957502bd632a1b1247b60`）、S5_SHORTCUT_PLAN、S5_SHORTCUT_ACCEPTANCE_ADDENDUM、ADR0050 为准。关联 R09/R10/R12/R18/R19/R20，重点覆盖冻结 AT-043 与 AT-078 的 Mac 焦点/快捷键/IME 部分，保留 AT-082/083/084 的隐私、证据真实性及能力边界；冻结 ACCEPTANCE_V1 与 acceptance_cases.json 未修改。新增阶段 C/F/J/P/N 用例不占用退役 AT 编号。

## 实现边界

41 项现有稳定命令 ID；固定 Esc 不可配置。完整 JSON schema v1 快照，可携带最多四个别名。保留 Delete/Backspace、Redo Primary+Y/Primary+Shift+Z；六项此前未在 GUI 生效的 registry 默认仍为空。不新增命令系统、业务几何、依赖或工程 schema。

设置支持查询分类/作用域/默认/当前绑定/修改状态、单命令录制候选、清除、单项与全部默认恢复；确认后独立后台任务原子保存。导入完整验证后形成带配置 generation、候选/catalogue/default 哈希和平台的不可变预览；明确完整替换、变化/清空/补缺/默认退让和跨平台映射。失败不部分安装、不覆盖旧文件。

独立 shortcuts.json 复用当前用户配置目录，与 preferences.json 和 .rcam 分离。缺文件默认；坏版本/损坏/不可读取使用可见安全回退，保护原文件，需明确默认重建。OS 自动释放 advisory lock + 锁内旧 fingerprint 检查；同目录 create_new 临时文件、flush/sync、原子 rename 为提交点、随后发布预编译 Keymap。提交后的观察/目录同步问题报告已提交耐久性警告；不伪报回滚。非协作外部编辑器仍有 TOCTOU 边界，未宣称锁可以阻止任意外部写入。

按原始后端 pressed&&!repeat 有序序列，在本帧 UI 取得焦点后经已有 ShortcutResolver 和 CommandDispatcher/现有 handler 分发；事件时间 TextInput/IME/Popup owner 同时保留。逐按键列表在第一次路由时消费，避免 egui 多 pass 重复派发。录制不执行命令；按键释放继续进入 egui，IME 后等待释放屏障。文本输入保留系统重复 Backspace/Delete/导航行为。

测试用配置目录和七个强退出阶段（含部分临时写入）仅在 internal-evidence feature 中启用，受 /private/tmp/rcam-k1-native-* 边界约束；公开程序没有该控制。原生截图/观测复用已有独立 evidence probe，不以注入 raw_input 代替真实 Mac 工具操作。

## 验收状态和证据

实际命令、退出码、环境、独立审查、原生操作/截图、源码与二进制 identity 统一放在新运行 ID `exports/S5K1_*/` 中，历史记录不覆盖。请以该目录 REVIEW.md 和 schema_version=2 结果判定，本文不把设计门槛写成测得结果。完整 diff 包括新文件，冻结后附于同目录。

Windows 原生 deferred / not executed；另一台真实 Mac 的迁移未执行时明确注明。最终 clean commit、正式四件套和复审由主任务另行安排。本阶段不表示完整 V1、CORE10 10/10、P100K 或 S5-M2 通过。

## RC1 冻结复审整改

2026-10-02 独立冻结复审发现 B1-01/B2-01/B2-02，v6冻结已解除，旧证据保留。当前整改共用 command_enabled/command_context_blocked、后台提交失败序号、实际按钮 handler 和当前配置 hint；菜单/属性/Block/Layer 同命令入口纳入。实际菜单文字/真实 egui 按钮点击和成功/满/断开队列回归在 main::shortcut_rc1_regressions。新适用门禁与新冻结结果以 exports/S5K1_20261002_candidate/checks-rc1、source-freeze-rc1、RC1_REVIEW.md 为准；未取得同一独立审查通过前不commit。用户要求当前任务结束后暂停，不开始 S5-M2；原生受限时保存可恢复状态并明确阻塞，旧二进制/截图不代表新源码。

用户于2026-10-02 19:51 Asia/Shanghai明确确认“按住键时确认/取消”和“中文输入法预编辑”两组正常，未提供截图。仅记 USER_REPORTED_MANUAL_PASS_NO_VISUAL；观测JSONL与保存状态可佐证输入状态及配置提交，不能证明有制造对象的删除阻断或具体物理按键/时长。完整记录见 versioned evidence user-manual-20261002-1951，旧源码/binary identity保持原值。

## 本次 checkpoint 和停止边界（2026-10-02）

原独立审查者关闭B1-01/B2-01/B2-02并有效重跑12/12，RC1冻结529文件身份一致。串行10项门禁全部exit0：targeted4/4、fmt/check/clippy、内部与公开release、workspace806通过/0失败/47忽略、automation_contract、headless_workflow、服务正常依赖树（无egui/eframe/wgpu/winit）。实际命令、退出与macOS26.5.1/Apple M1/Rust1.89环境见checks-rc1-v2、final-source-verification-rc1.json；ignored不计通过，不重跑长测试。

本checkpoint仅在已审源码基础上更新本交接文档与MANIFEST；生产代码、测试、其余源文件逐字与RC1冻结一致，差异及完整commit SHA另存versioned checkpoint record。既有RC1内部/public二进制保持dirty候选SHA，未重新构建或冒称来自新clean提交。没有push，没有触碰主目录旧修改。此前“未提交”“等待提交”说明保留为候选审查历史，以本节checkpoint记录为当前状态。

原生验收仍阻塞：五个剩余原生强退出点、完整.rcam/制造编辑组合、全部默认/接收旧自定义完整替换/动态菜单与禁用入口的原生矩阵、真实慢IO/连续UI、完整public settings smoke及F07等；Windows/另一Mac迁移未执行。用户两组手测已确认无截图，不重复索取或扩大结论。最终clean/fresh四件套未执行，阶段仍NO-GO。

重启恢复：工作区.worktrees/s5-shortcut-settings、分支codex/s5-shortcut-settings；任务docs/RCam_S5K1_SHORTCUT_SETTINGS_MIGRATION_NEXT_TASK.md；报告exports/S5K1_20261002_candidate/RC1_REVIEW.md、RESULTS.json、REMAINING_NATIVE_HANDOFF.md；完整RC1 diff在source-freeze-rc1；二进制在binaries-rc1；保存的手测空层工程/独立配置/JSONL在pause-state-pre-RC1（不依赖/tmp持久）。旧/新测试app已原生CmdQ退出，串行Cargo已exit0；提交后再次只读核验任务进程。

checkpoint后彻底暂停；不自行重启，不改电源/系统设置，不自动恢复测试，不启动S5-M2。重启后等待用户明确继续，先补本K1剩余适用原生与最终包验收，不把实现提交当作阶段最终PASS。
