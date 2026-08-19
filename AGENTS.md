# Slab — AI 协作上下文

DDD + 垂直切片（endpoint + repository）+ contract 公共表面。
跨域通道：只读 Port、Outbox/Inbox 事件、模块内领域事件；**例外**——跨域同事务写 Port（`audit_contract::AuditService` 先例，动词方法 `record_create` / `record_updated` / `record_deleted`，与只读 Port 名词方法区分）。

## 前端（`frontend/`）

独立 SPA。约定只看 `frontend/AGENTS.md`，不要在本文件展开。

## 代码导航

Grep 符号 / 字面量，Read 局部。用户已给路径则直接 Read，不要先搜。

**禁止** GitNexus：不要调用 `user-gitnexus` MCP，不要加载 `gitnexus-*` skill。同样不要用 `codebase-memory-mcp`。

公开面改动前 Grep 符号名确认调用方，调用面大时先警告再改：`*_contract`（Port / Entity / Event / 领域错误）、跨域写 Port、`cross_domain/` 或被 ≥2 个 feature 依赖的 `pub` API、重命名/移动/拆分、`FILTER_SCHEMA` / 事件 payload / OpenAPI / DTO 形状。禁止 find-and-replace 重命名公开符号。

私有 `execute` / handler / 测试（不改公开签名）、注释/文案/locale/样式/文档/配置、复制 template 新建端点：不用先扫调用方。

## Skills（按需读全文）

编码约定已经在本文件。系统提示里的 skill **description** 够用来决定要不要打开；**禁止**开场把多份 skill / `CONTEXT.md` / architecture 全文读进上下文。

| 场景 | 才读 |
|------|------|
| 新建域、新加 endpoint 文件、加 Job/流程/事件 | `docs/ai/backend.md`（对应小节） |
| 新增测试模块、某端点第一次补集成测试、写 Hurl | `.agents/skills/rust-tests/SKILL.md` |
| 用户明确要求 TDD | `.agents/skills/tdd/SKILL.md` |
| 改表格 / keep-alive / 主题，且 `frontend/AGENTS.md` 陷阱不够 | `frontend/docs/architecture.md` §5 |
| 引入或争议领域术语 | `CONTEXT.md` |

改已有 endpoint 的局部实现、在已有 `mod tests` 里加断言：不要先读 skill。

grill / wayfinder / to-spec 等流程 skill 在 `.agents/optional-skills/`，**不自动加载**；用户点名时再读。

## 读文件（省 token）

- 改 endpoint 实现：Read 到 `mod tests` 之前，不要整文件（测试块大约占一半）
- **禁止**整份 Read `frontend/openapi.json`（234KB）或 `frontend/src/lib/api-schema.d.ts`（130KB）。Grep 类型名 / `jq` 单 path；刷新契约用 `pnpm gen:api`
- 大文件（如 `infrastructure/job_queue/lib.rs`）：Grep 符号后 Read `offset`/`limit`，不要整份
- 加 Job / 流程 / 事件 / 新域：再读 [docs/ai/backend.md](docs/ai/backend.md) 对应小节

## 命令输出

- `cargo check -p <crate>`、`cargo test -p <crate> --quiet`；**禁止**无 `-p` 的 workspace test / clippy
- `just pre_commit` 只在提交前
- 前端日常：`tsc --noEmit` + `pnpm run check`；`pnpm run build` 只在提交前
- postgres-mcp / fff-mcp：用户没在查库就不要 `GetMcpTools`

## 依赖规则

```
features/{domain} (切片)
 ├──→ {domain}_contract ← 自己的公共表面
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
- 领域术语以根目录 `CONTEXT.md` 为准；**不要每次先读**，只在引入或争议术语时打开。**完成 / 检验结论 / 批准** 是三个不同概念；**审批流状态 / 生命周期状态** 是两条独立时间线；新术语定案时更新 `CONTEXT.md`
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

## 常用命令

| 命令 | 用途 |
|------|------|
| `cargo check -p <crate>` | 单 crate 编译 |
| `cargo test -p <crate> --quiet` | 单 crate 测试（不要去掉 `-p` / `--quiet`） |
| `cargo test -p server arch_test` | 架构边界检查（无需 DB） |
| `just pre_commit` | 提交前：machete → cargo-sort → fmt → clippy |
| `just e2e` | Hurl E2E |
| `just sqlx_up` | 迁移数据库 |
| `cd frontend && pnpm run dev` | 前端开发服务器（端口 3000） |
