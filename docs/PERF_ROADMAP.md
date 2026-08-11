# Slab 性能底座路线图

> 记录于 2026-08-11。目标：为模块化单体搭**可测量、可扩展、可运维**的高性能底座。
> 部署形态：**Dokploy（Traefik 反代 + 自动 HTTPS）** —— TLS 与 HTTP/2 终止在代理层，后端 http1.1 明文，**无需服务端 h2**。

## 一、现状基线（已具备）

| 维度 | 现状 |
|---|---|
| 运行时 | Tokio multi-thread + tower 中间件链，mimalloc 全局分配器 |
| 编译产物 | release：LTO fat + codegen-units 1 + strip；dev：line-tables + sccache |
| DB 竞争 | job_queue 用 `FOR UPDATE SKIP LOCKED` 竞争安全拉取，内建孤儿恢复 |
| 一致性 | 事件走 Pg Outbox 同事务（先保一致，吞吐后调） |
| 可插拔后端 | kv / job_queue / event_bus / blob 全 feature 切换，换高性能后端零改业务 |
| 限流/超时 | axum-governor 50rps（进程内）+ TimeoutLayer |
| 外呼 | http_client：reqwest + retry（ExponentialBackoff）+ 超时 |
| 观测管线 | OTLP trace / log / **metrics 三个 provider 已接线**（但 metrics 零埋点） |
| 架构接缝 | 10 个只读 Port → 读副本路由有天然接缝 |

## 二、P0 缺口（先有数据，再谈优化）

1. **Metrics 埋点 ✅（2026-08-11 已落地）** — 全仓从 0 到有：
   - `http.server.request.duration` 直方图（axum 中间件，属性 method / route 模板 / status）
   - `db.query.duration` 直方图（`SqlxQueryMetricsLayer` 裸 Layer + event_enabled 自过滤，消费 `sqlx::query` 日志事件的 `elapsed_secs`；db 池开 `log_statements(Info)`，日志噪音由 EnvFilter `sqlx::query=off` 屏蔽）
   - `job_queue.pending / running / failed` + `event_bus.pending / failed / deliveries.pending` gauge（`BacklogMetrics` 周期任务每 30s 采样，调 `JobBus::backlog` / `EventBus::backlog` crate API，不碰表）
   - `db.pool.connections / idle` gauge（sqlx 官方 `Pool::size()` / `num_idle()`，对应 issue #1896 的 USE 指标诉求——官方 API 直接采样，无需等官方落地）
   - **出口：OTLP/HTTP（protobuf，OpenObserve 官方推荐）**——OpenObserve 对 OTLP **gRPC** 直方图存 0 事件（issue #12345，修复 #12615 仅覆盖 OTLP/JSON），HTTP 路径原生正常；端点 `{OTLP_ENDPOINT}/api/{org}/v1/{traces,metrics,logs}`（无尾斜杠，exporter 自动追加 `/v1/...`；认证复用 OTLP_METADATA 的 authorization）。注意 self-hosted 的 HTTP 端口是 **5080**（5081 是 gRPC）
   - 实现要点：trace_kit 注册全局 meter provider；EnvFilter 改为各日志层 per-layer Filter（避免 `Layered::enabled` AND 链掐断指标事件）；sqlx 指标层为裸 Layer + `event_enabled`/`on_event` 自过滤、注册于 EnvFilter 之前
2. **响应压缩** — Traefik 层配 gzip（省事）或 tower-http `CompressionLayer`，二选一。JSON API 可压缩 5-10x。
3. **压测基线** — k6/wrk 打核心链路（登录、列表分页、单据创建），定 QPS/p99 预算，进 CI 防回归。
4. **DB 慢 SQL 盲区** — docker-compose postgres 加 `shared_preload_libraries=pg_stat_statements`（+auto_explain），先看到最慢 SQL。

## 三、P1（规模化必需）

5. **进程内 L1 缓存** — kv 每次都是 DB/网络往返；热点只读数据（账号、目录、权限）缺 quick_cache/moka 内存层 + 失效协议。
6. **PgBouncer** — sqlx 池是应用侧；多实例高并发时 Postgres 连接数是硬瓶颈，缺 server 侧事务级复用。
7. **读副本路由** — 只读 Port 接缝已就绪，缺连接层主从分流（sqlx 无内建，需自研）。
8. **分布式锁** — cron 靠 `SERVER_MASTER` 静态标志（单点）；多实例互斥任务缺 PG advisory lock / Redis SET NX。
9. **背压** — tower concurrent-limit（注释标注"从未启用"）；job 积压深度指标 + 告警。

