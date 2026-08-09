# 后台任务队列（`worker`）

本文描述 `infrastructure/job_queue` 的架构、API 与语义边界，与当前代码一致。

## 1. 定位

- **用途**：点对点命令式后台任务（Job Queue）的强类型抽象：入队 → 竞争消费 → 重试退避 → 超时 → 终态。
- **不是**：不是事件总线（广播事实，那是 `infrastructure/event_bus`）；不是流程引擎（编排真相，那是 `infrastructure/flow`）。
- **职责边界**：只做"一次性任务的排队与执行"，不做编排（等待/信号/分支归 flow）；延迟投递是队列能力（`enqueue_after`），长时等待（跨小时/天）建议仍走 flow 的持久化 delay。
- **实现自研**：基于 sqlx（与仓库同版本 0.9），不依赖 Apalis 等第三方队列库（apalis 全系钉死 sqlx 0.8，与仓库不兼容，见 §6）。

## 2. 三通道边界

| 通道 | 语义 | 投递 | 消费 | 典型场景 |
|------|------|------|------|---------|
| **Job**（本 crate） | 命令式、一次性 | 点对点（`job_type` 竞争消费） | 重试/超时/终态（DLQ-in-table） | 生成报表、发通知、同步数据 |
| **Event**（event_bus） | 广播事实 | fan-out（所有订阅者） | 各自幂等、at-least-once | 单据完成 → 通知多域 |
| **Workflow**（flow） | 编排真相 | 实例化执行轨迹 | 等待/信号/超时/分支/续跑 | 审批联动、跨步骤长流程 |

**Job 不做编排**——不等待、不等待信号、不分流；需要时在 flow 里等完再 enqueue，或用事件驱动 enqueue。

## 3. 公开 API

### `Job` trait —— payload 即类型本身

```rust
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NAME: &'static str;              // 注册表键 + 队列路由键（蛇形命名）
    const RETRIES: usize = 3;              // 失败后追加重试次数（总执行 ≤ RETRIES + 1）
    const CONCURRENCY: usize = 1;          // 该 job_type 最大并行执行数
    const TIMEOUT: Duration = Duration::from_secs(60);  // 单次执行超时
}
```

### `JobBus` —— 入队（AppCtx 持有）

```rust
pub async fn enqueue<T: Job>(&self, job: T) -> Result<()>;                    // 立即
pub async fn enqueue_after<T: Job>(&self, job: T, delay: Duration) -> Result<()>; // 延迟投递
```

- **类型擦除发生在入队侧**：`(NAME, JSON)` 落 `worker_jobs` 表，业务代码不知道任何后端细节；
- **独立连接**：不参与业务事务（同 event_bus 约定）——业务回滚后任务可能已入队，消费端必须幂等（at-least-once）；
- 延迟是投递参数（`run_at` 未来时间），不进 payload。

### `JobRegistry<C>` —— 消费 handler 注册（ModuleRegistrar 持有）

```rust
pub fn register<T: Job, F>(&mut self, handler: F) -> &mut Self
where F: for<'a> Fn(T, &'a C) -> BoxFuture<'a, Result<()>> + Send + Sync + 'static;
```

- handler 是纯函数 `async fn(T, &C) -> rootcause::Result<()>`，调用方包一层 `Box::pin`：
  ```rust
  r.jobs.register::<SyncOrder>(|job, ctx| Box::pin(handle_sync_order(job, ctx)));
  ```
- 上下文泛型（同 `event_bus::Registry<C>` 模式）：crate 不依赖 appctx，由 `module` crate 实例化为 `JobRegistry<AppCtx>`，避免循环依赖；
- **同名冲突是编程错误**：同一 `Job::NAME` 重复注册立即 panic（fail fast）。

### `WorkerManager<C>` —— 消费侧运行时

```rust
WorkerManager::new(bus: JobBus, ctx: C)         // 默认参数
    .with_options(WorkerOptions { .. })          // 测试可覆盖：轮询间隔/退避/孤儿扫描
    .register_all(&registrar.jobs)               // 收编注册表
    .run(shutdown_rx)                            // 每 job_type 一个轮询循环 + 孤儿扫描
```

- **进程内运行**：随 server 生命周期 spawn（同 event_bus dispatcher）；多进程共享同一表竞争消费的能力保留（`FOR UPDATE SKIP LOCKED`）；
- 每 job_type 一个轮询循环，并发受 `Semaphore(CONCURRENCY)` 约束。

## 4. 数据模型与语义契约

### `worker_jobs` 表（由 `JobBus::try_new_pg` 幂等自建——基础设施自管表不进 migration，
同 event_bus 先例：事件总线表同样完全由 `PgBackend::try_new` 自建）

| 列 | 说明 |
|------|------|
| `id` | BIGSERIAL 主键 |
| `job_type` | `Job::NAME`（路由键） |
| `payload` | JSONB（序列化后的 Job） |
| `status` | `Pending` / `Running` / `Done` / `Failed`（文本而非 smallint：运维直接 SELECT 排查） |
| `attempts` | 已执行次数（失败自增） |
| `max_attempts` | 执行上限（入队时 = `RETRIES + 1`） |
| `run_at` | 到期时间（未来 = 延迟投递） |
| `last_error` | 最近一次失败原因（终态留痕） |
| `lock_by` / `lock_at` | 拉取锁（Running 持有） |
| `done_at` | 终态时间 |

### 状态机

```
入队(Pending, run_at=now|future)
   │ 轮询拉取（FOR UPDATE SKIP LOCKED，run_at <= now）
   ▼
Running ── 成功 ──► Done
   │
   └── 失败/超时 ── attempts+1
        ├─ attempts < max_attempts ──► Pending（run_at 后移 = 指数退避：base × 2^(n-1)，cap 60s）
        └─ attempts ≥ max_attempts ──► Failed（last_error 留原因 = DLQ-in-table）
```

