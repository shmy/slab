# Slab

**Rust 模块化单体框架。** 垂直切片、Contract 接缝、拆开即微服务。

每个业务域是一块预制板——独立成型、统一规格、同一套接缝。今天拼在一起跑，明天拎出来部署。

```rust
use slab::Module;

impl Module for identity::Module {
    fn name(&self) -> &'static str { "identity" }

    fn protected_routing(&self) -> OpenApiRouter<AppState> { ... }
    fn unprotected_routing(&self) -> OpenApiRouter<AppState> { ... }
    fn register(&self, registrar: &mut Registrar) {
        registrar.queue.register(AccountCreatedHandler);
        registrar.scheduler.add(MyCronJob);
    }
}
```

## 架构

```
features/
├── audit_contract/       ← 变更历史公共表面：AuditEvent + record（跨域写 Port）
├── audit/                ← 变更历史查询切片：GET /api/v1/audit-logs（读时算 diff）
├── identity_contract/     ← 公共表面：实体、事件、端口、错误
├── identity/              ← 垂直切片：端点、仓储、订阅
├── file_contract/
├── file/
├── shared_contract/       ← 共享内核：ID、分页、值对象
└── health/                ← 基础设施，无需 contract

infrastructure/
├── db/                    ← PgPool
├── queue/                 ← Outbox/Inbox 队列（pg_queue）
├── cache/                 ← UNLOGGED KV + TTL（pg_cache）
├── flow/                  ← sayiir 持久化工作流引擎
├── web/                   ← 提取器、响应封装、Problem Details
├── http_auth/             ← JWT 鉴权中间件
├── locale/                ← Fluent 本地化
├── migration/             ← SQL 版本迁移
├── appctx/                ← 应用上下文（AppCtx 组装）
└── ...                    ← approval / blob / jwt / costing 等领域基础设施

bin/server/                ← 组装点：路由、中间件、任务编排
```

## 核心原则

**Contract 独立。** `identity_contract` 不依赖 `file_contract`。每个 contract 是完全自治的公共 API 表面。

**Port 只读，Repository 写库。** 跨域读走 `{Domain}Port`（放在 contract），本域写走 `{Aggregate}Repository`（放在切片 crate）。`cargo test -p server arch_test` 强制检查。

**单文件端点。** 一个动作一个文件：DTO + `#[utoipa::path]` + `handler` + `execute` + 测试。复制 `account_create.rs` 就是新端点模板。

**Outbox 模式。** 领域事件和业务写在同一个事务里入队。消费通过 `infrastructure/queue` dispatcher，将来拆微服务可以无缝切到 Kafka/Debezium。

## 技术栈

| 层 | 选型 |
|---|------|
| 运行时 | Tokio multi-thread |
| HTTP | Axum 0.8 |
| 数据库 | PostgreSQL + sqlx 0.9 |
| 消息队列 | PostgreSQL Outbox（`infrastructure/queue`） |
| 流程编排 | sayiir 持久化工作流（`infrastructure/flow`） |
| 缓存 | UNLOGGED 表 KV + TTL（`infrastructure/cache`） |
| 鉴权 | JWT（access + refresh，双 realm） |
| 定时任务 | tokio-cron-scheduler（`sched_kit`） |
| 对象存储 | S3 兼容（opendal） |
| 可观测性 | OpenTelemetry（OTLP） |
| API 文档 | OpenAPI + Scalar UI |
| 内存分配器 | mimalloc |

## 快速开始

```bash
# 环境准备
rustup toolchain install stable
brew install just hurl

# 启动数据库
docker compose up -d

# 配置文件
cp .env.example .env

# 迁移 + 启动
cargo run -p server

# 测试
cargo test -p server arch_test     # 架构边界检查
cargo test -p identity             # 集成测试
just e2e                           # Hurl 端到端
```

## 新增业务域

```bash
# 1. 创建 contract 和切片 crate
mkdir -p features/inventory_contract/{entity,port,value_object}
mkdir -p features/inventory/{endpoint,repository}

# 2. 实现 Module trait
# features/inventory/lib.rs

# 3. 注册
# bin/server/modules.rs — MODULES 数组里加一行

# 4. 验证边界
cargo test -p server arch_test
```

## 文档

- `docs/ARCHITECTURE.md` — 完整架构说明
- `docs/FLOW.md` — 流程引擎（sayiir 工作流）设计
- `docs/PG_QUEUE.md` — 队列设计
- `docs/PG_CACHE.md` — 缓存设计
- `docs/E2E_HURL.md` — E2E 测试约定
- `AGENTS.md` — AI 助手上下文

## License

MIT
