# 上游来源与二次开发归属

Penlight-Notes-API 的在线游戏通信实现基于 [Srirus-Project/sirius-api-proxy](https://github.com/Srirus-Project/sirius-api-proxy)（MIT License）进行二次开发。Sirius 项目自身源自 [Team-Haruki/Haruki-Sekai-API](https://github.com/Team-Haruki/Haruki-Sekai-API)。两者的原始版权声明与开源许可均完整保留。

## 架构与二次开发范围

协议实现位于 `crates/sirius-api-proxy/`，作为项目内的核心库编译进同一个进程。官方 HTTP/2 gRPC、Protobuf 编解码、现有账号凭据、账号池与锁、上游响应缓存、节点查询路由、Master 解密和快照校验均由此模块提供。在此基础上，本项目进行了原生协议层扩展，新增激奏 Arena（竞技场）等专属路由与 Operation 定义。Penlight 的高层资源整理、日文文本解析、账号数据拆分和榜线缓存位于根目录 `src/`。

- 上游基础版本：1.3.6（基线提交：`3ee3addc072dfb8bb3835750ae1cd68e2e9738b5`）。
- `crates/sirius-api-proxy/UPSTREAM.json` 记录基础来源与二次开发说明。
- `crates/sirius-api-proxy/LICENSE` 保留 Haruki Dev Team 和 Sirius Project 的 MIT 版权与完整许可。
- `crates/sirius-api-proxy/LICENSE-protobuf` 保留 Google Protobuf 的许可证。
- 集成逻辑在 `src/client/sirius.rs`。

## 更新与分发

分发源码、二进制或容器时一并附带上述许可证和来源声明。Dockerfile 已将许可和归属文档复制到最终镜像的 `/usr/share/doc/penlight-notes-api/`。

本项目不代表或替代上游项目，未声明上游作者参与了 Penlight 的开发或提供支持。
