# ADR 0048：Git 与 no-.git 归档构建源身份

2026-10-01，INFRA2。Git 根目录必须等于工作区根且本根有 .git；优先使用 Git HEAD/status。脏构建显示 <HEAD>-dirty，不声明 pristine。自己的 Git 失败即失败，不使用旧归档 metadata；祖先 Git 不属于本源目录。

无 .git 构建必须校验 schema v1 PACKAGE_INFO、PACKAGE_MANIFEST 对 INFO/MANIFEST/每个源文件的哈希、INFO 中 manifest SHA/路径/计数与 MANIFEST/实际分发集完全一致，并要求 clean_worktree=true、两个提交字段相同且全长小写 SHA-1。未通过即中止构建，不产出宣称 pristine 的二进制。

复用 editor-core 的纯 Rust SHA-256 模块，不复制算法；增加已有锁定 serde_json 的 build dependency（MIT/Apache-2.0，版本见 Cargo.lock），无新第三方版本。正常 editor-service 依赖不变。构建脚本每次 Cargo 调用重新执行，避免新文件和 worktree refs 等变更造成增量身份过期；局部约 500 文件校验只发生在构建期，不是 UI 路径。

清单自洽只证明内容完整性；真实性依赖审核绑定的外部 ZIP 哈希及提交证据，不宣称签名认证。最终提交后重新生成清单/源码包，并 fresh-extract 验真。

独立复审整改：rerun 输入改为 Cargo.toml 文件的子路径，合法 Cargo manifest 必须是文件，该路径无法由普通哨兵文件变成存在状态；锁定 Cargo 1.89.0 的实际增量测试覆盖旧根哨兵预存/后加/旧 mtime。Rust/Python Git 子进程共同移除所有继承 GIT_* 变量，包括目录、work tree、index/common-dir 和 config 注入。使用不跟随链接的 .git marker 检查，拒绝所有 .git 符号链接（含损坏链接），保留合法普通 .git worktree 文件；自身 marker 错误不回退归档。

R3 类型一致性整改：归档 schema tag 与计数必须为 JSON 整数（Python type(value) is int，bool/float/string/null 不可代替）；Boolean、String、路径数组保持严格语义值，不自动转换。Python 不允许通过重打包把非标准 NaN/Infinity 或孤立 surrogate 转成 Rust 可接受输入。MANIFEST 必须按原始 UTF-8 字节绑定，不通过 universal newline 归一化；PACKAGE_MANIFEST 的分行规则使用 LF/CRLF，与 Rust str::lines 一致。验证失败发生在创建目标 ZIP 之前。

JSON 元数据输入沿用锁定 serde_json 1.0.151 的默认 128 递归预算（最多 127 个容器）和超出 i64/u64 整数的有限 f64 边界；不是新增制造数量/精度门槛。正确绑定的 CRLF 清单仍可验证与重打包，错误 LF 摘要不能靠换行归一化放行。
