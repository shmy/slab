# 事件总线（`event_bus`）

本文描述 `infrastructure/event_bus` 的架构、语义与运维边界，与当前代码一致。

## 1. 后端架构与选型

`event_bus` 提供统一门面 `EventBus` 枚举 + 方法 API（模式与 `infrastructure/kv` 一致），后端编译期按 feature 装配：

| 后端 | feature | 实现 | 发布语义 | 消费 |
|------|---------|------|---------|------|
| **Pg**（默认） | `pg` | `PgBackend`（Outbox 表 `_pg_events` + 进程内 dispatcher） | 后端自取连接发布（独立于调用方事务） | 批事务轮询 + 每订阅者分发状态/重试 |
| **Nats** | `nats` | `NatsBackend`（JetStream 直发） | 直发（不参与 PG 事务；延迟用 ADR-51 `@at` 调度） | 每订阅者一个 durable pull consumer |

**选型规则**：

- 默认（无显式选择）→ `PgBackend`：零新增组件、发布与业务同事务（Outbox 语义）。
- 需要跨进程分发 / 独立事件中间件 → `event-bus-nats`（JetStream，本地 `nats-server -js` 即可）。

**feature 切换**（`bin/server`，互斥开启其一；未显式选择时默认 `event-bus-pg`）：

| 命令 | 后端 |
|------|------|
| `cargo run -p server`（默认） | `EventBus::Pg` |
| `cargo run -p server --features event-bus-nats` | `EventBus::Nats`（配置 `NATS_URL` / `NATS_STREAM_NAME`） |

**发布语义**：两个后端均为「独立连接/直发」——`publish(event)` 不接收调用方事务连接（pg 后端内部从池取连接，nats 直发 JetStream）。**业务事务回滚后事件可能已发布/已分发**——消费端 handler 必须幂等（at-least-once）。

> 以下 §2–§11 为 **Pg 后端**（Outbox + dispatcher）的深度文档；Nats 后端消费为「每个订阅者一个 durable consumer，回调拿 PG 连接执行 `Subscriber::handle`」。

## 2. 定位（Pg 后端）

- **用途**：事务内发布（outbox-ish），由独立 **dispatcher** 在同一应用进程内拉取并消费；适合「与主业务同一 DB 事务提交、再异步处理」的场景。
- **广播语义**：一个事件分发给同一 topic 的**所有**订阅者（每个 `Subscriber` 一个分发行，各自独立重试/终态）。

## 2.1 数据模型

> 建表：`PgBackend::try_new` 幂等自建全部事件总线表（`_pg_events` / `_pg_event_deliveries` + 索引 + 触发器），
> 不依赖 migration 版本——基础设施自管表不进 migration（`infrastructure/migration/versions/` 中无其定义），
> 与 `infrastructure/worker` 的 `worker_jobs` 表同一模式（crate 内自建为唯一事实源）。

### `_pg_events` — 事件本体（一行一个事件）

| 列                          | 说明                                                                                                                   |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `topic`                     | 路由键，与 `Subscriber::topic()` 一致；同一 topic 可注册**多个**订阅者                                            |
| `payload`                   | **`TEXT`**，存 **JSON 文本**（`serde_json::to_string`）；库内不做 JSON 校验，由 `publish` / dispatcher `from_str` 保证 |
| `status`                    | 1=pending，2=delivered，3=failed；**聚合语义**：pending=还有订阅者未完成，delivered=全部成功，failed=存在终态失败或无人订阅 |
| `attempts` / `max_attempts` | 兼容保留（广播下重试计数迁移到 `_pg_event_deliveries`，本列不再更新）                                                        |
| `next_attempt_at`           | 行级拉取时间：由 dispatcher 聚合刷新为「最早未完成分发的时间」                                                          |
| `last_error`                | 最近一次终态失败说明（有分发失败时汇总最近一条）                                                                       |
| `delivered_at`              | 全部分发成功的时间；GC 依赖                                                                                            |
| `created_at` / `updated_at` | 审计与触发器维护                                                                                                       |

**部分索引**：`status=1` 且 `attempts < max_attempts` 上的 `(next_attempt_at, id)` 与 `(topic, next_attempt_at, id)`。

### `_pg_event_deliveries` — 分发状态（事件 × 订阅者，一行一次分发）

| 列             | 说明                                                          |
| -------------- | ------------------------------------------------------------- |
| `event_id`   | 外键 → `_pg_events(id)`，`ON DELETE CASCADE`                       |
| `handler`      | 订阅者标识（`Subscriber::name()`），与 `message_id` 组成主键 |
| `status`       | 1=pending，2=delivered，3=failed                              |
| `attempts`     | 该订阅者的重试计数                                              |
| `max_attempts` | 继承自事件行（发布时指定）                                      |
| `next_attempt_at` | 该订阅者下次可被拉取的时间（退避）                           |
| `last_error`   | 该订阅者最近一次错误/终态说明                                  |
| `delivered_at` | 该订阅者成功分发时间                                            |

