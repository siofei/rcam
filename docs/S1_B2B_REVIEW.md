# S1-B2b Rotate / Mirror 交付与验收

日期：2026-09-15；起点：256dbc9；最终运行 ID：s1b2b-20260915-125200。
**结论：PASS（macOS 本阶段实现门禁）；完整 V1 未完成。**

## 范围与改动

阶段 S1-B2b；R01/R04/R05/R10/R11/R14/R15/R16/R19/R21/R22。
局部关联 AT-013/014/015/016/033/034/039/040/042/054/055/086/088/090/091/095/097。
允许且实际修改：editor-core 制造变换/Region 校验/历史复用、editor-service DTO 与分派、相关测试、
设计/用例追加说明、ADR、README、源码清单和公共证据。未新增依赖；gerber-io writer 和 editor-app
源码未改动。用户交接文档、私有样本、冻结 CORE10、字体和历史证据未覆盖。

1. 新增真实服务 objects.rotate / objects.mirror，显式对象、单层、expected_revision，成功一次原子 Modify。
   复用 Move 的候选验证与预算计费；失败不改内容、不推进 revision、不清 Redo。
2. Flash 同时变换中心及局部方向，按 R * M * scale 组合正交矩阵并确定性分解核对；光圈引用、scale、
   exposure、origin、对象 ID、当前曝光顺序保持。矩形/长圆/多边形和非对称宏的局部孔洞经独立覆盖点验证。
3. Line 支持任意有限角；Arc 变换 start/end/center，镜像翻转 CW/CCW，保留 full_circle、zero_sweep、source
   与偏差语义；Region 保持轮廓/边序并重新做语义校验。未重拟合制造圆心，未使用 Mesh 反推几何。
4. RectangularSweep 仅接受精确整数90°旋转，奇数转交换宽高；任意斜角的混合选择整体 UNSUPPORTED_FEATURE。
   Mirror 仅世界 horizontal(y=c)/vertical(x=c)。capabilities 明示限制，无“所有对象任意角”承诺。
5. 修复两处既有 Region 校验的方向/拓扑问题：两边闭合轮廓豁免两个合法共享端点；平行 cut-in 按方向
   向量识别，37° 后孔洞保持。原 1e-6 mm 容差、内区交叠拒绝、非平行切入和非法自交检查保留。
6. ADR 0011 冻结 GUI workspace 聚合单文件 service documents 的方向，并记录 manufacturing dirty 与 workspace
   lock/name/visibility 拆分要求。本轮未新增 layer.update，也未重构结构历史。

## 实际执行与结果

环境：macOS 26.5.1、arm64、Rust 1.89.0（详见 environment.json）；使用现有 .tools 工具链和缓存，
非干净发行环境。CPU 型号查询被沙箱限制，未引用旧机器信息替代；本轮不做 release 性能合格声明。

公共证据根：`evidence-public/s1-b2b/s1b2b-20260915-125200/`。
每条命令保留原始 stdout/stderr 与退出结果（公共副本只替换工作区绝对路径，原件见 evidence/ 同 run）。

| 实际命令 | 退出码 | 公共日志 |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | gates/01.log |
| `cargo check --workspace --all-targets --locked` | 0 | gates/02.log |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | gates/03.log |
| `cargo test --workspace --locked` | 0 | gates/04.log |
| `cargo test --locked -p editor-service --test s1b_edit_workflow` | 0 | gates/05.log |
| `cargo test --locked -p editor-service --test s1b2_edit_workflow` | 0 | gates/06.log |
| `cargo test --locked -p editor-service --test s1b2b_transform_workflow` | 0 | gates/07.log |
| `cargo test --locked -p editor-service --test automation_contract` | 0 | gates/08.log |
| `cargo test --locked -p editor-service --test headless_workflow` | 0 | gates/09.log |
| `cargo tree --locked -p editor-service --edges normal` | 0 | gates/10.log |
| `cargo build --release --locked -p editor-app` | 0 | gates/11.log |
| `python3 scripts/source_manifest.py --check` | 0 | gates/12-source-manifest.log |

