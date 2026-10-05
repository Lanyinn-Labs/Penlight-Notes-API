# 下载包使用

从 [GitHub Releases](https://github.com/Lanyinn-Labs/Penlight-Notes-API/releases) 下载对应平台的包：

| 包后缀 | 平台 |
| --- | --- |
| `linux-x64.tar.gz` | Linux x86_64，glibc 2.35 或更高 |
| `macos-arm64.tar.gz` | Apple Silicon macOS |
| `windows-x64.zip` | Windows x86_64 |

Release 附 `SHA256SUMS.txt` 用于核对下载包，包内 `release-manifest.json` 记录文件摘要。

解压后进入包的根目录，保持 `vendor/sirius-api-proxy/protocol` 的相对位置：

```bash
cp .env.example .env
./penlight-notes-api
```

Windows 使用 `penlight-notes-api.exe`。基础健康检查无需账号；在线查询需按 [账号配置](account-setup.md) 配置已有凭据，部署参数见 [配置参考](configuration.md)。

包内账号和 Master 辅助工具见 [工具说明](../scripts/README.md)。使用 Master 导入工具时指定可执行文件：

```bash
uv run --no-project --with py3rijndael python scripts/import_jp_master.py \
  /path/to/copied-master /path/to/master-store \
  --metadata /path/to/global-metadata.dat --executable ./penlight-notes-api
```

Docker 部署见 [Docker 配置](configuration.md#docker)，客户端与 Master 更新见 [自动更新](updates.md)。
