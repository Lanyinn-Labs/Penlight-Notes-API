# 配置参考

部署配置来自环境变量和启动目录下的 `.env`；客户端版本、协议路径、CDN 认证及 Master key/IV 从运行时 [客户端 JSON](../data/jp-client.json) 读取；Docker 自动下载并持久缓存。优先级为 **进程环境变量 > `.env` > 运行时客户端 JSON > 内置兜底值**。相对路径以启动工作目录为基准；`.env.example` 和 `.env.local` 不会自动加载。无 `.env` 可使用默认值离线启动；存在但格式错误或无法读取的 `.env` 会导致启动失败。

## 常规部署

复制 `.env.example` 为 `.env`，填写两项即可启用现有账号的日服接口：

```dotenv
PENLIGHT_API_KEY=your-api-key
PENLIGHT_JP_ACCOUNTS=secrets/jp-account.json
```

账号凭据文件仍是独立的私有数据，格式见 [账号配置](account-setup.md)。多个账号路径用逗号分隔，路径本身不能包含逗号。账号名默认取文件名去掉扩展名，也可使用 `existing=secrets/jp-account.json` 显式指定。账号名必须唯一，长度 1–64，只接受字母、数字、下划线和连字符。

```bash
cargo run --locked -- check-config
cargo run --locked
```

`check-config` 校验配置、协议及账号池初始化，不发起游戏请求，不输出密钥。修改环境变量或账号凭据后重启服务。

## 服务与请求策略

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `PENLIGHT_LISTEN` | `127.0.0.1:8081` | 完整监听地址；IPv6 使用 `[::1]:8081` |
| `PENLIGHT_API_KEY` | 空 | API 访问密钥；为空时公共查询免认证，`/user/*` 不可访问 |
| `RUST_LOG` | `info` | 日志过滤 |
| `PENLIGHT_MAX_CONCURRENT` | `16` | `/api/*` 并发上限，范围 1–65536 |
| `PENLIGHT_REQUESTS_PER_SECOND` | `30` | 全实例令牌桶每秒补充速率 |
| `PENLIGHT_REQUEST_BURST` | `60` | 令牌桶容量 |
| `PENLIGHT_REQUEST_TIMEOUT_SECONDS` | `30` | API 请求总超时 |
| `PENLIGHT_STATUS_TTL_SECONDS` | `300` | 最近上游通信状态有效期 |
| `PENLIGHT_RANKING_FRESH_SECONDS` | `30` | 榜线新鲜缓存时间 |
| `PENLIGHT_RANKING_STALE_SECONDS` | `300` | 刷新失败时旧榜线最长保留时间 |
| `PENLIGHT_RANKING_RETRY_SECONDS` | `5` | 榜线刷新失败后的重试间隔 |

请求额度、超时、状态有效期和榜线缓存时间须为正数，stale 不得小于 fresh。限流在认证后生效，健康及服务状态查询不占额度，多实例独立计数。Master 下载使用独立网络超时。

## 区服和日服协议

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `PENLIGHT_GLOBAL_ENABLED` | `true` | 国际服离线接口是否启用 |
| `PENLIGHT_GLOBAL_SNAPSHOT_DIR` | 空 | 已验证国际服 APK Master JSON 目录 |
| `PENLIGHT_JP_ENABLED` | `true` | 日服接口是否启用 |
| `PENLIGHT_JP_ACCOUNTS` | 空 | 一个或多个现有账号凭据文件 |
| `PENLIGHT_JP_ONLINE` | 按需自动开启 | 有账号路径或 Master 模式为 download/sync 时开启，可显式覆盖 |
| `PENLIGHT_JP_CLIENT_VERSION` | 客户端 JSON | 在线请求客户端版本；通常不需要覆盖 |
| `PENLIGHT_JP_ENDPOINT` | `https://api.bang-dream-on.jp` | HTTPS 游戏服务 origin |
| `PENLIGHT_JP_PROTOCOL_DIR` | `vendor/sirius-api-proxy/protocol/sirius/1.0.3` | 随附 Protobuf 目录 |
| `PENLIGHT_JP_SESSION_LOCK` | `true` | 序列化同一账号的会话 |
| `PENLIGHT_JP_TIMEOUT_MS` | `20000` | 上游请求超时 |
| `PENLIGHT_JP_MAX_INFLIGHT` | `64` | 上游并发上限 |
| `PENLIGHT_JP_ACCOUNT_FAILURE_THRESHOLD` | `2` | 账号连续失败阈值 |
| `PENLIGHT_JP_ACCOUNT_COOLDOWN_SECONDS` | `30` | 失败账号冷却时间 |
| `PENLIGHT_JP_PROXY_URL` | 空 | 游戏及 Master 下载代理地址 |
| `PENLIGHT_JP_PROXY_AUTH` | 空 | 代理认证头，需要同时设置代理地址 |
| `PENLIGHT_JP_CACHE_TTL_SECONDS` | `0` | 大于零时开启内存响应缓存，默认上限 1024 条、32 MiB |
| `PENLIGHT_JP_PEER_URL` | 空 | 可选 HTTPS 同步查询节点，本地优先 |
| `PENLIGHT_JP_PEER_TOKEN` | 空 | 配置查询节点时必填 |
| `PENLIGHT_JP_SNAPSHOT_DIR` | 空 | 已验证日服 APK Master JSON 目录 |
| `PENLIGHT_JP_MASTER_DIR` | `artifacts/jp/master-store` | 在线协议使用的 Sirius Master 存储目录 |

