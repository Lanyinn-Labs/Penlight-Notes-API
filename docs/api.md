# API 参考

## 通用约定

游戏接口以 `/api` 为路径前缀，使用小写区服标识：

| 标识 | 区服 |
| --- | --- |
| `global` | 国际服 |
| `jp` | 日服 |

响应使用 JSON 格式。配置 `API_KEY` 后，`/api/*` 请求必须携带 `X-API-Key: <API_KEY>` 或 `Authorization: Bearer <API_KEY>`。认证在接口处理前执行。

## 服务接口

以下接口无需认证。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/health` | 查询服务运行状态 |
| GET | `/version` | 查询服务名称、版本与开发阶段 |
| GET | `/servers` | 查询区服配置与接入状态 |

### GET /health

返回 HTTP 200。`status` 表示服务运行状态，`upstream_ready` 表示游戏服务接入状态，两者独立。

```json
{
  "status": "ok",
  "service": "penlight-notes-api",
  "upstream_ready": false
}
```

### GET /version

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `name` | string | 服务名称 |
| `version` | string | 服务版本 |
| `stage` | string | 开发阶段；当前为 `scaffold` |

### GET /servers

返回 `servers` 数组，每项包含以下字段：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `region` | string | 区服标识 |
| `enabled` | boolean | 区服是否启用 |
| `client_version` | string 或 null | 配置的游戏客户端版本 |
| `upstream_configured` | boolean | 是否配置游戏服务地址 |
| `upstream_ready` | boolean | 游戏服务是否接入完成 |
| `status` | string | `disabled` 或 `protocol_pending` |

当前版本的 `upstream_ready` 固定为 `false`。`upstream_configured` 仅反映地址是否已填写，不表示地址有效或可用。服务不会返回配置中的完整地址或认证信息。

## 游戏接口

### GET /api/{region}/application

应用信息接口，当前尚未实现游戏协议。区服启用时返回 HTTP 501 `protocol_pending`；区服关闭时返回 HTTP 503 `region_disabled`。

默认配置下，国际服启用，日服关闭。启用日服后，该接口同样返回 HTTP 501。

卡牌、歌曲、活动、排行及用户接口暂未开放。

## 错误响应

```json
{
  "error": {
    "code": "protocol_pending",
    "message": "Our Notes upstream protocol has not been implemented"
  }
}
```

| HTTP 状态码 | 错误码 | 说明 |
| --- | --- | --- |
| 400 | `unsupported_region` | application 接口的区服标识无效 |
| 401 | `unauthorized` | API Key 缺失或无效 |
| 404 | `not_found` | 请求路径不存在 |
| 405 | `method_not_allowed` | 请求方法不受支持 |
| 501 | `protocol_pending` | 游戏协议尚未实现 |
| 503 | `region_disabled` | 区服未启用 |
