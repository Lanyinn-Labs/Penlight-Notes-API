# 数据与安卓版本更新

范围：自动更新 Master，检测日服安卓版本变化并准备适配；不接入图片、音乐等资源导出器。

## Master 自动更新

后台任务在服务启动后运行，默认示例每 300 秒检查一次。数据版本不变时复用已校验的快照；有新版本时下载、解密并校验后原子切换。失败保留原有快照。

复制 [自动更新配置示例](../config/jp.master-update.example.json) 为私有配置，填写现有账号路径，并设置 `OURNOTES_JP_PROTOCOL_CONFIG` 指向它。自动下载需要以下环境变量：

| 环境变量 | 来源与用途 |
| --- | --- |
| `SIRIUS_CDN_USERNAME` | 日服资源服务器的 HTTP Basic 用户名 |
| `SIRIUS_CDN_CREDENTIAL` | 对应 CDN 凭据，与当前游戏返回的 CDN 身份匹配 |
| `SIRIUS_MASTER_KEY_HEX` | 已验证客户端 Master 解密密钥，32 字节十六进制 |
| `SIRIUS_MASTER_IV_HEX` | 已验证客户端 Master IV，32 字节十六进制 |

CDN 认证用于从资源服务器下载 Master；现有账号的 player_id/credential 用于游戏 RPC；API_KEY 用于访问本服务。三者分别配置。客户端常量应从已核对的构建中取得，不能通过修改指纹绕过校验。

凭据未配置时，继续使用原来的配置和已导入快照；不要启用自动更新模板。启用后，缺失或无效的配置会使任务初始化失败。

首次部署建议在服务停止时执行一次更新，确认下载认证和解密格式可用：

```bash
cargo run --locked -- master-update
```

下载、版本或校验失败时该命令以非零状态退出，不替换有效快照。然后正常启动服务，每 300 秒自动检查。`master_update` 与 `master_sync` 互斥，多实例部署可使用一个下载者和多个同步者。

读取任务状态：

```http
GET /api/jp/master-updater
```

返回通用封装中的 data.status：disabled、pending、running、ready 或 failed。成功时含本次更新结果，失败时含上游固定错误码；不返回凭据。部署设置了 API Key 时需认证。自动更新状态与已有快照能否读取是不同状态。

Docker 中应以可写方式挂载 Master 存储，并给 UID 10001 写权限；环境变量传入容器，见 [配置参考](configuration.md#docker)。

## 安卓版本检测

[检测脚本](../scripts/check_android_version.py) 读取日本区 Google Play 官方页面，验证包名 `com.bushiroad.sirius`，与 `config/jp.example.json` 的已验证客户端版本比较。它不会下载 APK，也不会改变 client_version、协议或登录凭据。

```bash
python3 scripts/check_android_version.py
python3 scripts/check_android_version.py --write-candidate docs/android-update-candidate.md
```

脚本返回 current、update_available 或 store_older。商店页面结构不再匹配、没有明确版本或应用身份错误时检测失败，避免从页面任意数字猜版本。商店版本可能受到灰度发布影响，不保证所有设备都能立即取得相同构建。

## 自动准备适配

[Android version monitor](../.github/workflows/android-version.yml) 每六小时检查一次，也可手动运行。GitHub 定时工作流可能延迟，不能作为严格定时器。

发现新版本后，工作流将适配清单写入专用 automation/android-client-update 分支，创建或更新草稿 PR，并显式触发 CI。相同候选内容不重复提交。仅更新专用自动化分支，不直接修改 main 或部署配置。

仓库 Actions 设置需允许 GitHub Actions 创建 PR；组织策略也必须允许该操作。工作流使用 GITHUB_TOKEN，不需要游戏或 CDN 凭据。该令牌创建的 PR 不会自动触发普通 PR 工作流，因此通过 workflow_dispatch 显式启动 CI。

适配候选在专用草稿 PR 中生成，记录商店版本与已验证配置的差异。合并候选说明不等于完成适配；核对新构建、现有账号认证、Master 格式和协议后再更新配置，并发布本项目版本。纯版本检测不能自动恢复未知协议或加密格式。
