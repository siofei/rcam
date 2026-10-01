# INFRA2 构建源身份整改（S4-D2 后置，送审前）

用户于 2026-10-01 授权本整改及复审通过后的阶段 Git commit。基线 8512a7c3d1d9000f7374f1d1484d413bbb9b41cb；分支 codex/infra2-build-source-identity。本轮只修复 no-.git 源 ZIP 构建版本来源，不启动其他产品功能。

对应 R01/R18/R19/R20、AT-001/002/085 的构建/证据局部检查，回归 AT-086/087；不改变冻结 96 个用例或门槛。允许 editor-app build.rs/build_identity.rs/Cargo.toml 和身份测试、源码打包脚本/测试、MANIFEST.sha256、本任务/设计/ADR/证据文档。制造模型、服务 API、UI 编辑行为不变。

- BI-01：仅工作区自己的 Git 可提供 HEAD，Git 优先，脏状态追加 -dirty。
- BI-02：无 .git 必须通过 PACKAGE_INFO、PACKAGE_MANIFEST、MANIFEST 和全量分发源码的 SHA-256/路径/数量/集合校验，才能声明 archive-verified；缺失、损坏、过期、修改或新增源码均构建失败。
- BI-03：无关父 Git 仓库不提供源身份；自己的 Git 出错不回退归档。
- BI-04：同一 target 的源码/metadata/manifest/HEAD/脏状态变化触发身份重算。
- BI-05：Git packager 验证请求提交等于本树 HEAD；非 Git 重打包须验证旧包，不得替新源码签发旧 clean 身份。
- BI-06：复审通过后提交；新干净提交再次构建、打包、fresh-extract 构建核对真实身份，保留原始日志。Windows deferred；Mac-first bounded 不代表完整 V1/CORE10/P100K。

信任边界：SHA-256 是内容绑定，不是数字签名。归档 PACKAGE_INFO 中的提交声明须由外部可信交付 ZIP 哈希和提交核对授权；恶意同时重写源码与所有元数据无法由自包含清单证明其 Git 历史。禁止把语法合法的任意 commit 当作认证证据。

预提交实现及测试后停在独立复审点。最终 commit/干净打包证据待复审安排，不提前宣称完成。

首次独立复审 REQUEST CHANGES：本次只修复 root 哨兵可关闭 rerun、继承 Git 环境可取外部身份、损坏 .git symlink 回退三项，并纳入既有 19 条变异。新回归覆盖哨兵预存/后加/旧 mtime、Git DIR/WORK_TREE/INDEX/COMMON_DIR/config 环境的构建与打包隔离、Git marker symlink 拒绝。完整门禁重跑后再次冻结送审，不提交，不启动业务阶段。

R2 复审关闭原三项，新增 schema true/1.0 重打包类型不一致 P2。R3 仅整改严格版本/计数和邻近字段、原始清单字节及 JSON 边界；添加两语言实际 validator/重打包拒绝矩阵，正确整数 schema v1 继续通过。适用门禁后再次冻结送审，不提交。

## R3 放行后的阶段收口（2026-10-01）

独立 R3 源码复审为 PASS（Mac-first bounded），关闭 P2 类型差异且无新阻塞，见 `exports/INFRA2_R3_INDEPENDENT_REVIEW_20261001T204924Z/INFRA2_R3_INDEPENDENT_REVIEW.zh-CN.md`。用户随后授权 Mac 原生验证 → 本阶段提交 → 新 clean HEAD 完整门禁与四件套/fresh-extract 实际 editor-app 构建 → Library 交付。

提交前代码与 R3 冻结的 502 文件快照逐项一致。受控合成原生 worker/ApplicationService/Metal 21 项断言通过，进程 exit0，窗口捕获成功，预提交身份明确为 8512a7c3d1d9000f7374f1d1484d413bbb9b41cb-dirty；本节只追加授权和验证事实，不再改产品代码。最终新提交、public/internal 二进制、archive-verified 身份、包哈希及新测试结果以 `exports/INFRA2_FINAL_20261001T210339Z/REVIEW.zh-CN.md` 及其原始证据为准，不使用本段预先宣称这些后续步骤成功。

保留主工作区旧未提交改动；不 push，不启动 S5-M1 或其他阶段。原生证据复用 S4-D2 合成驱动，仅作为 INFRA2 构建身份和 GUI/Metal 回归，不主张人工键鼠/IME、Windows、完整 V1/CORE10/P100K 或签名/公证验收。
