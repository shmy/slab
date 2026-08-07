# 缓存后端（`cache`）

本文描述 `infrastructure/cache` 的语义、后端选型、边界与运维，与当前代码一致。

## 1. 定位

- **用途**：进程/实例间共享的**短期、可丢**键值缓存（TTL + 主键 upsert），典型场景为 **JWT 刷新态、access jti 吊销表** 等安全会话数据。
- **不是**：持久化业务主数据、跨机房复制、强一致分布式缓存；崩溃后未落盘的写入可能丢失（见 §6）。

## 2. 后端架构与选型

`cache` 提供统一门面 `KvBackend` 枚举 + 方法 API，后端实现编译期按 feature 装配：

| 后端 | feature | 实现 | 语义 |
|------|---------|------|------|
| **Pg**（默认） | `pg` | `PgCache`（PostgreSQL `caches` UNLOGGED 表） | 跨实例共享、可丢、原子 `take` |
| **Redb** | `redb` | `RedbCache`（redb 4 嵌入式 KV） | 单实例本地文件、`Durability::None` 可丢 |
| **Redis** | `redis` | `RedisCache`（bb8 连接池） | 跨实例共享、原生 TTL |

**选型规则**：

- 默认（无显式选择）→ `PgCache`（cache crate 默认 feature；server 默认已切 `kv-redis`，见下）。
- 单实例部署想解放 PG 连接池 → `kv-redb`（嵌入式本地文件）。
- 多实例部署需要共享吊销/会话 → `kv-redis`（或保持 `kv-pg`）。

**feature 切换**（`bin/server`，互斥开启其一；未显式选择时默认 `kv-redis`，切换后端用 `--no-default-features` 显式指定）：

| 命令 | 后端 |
|------|------|
| `cargo run -p server`（默认） | `KvBackend::Redis`（配置 `REDIS_URL`） |
| `cargo run -p server --no-default-features --features kv-pg,queue-pg,blob-fs` | `KvBackend::Pg` |
| `cargo run -p server --no-default-features --features kv-redb,queue-pg,blob-fs` | `KvBackend::Redb`（配置 `CACHE_DB_PATH`） |

`default = ["pg"]`（`cache` crate）保证任何依赖方无 feature 时也可独立编译；构造器按后端拆名（`try_new_pg` / `try_new_redb` / `try_new_redis`），pg 可与 redb/redis 并存，唯 redb+redis 互斥（见 §4）；server 默认 `kv-redis`，测试组装固定用 `new_for_test`（PG 池，见 §7）。

## 3. 数据模型

### 3.1 Pg 后端（`caches` 表）

- 迁移见 `infrastructure/migration/versions/0001_create_foundations.sql`；`PgCache::try_new` 会**幂等自建**（`CREATE UNLOGGED TABLE IF NOT EXISTS` + 索引），不依赖 migration 版本。
- `UNLOGGED`：性能更好，但进程/主机崩溃后可能丢失崩溃前未落盘的写入——适合「丢了可重建」的会话语义。
- 列：`key TEXT PRIMARY KEY`、`value TEXT NOT NULL`（JSON 文本）、`expires_at TIMESTAMPTZ NOT NULL`；索引 `idx_caches_expires_at (expires_at)` 供批量清理。

### 3.2 Redb 后端

- 单文件数据库（默认 `data/cache.redb`），`Durability::None`（不 fsync）对齐 UNLOGGED 可丢语义。
- 值封装 `Entry { value, expires_at }`，TTL 惰性判活 + `delete_expired` 扫表清理。
- **单进程限制**：redb 数据库文件禁止多进程并行打开；多实例部署时每实例一份文件，跨实例吊销不共享——此场景用 `kv-pg` / `kv-redis`。

### 3.3 Redis 后端

- TTL 由 Redis 原生过期处理，`delete_expired` 返回 0（无需清扫）。

## 4. API（`KvBackend` 方法门面）

```rust
pub enum KvBackend { Pg(PgCache), Redb(RedbCache), Redis(RedisCache) }

impl KvBackend {
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    pub async fn set_ex<T: Serialize + Send + Sync>(&self, key: &str, value: &T, period: Duration) -> Result<()>;
    pub async fn take<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    pub async fn del(&self, key: &str) -> Result<()>;
    pub async fn delete_expired(&self) -> Result<u64>;
}
```