- workspace **164 passed / 0 failed / 2 ignored**。两项 ignored 仍为原生 GPU 与显式私有 CORE 审计入口，未计通过。
- S1-B1 **17/17**；S1-B2a **27/27**；S1-B2b **31/31**；automation_contract **4/4**；headless_workflow **2/2**。
- 正常服务依赖树无 egui/eframe/egui-wgpu/wgpu/winit；结果见 dependency-audit.json。
- 额外实际运行 `python3 scripts/test_audit_core10.py`：退出0，7/7（只验证审计脚本，不是 CORE10 产品验收）。
- 96 个有效 cases、退役 AT-079 和全部逐平台 required_platforms 与基线逐项一致，见 spec-integrity.json。

## 制造几何与失败证据

workflow/ 包含合成 source.gbr、实际 exported edited.gbr、真实 JSON 请求/响应，覆盖打开→查询→变换→
Undo→Redo→验证→导出→新文档重开。已知点用独立矩阵公式，环/线/有孔矩形/非对称宏/方环 Region
使用独立物理覆盖公式核验，不以自身 parse/write 往返作为唯一真值；源文件字节和 source_sha256 保持。

专项覆盖非有限数、溢出/不可靠数值、0/整周/最终不变、未知/重复/空/超量 ID、锁定、stale revision、
历史条数/字节预算与失败不清 Redo。Undo/Redo 直接比较完整存储序列化，未使用逆变换恢复。
重复变换的角度按 [0,360) 规范化；一般 f64 点仍用原制造容差，精确90°整数样本回到原存储状态。

开发失败记录保留在 development/：

- first：23/25，Region 量化后第二共享端点误拒绝；修复后增加超端点容差内区拒绝对照。
- region-fix：24/25，孔洞 cut-in 旋转后未识别；改为共同方向检查，增加独立方环孔洞与混合方向拒绝。
- initial-gates 与 clippy-followup：测试中的单元素循环及区间比较触发 clippy；整理代码，未增加 allow 忽略。
- 最终独立 run 全门禁退出0；开发失败未删除，也未下调门槛或更换失败样本。

## 源码与二进制绑定

- tested-source-hashes.json：全部 Rust/crate manifests/shaders 与 Cargo/toolchain 锁定文件 SHA-256；归档后复核一致。
- release：`.tools/target/release/editor-app`。
- release SHA-256：`dc24f3fab4d882cf88df0c5474fc1a2a439e49761f31cd70f5b0f174cf9c5dc5`。
- MANIFEST.sha256：可分发源码与公共证据清单；不包含私有样本、字体、构建缓存或二进制。
- 公共证据中的 `$WORKSPACE` 是绝对路径替换标识；源码和合成 Gerber 字节未替换，具体映射规则见 README.md。

## 技术债基线与下一步

workflow/order-guard-baseline.json 的合成短 ID/单对象 Duplicate，debug 实测：

| 图层对象数 | 本机耗时 | 结果 |
|---:|---:|---|
| 10,000 | 0.021503125 s | 成功 |
| 100,000 | 0.108177750 s | 成功 |
| 150,000 | 0.050176500 s | RESOURCE_LIMIT |
| 500,000 | 0.164299791 s | RESOURCE_LIMIT |

这是当前 O(N) order guard 和 64 MiB 预算的可复核基线；不是 release 性能指标，不能签署 AT-072/075。
进入 S2 前按 ADR 0011 完成简短 model hygiene；Production Renderer/大文件编辑前单独处理紧凑顺序守卫。

本轮按交接边界完成 S1-B2b 后停止，未自动开展完整 GUI。
以下保持未执行/未完成：Windows 原生/DX12，macOS GUI 编辑/Metal/IME/干净发行环境，文字，
layer.update/edit.batch/跨文档编辑/完整脚本扩展，CORE10 10/10，独立外部查看器本轮复核，
release 性能/长时稳定性、生产输出资格、签名/公证。原 CORE-03/07/08 和独立圆弧参考差异继续保留。
因此不宣布完整 AT、双平台 V1 或生产加工通过。
