# 后端按需参考（AI）

根目录 `AGENTS.md` 是 always-on 约定。**本文件不要每轮先读**——只在新建域、新 endpoint 文件、加 Job/流程/事件、或动基础设施时打开对应小节。

切片边界、错误 key、Port/Repository 动词名词：以 `AGENTS.md` 为准，这里不重复。

## 新增功能

```
├── 已有域加端点 → 复制 docs/templates/endpoint_template.rs → 改 DTO/execute/SQL → 注册 mod
├── 新建域 → contract（实体/Port/事件/错误）→ runtime（端点/仓储/lib.rs）→ workspace + modules.rs
├── 跨域读 → import {other}_contract::port::{Domain}Port（禁止 import features/{other}/*）
├── 写端点接入变更历史 → 同事务调 `audit_contract::AuditService`（`record_create` / `record_updated` / `record_deleted`，传 `txn` + `&Operator` + before/after 实体；漏接不报编译错，端点测试断言 `audit_logs` 兜底；identity 三端点作示范）
├── 加事件 → contract/events.rs 实现 `shared_contract::event::Event` + subscriber/ + Module::register + publish
├── 加流程 → infrastructure/flow 定义 `#[task]` + `workflow!`，AppCtx.flow.run/resume（见 docs/FLOW.md）
├── 加 Job → 域内定义 `Job` trait 实现（payload 即类型）+ handler `async fn(T, &AppCtx)`，
│   在 `DomainModule::register` 里 `r.jobs.register::<T, _>(|job, ctx| Box::pin(handler(job, ctx)))`，
│   端点入队 `state.jobs.enqueue(T { .. }).await?`（见 docs/JOB_QUEUE.md）
├── 加周期任务 → 同加 Job 定义 `Job` + handler，再在 `register` 里一行 `r.scheduled("0 0 3 * * *", T { .. })`
│   （cron 到点 enqueue，执行语义归 job_queue；触发 master-only，执行多进程竞争）
├── 改 DB → infrastructure/migration/versions/ 新 .sql
```

## 关键基础设施

| Crate | 用途 | 详情 |
|-------|------|------|
| `infrastructure/db` | PgPool | |
| `infrastructure/event_bus` | 广播事件（Pg Outbox 默认 / NATS JetStream） | [docs/EVENT_BUS.md](../EVENT_BUS.md) |
| `infrastructure/flow` | 持久化工作流 | [docs/FLOW.md](../FLOW.md) |
| `infrastructure/kv` | 可插拔 KV | [docs/KV.md](../KV.md) |
| `infrastructure/job_queue` | 点对点 Job | [docs/JOB_QUEUE.md](../JOB_QUEUE.md) |
| `infrastructure/web` | ValidJson / ValidQuery / ValidPath + Problem Details | |
| `infrastructure/http_auth` | Bearer JWT | |
| `infrastructure/locale` | Fluent | |
| `libs/authn_kit` | JWT 提取与缓存 key | |
| `libs/authz_kit` | Cedar（待接入） | |
| `libs/trace_kit` | OpenTelemetry | |
| `libs/filter_kit` | RSQL 筛选；协议经 meta 导出，前端 `pnpm gen:api` | [docs/adr/0003](../adr/0003-filter-schema-contract.md) |
| `libs/sched_kit` | cron | |
| `shared_contract` | ID、cursor 分页、PhoneNumber、`event::Event` | |

动这些 crate 的 `pub` API 前 Grep 符号名确认调用方。大文件（如 `infrastructure/job_queue/lib.rs`）Grep 后 Read 局部，不要整份读进上下文。

## 切片落位（新建域 / 新文件时）

- **`features/shared_contract/`**：通用 ID、分页等
- **`features/{domain}_contract/`**：实体、值对象、领域错误；Port 仅跨域只读
- **`features/{domain}/endpoint/`**：单文件 HTTP 动作；`endpoint.rs` 汇总，不用 `endpoint/mod.rs`
- **`features/{domain}/repository/`**：本域写库；仅本域 `execute` 可 `use crate::repository::…`
- **`features/{domain}/subscriber/`**：域外队列 `QueueHandler`（按需）
- **`features/{domain}/shared/`**：跨 HTTP 切片编排，不演化为万能层
- **`infrastructure/`**：技术适配，禁止业务逻辑
- **`libs/`**：与业务无关的内部 crate
- 蓝本：`docs/templates/endpoint_template.rs`、`features/identity/endpoint/account_create.rs`
- 路由：受保护 `routing()`，公开（登录/刷新）`public_routing()`；聚合见 `bin/server/router.rs`

### execute 读 vs 写

- **查询**：直连 DB → Response；不用 `SELECT *`；静态 SQL 用 `sqlx::query! / query_as! / query_scalar!`，动态用 SeaQuery + `SqlxBinder`
- **写入**：DTO → Validify + contract 值对象 → `execute` → `*Repository`；错误用领域错误

### 类型

- 手机号：`shared_contract::value_object::phone_number::PhoneNumber`
- 数字枚举：`serde_repr` + `#[repr(i16)]`；DB `sqlx::{Type, Decode, Encode}`
- Row → DTO：能 `FromRow` 就不要手写映射

## 域外队列

详见 [docs/QUEUE.md](../QUEUE.md)。入队在业务事务内 `enqueue_event`；消费 `subscriber/` + `register`；at-least-once；handle 内禁止长 IO。

## 热点 KV

详见 [docs/CACHE.md](../CACHE.md)。`infrastructure/cache`，表 `caches`（UNLOGGED）。禁止放不可丢的主数据。
