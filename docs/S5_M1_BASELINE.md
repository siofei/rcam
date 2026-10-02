# S5-M1 Mac baseline（冻结协议；性能待测）

基线commit 2cde95380ab9a7161713cf5f1c15a843edb47803。环境原始探测另存 exports/S5M1_BASELINE_20261001T2125Z；与S0历史BASELINE分开。macOS26.5.1/25F80 arm64，Apple M1 CPU8/GPU8，RAM17179869184 bytes，AC Power；Rust/Cargo1.89.0（后续每个运行再记录二进制SHA/编译配置）。锁文件与96用例SHA见entry-baseline.json。

CoreGraphics/AppKit真实屏幕UP27R3，3840×2160pixels、1920×1080logical、backingScale2，当前mode113/144Hz。默认system_profiler没返回显示器，不据此宣称没有显示器。包括duplicate/HiDPI模式后发现同几何mode117/60Hz；性能运行时临时切至117并实测确认，结束恢复113/144Hz，不永久更改用户设置。2026-10-01 UTC首轮已实测切至60Hz，并恢复113/144Hz，逐次原始JSON随run保存。未测真实present/scanout，所有interval称app帧，原生窗口非最小化、焦点及物理区域每帧核对。

目标画布1600×900physical，即此屏800×450logical；按实际canvas矩形自动校准，不把整个窗口或logical面积冒充画布。导航活动70秒（warm10+frozen60）各3run，3个独立进程；连续60秒每帧全保留，nearest-rank p95/p99，无>200ms，首帧仅无前帧interval，其后不丢帧。轨迹sinusoidal平移+有界缩放参数写入protocol后不可调门槛。记录录像与单窗口surface图，不采私人桌面。

P100K复用generate_s2b32.py v1既有1000×100网格C0.5/1mm pitch，100000 Flash，1个光圈，无随机种子（确定算法）。SHA2568111ecada7a66defe30f614cd3851a328d861bbab4c595cb12ee3131fa465a31。固定200个row-major序号/坐标和全视图独立预期在新protocol，不使用产品选择结果作唯一真值。新P10K_CROP为100×100/1mm pitch，C/R/O/P各2500，尺寸/布局/曝光固定，由generate_s5m1 v1补齐，旧P10K圆样本不覆盖。编码前已冻结manifest；框选起点在首测前从[0,0]修订为空白边距[-40,-20]，终点[1040,120]；样本、200点、阈值不变，原草案和理由另存。当前protocol SHA196a7888ec13936e0f1fbd1fcc5bf266a7b22216e84147c2e580077d318e290e。

首次完整加载：冷=独立新app进程、无app缓存（OS页cache不强制purge、明确非磁盘冷），热=同进程Close→再次Open，P10K和P100K各冷/热3次；起点取正常Open入队（保守上界包含真实read前检查），终点同序列完整scene/object count、Fit结束、正常production callback绘制后的surface提交GPU完成保守上界。不可操作/错误/last-good旧帧均不算完成。

闲置：P10K稳定后无主动harness重绘静置60秒，外部每0.5秒ps RSS/%CPU采样；任务由外部受控事件在60秒后送达，worker完成request_repaint的真实更新须记录。重绘数包含普通产品recovery/tooltip等真实原因，不事后排除。

内存：单进程P100K20轮，每轮3个倍率1/2/4再回Fit、Close后静置5秒，统一RSS bytes；外部ps采样及macOS wait4对实际app子进程ru_maxrss的生命周期峰值（bytes，不含Cargo/compiler/helper），GPU显式分配来自真实Buffer.size/Texture统计，统一内存不相加。静置20−5<=100MiB且无持续线性增长；峰值RSS<=1GiB、显式GPU<=512MiB；不更改显示准入策略。

点选：200个固定制造中心，计时前由受控固定相机设置放至可点位置，等待正常viewport准备完成（该设置不冒充输入导航性能），RawInput Pointer事件经原gesture/ProbeDrag，trace从app接受事件起，不直接写SelectionSet。CPU精确查询p95<=20ms；正常callback可见highlights的保守GPU完成上界p95<=100ms。整图Window marquee含全部100000对象，源曝光顺序完整ID哈希独立校验，结果就绪<=300ms；E2E边界及UI帧一并报告。不冒充人类硬件输入。

## S5-REV-02 采集边界补充（2026-10-02；不改样本/轨迹/阈值）

上文“首帧仅无前帧interval”描述旧采集实现。整改后 start 输入帧作为明确前帧，raw frames 为 `(start.frame_id, end.frame_id]`，首 navigation interval 也有真实时间差；不省略首段。start 另记录进入冻结阶段的 phase_origin_ns；end 为 input 时钟首次达到60秒的帧，随后正常 paint 的 acknowledgement 仍纳入原始记录。所有 interval 与 elapsed 必须独立由同一单调时钟复算，start/end 自身的 elapsed 与其单调记录时间一致。没有任意最小帧数门槛，首段及全区间使用既有200ms停顿标准；nearest-rank 对完整区间间隔计算。此补充只是完整证据定义，不能推断回填历史缺字段记录；旧样本/机器/60秒轨迹/门槛/失败文件原样保留。
