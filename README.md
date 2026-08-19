# Slab

**Rust 模块化单体。** 垂直切片、Contract 接缝；今天拼在一起跑，明天可以拎出来部署。

```rust
impl DomainModule for identity::Module {
    fn name(&self) -> &'static str { "identity" }

    fn protected_routing(&self) -> OpenApiRouter<AppCtx> { ... }
    fn unprotected_routing(&self) -> OpenApiRouter<AppCtx> { ... }
    fn register(&self, r: &mut ModuleRegistrar) {
        r.events.register(AccountCreatedSubscriber);
        r.scheduled("0 0 3 * * *", MyJob { .. });
    }
}
```

## 结构

```
features/{domain} + {domain}_contract   垂直切片 + 公共表面
cross_domain/                            跨域业务规则（approval / 单号 / 成本 / 库存账）
infrastructure/                          技术适配（db / event_bus / kv / flow / job_queue / …）
bin/server/                              组装：路由、中间件、模块列表
frontend/                                管理后台 SPA（独立 workspace）
```

**Contract 互不依赖。** 跨域读走 `{Domain}Port`，本域写走 `{Aggregate}Repository`；变更历史是例外的同事务写 Port。`cargo test -p server arch_test` 强制检查。

## 技术栈

| 层 | 选型 |
|---|------|
| 运行时 | Tokio |
| HTTP | Axum 0.8 |
| 数据库 | PostgreSQL + sqlx |
| 事件总线 | Pg Outbox（默认）/ NATS JetStream |
| 流程 | sayiir（`infrastructure/flow`） |
| KV | Pg UNLOGGED（默认）/ redb / Redis（`infrastructure/kv`） |
| Job | `infrastructure/job_queue` |
| 鉴权 | JWT（access + refresh） |
| 对象存储 | 腾讯云 COS（默认）/ 本地文件系统 |
| 可观测性 | OpenTelemetry |
| 前端 | React 19 + Rsbuild + TanStack + shadcn/ui（`frontend/`，端口 3000，dev 代理 `:8081`） |

## 快速开始

```bash
rustup toolchain install stable
brew install just hurl
docker compose up -d
cp .env.example .env
cargo run -p server

# 另开终端
cd frontend && pnpm install && pnpm run dev   # http://localhost:3000

cargo test -p server arch_test
cargo test -p identity --quiet
just e2e
```

## 文档

- [AGENTS.md](AGENTS.md) — AI 协作摘要
- [docs/README.md](docs/README.md) — 按场景索引
- [frontend/AGENTS.md](frontend/AGENTS.md) — 前端约定

## License

MIT
