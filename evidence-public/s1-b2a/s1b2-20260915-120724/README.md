# S1-B2a macOS 运行证据

本目录保留独立运行 ID。gates 为实际命令输出，commands.json 含退出码与耗时；
tested-source-hashes.json 绑定 Rust 源码／Cargo 文件，release-sha256.json 绑定实际 release 二进制。
所有工作流输入来自仓库合成测试或测试中的公开构造字符串，不含私有 Gerber、真实生产样本或字体。
s1b-workflows / s1b2-workflows 保留真实 JSON 请求、输出与合成 Gerber，便于复核服务链路。
主机绝对路径替换为 <workspace>/<user>；redaction-manifest.json 记录原件及脱敏件 SHA。

开发失败也保留在 development：初次测试把 1e8 mm 错当越界；空图层实际导出失败；
枚举命名 clippy 告警；旧 capabilities 测试仍把新实现列为 unsupported。
分别更正测试真值、修复空图层支持、简化枚举名称、更新能力契约；未删掉失败用例或降低门槛。
最后运行 gates/01–10 全部退出 0。

acceptance-results.json 中完整 V1 的全部逐平台槽仍为未执行，局部阶段结果单列。
132/0/2 不包含被忽略 GPU／私有审计项；Windows、GUI、IME、CORE10 完整流程及生产输出未通过。
历史 S1-B1 结果单独归档在 evidence-public/s1-b1，不当作本轮重新执行旧提交。
