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

## 安卓版本检测与适配

[检测脚本](../scripts/check_android_version.py) 读取日本区 Google Play 官方页面，验证包名 `com.bushiroad.sirius`，与 [客户端配置](../data/jp-client.json) 比较。配置中的版本、CDN 用户名/密码、Master key/IV 和协议路径由启动时读取；Docker 自动下载并缓存最新 JSON。

```bash
python3 scripts/check_android_version.py
# 只生成适配说明。
python3 scripts/check_android_version.py --write-candidate docs/android-update-candidate.md
# 在审查工作区准备构建版本变更与适配说明。
python3 scripts/check_android_version.py \
  --prepare-update --write-candidate docs/android-update-candidate.md
```

脚本返回 current、update_available 或 store_older。商店身份或页面元数据不再匹配时检测失败，不从页面任意数字猜版本；商店版本回退不会降低构建版本。默认只检测，不下载 APK或修改部署中的环境变量。

[Android version monitor](../.github/workflows/android-version.yml) 每六小时检查一次，也可手动运行。发现新版本后，在专用 automation/android-client-update 分支更新 JSON 中的客户端版本与适配清单，保留原有 CDN 认证和 key/IV 等字段，创建或刷新草稿 PR，并显式触发 CI。

PR 基于仓库实际默认分支的最新基线；内容相同不重复推送，PR 缺失时补建。商店版本与默认分支一致时关闭遗留候选 PR。GitHub 定时任务可能延迟；仓库 Actions 和组织策略须允许 GITHUB_TOKEN 创建 PR。该令牌创建的 PR 不会自动触发普通 PR 工作流，因此显式启动 CI。

维护者核对新客户端、现有账号认证、CDN 认证、Master 密钥/IV/格式和协议，在同一个 PR 更新 `data/jp-client.json` 与必要的协议代码后合并。Google Play 检测不会取得新 CDN 认证或 key/IV；草稿自动改版本不代表兼容性验证通过。

仅版本号、CDN 认证或 Master key/IV 变化时，适配 PR 合并后无需等待镜像重新编译。已经安装运行时配置入口的 Docker 镜像，重启时会下载 main 上的最新 JSON：

```bash
docker compose restart api
```

入口脚本校验后原子替换持久缓存，下载或校验失败保留已有配置；运行中的进程不热重载。独立二进制可替换本地客户端 JSON，再重启进程。应省略 `.env` 中的客户端常量覆盖值。

协议或算法变化时，仍需适配代码、等待 main 的新镜像发布，然后执行 `docker compose up -d --pull always`。旧程序无法仅靠 JSON 获得新协议能力。镜像标签和 GitHub Release 不会因为远程客户端 JSON 更新而被修改；同一镜像的客户端运行参数可更新。

第一次迁移到这套流程需要更新一次镜像，之后参数更新只需重启。PR 中仍需完成在线验证，重启存在短暂中断；这套机制缩短参数适配部署流程，不能保证官方服务永远不中断。
