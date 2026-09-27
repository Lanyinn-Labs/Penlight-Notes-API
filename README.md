# Penlight-Notes-API

基于 Rust 和 Axum 的 BanG Dream! Our Notes 非官方 API 服务。

## 功能

- 日服应用状态、公告及公开玩家资料查询
- 现有账号资料、编队、卡牌、道具及歌曲成绩查询
- 歌曲排行、活动排行及活动榜线缓存
- Master 快照查询、资源列表与详情、日文名称解析

日服在线接口已接入；国际服目前提供离线 Master 查询。实际活动榜线分数待活动开放后验证。

## 快速开始

预编译运行包见 [GitHub Releases](https://github.com/Lanyinn-Labs/Penlight-Notes-API/releases)。从源码运行需要 Rust 1.97：

```bash
cp .env.example .env
cp config/jp.example.json config/jp.local.json
```

按 [账号配置](docs/account-setup.md) 填写现有账号凭据，并配置 Master 存储路径。在 `.env` 中设置：

```dotenv
OURNOTES_JP_PROTOCOL_CONFIG=config/jp.local.json
API_KEY=your-api-key
```

```bash
cargo run --locked
```

默认监听 `http://127.0.0.1:8081`，API 请求使用 `X-API-Key` 或 Bearer 认证。

### Docker

```bash
docker compose up -d --build
```

配置与数据挂载见 [Docker 配置](docs/configuration.md#docker)。

## 文档

- [API 参考](docs/api.md)
- [配置参考](docs/configuration.md)
- [账号配置](docs/account-setup.md)
- [自动更新](docs/updates.md)
- [构建与发布](docs/releases.md)
- [维护工具](scripts/README.md)

## 许可证

[MIT](LICENSE) · [第三方许可证与署名](THIRD-PARTY-NOTICES.md)