| 方法 | 行为 |
|------|------|
| `get<T>(key)` | 未过期返回 `Some(T)`，否则 `None`；反序列化失败**静默**返回 `None`（不区分「无 key / 已过期 / 坏数据」）。 |
| `set_ex(key, value, period)` | 写入并刷新 TTL（`Duration`）。Pg：`INSERT … ON CONFLICT DO UPDATE`；Redb：写事务 upsert；Redis：`SET … EX`。 |
| `take<T>(key)` | **原子消费**：未过期则取出并删除，否则 `None`。Pg：`DELETE … RETURNING`；Redb：写事务内 get+remove；Redis：`GETDEL`。用于 refresh token 防重放。 |
| `del(key)` | 按 key 删除，不区分是否过期。 |
| `delete_expired()` | 清理过期条目，返回删除条数（Redis 返回 0）。 |

构造走各后端独立构造器 `KvBackend::try_new_pg(pool)` / `try_new_redb(path)` / `try_new_redis(pool)`（签名随后端不同：`PgPool` / 路径 / bb8 Pool；拆名避免同名 `try_new` 在 feature 并集下的方法重名冲突），或直接构造变体；测试统一用 `KvBackend::new_for_test(pool)`（固定复用测试 PG 池）。

## 5. 运行时与 GC

- **后台任务**：`bin/server/gc_jobs.rs` 的 `CacheGc`，Cron **每 5 分钟**调用 `state.kv.delete_expired()`；Pg 后端为幂等 `DELETE WHERE expires_at < now()`（无 advisory lock，多实例并发无害），Redb 后端扫表清理，Redis 后端空操作。
- **与 `queue` 并行**：queue 的 GC 独立于 cache，两者互不干扰。

## 6. 语义与实现注意点

### 6.1 缓存写不参与调用方 PG 事务

`cache` 是**可丢辅助数据**：每次操作独立取连接/事务，**不再**与 identity 写库同事务提交。调用方约定：**先提交业务事务，再写缓存**（如 `token_ops::issue_tokens` 在登录/刷新完成后写入）；缓存写失败不影响业务（吊销延迟到 TTL 过期，可接受）。

### 6.2 原子 take 与防重放

`take` 在各后端均为原子操作（DELETE RETURNING / 写事务 / GETDEL），refresh token 消费后立即失效，同一 token 二次刷新被拒。

### 6.3 多实例语义

| 后端 | 多实例 |
|------|--------|
| Pg / Redis | 共享：实例 A 登出，实例 B 的 jti 校验立即生效 |
| Redb | 每实例一份文件，吊销**不跨实例**生效（TTL 过期前失效）——单实例部署专用 |

### 6.4 TTL 与时钟

过期判断依赖后端时钟（PG `now()` / 应用侧 `Utc::now()` 写入的 `expires_at`）；需保证应用与 DB 时钟大致同步（常规 NTP 即可）。

### 6.5 不要用 `cache` 存不可丢数据

可丢语义（UNLOGGED / Durability::None）：订单、余额等**必须**走正式业务表，不得仅依赖本表。

## 7. 调用方与组装

| 位置 | 用途 |
|------|------|
| `features/identity/shared/token_ops.rs` | 签发 token 时写入 refresh / subject↔refresh / access jti；刷新时 `take` 消费 refresh；吊销时 `del`。 |
| `infrastructure/http_auth/middleware/authorize.rs` | 校验 access token 时 `get` 存储的 jti，与 JWT claims 比对，不匹配视为吊销。 |
| `features/identity/endpoint/account_logout.rs` | 经 `token_ops::revoke_tokens` 清理缓存。 |
| `bin/server/config.rs` | 按 feature 组装 `KvBackend`（kv-pg / kv-redb / kv-redis，互斥开启其一）。 |
| `infrastructure/appctx/testing.rs` | 测试组装：固定 `KvBackend::new_for_test` 复用测试 PG 池（不随 kv-* 切换；各后端正确性由 cache crate 单测覆盖）。 |

端点经 axum `State<KvBackend>` 提取（`AppCtx` 的 `FromRef`）。

## 8. 相关路径速查

| 路径 | 职责 |
|------|------|
| `infrastructure/cache/lib.rs` | `KvBackend` 枚举 + 方法门面 |
| `infrastructure/cache/pg.rs` | `PgCache`（UNLOGGED 表） |
| `infrastructure/cache/redb.rs` | `RedbCache`（redb 4） |
| `infrastructure/cache/redis.rs` | `RedisCache`（bb8） |
| `bin/server/gc_jobs.rs` | `CacheGc` 过期清理 |
| `features/identity/shared/token_ops.rs` | Token 与缓存协作 |
| `.env.example` | `CACHE_DB_PATH` / `REDIS_URL` |
