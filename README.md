# Penlight-Notes-API

BanG Dream! Our Notes 非官方第三方 API，基于 Rust 和 Axum。

日服在线通信直接复用 [Srirus-Project/sirius-api-proxy](https://github.com/Srirus-Project/sirius-api-proxy) 的 MIT 源码，编译进同一进程。保留 Haruki Dev Team、Sirius Project 的贡献与完整许可证，详见 [来源说明](docs/upstream-attribution.md) 和 [第三方声明](THIRD-PARTY-NOTICES.md)。

已提供应用状态、公告、公开玩家资料、歌曲及活动排行、现有账号数据，以及十类 Master 资源的列表与详情。2026-09-27 已使用自有日服账号完成在线验证，并导入与服务器版本一致的 235 张 Master 表；该快照的活动列表为空，活动排名请求返回空排名。活动开始后的实际榜线分数仍需验证。国际服目前提供离线 Master 查询，在线接入尚未完成。

## 下载与发布

可从 [GitHub Releases](https://github.com/Lanyinn-Labs/Penlight-Notes-API/releases) 下载 Linux x64、macOS ARM64 和 Windows x64 的完整运行包。版本标签发布时同时构建 GHCR Docker 镜像；具体发布条件、校验方法和下载包用法见 [发布说明](docs/releases.md)。首次正式发布前可能还没有下载资产。

## 运行

需要 Rust 1.97（见 `rust-toolchain.toml`）。

```bash
cp .env.example .env
cargo run --locked
```

默认监听 `http://127.0.0.1:8081`。未配置在线协议时，可查询健康状态和内置表结构；在线接口返回 `501 protocol_pending`。

### 接入日服现有账号

1. 将 `config/jp.example.json` 复制为 `config/jp.local.json`。
2. 将 `accounts[].credentials_file` 改为已有凭据 JSON 的路径。凭据格式为 `{"player_id":"...","credential":"..."}`；文件应使用 `0600` 权限。提取自有账号凭据的方法见 [账号配置说明](docs/account-setup.md)。
3. 在 `.env.local` 配置 `OURNOTES_JP_PROTOCOL_CONFIG=config/jp.local.json`，并设置自己的 `API_KEY`。
4. 将配置中的 `master_directory` 指向已导入的 Master 存储，随后启动服务。

协议目录与官方客户端版本是两个配置项；随附 JP 协议目录为 `1.0.3`，本次实测客户端版本为 `1.0.2`。无需启动另一套 HTTP 代理服务。

```bash
curl http://127.0.0.1:8081/health
curl -H "X-API-Key: $API_KEY" http://127.0.0.1:8081/api/jp/application
curl -H "X-API-Key: $API_KEY" http://127.0.0.1:8081/api/jp/music
curl -H "X-API-Key: $API_KEY" http://127.0.0.1:8081/api/jp/user/profile
```

示例中的 `$API_KEY` 需先设置到当前 shell；程序读取 dotenv 不会改变父 shell 的环境变量。完整接口与配置见 [API 文档](docs/api.md)、[配置参考](docs/configuration.md)。

### 导入 Master

可从自有 Android 客户端复制 `files/Master`，用已验证的日服 1.0.2 元数据导入：

```bash
cargo build --locked
uv run --no-project --with py3rijndael python scripts/import_jp_master.py \
  artifacts/jp/phone-master artifacts/jp/master-store
```

默认元数据路径为 `artifacts/jp/global-metadata.dat`；可通过 `--metadata` 指定。脚本读取客户端常量并调用本程序的 `master-import`，不会打印密钥。发布后的存储包含版本、快照和摘要，接口会校验这些信息。此步骤是本地导入；自动更新需要另行配置上游的 Master 更新或同步能力。

未配置在线 Master 存储时，保留原有 APK 快照查询方式；国际服可用 `scripts/decrypt_master.py /path/to/client.apk` 导出已验证的 1.0.1 APK，并设置 `OURNOTES_GLOBAL_MASTER_DIR` 指向输出目录；工具说明见 [scripts/README.md](scripts/README.md)。

## Docker

```bash
docker compose up -d --build
```

默认 Compose 读取 `.env`，提供基础服务。日服配置、凭据与 Master 存储需额外挂载；见 [配置参考](docs/configuration.md#docker)。镜像内包含协议文件和第三方许可证，以 UID 10001 运行。

## 开发

```bash
python3 scripts/check_upstream.py
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

上游来源固定到提交 `c2df04b7979cadc89fd25fa120cb8c406aa4ef86`，校验脚本验证所复用的 179 个文件未被修改。后续计划见 [开发计划](docs/roadmap.md)。

## 许可证

本项目采用 [MIT License](LICENSE)，Copyright (c) 2026 Lanyinn-Labs。第三方代码遵循其保留的原始许可。本项目与游戏开发商及运营方无关联。
