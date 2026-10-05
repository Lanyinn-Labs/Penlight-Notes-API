# API 参考

## 通用约定

游戏接口以 `/api/{region}` 开头，区服为 `jp` 或 `global`。以下在线接口目前支持日服；未配置日服协议、或查询未接入的国际服时返回 `501 protocol_pending`。关闭区服返回 `503 region_disabled`。

设置 `PENLIGHT_API_KEY` 后，所有 `/api/*` 和 `/internal/v1/*` 请求必须提供 `X-API-Key: <key>` 或 `Authorization: Bearer <key>`。留空时这些查询免认证；`/user/*` 始终要求配置并通过 API Key 认证。响应为 JSON；原始 Protobuf JSON 中的 64 位整数可能使用字符串。

## 服务状态

| GET 路径 | 说明 |
| --- | --- |
| `/health` | 进程状态、上游最近通信及 Master 更新状态 |
| `/version` | 服务版本、upstream_version、运行协议 protocol_version、stage: online、协议来源及固定提交 |
| `/servers` | 各区服启用状态、版本、协议是否配置及可用状态 |

这些接口无需认证，不执行网络探测，也不占用接口限流额度。`/health` 的 `status: ok` 表示 HTTP 服务运行，`upstream` 包含最近实际通信的 `last_observed_at`、`age_ms`、`grpc_status` 和 `maintenance`。状态为 disabled（未启用）、unknown（尚无记录）、available、unavailable 或 stale（超过 `PENLIGHT_STATUS_TTL_SECONDS`，默认 300 秒）。记录来自底层客户端，包括普通查询和后台 Master 版本检查；缓存命中、本地 Master 读取不刷新时间。`upstream_ready` 保留为兼容字段，仅在 available 时为 true。它不代表所有账号、接口均可用。

`upstream.application_code` 展示上游返回的应用错误码，无错误时为 null。例如 `CLIENT_UPDATE_REQUIRED` 表示当前客户端版本已被官方拒绝，需要适配并更新客户端配置；单独的 `grpc_status: 2` 无法区分具体原因。

`master_update` 单独展示后台任务的 status、started_at、completed_at，不返回私有数据。区服 status 为 disabled、protocol_pending 或 protocol_configured；配置成功不等于官方服务可用。

## 请求保护

认证通过后，`/api/*` 使用全实例共享的令牌桶和并发限制。默认持续 30 请求/秒、突发 60 个、最多同时处理 16 个请求；超过速率或并发分别返回 `429 api_rate_limited`、`429 api_busy`，带 `Retry-After: 1`，不会排队。请求总超时默认 30 秒，返回 `504 api_timeout`。调用方应退避重试，限流配置见 [配置参考](configuration.md)。多容器实例各自计数。

## 在线查询

`GET /internal/v1/jp/resources/snapshot` 刷新并返回日服资源快照，供资源更新器读取资源版本、平台摘要及 CDN 目录。响应包含 `snapshot` 和 `stale: false`；请求受与 `/api/*` 相同的认证和限流保护。未配置日服在线协议时返回 `501 protocol_pending`。

| GET 路径（均以 `/api/jp` 为前缀） | 说明 |
| --- | --- |
| `/application` | 官方版本、维护与服务可用状态 |
| `/announcements?tab=0` | 公告列表；tab 允许 0、1、2，默认 0 |
| `/announcements/{id}` | 公告详情 |
| `/players/by-profile-id/{id}` | 用公开 profile ID 查询玩家资料 |
| `/events/{event_id}/rankings?ranks=100,1000` | 指定名次的原始活动排名 |
| `/events/{event_id}/players/{player_id}/deck` | 活动中的玩家队伍 |
| `/music/{id}/rankings` | 歌曲排名，移除调用账号的 myRank/myScore |
| `/challenge-music/{id}/rankings` | 挑战歌曲排名，移除调用账号的 myRank/myScore |

ID 必须为正整数。活动 rankings 必须指定 ranks；cutoffs 可省略 ranks 使用服务默认档位。显式 ranks 必须包含 1 至 20 个逗号分隔的正整数，排序并去重；空字符串、超出 int32 范围或未知查询参数返回 400。

在线响应封装如下，data 内部保留上游 Protobuf JSON 结构：

