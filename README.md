# Penlight-Notes-API

基于 Rust 和 Axum 的 BanG Dream! Our Notes 非官方 API 服务。

## 功能

- 日服应用状态、公告及公开玩家资料查询
- 现有账号资料、编队、卡牌、道具及歌曲成绩查询
- 歌曲排行、活动排行、当前活动与默认/自定义档位的活动档线缓存
- Master 快照查询、资源列表与详情、日文名称解析

日服在线接口已接入；国际服目前提供离线 Master 查询。当前活动及档线接口识别官方排名开关；2026-10-05 活动 ID 1 总分排名关闭，歌曲排名已实服验证。

## 快速开始

预编译运行包见 [GitHub Releases](https://github.com/Lanyinn-Labs/Penlight-Notes-API/releases)。从源码运行需要 Rust 1.97：

```bash
cp .env.example .env
```

按 [账号配置](docs/account-setup.md) 保存现有账号凭据，在唯一配置文件 `.env` 中填写：

```dotenv
PENLIGHT_API_KEY=your-api-key
PENLIGHT_JP_ACCOUNTS=secrets/jp-account.json
```

```bash
cargo run --locked -- check-config
cargo run --locked
```

默认监听 `http://127.0.0.1:8081`，API 请求使用 `X-API-Key` 或 Bearer 认证。请求保护和缓存使用内置默认值，无需逐项填写。

需要自动更新 Master 时，在同一个 `.env` 设置 `PENLIGHT_JP_MASTER_MODE=download` 即可；CDN 认证和解密参数来自运行时客户端 JSON，见 [自动更新](docs/updates.md)。

### Docker

```bash
docker compose up -d --pull always
```

配置与数据挂载见 [Docker 配置](docs/configuration.md#docker)。

## 文档

- [开发与提交规范](CONTRIBUTING.md)
- [API 参考](docs/api.md)
- [配置参考](docs/configuration.md)
- [账号配置](docs/account-setup.md)
- [自动更新](docs/updates.md)
- [构建与发布](docs/releases.md)
- [维护工具](scripts/README.md)

## 许可证

[MIT](LICENSE) · [第三方许可证与署名](THIRD-PARTY-NOTICES.md)
