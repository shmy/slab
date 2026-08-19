# 后端按需参考（AI）

根目录 `AGENTS.md` 是 always-on 约定。**本文件不要每轮先读**——只在新建域、新 endpoint 文件、加 Job/流程/事件、或动基础设施时打开对应小节。

切片边界、错误 key、Port/Repository 动词名词：以 `AGENTS.md` 摘要为准，细则 [conventions.md](conventions.md)，这里不重复。

## 新增功能

```
├── 已有域加端点 → 对照 identity 蓝本改 DTO/execute/SQL → 注册（见下）
├── 新建域 → contract（实体/Port/事件/错误）→ runtime（端点/仓储/lib.rs）→ workspace + modules.rs
├── 跨域读 → import {other}_contract::port::{Domain}Port（禁止 import features/{other}/*）
├── 写端点接入变更历史 → 同事务 `AuditService::record_*`（见下「变更历史」）
├── 加事件 → contract/events.rs 实现 `shared_contract::event::Event` + subscriber/ + Module::register + publish
├── 加流程 → infrastructure/flow 定义 `#[task]` + `workflow!`，AppCtx.flow.run/resume（见 docs/FLOW.md）
├── 加 Job → 域内定义 `Job` trait 实现（payload 即类型）+ handler `async fn(T, &AppCtx)`，
│   在 `DomainModule::register` 里 `r.jobs.register::<T, _>(|job, ctx| Box::pin(handler(job, ctx)))`，
│   端点入队 `state.jobs.enqueue(T { .. }).await?`（见 docs/JOB_QUEUE.md）
├── 加周期任务 → 同加 Job 定义 `Job` + handler，再在 `register` 里一行 `r.scheduled("0 0 3 * * *", T { .. })`
│   （cron 到点 enqueue，执行语义归 job_queue；触发 master-only，执行多进程竞争）
├── 改 DB → infrastructure/migration/versions/ 新 .sql
```

写完端点必须注册：`features/{domain}/endpoint.rs` 加 `pub(crate) mod`；`lib.rs` 的 `routing()` / `public_routing()` 挂路由。新建域另要 `bin/server/modules.rs` 的 `MODULES` + workspace `Cargo.toml` 两个成员。然后 `just ai-check {domain}`。

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
| `libs/filter_kit` | RSQL 筛选；`FILTER_SCHEMA` 经 meta 导出，前端 `pnpm gen:api` | |
| `libs/sched_kit` | cron | |
| `shared_contract` | ID、cursor 分页、PhoneNumber、`event::Event` | |

动这些 crate 的 `pub` API 前 Grep 符号名确认调用方。大文件（如 `infrastructure/job_queue/lib.rs`）Grep 后 Read 局部，不要整份读进上下文。

## 变更历史

写端点在**同一事务**内调 `audit_contract::AuditService::record_create` / `record_updated` / `record_deleted`（`txn` + `&Operator` + before/after 实体）。**禁止提交后再写**。漏接不报编译错：arch_test 守护 + 端点测试断言 `audit_logs`（identity 三端点作示范）。

- 存 before/after 快照，查询端 `json_diff` 现算；`change_type` 不落库
- 敏感字段实体上 `#[serde(skip)]`（如 password、乐观锁 version）
- 写 SQL 在 `audit_contract/lib.rs`，不进 `port.rs`（arch_test 禁 port 写 SQL）
- `Operator` 在 `shared_contract`；HTTP 提取器 `OperatorContext` 在 `http_auth`（contract 不得拖鉴权栈）
- 不记请求级审计、不记失败请求（无变更即无历史）

## 切片落位（新建域 / 新文件时）

- **`features/shared_contract/`**：通用 ID、分页等
- **`features/{domain}_contract/`**：实体、值对象、领域错误；Port 仅跨域只读
- **`features/{domain}/endpoint/`**：单文件 HTTP 动作；`endpoint.rs` 汇总，不用 `endpoint/mod.rs`
- **`features/{domain}/repository/`**：本域写库；仅本域 `execute` 可 `use crate::repository::…`
- **`features/{domain}/subscriber/`**：域外队列 `QueueHandler`（按需）
- **`features/{domain}/shared/`**：跨 HTTP 切片编排，不演化为万能层
- **`infrastructure/`**：技术适配，禁止业务逻辑
- **`libs/`**：与业务无关的内部 crate
- 蓝本：`features/identity/endpoint/account_create.rs`（写）、`account_search.rs`（列表）、`account_get.rs`（读）
- 路由：受保护 `routing()`，公开（登录/刷新）`public_routing()`；聚合见 `bin/server/router.rs`

**已知取舍（勿按统一模板强改）**：

- `health` 无 `health_contract`：纯探针
- `file_contract` 无 entity/port：上传策略在 runtime `lib.rs`
- `planning_contract` 只有 port：只读计算域，不产生自身实体
- `product_contract` / `production_contract` 无 port：暂无他域跨域读，按需再加
- 大部分域不设 `repository/`：≥2 个写端点共用才抽；`purchase` 的状态端点是例外

### execute 读 vs 写

- **查询**：直连 DB → Response；不用 `SELECT *`；静态 SQL 用 `sqlx::query! / query_as! / query_scalar!`，动态用 SeaQuery + `SqlxBinder`
- **写入**：DTO → Validify + contract 值对象 → `execute` → `*Repository`；错误用领域错误

### 类型

- 手机号：`shared_contract::value_object::phone_number::PhoneNumber`
- 数字枚举：`serde_repr` + `#[repr(i16)]`；DB `sqlx::{Type, Decode, Encode}`
- Row → DTO：能 `FromRow` 就不要手写映射

## 域外队列

入队在业务事务内 `enqueue_event`；消费 `subscriber/` + `register`；at-least-once；handle 内禁止长 IO。详见 [EVENT_BUS.md](../EVENT_BUS.md)。

## 热点 KV

`infrastructure/kv`，表 `caches`（UNLOGGED）。禁止放不可丢的主数据。详见 [KV.md](../KV.md)。
