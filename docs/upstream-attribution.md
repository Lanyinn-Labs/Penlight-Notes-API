# 上游来源与贡献保留

Penlight-Notes-API 的在线游戏通信实现复用 [Srirus-Project/sirius-api-proxy](https://github.com/Srirus-Project/sirius-api-proxy)，按 MIT 许可使用。Sirius 项目自身源自 [Team-Haruki/Haruki-Sekai-API](https://github.com/Team-Haruki/Haruki-Sekai-API)。两者的原始版权声明均保留。

## 本地复用范围

上游源文件位于 `vendor/sirius-api-proxy/`，作为本地 Cargo 库依赖编译进同一个进程。官方 HTTP/2 gRPC、Protobuf 编解码、现有账号凭据、账号池与锁、上游响应缓存、节点查询路由、Master 解密和快照校验来自上游。Penlight 自己的资源整理、日文文本解析、账号数据拆分和榜线缓存位于根目录 `src/`。

- 上游版本：1.3.3。
- 来源提交：`cd19e2fe1d7f6c9a69304e6718e95fbc61817561`。
- `vendor/sirius-api-proxy/UPSTREAM.json` 记录来源及所复用文件的 SHA-256。
- `vendor/sirius-api-proxy/LICENSE` 保留 Haruki Dev Team 和 Sirius Project 的 MIT 版权与完整许可。
- `vendor/sirius-api-proxy/LICENSE-protobuf` 保留 Google Protobuf 的许可证。
- 上游原始源码未作修改；集成逻辑在 `src/client/sirius.rs`。

## 更新与分发

分发源码、二进制或容器时一并附带上述许可证和来源声明。Dockerfile 已将许可和归属文档复制到最终镜像的 `/usr/share/doc/penlight-notes-api/`。

本项目不代表或替代上游项目，未声明上游作者参与了 Penlight 的开发或提供支持。
