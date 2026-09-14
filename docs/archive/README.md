# 归档清单身份

`design-package-1.1.sha256` 是修补前根目录 MANIFEST.sha256 的原样副本，
用于保留原设计包所声称的文件身份。原包 README 等文件已被实现阶段修改，
故它不能验证当前工作区，也不表示本轮已找回或验证原设计包全部字节。

根目录 `MANIFEST.sha256` 自 S0-B 起只列当前可交付源代码、文档、脚本和公开合成小样，
不列自身，不含私有 Gerber、字体、缓存或二进制。生成和核对：

```
python3 scripts/source_manifest.py
python3 scripts/source_manifest.py --check
```

Windows 可用 `python` 替代 `python3`；脚本无第三方依赖。
每次实际构建的二进制另存于该运行的 `binary-manifest.sha256`，不能混作源码清单。
历史源哈希、失败输入/日志和新运行结果均另存，不覆盖之前的验收证据。
