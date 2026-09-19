# S2-B3.1 实施与验收报告

**生产 renderer 结构性阻塞已修复；1000 selection 原生指标通过。完整阶段仍未签署：规定尺寸的原生 Pan/Zoom 和三轮10秒拖动证据未齐。**

受测提交 `04c0cbf13d803d9fffa1a2ebcc1b612386c55d5b`，本地分支 `codex/s2b31-scalable-renderer`。
在 `/private/tmp/rcam-s2b31-final-20260920` 干净隔离检出提交并执行final gates；
1540个跟踪文件在gates和native前后hash完全一致，Git保持clean。
原工作区既有S2-B2/B3改动保留，未重置、切换或代为提交原索引；隔离提交已取回本地分支。
带入前阶段正式报告/公开证据与原工作区已有的历史任务文件删除，manifest显式移除已不存在的旧任务文件。

## 范围与实现

Mac-first S2-B3.1，R04/R05/R07/R08/R09/R10/R11/R16/R17/R19/R21。
局部AT-010–018、022–025、029–032、039–040、062–070、075、081、086–087；不表示这些完整AT已通过。
修改editor-app显示/GPU/验证、synthetic/scripts/docs；core/service/gerber-io/Cargo.lock相对dbde56e零差异。
有界world bins、顺序CSR、曝光/层隔离、局部孔洞/宏保持，reference.wgsl保留；
高亮四路有序候选合并，preview平移边界，无全局selected扫描。
跨LOD使用固定Region保守边界复用基础index；预览只建派生显示数据，释放保持一个制造事务。
对象/网格/引用/单格/候选工作和原geometry预算保留；未提高2000000000工作上限，未新增依赖。

## 已执行结果

macOS arm64 / Apple M1 / Metal / Rust1.89.0（详见environment.json、native.log）。
20条自动命令exit0，workspace **329 passed / 0 failed / 7 ignored**。
其中6个GPU忽略项以原生release显式运行全部通过；余下私有CORE10忽略项未执行。
原reference独立制造覆盖探针继续保留；生产与reference **90组、每组16384像素完整RGBA字节相等**，零容差。
覆盖15个公开场景×3种selection×2种delta，含P1K、跨层Clear、Dark/Clear、Arc/Region/Macro/孔洞。
固定P1K逐字节等于上一阶段metrics_1000.gbr，1000圆/40×25、直径0.5mm、边界[0.75,0.75,40.25,25.25]。
指标原始JSON见1000-selection-metrics.json，1000exact/0unsupported，独立公式A=1000π/16、P=500π。
导航/预览前后metrics完整JSON、writer字节不变；制造revision/history不变；释放后一个Undo，全部坐标断言和Undo恢复通过。
服务正常依赖树无窗口/GPU依赖，保留gates/15.log。

原生GUI：实际物理画布1700×1470；Open/Fit、1000框选、1000/1000指标、三次短拖动/逐次一次Undo、30.609秒40次平移动作通过。
面板面积196.349541mm²、周长1570.796327mm；截图02/03/04/05与native.log对应。
三次拖动worker完成时间均13ms；这是worker时间，不是释放到最终显示时延。
截图03的object-1000从(40,25)到(41.789979,23.713454)，一次Undo恢复(40,25)和clean。
原生日志含应用帧间隔（非呈现时间），保留空闲长间隔，不通过筛掉慢帧伪造p95。

1600×900 release离屏补充：三轮各10秒、全部1000 selected预览，CPU准备+GPU fence p95分别
43.459208 / 40.989208 / 40.292917 ms。
原始数据offscreen-frames.json和metal-native.log；**离屏计时不等于原生GUI帧间隔，不签署AT-075**。

## 实际自动命令

|命令|退出码|日志|
|---|---:|---|
|`cargo fmt --all -- --check`|0|gates/01.log|
|`cargo check --workspace --all-targets --locked`|0|gates/02.log|
|`cargo clippy --workspace --all-targets --locked -- -D warnings`|0|gates/03.log|
|`cargo test --workspace --locked`|0|gates/04.log|
|`cargo test --locked -p editor-core --test geometry_metrics`|0|gates/05.log|
|`cargo test --locked -p editor-service --test metrics_workflow`|0|gates/06.log|
|`cargo test --locked -p editor-core --test s2b_rect_selection`|0|gates/07.log|
|`cargo test --locked -p editor-service --test s2b_select_rect_workflow`|0|gates/08.log|
|`cargo test --locked -p editor-service --test s1b_edit_workflow`|0|gates/09.log|
|`cargo test --locked -p editor-service --test s1b2_edit_workflow`|0|gates/10.log|
|`cargo test --locked -p editor-service --test s1b2b_transform_workflow`|0|gates/11.log|
|`cargo test --locked -p editor-service --test s1b2c_workspace_workflow`|0|gates/12.log|
|`cargo test --locked -p editor-service --test automation_contract`|0|gates/13.log|
|`cargo test --locked -p editor-service --test headless_workflow`|0|gates/14.log|
|`cargo tree --locked -p editor-service --edges normal`|0|gates/15.log|
|`cargo build --release --locked -p editor-app`|0|gates/16.log|
|`python3 scripts/source_manifest.py --check`|0|gates/17.log|
|`python3 scripts/test_audit_core10.py`|0|gates/18.log|
|`python3 scripts/test_package_source.py`|0|gates/19.log|
|`cargo test --release --locked -p editor-app thousand_circles_metrics_writer_navigation_preview_and_history -- --nocapture`|0|gates/20.log|

额外native命令/exit0见native-commands.json；本机GPU访问在沙箱外执行。
开发时沙箱内Metal返回无适配器，及早期选择边缘parity失败，均保留在本机development evidence；
经有序归并修复后，冻结提交的final日志全部通过，未放宽容差或删除场景。

## 未完成、风险与下一步

- **B1：S2B31-NATIVE-AT075**。CUA仅提供短drag，没有按住/中间轨迹/持续时间接口。规定的原生10秒×3尚未执行；p95≤50ms、release≤300ms不能靠离屏/短拖动签署。
- **B1：S2B31-NATIVE-FIXED-CANVAS-ZOOM**。原生实际1700×1470（比目标大），1600×900只在离屏和自动服务测试执行。CUA没有捏合缩放接口；规定固定尺寸原生Fit/Pan/Zoom未全部验证。
- S2B3-LARGE-SELECTION中的“预算拒绝且不能原生查询1000指标”已解除；整体renderer退出条件仍因以上证据缺口保留。
- preview索引当前同步构建，P1K CPU准备数据已采集；200000对象上限场景可能卡UI。后续大样本改后台/增量，不宣称P100K通过。
- 保守max-cell工作预算、Region全圆包络可能拒绝密集合法文件；保持明确RESOURCE_LIMIT，不以漏画提速。
- Windows deferred / not executed；CORE10、完整AT-071/072/073/075、双平台V1、发行签名/公证均未完成。
- 无Grid/Snap、文字、Final Layer Area或多格式扩张。

本轮源代码包与当前公开证据companion ZIP分开交付；source ZIP内包含上一阶段公开证据，当前final证据不回写受测HEAD。
