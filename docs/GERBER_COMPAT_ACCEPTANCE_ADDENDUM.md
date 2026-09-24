# Gerber Compatibility 验收附录（S4-B3 后，Mac-first）

关联 R02/R03/R04/R06/R14/R19 与 AT-005/006/009/015/020/053/080。本附录不修改 `ACCEPTANCE_V1.md` 或 `acceptance_cases.json` 的 96 个正式 AT 身份、步骤、预期与双平台门槛。`AT-079` 仍为退役编号。

| Case | 输入与独立真值 | 产品路径和通过条件 |
| --- | --- | --- |
| GC-01 | 前导零 FS 整数位不足、重复 FS；已知坐标 | Strict 拒绝；Service 兼容导入；坐标与独立数值一致，警告保留 |
| GC-02 | 非恒等 MI；已知 Flash 坐标及圆弧方向 | Strict `Unsupported`；Service 导入；坐标镜像而光圈局部形状不变 |
| GC-03 | 近闭合、显式闭合线段、G74→G75 Region；解析式覆盖点与边界 | 修复类别可见；编辑、工程保存/重开保留警告；独立几何比较通过才可确认生产导出 |
| GC-04 | AM20 水平/竖直/任意角、旋转、非零端点、Dark/Clear；平端矩形解析式 | Service 兼容导入；宽、长、方向、中心及覆盖点与解析式一致 |
| GC-05 | AM22 非零下左角/旋转/负坐标/表达式；等价中心矩形解析式 | Service 兼容导入；四角及覆盖点一致 |
| GC-06 | Thermal 7 非零中心/旋转/尺寸；外圆减内圆与十字缝解析式 | Strict `Unsupported`；Service 导入；Dark/Clear 顺序和覆盖点一致 |
| GC-07 | 斜向矩形扫掠；凸包解析式 | Strict `Unsupported`；Service 导入；边界和覆盖点一致 |
| GC-08 | 含非 UTF-8 注释但 ASCII 几何有效 | Strict `InvalidUtf8`；Service 导入；只替换文本，制造坐标与对象不变 |
| GC-09 | `C,0`；原始零面积与 2 µm 占位圆面积 | 警告声明有损转换，`.rcam` 往返仍可见；Gerber 导出需要显式确认，不宣称与源等价 |
| GC-10 | 同一冻结 856 清单与哈希 | Product Service 逐文件记录 STRICT/COMPAT/REJECTED/IO，并与 parser 结果逐项核对；任何失败如实保留 |
| GC-11 | 兼容几何在工程精度下量化失效 | 返回 `compatibility_precision_override`、工程与所需 mm；仅显式授权的本次导出使用更细精度 |
| GC-12 | `CompatibilitySolid`，EP11BAM top/bottom、art08；本地独立 CAM probes | 导出确认含数量、图层、问题类别；外部几何差异未消除前不批准生产规范化导出 |

AM5/AM6、未知绘图命令、资源上限仍 fail-closed。最大对象数 1,000,000、解析命令数 4,000,000、源字节 64 MiB 不扩大。803,226 对象成功解析/建场景不等于原生 GUI 交互或 P100K 通过。Windows、CORE10、完整 V1 门槛不因本附录通过。
