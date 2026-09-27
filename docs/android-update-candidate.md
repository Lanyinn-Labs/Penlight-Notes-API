# 日服安卓适配候选

- 当前配置版本：`1.0.2`
- Google Play 显示版本：`1.0.3`
- 包名：`com.bushiroad.sirius`
- 来源：[Google Play](https://play.google.com/store/apps/details?id=com.bushiroad.sirius&hl=ja&gl=JP)

本草稿已更新客户端配置中的版本，保留原有 CDN 认证和 key/IV；尚未验证新客户端。商店显示版本不保证所有设备已完成灰度更新。

## 验证清单

- [ ] 取得并核对 Android 新构建的版本和包名
- [ ] 验证现有账号的 Version、Whoami 和 GetPlayerData
- [ ] 比较客户端版本要求、认证和 Protobuf 变化
- [ ] 核对并更新 `data/jp-client.json` 的版本、协议路径、CDN 认证和 Master key/IV；不要放入个人凭据
- [ ] 比较本地存档格式；变化时重新适配工具
- [ ] 必要时更新 Sirius 来源提交、许可证记录和文件摘要
- [ ] 调整配置后验证 Master、公告、玩家资料和排行
- [ ] 通过 CI 及在线验证后合并；仅参数变化无需重新编译
- [ ] 部署端执行 `docker compose restart api` 拉取并应用新配置；协议/算法变化时先更新镜像
