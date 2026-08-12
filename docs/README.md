# Slab 文档索引

> 各文档均自包含，可独立阅读。按场景选择入口。

## 新人上路（按顺序读）

1. [README.md](../README.md) — 项目概览、技术栈、快速开始
2. [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) — 业务背景、已实现能力、扩展方向
3. [ARCHITECTURE.md](ARCHITECTURE.md) — 完整架构说明（跨域依赖、Port/Repository 分工、编码约定）

## 架构深潜

| 文档 | 内容 | 读它的时候 |
|------|------|-----------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | 依赖方向、Port/Repository、endpoint 模式、开发约定 | 写复杂跨域逻辑前、新建业务域前 |
| [EVENT_BUS.md](EVENT_BUS.md) | 事件总线（广播）设计、at-least-once 语义 | 加新事件/订阅者、切换后端时 |
| [KV.md](KV.md) | 可插拔 KV 缓存后端（Pg / redb / redis）设计、Token 吊销实现 | 操作 token 吊销/缓存状态、切换后端时 |
| [FLOW.md](FLOW.md) | sayiir 持久化工作流（信号/超时/分流编排）、适用场景与陷阱 | 加长流程/跨单据联动/审批超时升级前 |
| [frontend/docs/architecture.md](../frontend/docs/architecture.md) | 前端架构：多标签页 / keep-alive / 虚拟表格 / React Compiler 踩坑 | 改前端表格、标签页、主题前 |

## 测试

| 文档 | 内容 | 读它的时候 |
|------|------|-----------|
| [E2E_HURL.md](E2E_HURL.md) | Hurl E2E 编写规范、变量约定、错误断言约定 | 改路由/错误码/种子数据后更新 E2E 前 |

## AI 协作

| 文件 | 内容 | 使用对象 |
|------|------|----------|
| [AGENTS.md](../AGENTS.md) | 仓库完整上下文（架构、约定、决策树、命令） | Claude Code / 其他 AI 助手 |
| [frontend/AGENTS.md](../frontend/AGENTS.md) | 前端上下文（React 19 + Rsbuild + TanStack 约定、三查命令） | AI（改前端代码时） |
| [.agents/skills/rust-backend/SKILL.md](../.agents/skills/rust-backend/SKILL.md) | 后端垂直切片实现规范 | AI（触发式加载） |
| [.agents/skills/rust-tests/SKILL.md](../.agents/skills/rust-tests/SKILL.md) | 测试编写规范 | AI（触发式加载） |

## 外部参考

- [sayiir](https://docs.sayiir.dev) — 持久化工作流引擎（`infrastructure/flow` 底层）
- [fullstackhero Modular Monolith](https://fullstackhero.net/) — 架构参考
- [Axum](https://docs.rs/axum) — HTTP 框架
- [sqlx](https://docs.rs/sqlx) — 数据库驱动
- [SeaQuery](https://docs.rs/sea-query) — 动态 SQL 构建器
- [Hurl](https://hurl.dev) — E2E HTTP 测试
