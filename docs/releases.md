# 构建与发布

本项目参考 [Sirius API Proxy 的工作流](https://github.com/Srirus-Project/sirius-api-proxy/tree/main/.github/workflows)，采用检查、打包验证和发布三个步骤。提交及版本标签不要求签名。

## 日常检查

推送到 main 或向 main 提交 PR 时：

- CI 在 Linux、Windows 检查上游源码摘要、格式、编译、Clippy 和测试。
- Docker 工作流在 Linux 构建镜像，并关闭容器网络验证启动、协议文件、许可证、运行用户和 API 认证。PR 仅在与镜像有关的文件变化时触发。
- 使用 Rust 缓存与 Docker 构建缓存；同一分支的新检查会取消旧检查。

CI 不使用真实游戏凭据，不查询官方服务器。

## 发布版本

1. 修改根目录 `Cargo.toml` 的 package.version，并通过 Cargo 更新锁文件中的本项目版本。
2. 更新 `CHANGELOG.md`，提交变更，确认 main 的检查通过。
3. 为该提交创建匹配的版本标签并推送。例如当前版本 0.2.0：

```bash
git tag v0.2.0
git push origin main
git push origin v0.2.0
```

版本标签必须与根 Cargo.toml 完全匹配，不匹配会在构建前失败。操作前确认当前提交就是准备发布的版本。

### 二进制

Release 工作流在三个原生 runner 构建并测试：

| 下载包后缀 | 平台 |
| --- | --- |
| linux-x64.tar.gz | Linux x86_64，使用 Ubuntu 22.04 构建（glibc 2.35 或更高） |
| macos-arm64.tar.gz | Apple Silicon macOS，最低系统版本取决于 GitHub macos-latest 的构建环境 |
| windows-x64.zip | Windows x86_64 |

每个包包括可执行文件、协议目录、配置示例、账号/Master 辅助工具、运行文档和完整许可证。包内 release-manifest.json 记录每个文件的 SHA-256；Release 同时附 SHA256SUMS.txt，用于核对下载包。

打包使用公开文件清单，排除账号、Master 明文记录、抓包、分析归档及本地配置。随后实际解压、核对摘要，并使用空账号和本地不可用上游地址启动服务，检查协议初始化及认证行为。全部平台通过后，使用仓库的 GITHUB_TOKEN 创建 GitHub Release，附自动生成的发布说明。

手动运行 Release 仅产生 Actions 下载产物，不自动发布 Release。

### Docker 镜像

标签发布验证通过后，推送到：

```text
ghcr.io/lanyinn-labs/penlight-notes-api
```

实际镜像路径由 GitHub 仓库名称自动生成并转换为小写。目前构建 linux/amd64；每次版本发布生成完整版本、主次版本和提交 SHA 标签，稳定版本还会生成 latest。

```bash
docker pull ghcr.io/lanyinn-labs/penlight-notes-api:0.2.0
```

日常 main push 只构建验证。手动运行 Docker 时，默认不发布；选中 publish 才推送。GHCR 使用 GITHUB_TOKEN，无需配置 Docker Hub 密码。组织的 Actions/Packages 权限必须允许写入包；首次发布后，在 GitHub 包设置中检查公开可见性。

容器以 UID 10001 运行。真实使用时仍需挂载协议配置、凭据和 Master 存储，见 [配置参考](configuration.md#docker)。

## 使用下载包

解压后进入包的根目录，保持 vendor/sirius-api-proxy/protocol 的相对位置：

```bash
cp .env.example .env
./penlight-notes-api
```

Windows 使用 `penlight-notes-api.exe`。基础健康检查无需账号；在线查询需按 [账号配置](account-setup.md) 配置现有账号。

调用包内 Master 导入工具时需指定可执行文件，例如：

```bash
uv run --no-project --with py3rijndael python scripts/import_jp_master.py \
  /path/to/copied-master /path/to/master-store \
  --metadata /path/to/global-metadata.dat --executable ./penlight-notes-api
```

## 本地验证

Python 3.11 或更高版本，无额外 Python 依赖：

```bash
python3 scripts/check_release.py
cargo build --release --locked
python3 scripts/package_release.py --target linux-x64
python3 scripts/smoke_release.py dist/penlight-notes-api-0.2.0-linux-x64.tar.gz
```

Docker 环境可用时：

```bash
docker build -t penlight-notes-api:local .
python3 scripts/smoke_container.py penlight-notes-api:local
```

工作流和脚本保存在本仓库；首次推送后仍需确认 GitHub runner 上的三个平台及容器构建结果。本地 Linux 验证不能替代其他平台构建。
