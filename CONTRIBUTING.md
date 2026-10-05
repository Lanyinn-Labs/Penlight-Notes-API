# 开发与提交规范

## 项目结构

- `src/api/`：HTTP 路由、认证和请求保护；活动入口集中在 `events.rs`。
- `src/client/`：进程内 Sirius 适配、Protobuf 响应转换和玩家资料补充。
- `src/events.rs`：Master 活动时间和排名能力；`src/ranking.rs`：档线缓存及并发管理。
- `src/config.rs`：统一环境配置和启动校验；`data/jp-client.json`：经验证的公开客户端参数。
- `vendor/sirius-api-proxy/`：固定版本的未修改上游；`UPSTREAM.json` 是版本和来源摘要的唯一记录。
- `tests/`：HTTP 集成和维护脚本回归；`docs/`：API、配置、运行及发布文档。
- `artifacts/`、`.local-backups/`、`secrets/`：本地数据和验证产物，Git 忽略，不进入发布包。

不要修改 vendor 内的上游源码。升级时按明确 tag/commit 更新源码和协议，重建所有文件摘要，保留完整许可证；适配代码写在本项目 `src/`。更新运行文档和来源声明，按 [来源流程](docs/upstream-attribution.md) 验证。

## 检查与 API 约定

检查工具用于源码克隆，预编译运行包无需 Rust 开发环境。使用 Rust 1.97、Python 3.11+，在仓库根目录运行：

```bash
python scripts/check_project.py
# 本地已有依赖缓存时：
python scripts/check_project.py --offline
```

该命令与 Linux/Windows CI 相同，依次检查来源摘要、Python 回归、Rust 格式、编译、Clippy、测试和差异空白。新增行为用真实边界和错误路径验证，避免仅复述实现的测试。发布时另行运行打包和解压 smoke checks。

保持 `/api/{region}`、现有认证和 `error.code` 稳定；新增字段优先采用兼容方式。参数在官方请求前校验。缺失名次为 null，真实零分为 0；缓存命中或失败不更新观测时间。基于 Master 的时间按已验证的日本时区解析，保留来源版本。实服验证与离线测试分别记录，未开放能力不能声明已实服通过。

本项目的国际服仍只提供离线 Master；Sirius 独立服务的多区域 SDK、数据库、Git 发布、压缩等可选能力，不会因 vendor 升级而自动变为 Penlight 的外部 API。启用新能力需完成配置、路由和验证。

## Commit message

使用 Conventional Commits，标题为 `type(scope): summary`；scope 可省略，标题最多 72 个字符，不加句号。标题采用英文动词原形，描述完成后的变化；每次提交围绕一个可独立解释和验证的修改。

常用类型：`feat` 新功能、`fix` 修复、`refactor` 结构调整、`docs` 文档、`test` 测试、`build` 构建、`ci` 工作流、`chore` 维护。正文与标题空一行，说明具体问题、行为变化和已经完成的验证，避免写对话过程。重大不兼容变更使用 `!` 并提供 `BREAKING CHANGE:` 和迁移方式。

```text
feat(events): add current JP event cutoffs

Resolve the active event from verified Master schedules and honor ranking
capabilities before dispatch. Preserve missing ranks as null.

Validation: HTTP regression tests and a live ranking-disabled response.
```

本仓库附带 `.gitmessage`。可为当前克隆配置模板：

```bash
git config --local commit.template .gitmessage
python scripts/check_commit_messages.py --file /path/to/message.txt
python scripts/check_commit_messages.py --range origin/main..HEAD
```

提交前检查暂存范围，排除账号、安装包、私密日志和运行数据。不要重写已有公共提交历史；新提交采用上述格式。
