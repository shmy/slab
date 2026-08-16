# Slab — AI 协作上下文

DDD + 垂直切片（endpoint + repository）+ contract 公共表面。
跨域通道：只读 Port、Outbox/Inbox 事件、模块内领域事件；**例外**——跨域同事务写 Port（`audit_contract::AuditService` 先例，动词方法 `record_create` / `record_updated` / `record_deleted`，与只读 Port 名词方法区分）。

## 前端（`frontend/`）

管理后台 SPA：**React 19 + Rsbuild + TanStack Router/Store/Table v9 + shadcn/ui（base-nova，@base-ui/react）+ Tailwind 4（Nord 语义变量主题）**。独立于后端 workspace（pnpm 管理，开发端口 3000）。

- 前端上下文以 `frontend/AGENTS.md` 为准；改表格 / keep-alive / 主题前必读 `frontend/docs/architecture.md`（§5 踩坑记录）
- 文件式路由 `frontend/src/routes/`，`routeTree.gen.ts` 自动生成（禁手改）；布局 `_app.tsx` = 登录守卫 + 侧边栏 + 顶栏 + PageTabs + KeepAliveOutlet
- 多标签页 / keep-alive：`components/PageTabs.tsx` + `components/keep-alive.tsx`（Suspense 挂起冻结，**勿换回 `<Activity>`**）；页面用 `staticData: { keepAlive: true }` 参与缓存
- 表格：`components/VirtualTable.tsx`（TanStack Table v9 `useTable` + 虚拟滚动）；`VirtualTable.tsx` / `users.tsx` 顶部 `'use no memo'` 豁免（React Compiler 冲突）**不可删**
- 状态：`@tanstack/react-store`，`store/`（auth/theme/fontSize/sidebar/tabs）；登录已接后端 `identity`（xior，Bearer JWT + 401 单飞刷新；dev 走 rsbuild proxy `/api` → `127.0.0.1:8081`）
- 命令：`pnpm run dev` / `typecheck` / `check` / `build`；改完跑 `pnpm exec tsc --noEmit && pnpm run check && pnpm run build`

## 代码导航

| 想做什么 | 用什么 |
|---------|--------|
| 自然语言搜符号/函数 | `search_graph` query="xxx" |
| 追踪调用链（谁调了它/它调了谁） | `trace_path` |
| 模块架构/热点概览 | `get_architecture` |
| 按限定名读源码 | `get_code_snippet` |
| 复杂图查询 | `query_graph` (Cypher) |
| 文本字面搜索 | `search_code` (graph-augmented grep) |
| 建立索引 | `index_repository` |

> 上表工具来自 `codebase-memory-mcp`（MCP 网关），调用名带前缀，如 `mcp({ tool: "codebase-memory-mcp_search_graph", args: {...} })`；服务器未连接时先 `mcp({ connect: "codebase-memory-mcp" })`。批量调用可用 `mcpScript`。

## 依赖规则

```
features/{domain} (切片)
    ├──→ {domain}_contract           ← 自己的公共表面
    └──→ 其他 *_contract（只读 Port）

cross_domain/（共享业务件，跨域通道的例外栖息地）
    ├──→ 被 ≥2 个 feature 依赖的**业务规则**（非技术件）
    ├──→ 不走 contract 只读 Port / 事件（等同事务写 Port 例外，如 inventory_ledger 被 4 域直写）
    └──→ 现状：approval / doc_numbering / costing / inventory_ledger

禁止：
✗ contract 之间互相依赖
✗ 切片依赖其他 features/{other} runtime crate
✗ 往 cross_domain/ 塞技术件（可替换、无业务词汇的件归 infrastructure/）
```

验证：`cargo test -p server arch_test`

## 编码约定

### Endpoint
- 单文件 `features/{domain}/endpoint/{resource}_{action}.rs`：DTO + `#[utoipa::path]` + handler + execute + tests
- 新端点复制 `docs/templates/endpoint_template.rs`，不要复制已有端点
- handler + execute 加 `#[tracing::instrument]`，execute 另加 `#[inline]`
- 含 `pg_pool` 参数的 handler/execute 统一 `#[tracing::instrument(skip(pg_pool))]`，避免 `Pool {...}` 大对象刷屏（保留 path/request 等业务字段）
- 禁止拆 `endpoint/{action}/` 子目录