## 四、P2（锦上添花）

10. outbox 轮询批大小/背压调优。
11. CDC 评估（wal2json / Debezium vs outbox，高吞吐场景）。
12. 大表分区（inventory_ledger 被 4 域直写，最该分区；pg_partman 维护）。
13. tokio-console（运行时内省）+ pprof / DHAT（CPU/内存剖析）。
14. 前端产物接线（axum-embed 已声明未用）。

## 五、扩展：通用底座组件（与业务无关）

### 韧性 Resilience
- **熔断/舱壁** — http_client 已有重试，补熔断（自研或 failsafe）；对 COS/NATS 等外部依赖做隔离。
- **写端点幂等** — 客户端 `Idempotency-Key` + PG 唯一约束，防重试造成重复单据。
- **请求 ID 贯穿** — OtelInResponseLayer 已回带 traceparent；可加 `X-Request-ID` 回显便于排障。

### 可观测性 Observability
- **Grafana 面板 + 告警规则** — 现状只有 OTLP push，无面板无告警，是最先要补的观测闭环。
- **结构化 JSON 日志** — 现状 fmt+ansi，容器环境 JSON 更利于采集检索。
- **采样策略配置化** — 已有 RandomIdGenerator，可加比率/tail 采样 + 采样率配置。
- **tokio-console / pprof / DHAT** — 运行时内省 + CPU/内存剖析。

### 数据层 Data
- **备份 / PITR** — 现状裸 docker volume，**无任何备份**（运维硬缺口）：wal-g / pgBackRest + 归档。
- **HA / 故障转移** — 现状单实例；规模化后 Patroni / repmgr。
- **全文检索** — 起点用 PG FTS；规模后 Meilisearch / OpenSearch。
- **OLAP** — 现阶段 PG 直查；报表量级上来后 ClickHouse。
- **pg_partman** — 分区自动维护。

### 安全 Security
- **密钥管理** — 现状 `.env` 明文；迁 SOPS / Vault / 云 Secrets。
- **安全响应头** — CSP / HSTS / X-Content-Type-Options（tower-http set-header 全家桶）。
- **字段级加密** — pgcrypto / tink（敏感字段落库加密）。
- **日志/响应脱敏** — 敏感字段不进日志与错误响应。

### 交付/运维 Delivery
- **零停机迁移策略** — 现状启动时 `run_migrations`，大表 DDL 会锁表；评估分批/锁超时/在线变更。
- **功能开关 Feature Flag** — 轻量自研（配置表 + KV 缓存）或 unleash；灰度发布配套。
- **蓝绿 / 金丝雀** — Dokploy 原生支持，无需自建。
- **契约测试** — 暂无外部 API 消费方，缓。

### 中间件/协议 Middleware
- **SSE / WebSocket** — axum 原生支持，全仓未用；实时进度/通知可用。
- **流式上传/下载** — Range 下载、分片/断点续传（file 域 blob 之上）。
- **HTTP 缓存** — ETag / Cache-Control（只读 GET 端点）。
- **批处理 / 嵌套 query** — `serde_qs` 已声明未用，过滤参数场景可启用。

### 事件/消息 Events
- **死信策略** — outbox 投递重试上限 + DLQ 落表可查。
- **事件版本化 / schema 演进** — contract 事件字段演进策略。
- **幂等消费** — event handler 幂等 key。

### 已声明未接线（workspace deps，0 使用）
| 依赖 | 用途 |
|---|---|
| `axum-embed` | 前端产物嵌入单二进制（dist 未托管） |

## 六、落地顺序

```
第 1 步：Metrics 埋点（请求延迟 + sqlx 耗时 + 队列/outbox 积压）✅ 已完成
第 2 步：pg_stat_statements + Traefik gzip 压缩
第 3 步：k6 压测基线 + QPS/p99 预算（3-5 条核心链路）
第 4 步：备份（wal-g/pgBackRest）+ 安全响应头
第 5 步：L1 缓存 → PgBouncer → 分布式锁（多实例化）
```

**原则**：先能测量（metrics/压测/慢 SQL），再谈优化；一切加装件保持 feature 可插拔，不侵入业务切片。