**部分索引**：`(next_attempt_at, message_id) WHERE status = 1 AND attempts < max_attempts`（dispatcher 拉取计划稳定）；`(delivered_at) WHERE status = 2`（观测/GC）。

## 3. 运行时拓扑

```
HTTP / 其它入口
  └── 业务 execute 内 sqlx 事务（`sqlx::Transaction<Postgres>` 或同一 `PgConnection`）
        └── publish / publish_with_delay  → INSERT _pg_events（一个事件一行）
  └── COMMIT 后行对下游可见

bin/server/server.rs
  └── run_dispatcher(PgPool, FrozenRegistry, DispatcherConfig, shutdown)
        └── 轮询 + SELECT … FOR UPDATE SKIP LOCKED（批内单事务，按事件行拉取）

features/{domain}（`lib.rs` 中 `register`）
  └── subscriber/ 内各 `Subscriber` 实现，在 `register()` 中注册；
      同一 topic 多个订阅者 = 一个事件多个订阅者（广播）
```

- **注册表**：`Registry::register` 为**追加**（同 topic 多个订阅者全部保留）→ `freeze()` → `FrozenRegistry`，启动时一次性构建。
- **GC**：`bin/server/gc_jobs.rs` 调用 `delete_delivered_older_than_in_transaction`，带 `pg_advisory_xact_lock`，避免多实例同时删。删除事件行时 `_pg_event_deliveries` 由外键级联清理。

## 4. 分发语义

- **广播**：一个事件对同一 topic 的每个注册订阅者各生成一行分发，**全部**执行（`_pg_event_deliveries` 主键 `(event_id, handler)` 保证每个订阅者至多一份）。
- **At-least-once**：同一个事件在崩溃、重试后可能再次进入 `handle`；**实现 `Subscriber` 时必须幂等**。
- **订阅者隔离**：每个订阅者独立 SAVEPOINT、独立重试、独立终态；一个失败不影响其它订阅者，也不阻塞同批其它事件。
- **无独立「处理中」状态**：靠 `FOR UPDATE SKIP LOCKED` 在事件行上锁定，提交后锁释放；进程崩溃未提交则分发仍为 pending，可被其它 worker 拉取。

## 5. 单 cycle 内行为（dispatcher）

1. `BEGIN`，拉取「有待处理分发」的事件行（**行级状态不参与拉取**，纯聚合/告警语义）：
   - 从未生成过分发（新事件 / 旧事件迁移）**或**
   - 存在到期未完成的分发（`_pg_event_deliveries` 中 status=pending、attempts 未耗尽、`next_attempt_at` 已到）
   - 因此部分失败 + 部分退避的事件仍可被拉取，直到所有分发终态；`FOR UPDATE SKIP LOCKED LIMIT n`。
2. 对每条事件：
   - 无订阅者订阅该 topic → 事件行终态失败（`no_handler_for_topic:…`，防呆告警）。
   - 为当前注册的每个订阅者 `INSERT ... ON CONFLICT DO NOTHING` 生成分发行（新订阅者上线自动补上仍 pending 的事件）。
   - 逐个执行**已到期**的分发（**严格按各自 `next_attempt_at` 门控**，退避中的订阅者不被牵连）：`SAVEPOINT` → `subscriber.handle(tx, payload)` → 成功 `mark delivered`，失败 `ROLLBACK TO SAVEPOINT` 再按退避记录重试。
3. 聚合刷新事件行状态（见 §6 行级状态）。
4. 全部成功后 `COMMIT`。

**handler 业务错误**：不向外抛错，该分发行更新为退避或 `failed`，**不阻塞**其它订阅者与同批后续事件。

**整批回滚**：若因 **DB 层**错误返回 `Err`（极少见），整批事务回滚，下轮会重新拉取同一批（仍为 at-least-once）。

## 6. 行级状态机（聚合，**终态失败优先**）

- 存在终态失败分发 → `_pg_events.status=3`，`last_error` 汇总最近一条失败；**即使还有 pending 分发**（拉取仍会继续，直到所有分发终态）。
- 无失败但还有 pending 分发 → `_pg_events.status=1`，`next_attempt_at` = 最早未完成分发的到期时间。
- 全部分发成功 → `_pg_events.status=2` + `delivered_at`（GC 依据）。
- 全部分发终态后 `next_attempt_at=infinity`，行不再被拉取。

**人工修复路径**：将失败分发行改回 `status=1`、重置 `attempts` 与 `next_attempt_at`，**同时**把事件行 `next_attempt_at` 拨回过去（行状态由下次处理自动刷新）：

```sql
UPDATE _pg_event_deliveries SET status = 1, attempts = 0, next_attempt_at = NOW()
WHERE event_id = <id> AND handler = '<name>';
UPDATE _pg_events SET next_attempt_at = NOW() WHERE id = <id>;
```

## 7. 发布 API（`EventBus` 方法门面）