### Port / Repository
- Port（跨域只读）：`{Domain}Port`，方法名词（`by_id`）
- Repository（本域写库）：`{Aggregate}Repository`，方法动词（`create`、`update_status`）
- 参数顺序：`conn: &mut PgConnection`（或 `tx.as_mut()`）→ 业务参数
- Port 统一放单文件 `{domain}_contract/port.rs`（port 与其专属值对象同文件），**不建** `port/` 子目录
- Repository **按需创建**：仅当同一域 ≥2 个写端点共用同类 SQL / 变更逻辑时抽；**禁止空占位**（如只有一行注释的 `repository.rs`）

### 错误
- `rootcause::Result<T>`
- 领域错误用 `thiserror` 枚举，**禁止** `report!()` / `from_msg()` 等 ad-hoc 错误
- `#[error("...")]` 消息必须是 **snake_case key**（`^[a-z0-9_]+$`），如 `#[error("purchase_order_not_found")]`；句子风格、空格、冒号一律禁止（locale 测试结构性扫描强制）
- 每个 key 必须在 `infrastructure/locale/locales/{en-US,zh-CN}/` 有翻译；跨域共享的 key（如 `invalid_status_transition`）只放 `shared.ftl`，**禁止**在多个 ftl 重复定义（Fluent bundle 重复 key 会 panic）
- **禁止参数化消息**（含 `{`）：参数细节进字段供日志/调试，不进 Display；仅内部库（`libs/image_kit`、`libs/authz_kit`）豁免（内部故障永远 500，不进 locale）
  - 例外：web 层参数解析 rejection 的 detail 允许 Fluent 参数（`{ $field }`，字段路径来自 `serde_path_to_error`（body/query）/ axum `ErrorKind`（path）/ 结构化 multipart 错误），属 locale 渲染层插值，不违反 Rust Display 参数化禁令
- **禁止字符串参数当错误区分器**：`InvalidStatus("need at least one line")` 是反模式——一个语义一个变体（拆成 `EmptyOrder` 等）
- HTTP 语义（`web::error` 自动处理）：key → 400（特例：`access_token_*` → 401、`*_version_conflict` → 409、`internal_server_error` → 500）；非 key（内部故障）→ 500
- **禁止 `#[allow(clippy::expect_used)]` / `#[allow(clippy::unwrap_used)]` 等 lint 豁免注解**：用构造期校验（`Result` 上下文）、无 Option 的 API、或字段缓存替代

### 领域语言
- 领域术语以根目录 `CONTEXT.md` 为准（统一语言词汇表）：**完成 / 检验结论 / 批准** 是三个不同概念；**审批流状态 / 生命周期状态** 是两条独立时间线；新术语定案时更新 `CONTEXT.md`
- 架构讨论用深模块词汇：模块 / 接口 / 深度 / 接缝 / 适配器 / 杠杆 / 局部性（见 `.agents/skills/codebase-design`）

### 常见陷阱
- `#[repr(i16)]` 枚举 → 必须 `serde_repr`，不是普通 `#[derive(Deserialize)]`
- 自引用结构（`children: Vec<Self>`）→ 加 `#[schema(no_recursion)]`，否则 utoipa 栈溢出
- 单 `routes!()` 内不能有两个相同 HTTP method 的 handler
- 列表查询用 SeaQuery，禁止 NULL 哨兵
- 搜索端点用 `shared_contract::query::cursor_page::paginate*`（keyset 游标分页深模块，勿再内联游标/排序/limit+1 样板；LEFT JOIN 场景用 `paginate_with` + 限定列，见 [docs/adr/0002](docs/adr/0002-cursor-pagination.md)）
- 加可筛字段：后端 `FILTER_SCHEMA` 一处声明（`pub` 导出，供 meta 端点收集）→ 重新 `pnpm gen:api` → 前端 label 映射补一行（`satisfies Record<XxxFilterField, ...>` 缺则 tsc 报错）；加可筛实体另需 `bin/server/meta.rs` 注册一行
- Contract Entity 不承载 created_at / updated_at
- `#[derive(Validify)]` 文件不要写 `use rootcause::Result`
- migration 文件应用后不可编辑，改 schema 新建下一个版本
- 响应是扁平 JSON，Hurl jsonpath 用 `$.id` 不是 `$.data.id`

## 测试

### 集成测试
- 端点同文件 `#[cfg(test)] mod tests`
- `#[sqlx::test]` → `migration::run_migrations(&pool)` → `appctx::testing::build(pool)`
- 先测 execute 再测 handler

### Hurl E2E
- `just e2e` → 分文件顺序执行，间隔 2s 防 429
- 变量文件 `e2e/env`，调试验证用 `--test --variables-file`