没有账号且未启用在线模式时，服务保持离线。显式 `PENLIGHT_JP_ONLINE=true` 可启用无需账号的官方查询，需要账号的接口仍要求有效凭据。没有在线协议时 `/servers` 的 client_version 为 null。

APK 快照和 Sirius Master 存储是两种不同格式，只能配置一个日服 Master 来源。设置 `PENLIGHT_JP_SNAPSHOT_DIR` 后，不再使用默认 Master 存储；不能同时显式设置 `PENLIGHT_JP_MASTER_DIR`，也不能启用下载或同步模式。

## Master 模式

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `PENLIGHT_JP_MASTER_MODE` | `local` | local 读取现有存储，download 自动下载，sync 从其他实例同步 |
| `PENLIGHT_JP_MASTER_INTERVAL_SECONDS` | `300` | 下载/同步间隔，范围 60–86400 秒 |
| `PENLIGHT_JP_MASTER_TIMEOUT_SECONDS` | `600` | 单轮下载/同步总超时 |
| `PENLIGHT_JP_MASTER_CONNECT_TIMEOUT_MS` | `10000` | 下载连接超时 |
| `PENLIGHT_JP_MASTER_REQUEST_TIMEOUT_MS` | `60000` | 单次下载请求超时 |
| `PENLIGHT_JP_MASTER_ATTEMPTS` | `1` | 下载总尝试次数，范围 1–8 |
| `PENLIGHT_JP_MASTER_RETRY_DELAY_MS` | `250` | 下载重试起始延迟 |
| `PENLIGHT_JP_MASTER_MAX_RETRY_DELAY_MS` | `5000` | 下载重试最大延迟 |
| `PENLIGHT_CDN_USERNAME` | 客户端 JSON | 适配测试可覆盖 CDN Basic 用户名；正式部署省略 |
| `PENLIGHT_CDN_PASSWORD` | 客户端 JSON | 适配测试可覆盖 CDN 密码；正式部署省略 |
| `PENLIGHT_MASTER_KEY_HEX` | 客户端 JSON | 适配测试可覆盖 Master 密钥；正式部署省略 |
| `PENLIGHT_MASTER_IV_HEX` | 客户端 JSON | 适配测试可覆盖 Master IV；正式部署省略 |
| `PENLIGHT_JP_MASTER_SYNC_ORIGIN` | 空 | sync 模式 HTTPS 发布者地址 |
| `PENLIGHT_JP_MASTER_SYNC_TOKEN` | 空 | sync 模式发布者访问令牌 |

模式是单个枚举，不可能同时启用下载和同步。启用下载只需设置模式；CDN 用户名、密码和 Master key/IV 均由客户端 JSON 提供，显式覆盖值仍会校验。参数格式错误会启动失败，错误信息不回显值。CDN 拒绝请求、下载或校验失败时保留现有快照。关闭日服不会启动其后台任务。详见 [自动更新](updates.md)。

## Master 导入

从自有 Android 客户端复制 `files/Master`，使用已验证的日服 1.0.2 元数据导入：

```bash
cargo build --locked
uv run --no-project --with py3rijndael python scripts/import_jp_master.py \
  artifacts/jp/phone-master artifacts/jp/master-store
```

默认元数据为 `artifacts/jp/global-metadata.dat`，可用 `--metadata` 指定。脚本通过子进程环境传入客户端常量，不打印密钥。

