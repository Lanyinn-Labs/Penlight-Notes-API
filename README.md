# Penlight-Notes-API

BanG Dream! Our Notes 非官方第三方 API 服务，基于 Rust 和 Axum 构建。

项目目前处于初期开发阶段，已实现服务基础设施与区服配置。游戏协议及数据接口尚在开发中。各接口的实现状态见 [API 文档](docs/api.md)。

## 快速开始

### 环境要求

- Rust 1.97，版本由 `rust-toolchain.toml` 指定
- Python 3.9 或更高版本，仅 APK 检查工具需要

在项目根目录执行：

```bash
cp .env.example .env
cargo run --locked
```

服务默认监听 `http://127.0.0.1:8081`。

```bash
curl http://127.0.0.1:8081/health
curl http://127.0.0.1:8081/servers
```

生产构建：

```bash
cargo build --release --locked
./target/release/penlight-notes-api
```

## 配置

配置通过环境变量加载，优先级依次为：进程环境变量、`.env.local`、`.env`。配置项说明见 [配置参考](docs/configuration.md)。

设置 `API_KEY` 后，访问 `/api/*` 需提供以下任一请求头：

```http
X-API-Key: <API_KEY>
Authorization: Bearer <API_KEY>
```

`/health`、`/version` 与 `/servers` 无需认证。

## Docker 部署

在项目根目录创建 `.env` 后执行：

```bash
docker compose up -d --build
```

Compose 默认映射至 `127.0.0.1:8081`。宿主机端口由 `.env` 中的 `PORT` 指定；容器内部端口固定为 `8081`，进程以非 root 用户运行。

## 项目结构

```text
src/
  api/          HTTP 路由、认证与请求处理
  client/       游戏服务客户端
  config.rs     服务与区服配置
  error.rs      错误类型与响应格式
  region.rs     区服定义
  lib.rs        库入口
  main.rs       服务入口
scripts/        APK 分析工具
tests/          接口集成测试
docs/           接口文档、配置参考与开发计划
```

## 开发

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

开发阶段与后续工作见 [开发计划](docs/roadmap.md)。

## 许可证

本项目采用 [MIT License](LICENSE)。

Copyright (c) 2026 Lanyinn-Labs.

本项目为非官方项目，与 BanG Dream! Our Notes 的开发商及运营方无关联。