- `bus.publish(event)`：`event` 须实现 `Event` trait（携带 `TOPIC` 常量）。pg 写 outbox 表（内部自取连接）；nats 直发 JetStream。
- `bus.publish_in_tx(tx, event)`：**强一致发布（pg 后端）**——与业务同一事务，回滚即不分发（Outbox 语义）。需要「业务提交成功则事件必落库」的可靠性时用（如创建类事件）；nats 后端无事务语义，等价于普通发布。
- `bus.publish_with_delay(event, delay)`：pg 推迟首次可见时间（`next_attempt_at`）；nats 用 JetStream ADR-51 `Nats-Schedule` 调度。

**语义**：普通发布**不参与调用方事务**——业务回滚后事件可能已发布/已分发（**幽灵事件**），消费端 handler 必须幂等 + **校验实体存在性**（见 §8）。

```rust
// endpoint 内直接发布（AppCtx.bus 经 State 提取），无需事务连接
bus.publish(&AccountCreatedEvent { id }).await?;
```

**广播订阅**：事件只发布一行；多个订阅者各自注册同一 `topic` 即可，发布侧无需任何改动：

```rust
// 域 A 与域 B 的 lib.rs register 各自注册同一 topic 的订阅者
registrar.bus.register(ASubscriber);   // topic = AccountCreatedEvent::TOPIC
registrar.bus.register(BSubscriber);   // 同一 topic，同一事件两个订阅者都收到
```

## 8. `Subscriber` 实现约束

- `topic()`：要监听的事件 topic（可多个订阅者共用）。
- `name()`：订阅者唯一标识，默认取类型名；**同一 topic 下多个订阅者的 `name()` 必须互不相同**——`Registry::register` 会检测同名冲突并在启动时直接 `panic`（fail fast），避免分发行主键冲突导致的静默丢失。
- **短、确定性**：优先只做本库写；避免在持有事务与行锁时做 HTTP/S3 等慢 IO（会拖住连接与批内后续事件）。
- **幂等**：重复 `handle` 不得产生重复副作用。
- **错误**：`Err` 触发该订阅者自己的退避/终态；无对应 topic 的订阅者时事件行被标记为 terminal failure（`no_handler_for_topic:…`）。

## 9. 吞吐与扩展

- 单 dispatcher 批内**串行**处理（同一事件的多个订阅者也在同批内串行）；要提高吞吐可**多进程/多实例**各跑一个 dispatcher：`SKIP LOCKED` 自动分片。
- 默认约 **1s 轮询间隔**（`DispatcherConfig::poll_interval`）；低延迟场景可考虑后续加 `LISTEN/NOTIFY`（当前未实现）。

## 10. SQL 与驱动注意点（sqlx）

- **`make_interval(secs => $n)`** 的 `secs` 在 PostgreSQL 中为 **`double precision`**；用整型绑定时容易踩到类型不匹配。当前实现使用 `NOW() + ($n * interval '1 second')`，其中 `$n` 为 `bigint` 绑定。
- **数组绑定**：`unnest($n::text[])` 对应 Rust 侧 `&[String]`（sqlx 0.9 `TEXT[]`）。

## 11. 运维与可观测性

- **告警**：建议对 `SELECT count(*) from _pg_event_deliveries WHERE status = 3` > 0 或持续增长配置告警（比 `_pg_events.status=3` 更细粒度，能定位到具体订阅者）。
- **追踪**：dispatcher / cycle 已挂 `tracing`；周期失败会 `warn` 日志。
- **清理**：仅删除**已分发**且超过保留天数的事件行（`_pg_event_deliveries` 级联删除）；failed 行需人工策略（导出、修复、删）。

## 12. 相关路径速查

| 路径                                                           | 职责                               |
| -------------------------------------------------------------- | ---------------------------------- |
| `infrastructure/event_bus/lib.rs`                     | `EventBus` 枚举 + 方法门面 |
| `infrastructure/event_bus/pg.rs`                       | `PgBackend`（Outbox + dispatcher） |
| `infrastructure/event_bus/nats.rs`                     | `NatsBackend`（JetStream 直发 + durable consumer） |
| `infrastructure/migration/versions/0001_create_foundations.sql` | `fn_set_updated_at` 函数 |
| `infrastructure/event_bus/pg.rs` | `PgBackend` 运行时自建全部事件总线表（`_pg_events` / `_pg_event_deliveries` + 索引 + 触发器） |
| `bin/server/config.rs`                             | 按 feature 组装 `event_bus::EventBus`    |
| `bin/server/server.rs`                             | dispatcher 启动与 registry 构建    |
| `bin/server/gc_jobs.rs`                            | 已分发行 GC（nats 后端为空操作）    |
| `features/identity/subscriber/`                    | 各 topic 的 `Subscriber` 实现    |
| `features/identity/lib.rs`                                     | `register(&mut ModuleRegistrar)` 聚合注册 |
| 各域 `*_contract/events.rs`                                   | 事件定义（`Event` trait）          |