国际服旧 APK 快照可用 `scripts/decrypt_master.py /path/to/client.apk` 导出已验证的 1.0.1 APK，再设置 `PENLIGHT_GLOBAL_SNAPSHOT_DIR`。日服 APK 快照使用 `scripts/decrypt_master_split_apk.py` 导出，再设置 `PENLIGHT_JP_SNAPSHOT_DIR`。工具说明见 [维护工具](../scripts/README.md)。

## Docker

Compose 使用预编译的 `ghcr.io/lanyinn-labs/penlight-notes-api:main` 镜像，自动读取 `.env` 并挂载 `secrets/` 和 Master 存储。容器监听 `0.0.0.0:8081`，宿主机映射 `127.0.0.1:8081`；调整宿主机端口请修改 Compose 的 ports。

```bash
cp .env.example .env
# 填写 API Key、账号路径及所需的 Master 模式。
docker compose up -d --pull always
```

首次安装本版本的镜像后，客户端参数 PR 合并到 main 即可使用；无需等待新镜像构建。在部署服务器执行：

```bash
docker compose restart api
```

入口脚本从 main 下载最新 JSON（连接超时 3 秒、总超时 10 秒、最大 64 KiB），校验字段、HTTPS origin 和当前镜像中的协议文件，成功后原子替换 `/app/runtime/jp-client.json`。该目录存储在命名卷 `client-config`，镜像更新后仍保留；下载或校验失败使用已有配置，首次离线启动使用镜像附带配置；更换镜像后缓存已不兼容时，先回退到新镜像的有效配置。不会在运行中热重载。

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `PENLIGHT_CLIENT_CONFIG` | 二进制：`data/jp-client.json`；Docker：`/app/runtime/jp-client.json` | 启动时读取的客户端配置路径 |
| `PENLIGHT_CLIENT_CONFIG_URL` | Docker 使用本仓库 main 上的 raw JSON 地址 | 入口脚本启动时拉取；空值关闭拉取。独立二进制只读取本地文件 |

独立二进制可替换 `data/jp-client.json`，用 `./penlight-notes-api check-client-config data/jp-client.json` 校验，再重启进程。默认文件不存在时使用构建内置的兜底参数；显式指定的文件不存在或非法时启动失败。

仅客户端参数变化无需编译。若新客户端要求新的协议文件或算法，先适配并更新镜像：`docker compose up -d --pull always`。新 JSON 不会让旧程序获得缺失的协议实现，参数校验也不能代替在线兼容性验证。重启存在短暂中断。

确保 `.env` 中没有残留的 `PENLIGHT_JP_CLIENT_VERSION`、`PENLIGHT_JP_PROTOCOL_DIR`、`PENLIGHT_MASTER_KEY_HEX`、`PENLIGHT_MASTER_IV_HEX`、`PENLIGHT_CDN_USERNAME` 或 `PENLIGHT_CDN_PASSWORD` 覆盖值。
`PENLIGHT_IMAGE` 仅供 Compose 选择镜像，默认为 `ghcr.io/lanyinn-labs/penlight-notes-api:main`。需要回滚时将它设置为上一构建的 `sha-<commit>` 标签，再执行同一命令。固定镜像标签固定程序代码，但默认仍跟随 main 的客户端 JSON。需要固定运行参数时将 `PENLIGHT_CLIENT_CONFIG_URL` 设为空并固定本地缓存。

从源码开发时可用 `docker build -t penlight-notes-api:local .`，然后执行 `PENLIGHT_IMAGE=penlight-notes-api:local docker compose up -d --pull never`。

将 `PENLIGHT_JP_ACCOUNTS` 设为 `secrets/jp-account.json`。宿主机需允许 UID 10001 读取账号文件；自动更新时还需允许其写入 Master 存储目录。使用其他数据目录时同步调整环境变量和卷挂载。Docker 随附协议文件及初始客户端 JSON，并通过命名卷缓存最新客户端配置。

`.env`、账号文件、安装包、`artifacts/` 和 `cache/` 均已忽略。私有文件使用 0600 权限。

## 破坏性更新

已移除 `HOST`、`PORT`、`API_KEY`、`OURNOTES_*`、旧的请求/缓存环境变量、`MASTER_AUTO_UPDATE`、`SIRIUS_*` 默认凭据引用及原始 Sirius JSON/YAML 配置入口；没有别名或回退机制。`.env.local` 不再加载。将现有部署改为本文的 `PENLIGHT_*` 环境变量和单份 `.env`。