```json
{
  "region": "jp",
  "source": "official_game_service",
  "protocol_implementation": "sirius_api_proxy",
  "data": {}
}
```

### 活动档线（榜线）

| GET 路径 | 说明 |
| --- | --- |
| `/api/jp/events/current` | 经校验的 Master 中正在进行的活动及排名开关 |
| `/api/jp/events/current/cutoffs` | 当前活动的默认档位；可用 `?ranks=100,1000` 覆盖 |
| `/api/jp/events/{event_id}/cutoffs` | 指定活动的默认档位；可用 `?ranks=100,1000` 覆盖 |

默认档位为 `1,10,50,100,500,1000,2000,3000,5000,10000`，是服务查询预设，可通过 `PENLIGHT_RANKING_DEFAULT_RANKS` 修改，不代表官方奖励分档。原始活动 rankings 接口仍要求显式 ranks。

当前活动接口需要已配置的日服 Master 数据。根据 `_startAt`、`_endAt` 判定活动进行中（含边界），按日本时间 UTC+9 解析，输出 UTC ISO 8601 时间；没有进行中的活动返回 404，损坏或多个同时进行的活动返回 503 master_data_unavailable。响应包含 Master 版本/快照和 event：

```json
{
  "id": 1,
  "name_text_id": "Event_Name_0001",
  "starts_at": "2026-09-30T09:00:00Z",
  "ends_at": "2026-10-08T11:59:59Z",
  "display_ends_at": "2026-10-10T11:59:59Z",
  "ranking_enabled": false,
  "music_ranking_enabled": true,
  "total_music_ranking_enabled": true
}
```

当 Master 标记 `_isRankingDisabled: true` 时，当前活动和指定活动的 cutoffs、原始 rankings 返回 `409 event_ranking_disabled`，不请求官方，不返回伪造分数。配置 Master 后查询其中不存在的活动返回 404；未配置 Master 时仍允许按活动 ID 查询官方档线。其他歌曲/挑战排行接口不受总分排名开关影响。

可用的档线响应示例（仅用于说明结构，非实服分数）：

```json
{
  "region": "jp",
  "event_id": 42,
  "source": "official_game_service",
  "status": "fresh",
  "complete": false,
  "observed_at_unix_ms": 1791158400000,
  "age_ms": 0,
  "cutoffs": [{"rank": 100, "point": 12345}, {"rank": 1000, "point": null}]
}
```

`complete` 表示官方是否返回所有请求名次。未返回名次的 point 为 null；已返回的零分保留为 0。`status` 为 fresh 表示新鲜缓存，stale 表示刷新失败后返回的旧数据。失败和缓存命中均不改变观测时间；超过旧数据保留期返回实际错误。响应使用 `Cache-Control: private, no-store`，避免 HTTP 缓存冻结 age/status。

缓存时间由 `PENLIGHT_RANKING_*_SECONDS` 配置。相同活动和规范化档位共享缓存，并发请求合并，失败后按配置退避。缓存最多 1024 个键，正在使用的键不会被淘汰；容量全部被占用时返回 429 api_busy。


## 现有账号查询

所有路径均以 `/api/jp/user` 为前缀，查询部署者配置的现有账号。

| GET 路径 | 说明 |
| --- | --- |
| `/account` | Whoami 验证，仅返回 data.authenticated，不返回认证凭据 |
| `/data` | 完整 GetPlayerData 响应，缓存 15 秒 |
| `/profile` | myProfile |
| `/decks` | decks |
| `/cards` | memberCards |
| `/support-cards` | supportCards |
| `/items` | items |
| `/stamps` | stamps |
| `/characters` | characterRank |
| `/character-costumes` | characterCurrentCostumes，日服 1.0.4 当前服装 |
| `/unlocked-costumes` | characterUnlockedCostumes，日服 1.0.4 已解锁服装 |
| `/music-scores` | liveScore |
| `/music` | liveMusic |
| `/missions` | playerMissionData |
| `/login-bonuses` | loginBonusUpdate |
| `/gacha` | gachaCount |
| `/events` | events |
| `/tutorial` | tutorialProgress |

拆分接口将对应字段放入通用响应的 data；缺失的重复字段返回 []，缺失的 profile、missions、tutorial 对象返回 null。仅提供读取操作。

