# PostgreSQL 热点 KV（`cache`）

本文描述 `infrastructure/cache` 与表 `caches` 的语义、边界与运维，与当前代码一致。

## 1. 定位

- **用途**：进程/实例间共享的**短期、可丢**键值缓存（TTL + 主键 upsert），典型场景为 **JWT 刷新态、access jti 吊销表** 等安全会话数据。
- **不是**：持久化业务主数据、跨机房复制、强一致分布式缓存；崩溃后未刷盘的写入可能丢失（见下节）。

## 2. 数据模型（`caches`）

迁移见 `infrastructure/migration/versions/0001_create_foundations.sql`。

| 设计                              | 说明                                                                                                                         |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| **`UNLOGGED`**                    | 表为 `UNLOGGED`：性能更好，但**进程/主机崩溃后可能丢失崩溃前未落盘的写入**。仅适合「丢了可重建」的语义（重新登录即可恢复）。 |
| `key TEXT PRIMARY KEY`            | 业务自定义 key 字符串（见 `auth_kit` 的 key 构造）。                                                                         |
| `value TEXT NOT NULL`             | 由 `serde_json::to_string` 写入的 JSON 文本；读时 `from_str` 反序列化。                                                      |
| `expires_at TIMESTAMPTZ NOT NULL` | 绝对过期时间；`get` / `take` 仅当 `expires_at > now()` 视为命中。                                                            |

**索引**：`idx_caches_expires_at (expires_at)`，供批量清理扫描。

## 3. API（`cache` crate）

`cache` 全部 API 使用 `sqlx`，函数统一接受 `&mut sqlx::PgConnection`（既可为池中连接，也可为同一事务 `tx.as_mut()`），便于与 identity 写库同事务提交。

| 函数                                    | 行为                                                                                                                            |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `set_ex(client, key, value, ttl_secs)`  | `INSERT … ON CONFLICT (key) DO UPDATE`，刷新 `value` 与 `expires_at`（从 `Utc::now()` 起算 TTL）。                              |
| `get<T>(client, key)`                   | `expires_at > now()` 时返回 `Some(T)`，否则 `None`；反序列化失败时**静默**返回 `None`（`from_str` 失败 → `ok()`）。             |
| `take<T>(client, key)`                  | **未过期**则 `DELETE … RETURNING value` 并反序列化；用于 refresh token 等「一次性消费」。过期或不存在返回 `None`。              |
| `del(client, key)`                      | 按 key 删除，不区分是否过期。                                                                                                   |
| `delete_expired_in_transaction(client)` | 先 `pg_advisory_xact_lock`（专用 key，**勿与 `queue` GC 共用**），再 `DELETE WHERE expires_at < now()`；供后台定时任务调用。 |

## 4. 运行时与 GC

- **后台任务**：`bin/server/background/cache_gc_job.rs`，Cron **每 5 分钟** 开事务调用 `delete_expired_in_transaction`，提交后释放 advisory lock。
- **与 `queue` 并行**：`cache` 使用 advisory key `(884_422, 1)`，`queue` 使用 `(884_423, 1)`，两路 GC 可同库同时跑、互不阻塞。

## 5. 当前调用方（代码对齐）

| 位置                                               | 用途                                                                                                   |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `features/identity/shared/token_ops.rs`            | 签发 token 时写入 refresh / subject↔refresh / access jti；刷新时用 `take` 消费 refresh；吊销时 `del`。 |
| `infrastructure/http_auth/middleware/authorize.rs` | 校验 access token 时 `get` 存储的 jti，与 JWT claims 比对，不匹配则视为吊销。 |
| `features/identity/endpoint/account_logout.rs`        | 经 `token_ops::revoke_tokens` 清理缓存（测试中亦直接用 `kv_cache` 断言 key）。                         |

## 6. 语义与实现注意点

### 6.1 `get` 反序列化失败

`get` 在 `value` 不是合法 JSON 或类型不匹配时返回 `None`，**不区分**「无 key / 已过期 / 坏数据」。若需区分，应改 API 或记录指标（当前未做）。

### 6.2 TTL 与时钟

过期判断依赖 DB `now()` 与应用侧 `Utc::now()` 写入的 `expires_at`；需保证 DB 与应用时钟大致同步（常规 NTP 即可）。

### 6.3 热 key 与连接池

鉴权中间件对**每个请求**可能 `get` 一次 PG；高 QPS 时 `caches` 与连接池会成为瓶颈。后续若需可引入本地短 TTL 内存缓存或 Redis；当前设计优先「少依赖、同库事务」。

### 6.4 不要用 `cache` 存不可丢数据

`UNLOGGED` + 可丢语义：订单、余额等**必须**走正式业务表，不得仅依赖本表。

## 7. 相关路径速查

| 路径                                                           | 职责                 |
| -------------------------------------------------------------- | -------------------- |
| `infrastructure/cache/lib.rs`                      | 全部 API 实现        |
| `infrastructure/migration/versions/0001_create_foundations.sql` | `caches` 表与索引 |
| `bin/server/background/cache_gc_job.rs`                        | 过期行清理           |
| `features/identity/shared/token_ops.rs`                        | Token 与缓存协作     |
