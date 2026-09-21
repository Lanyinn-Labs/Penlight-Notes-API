# 配置参考

## 加载顺序

配置优先级从高到低为：

1. 进程环境变量
2. 项目根目录的 `.env.local`
3. 项目根目录的 `.env`
4. 程序默认值

`.env.example` 为配置模板，不参与自动加载。相对路径以服务启动时的工作目录为基准。

## 服务配置

| 配置项 | 默认值 | 说明 |
| --- | --- | --- |
| `HOST` | `127.0.0.1` | 监听 IP 地址，支持 IPv4 与 IPv6 |
| `PORT` | `8081` | 监听端口 |
| `RUST_LOG` | `info` | 日志过滤表达式 |
| `API_KEY` | 空 | `/api/*` 访问密钥；为空时不启用认证 |

`HOST` 或 `PORT` 格式无效时，服务启动失败。

Docker Compose 将容器内的 `HOST` 与 `PORT` 分别设为 `0.0.0.0` 和 `8081`；`.env` 中的 `PORT` 用于配置宿主机映射端口。

## 区服配置

配置前缀分别为 `OURNOTES_GLOBAL` 与 `OURNOTES_JP`。

| 后缀 | 国际服默认值 | 日服默认值 | 说明 |
| --- | --- | --- | --- |
| `_ENABLED` | `true` | `false` | 是否启用区服；仅接受 `true` 或 `false` |
| `_CLIENT_VERSION` | 空 | 空 | 游戏客户端版本 |
| `_BASE_URL` | 空 | 空 | 游戏服务地址，预留配置项 |

例如，国际服版本配置项为 `OURNOTES_GLOBAL_CLIENT_VERSION`。

模板中的国际服版本为 `1.0.1`，取自已记录安装包的文件名，尚未通过 AndroidManifest 验证。程序本身不提供默认客户端版本，也不执行版本自动检测。

当前版本尚未实现上游请求。`_ENABLED` 控制接口可用状态，`_BASE_URL` 仅用于记录地址配置状态。

## 本地文件

`.env`、`.env.local`、安装包、`artifacts/` 与 `cache/` 已列入 `.gitignore`。安装包检查报告可按区服保存在 `artifacts/global/` 或 `artifacts/jp/`。