## Master 查询

| GET 路径 | 说明 |
| --- | --- |
| `/api/jp/master-updater` | 后台 Master 更新/同步状态，只读 |
| `/api/jp/master-data` | 经校验的已发布快照清单、Master 版本、表摘要 |
| `/api/{region}/master/{table}` | 读取表原始记录，例如 MasterCharacter |
| `/api/{region}/master-schema` | APK 提取的静态表结构索引 |
| `/api/{region}/master-schema/{table}` | APK 提取的字段信息 |

配置日服协议的 master_directory 后，表和资源读取原始 Sirius 快照存储，source 为 master_snapshot；响应包含 master_version、snapshot、resource_version 和 entries。列表及日文文本固定到同一快照，校验清单和表摘要。存储缺失或损坏返回 503 master_data_unavailable。

未配置该存储时，旧表查询和资源接口读取 `PENLIGHT_JP_SNAPSHOT_DIR` / `PENLIGHT_GLOBAL_SNAPSHOT_DIR` 指向的已解密 APK JSON，source 为 apk_master_snapshot，附 client_version、apk_sha256。国际服表查询使用此离线方式。

master-schema 始终是构建时提取的 APK 结构，不等于线上最新结构。国际服为 240 张表，日服为 235 张表；records_available: false 表示记录未内置进可执行文件。

### 日服资源

`GET /api/jp/{resource}` 返回 entries；`GET /api/jp/{resource}/{id}` 返回 entry。保留原始字段，添加 id，并在 MasterText 存在对应文本时添加 name_ja、subtitle_ja。

| resource | Master 表 |
| --- | --- |
| cards | MasterMemberCard |
| music | MasterLiveMusic |
| events | MasterEvent |
| characters | MasterCharacter |
| bands | MasterBand |
| gacha | MasterGacha |
| items | MasterItem |
| stamps | MasterStamp |
| shops | MasterShop |
| login-bonuses | MasterLoginBonus |

资源详情 ID 无效返回 400，未找到返回 404。国际服这些整理后的资源接口尚未接入。

## 错误

统一格式为 `{"error":{"code":"...","message":"..."}}`，不会返回原始凭据或私有上游诊断。

| HTTP | code | 说明 |
| --- | --- | --- |
| 400 | unsupported_region | 区服无效 |
| 400 | invalid_query / invalid_ranking_query / invalid_master_id | 参数无效 |
| 401 | unauthorized | 密钥缺失、错误或私有接口未配置密钥 |
| 404 | not_found | 路径或资源不存在 |
| 405 | method_not_allowed | 方法不支持 |
| 409 | event_ranking_disabled | 活动总分排名关闭，未请求官方 |
| 501 | protocol_pending | 区服在线协议未配置或未接入 |
| 503 | region_disabled | 区服关闭 |
| 503 | master_data_unavailable | 快照缺失、损坏或来源不符 |
| 503 | upstream_authentication_unavailable | 游戏账号会话不可用 |
| 503 | upstream_rate_limited | 官方限流 |
| 503 | upstream_maintenance | 官方维护，不惩罚账号、不重试该请求 |
| 503 | upstream_unavailable | 官方服务不可用 |
| 502 | upstream_invalid_response | 返回数据不符合预期 |
| 502 | upstream_game_error | 官方 gRPC 业务错误，message 包含状态码 |
| 504 | upstream_timeout | 官方请求超时 |

## 玩家资料补充字段

`/api/jp/players/by-profile-id/{id}` 保留 `data.playerProfile`，并新增顶层 `summary`：

- `player_id`、`profile_id`、`name`、`rank_exp`：公开身份和等级经验。
- `player_level`：根据 MasterPlayerRank 的累计经验阈值换算。
- `last_updated_at`：官方秒级时间戳转换为 UTC ISO 8601 时间。
- `favorite_member_card.master_id`、`name`、`subtitle`：喜爱卡片 ID、日文成员名称和卡片副标题。
- `master_version`、`master_status`：补充字段所依据的快照版本及 ready、partial 或 unavailable。

同一请求固定一个经校验的 Master 快照。缺失的映射返回 null；Master 不可用时仍返回成功取得的公开资料。此处的 master_status 指补充表能否读取，不代表后台更新任务状态。
