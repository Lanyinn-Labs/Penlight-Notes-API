# 配置参考

## 加载顺序

进程环境变量 > `.env.local` > `.env` > 默认值。相对路径以启动工作目录为基准；`.env.example` 不会自动加载。

| 配置项 | 默认值 | 说明 |
| --- | --- | --- |
| `HOST` | `127.0.0.1` | 唯一 HTTP 监听地址 |
| `PORT` | `8081` | 唯一 HTTP 监听端口 |
| `RUST_LOG` | `info` | 日志过滤 |
| `API_KEY` | 空 | `/api/*` 访问密钥；为空时公共查询免认证，但 `/user/*` 不可访问 |
| `RANKING_CACHE_TTL_SECS` | `30` | 榜线新鲜缓存时间 |
| `RANKING_STALE_TTL_SECS` | `300` | 刷新失败时旧数据最长保留时间，须不小于新鲜缓存时间 |
| `RANKING_RETRY_DELAY_SECS` | `5` | 失败后同一查询的重试间隔 |
| `OURNOTES_JP_PROTOCOL_CONFIG` | 空 | 原始 Sirius JSON/YAML 配置路径，启用进程内日服通信 |

区服配置前缀为 `OURNOTES_GLOBAL`、`OURNOTES_JP`：

| 后缀 | 国际服默认值 | 日服默认值 | 说明 |
| --- | --- | --- | --- |
| `_ENABLED` | `true` | `true` | 仅接受 true/false |
| `_CLIENT_VERSION` | 空 | `1.0.2` | 未配置协议时展示的客户端版本 |
| `_BASE_URL` | 空 | 空 | 历史地址配置，本身不能启用在线请求 |
| `_MASTER_DIR` | 空 | `artifacts/jp/master-decrypted` | 原有已解密 APK JSON 快照目录 |

配置日服协议后，客户端版本和官方地址以协议配置为准。国际服在线协议尚未启用，不支持 `_GLOBAL_PROTOCOL_CONFIG`。

## 日服协议配置

模板：[config/jp.example.json](../config/jp.example.json)。读取后使用上游原始配置校验和客户端；错误配置会导致启动失败。

| 字段 | 作用 |
| --- | --- |
| `region` | 必须为 `jp` |
| `endpoint`、`platform`、`environment`、`client_version` | 官方服务地址及客户端身份 |
| `protocol_directory` | 随附 JP Protobuf 目录；须能从运行目录读取 |
| `accounts` | 现有账号池，每项指定 name 和 credentials_file |
| `session_lock` | 复用原始账号锁，避免同一账号的并行会话冲突 |
| `master_directory` | 原始 Sirius Master 快照存储；与旧的 `_MASTER_DIR` 格式不同 |
| `master_update`、`master_sync` | 可选更新/同步任务，随服务启动及关闭 |
| `default_cdn_root`、`cdn_credential_env` | CDN 来源及凭据环境变量名，自动 CDN 更新时需实际设置凭据 |
| `api_token_env`、`internal_token_env` | 原始配置结构要求的字段，本集成不启动原始 HTTP 服务，因此无需设置这些令牌 |

可使用原始账号池、请求缓存、向外查询节点路由及 Master 更新/同步能力。配置细节见 [随附原始文档](../vendor/sirius-api-proxy/docs/)。本集成不接受独立服务的数据库、Git 发布、通知、资源分发、TLS、独立访问日志、客户端认证或 peer token 配置；`listen` 不参与监听，实际使用本项目的 HOST/PORT。

账号接口只读查询现有账号；凭据文件不通过接口返回。账号数据缓存 15 秒，修改凭据配置后重启服务。不要将 credentials_file 设为公共文件或将凭据写入提交。

配置了 `master_directory` 后，资源接口只读取经过校验的该存储；缺失或损坏返回错误。没有配置此字段时才使用旧 APK 快照。导入步骤见 [Master 导入](#master-导入)。自动更新不是默认开启，本次验证使用从手机复制并导入的 Master。

## Master 导入

从自有 Android 客户端复制 `files/Master`，使用已验证的日服 1.0.2 元数据导入：

```bash
cargo build --locked
uv run --no-project --with py3rijndael python scripts/import_jp_master.py \
  artifacts/jp/phone-master artifacts/jp/master-store
```

默认元数据路径为 `artifacts/jp/global-metadata.dat`，可通过 `--metadata` 指定。将协议配置的 `master_directory` 设为导入后的存储目录。脚本读取客户端常量并调用本程序的 Master 导入器，不打印密钥；接口校验已发布快照的版本和摘要。

这是本地导入步骤，自动更新需另行配置 Master 更新或同步任务。国际服旧 APK 快照可用 `scripts/decrypt_master.py /path/to/client.apk` 导出已验证的 1.0.1 APK，再通过 `OURNOTES_GLOBAL_MASTER_DIR` 指向输出目录。工具参数见 [维护工具](../scripts/README.md)。

## Docker

容器内协议目录已随镜像提供；使用同样的相对路径。可创建本地 Compose 覆盖文件，例如：

```yaml
services:
  api:
    environment:
      OURNOTES_JP_PROTOCOL_CONFIG: /app/config/jp.local.json
    volumes:
      - ./config/jp.local.json:/app/config/jp.local.json:ro
      - ./secrets/jp-account.json:/run/secrets/jp-account.json:ro
      - ./artifacts/jp/master-store:/app/artifacts/jp/master-store:ro
```

运行 `docker compose -f docker-compose.yml -f /path/to/local-override.yml up -d --build`。设置配置里的凭据路径为 `/run/secrets/jp-account.json`。宿主机需让 UID 10001 能读取挂载文件及目录，同时限制其他用户访问。启用 Master 自动写入任务时，Master 挂载应可写且 UID 10001 有写权限。所需 CDN/同步令牌须另外传入容器环境。

Compose 容器监听 0.0.0.0:8081，宿主机仅映射 127.0.0.1，宿主机端口由 `.env` 的 PORT 决定。容器不会读取未挂载的 `.env.local`。

## 本地文件

`.env`、`.env.local`、`config/*.local.json`、`config/*.local.yaml`、安装包、`artifacts/` 和 `cache/` 均已忽略。凭据应存放于这些私有目录并使用 0600 权限。