- **孤儿恢复**：`Running` 且 `lock_at` 超龄（默认 10 分钟）→ 回置 `Pending`（`last_error='orphan_abandoned'`）——覆盖"进程崩溃在拉取后、落终态前"的窗口，**handler 必须幂等**；
- **重试持久化**：退避调度写库（`run_at` 后移），进程崩溃不丢重试进度；
- **超时**：单次执行超 `TIMEOUT` 按一次失败计（`job_timeout`），计入重试次数。

## 5. 接线

```
AppCtx.jobs: JobBus（入队）          ModuleRegistrar.jobs: JobRegistry<AppCtx>（注册）
        │                                      │
        └──────────────┬───────────────────────┘
                       ▼
              WorkerManager（server.rs spawn，进程内）
                       │
               worker_jobs 表（pg / sqlite，feature 切换）
```

- 域模块在 `DomainModule::register` 里：`r.jobs.register::<T>(|job, ctx| Box::pin(handler(job, ctx)))`；
- 业务端点入队：`state.jobs.enqueue(GenerateReport { .. }).await?`；
- 后端选择（bin/server feature，互斥；双开时 worker-pg 让位，同 blob 惯例）：
  - `worker-pg`（推荐生产）→ 表在业务 PG；
  - `worker-sqlite`（默认，单机部署）→ 本地 sqlite 文件（`--queue-sqlite-path`，默认 `worker.db`），**仅支持单进程消费**。

### 使用示例（三步，照着抄）

**① 定义 Job**（放在业务域内，如 `features/{domain}/job/`；payload 即类型，只含可序列化字段）：

```rust
// features/sales/job/export_orders.rs
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportOrders {
    pub order_id: i64,
}

impl job_queue::Job for ExportOrders {
    const NAME: &'static str = "export_orders";
    const RETRIES: usize = 2;                     // 失败后追加重试 2 次（总执行 ≤ 3）
    const CONCURRENCY: usize = 2;                 // 最多 2 个并行
    const TIMEOUT: Duration = Duration::from_secs(120);
}
```

**② 写 handler 并注册**（纯函数，`rootcause::Result<()>`；失败返回 Err 即走退避重试）：

```rust
// features/sales/lib.rs
async fn handle_export_orders(job: ExportOrders, ctx: &appctx::AppCtx) -> rootcause::Result<()> {
    // 用 ctx.pg_pool / ctx.blob / ctx.kv 干真实活；必须幂等（at-least-once）
    Ok(())
}

impl DomainModule for Module {
    fn register(&self, r: &mut module::ModuleRegistrar) {
        r.jobs.register::<ExportOrders>(|job, ctx| Box::pin(handle_export_orders(job, ctx)));
    }
}
```

**③ 端点入队**（立即 / 延迟投递，`AppCtx.jobs` 即 axum state 的 `FromRef`）：

```rust
// features/sales/endpoint/export_orders_create.rs（handler 内）
state.jobs.enqueue(ExportOrders { order_id }).await?;                                     // 立即
state.jobs.enqueue_after(ExportOrders { order_id }, Duration::from_secs(3600)).await?;    // 1 小时后
```

> 延迟投递适合分钟级起步的短等待；跨小时/天的长等待仍建议交给 `flow` 的持久化 delay（重启不丢）。
> 完整可运行示例见 `infrastructure/job_queue/lib.rs` 的 `#[cfg(test)] mod tests`（框架验证夹具）。

## 6. 选型说明（为何自研而非 Apalis）

- apalis 0.7 与 1.0-rc 全系钉死 **sqlx 0.8**，仓库是 **sqlx 0.9**——仓库池子（`db::PgPool`）喂不进 apalis storage，双 sqlx 共存意味着生产多一套连接池、测试无法直用 `#[sqlx::test]`；
- 表结构/拉取 SQL（`FOR UPDATE SKIP LOCKED`）借鉴 apalis 的成熟设计（MIT）；
- 仓库已有同款自研先例：event_bus 的 outbox + dispatcher（重试/终态/GC）即在 sqlx 0.9 上手写；
- 自研后后端可替换性天然成立：换 Redis Streams / NATS JetStream 时，`JobBus`/`WorkerManager` 接口不变，新增后端实现即可。

## 7. 测试策略

- 主矩阵走 **pg**（`#[sqlx::test]`，独立临时库），覆盖五场景：
  1. 立即入队 → 消费 → `Done`；
  2. 延迟入队 → 到期前不消费、到期后消费；
  3. 失败 → 退避重试 → 成功（断言 `attempts` 落库）；
  4. 超时 → 计入失败 → 耗尽 `Failed`（断言 `last_error='job_timeout'`）；
  5. 幂等（同一 payload 重放，效果只发生一次）。
- sqlite 一个冒烟测试（临时文件，`--features sqlite`）：enqueue → 消费 → `Done` + 延迟投递；
- 测试用 `WorkerOptions` 加速（poll 20ms / backoff 50ms），断言用带超时的轮询，不用裸 sleep。

## 8. 已知边界

- 无显式 DLQ 表：终态 `Failed` 留表 + `last_error` + tracing 日志，人工重放需求出现时再加管理端点；
- 无多队列隔离（无 priority）：单一队列 + 延迟投递，多队列等真实需求再引入；
- 无去重（dedup）：幂等是 handler 职责（与 event_bus 同一约定）；
- sqlite 后端仅单进程消费；延迟精度受轮询间隔（默认 200ms）与后端时间精度影响；
- 无内置管理 UI：排查走 SQL + 日志。
