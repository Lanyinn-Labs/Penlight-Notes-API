# 维护工具

| 脚本 | 用途 |
| --- | --- |
| `check_android_version.py` | 只读检测 Google Play 官方安卓版本并与当前配置比较 |
| `check_release.py` | 校验来源及 GitHub 版本标签 |
| `package_release.py` | 按公开文件清单打包运行环境 |
| `smoke_release.py` | 解压并校验下载包，验证离线启动及认证 |
| `docker-entrypoint.sh` | Docker 启动时下载、校验并原子替换客户端配置；失败保留缓存 |
| `smoke_container.py` | 关闭容器网络后验证镜像启动及认证 |
| `check_upstream.py` | 离线校验原始 Sirius 文件摘要及许可证；CI 使用 |
| `import_jp_master.py` | 从已验证的日服元数据读取常量，调用进程内 Master 导入器 |
| `inspect_jp_local_save.py` | 从自有日服存档副本恢复现有账号凭据 |
| `decrypt_master.py` | 导出已验证的国际服 1.0.1 APK Master JSON |
| `decrypt_master_split_apk.py` | 导出已验证的日服 1.0.3 分包 Master JSON，也为前两个日服工具提供元数据读取函数 |

发布工具需要 Python 3.11 或更高版本，不依赖第三方 Python 包。账号和 Master 工具需要 `py3rijndael`，可用 `uv run --no-project --with py3rijndael python scripts/<script>.py --help` 查看参数。Python 至少使用 3.10。

导出工具通过元数据偏移和 SHA-256 指纹校验客户端常量；已验证的在线客户端版本、CDN 认证和 Master key/IV 集中在 `data/jp-client.json`，由程序启动时读取，Docker 启动时拉取并缓存最新版本。部署者的账号、API Key 等个人认证不进入该文件。解密产物默认留在忽略的 `artifacts/`；这些脚本针对已经验证的具体构建，不自动适配新版本。

Frida 探针、协议探索和报告生成工具已移出发布目录。正式服务通过 Sirius 客户端完成通信。