## 新增功能

```
├── 已有域加端点 → 复制 docs/templates/endpoint_template.rs → 改 DTO/execute/SQL → 注册 mod
├── 新建域 → contract（实体/Port/事件/错误）→ runtime（端点/仓储/lib.rs）→ workspace + modules.rs
├── 跨域读 → import {other}_contract::port::{Domain}Port（禁止 import features/{other}/*）
├── 写端点接入变更历史 → 同事务调 `audit_contract::AuditService`（`record_create` / `record_updated` / `record_deleted`，传 `txn` + `&Operator` + before/after 实体；漏接不报编译错，端点测试断言 `audit_logs` 兜底；identity 三端点作示范）
├── 加事件 → contract/events.rs 实现 `shared_contract::event::Event` + subscriber/ + Module::register + publish
├── 加流程 → infrastructure/flow 定义 `#[task]` + `workflow!`，AppCtx.flow.run/resume（见 [docs/FLOW.md](docs/FLOW.md)）
├── 加 Job → 域内定义 `Job` trait 实现（payload 即类型）+ handler `async fn(T, &AppCtx)`，
│   在 `DomainModule::register` 里 `r.jobs.register::<T, _>(|job, ctx| Box::pin(handler(job, ctx)))`，
│   端点入队 `state.jobs.enqueue(T { .. }).await?`（见 [docs/JOB_QUEUE.md](docs/JOB_QUEUE.md)）
├── 加周期任务 → 同加 Job 定义 `Job` + handler，再在 `register` 里一行 `r.scheduled("0 0 3 * * *", T { .. })`
│   （cron 到点 enqueue，执行语义归 job_queue；触发 master-only，执行多进程竞争）
├── 改 DB → infrastructure/migration/versions/ 新 .sql
```

## 关键基础设施

| Crate | 用途 |
|-------|------|
| `infrastructure/db` | PgPool |
| `infrastructure/event_bus` | 事件总线（广播事件投递；Pg Outbox 默认 / NATS JetStream，feature 切换）→ [docs/EVENT_BUS.md](docs/EVENT_BUS.md) |
| `infrastructure/flow` | sayiir 持久化工作流（长流程/信号/超时编排）→ [docs/FLOW.md](docs/FLOW.md) |
| `infrastructure/kv` | 可插拔 KV 缓存后端（Pg UNLOGGED 默认 / redb / redis，feature 切换）→ [docs/KV.md](docs/KV.md) |
| `infrastructure/job_queue` | 后台任务队列（点对点命令式 Job：入队/延迟/重试退避/超时/终态；pg 默认 / sqlite 单机，feature 切换；自研 sqlx 0.9，无 Apalis）→ [docs/JOB_QUEUE.md](docs/JOB_QUEUE.md) |
| `infrastructure/web` | ValidJson / ValidQuery / ValidPath + Problem Details |
| `infrastructure/http_auth` | Bearer JWT 鉴权中间件 |
| `infrastructure/locale` | Fluent 本地化中间件 |
| `libs/authn_kit` | JWT 访问令牌提取与缓存 key 构建（auth 缓存 key 方案） |
| `libs/authz_kit` | Cedar 授权策略评估（待接入，暂无调用方） |
| `libs/trace_kit` | OpenTelemetry |
| `libs/filter_kit` | PostgREST 风格筛选解析（`name=ilike.*张*&created_at=gt.2024-03-15`，字段白名单防注入）；操作符矩阵为协议事实源，经 `GET /api/v1/meta/filter-schemas` 导出 → 前端 `pnpm gen:api` 生成 `src/lib/filter-schema.ts`（勿手抄，见 [docs/adr/0003](docs/adr/0003-filter-schema-contract.md)） |
| `libs/sched_kit` | tokio-cron-scheduler |
| `shared_contract` | ID、keyset 游标分页（`query::cursor_page::{paginate,paginate_with}`）、PhoneNumber 等共享值对象 + `event::Event`（跨域事件 trait） |

## 常用命令

| 命令 | 用途 |
|------|------|
| `cargo check -p <crate>` | 单 crate 编译（日常开发首选） |
| `cargo test -p <crate>` | 单 crate 测试 |
| `cargo test -p server arch_test` | 架构边界检查（无需 DB） |
| `just pre_commit` | machete → cargo-sort → fmt → clippy |
| `just e2e` | Hurl E2E |
| `just sqlx_up` | 迁移数据库 |
| `cd frontend && pnpm run dev` | 前端开发服务器（端口 3000） |
