# 离线表结构

这两个 JSON 文件被编译进程序，用于 `/master-schema` 及旧 APK 快照来源校验，因此随源码保留。

- `master-schema-global.json`：国际服 Android 1.0.1，240 张表；字段来源为客户端 IL2CPP 元数据。
- `master-schema-jp.json`：日服 Android 1.0.2，235 张表；字段类型从解密的 Master JSON 推断。

文件包含表结构和 APK 摘要，不包含账号凭据或完整游戏记录。它们是特定客户端构建的结构索引，不表示线上最新版。线上 Master 版本及表数据通过配置的 Sirius 快照存储读取。
