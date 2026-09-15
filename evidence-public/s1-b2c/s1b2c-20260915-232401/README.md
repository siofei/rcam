# S1-B2c 公共证据

原始日志在 evidence/ 同名 run；本目录仅将工作区绝对路径替换为 `$WORKSPACE`。
合成 Gerber 字节与源码 SHA-256 未改动，私有 Gerber/字体未纳入。
`tested-source-hashes.json` 与 `source-identity-after.json` 绑定同一源码候选commit，
涵盖该commit全部930受管理文件；后续公共证据与交付说明属于单独 packaging commit。
workspace 181 passed / 0 failed / 2 ignored；ignored 为原生GPU和显式私有CORE审计，未算通过。
workflow 保存实际JSON请求/响应与合成导出；development保留测试字段拼写编译失败与clippy失败日志。
本目录只证明Mac本阶段局部门禁，不代表96完整用例、GUI或双平台V1通过。
