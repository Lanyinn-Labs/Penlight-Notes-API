# 日服现有账号配置

本服务读取已有账号的凭据；不提供创建、迁移或删除账号的接口。

## 已有凭据

凭据文件使用以下 JSON 格式：

```json
{"player_id":"YOUR_PLAYER_ID","credential":"YOUR_CREDENTIAL"}
```

将文件放入 Git 忽略的私有目录（如 `secrets/`），设置 `0600` 权限，并在日服配置的 `accounts[].credentials_file` 指定其路径。模板见 [jp.example.json](../config/jp.example.json)。容器部署时需确保运行用户能读取挂载文件。

设置 `OURNOTES_JP_PROTOCOL_CONFIG` 和 `API_KEY` 后启动服务，查询 `/api/jp/user/account` 可验证账号是否可用；请求必须携带 API Key。账号认证凭据与本服务的 API Key 是不同的两项配置。

## 从自己的 Android 本地存档恢复

随附 `scripts/inspect_jp_local_save.py` 支持已验证的日服 Android 1.0.2 存档格式。需要自行取得客户端元数据和本地存档副本；无需修改存档。普通设备未必允许读取这些文件。

将元数据放入 `artifacts/jp/global-metadata.dat`，将存档副本放入单独目录，然后执行：

```bash
uv run --no-project --with py3rijndael python scripts/inspect_jp_local_save.py \
  /path/to/copied-saves --metadata artifacts/jp/global-metadata.dat
```

输出只允许写入 Git 忽略的 `artifacts/`，默认目录为 `artifacts/jp/local-save-decrypted-private/`。成功时会生成权限为 0600 的解密 JSON；含认证对象的存档还会生成 `*.credentials-0.json` 等凭据文件。脚本不打印凭据值。

这是针对特定客户端构建的本地恢复工具；客户端格式或元数据指纹变化时，应先重新验证。备份及输出可能包含完整账号数据，不应提交或上传。

## 运行边界

- 服务启动后通过 Sirius 原始客户端读取凭据并请求官方服务，运行时不需要手机连接或 MITM。
- 修改账号配置或凭据文件后重启服务。
- `/user/*` 返回部署者的账号信息；必须设置 API Key。
- 当前已验证应用状态、账号数据、公告、公开资料及歌曲排名。没有活动时，缺失的榜线分数为 null。

配置及接口说明见 [配置参考](configuration.md)、[API 文档](api.md)。
