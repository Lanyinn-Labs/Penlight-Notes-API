# 数据与安卓版本更新

## Master 自动更新

部署参数写入 `.env`；客户端版本、CDN 认证和 Master key/IV 在运行时 [客户端配置](../data/jp-client.json) 中维护：

```dotenv
PENLIGHT_JP_ACCOUNTS=secrets/jp-account.json
PENLIGHT_JP_MASTER_MODE=download
```

客户端 JSON 中的 CDN 身份用于下载 Master；账号文件中的 player_id/credential 用于游戏 RPC；`PENLIGHT_API_KEY` 用于访问本服务。三类凭据各有用途，不能互相替代。客户端常量应从已经核对的构建中取得。

服务启动后默认每 300 秒检查数据版本，下载、解密并校验后原子切换快照。版本不变时复用已校验快照；失败保留已有数据。设置 `PENLIGHT_JP_MASTER_INTERVAL_SECONDS` 可调整检查间隔，`PENLIGHT_JP_MASTER_DIR` 可调整存储目录。

启用下载时显式覆盖的 CDN 参数、密钥格式错误或策略无效会启动失败；CDN 认证被拒绝和网络错误记录为更新失败。常规离线部署保留 `PENLIGHT_JP_MASTER_MODE=local`，读取已经导入的 Master，无需额外填写下载凭据。

首次部署可先校验配置并单次执行更新：

```bash
cargo run --locked -- check-config
cargo run --locked -- master-update
cargo run --locked
```

单次更新失败以非零状态退出，不替换有效快照。后台任务状态由 `GET /api/jp/master-updater` 读取，返回 data.status：disabled、pending、running、ready 或 failed，不返回凭据。配置 API Key 时请求须认证。

多实例部署可用一个下载者和多个同步者，同步者的 `.env` 设置：

```dotenv
PENLIGHT_JP_MASTER_MODE=sync
PENLIGHT_JP_MASTER_SYNC_ORIGIN=https://your-master-owner.example
PENLIGHT_JP_MASTER_SYNC_TOKEN=your-owner-token
```

sync 使用发布者的日服路径，要求 HTTPS，不需要 CDN 或解密密钥。模式只能选择 local、download 或 sync 中的一项。Master 存储在 Docker 中需要可写挂载及 UID 10001 的写权限，见 [配置参考](configuration.md)。

## 客户端参数与程序更新

客户端版本、CDN 认证、Master key/IV 和协议路径由程序启动时读取。Docker 启动或重启时下载 main 上的最新客户端 JSON，校验后原子替换持久缓存；下载或校验失败保留已有配置，运行中的进程不热重载。

仅客户端参数变化时，重启容器即可应用：

```bash
docker compose restart api
```

协议或算法变化时，需要更新程序和协议：

```bash
docker compose up -d --pull always
```

独立二进制可替换本地 `data/jp-client.json` 后重启；协议或程序变化时更新下载包。显式环境变量覆盖会优先于 JSON，需要一并调整或移除。

固定镜像 tag 固定程序版本，默认仍可下载最新客户端 JSON。需要固定运行参数时，将 `PENLIGHT_CLIENT_CONFIG_URL` 留空，并保留指定的本地配置。重启存在短暂中断。
