# Slab — AI 协作上下文

DDD + 垂直切片（endpoint + repository）+ contract 公共表面。
跨域通道：只读 Port、Outbox/Inbox 事件、模块内领域事件；**例外**——跨域同事务写 Port（`audit_contract::AuditService`，动词 `record_create` / `record_updated` / `record_deleted`）。

细则按需打开，**禁止**开场把 conventions / backend / `CONTEXT.md` / skill 全文读进上下文。

## 前端（`frontend/`）

独立 SPA。约定只看 `frontend/AGENTS.md`。

## 代码导航

Grep 符号 / 字面量，Read 局部。用户已给路径则直接 Read，不要先搜。

**禁止** GitNexus：不要调用 `user-gitnexus` MCP，不要加载 `gitnexus-*` skill。同样不要用 `codebase-memory-mcp`。

公开面改动前 Grep 符号名确认调用方，调用面大时先警告再改：`*_contract`、跨域写 Port、`cross_domain/` 或被 ≥2 个 feature 依赖的 `pub` API、重命名/移动/拆分、`FILTER_SCHEMA` / 事件 payload / OpenAPI / DTO 形状。禁止 find-and-replace 重命名公开符号。

私有 `execute` / handler / 测试（不改公开签名）、注释/文案/locale/样式/文档/配置、对照 identity 蓝本新建端点：不用先扫调用方。

## 按需文档

| 场景 | 才读 |
|------|------|
| 新建域、新加 endpoint 文件、加 Job/流程/事件 | [docs/ai/backend.md](docs/ai/backend.md) 对应小节 |
| 写错误 key、HTTP 方法、可筛字段、陷阱拿不准 | [docs/ai/conventions.md](docs/ai/conventions.md) |
| 新增测试模块、某端点第一次补集成测试、写 Hurl | `.agents/skills/rust-tests/SKILL.md` |
| 用户明确要求 TDD | `.agents/skills/tdd/SKILL.md` |
| 改表格 / keep-alive / 主题，且 `frontend/AGENTS.md` 不够 | `frontend/docs/architecture.md` §5 |
| 引入或争议领域术语 | `CONTEXT.md` |

改已有 endpoint 的 `execute` / handler / 同文件加断言：本页摘要够用，不要先读 skill / conventions。

grill / wayfinder / to-spec / prototype / research / domain-modeling / codebase-design 在 `.agents/optional-skills/`，点名再读。索引：[.agents/skills-reference.md](.agents/skills-reference.md)。

## 读文件 / 命令

- 改 endpoint：Read 到 `mod tests` 之前
- **禁止**整份 Read `frontend/openapi.json` 或 `frontend/src/lib/api-schema.d.ts`（已在 `.cursorignore`）。Grep 类型名 / `jq` 单 path；刷新契约用 `pnpm gen:api`
- 大文件 Grep 符号后 Read `offset`/`limit`
- `cargo check -p <crate>`、`cargo test -p <crate> --quiet`；**禁止**无 `-p` 的 workspace test / clippy
- `just pre_commit`、`pnpm run build` 只在提交前；前端日常 `tsc --noEmit` + `pnpm run check`
- postgres-mcp / fff-mcp：用户没在查库就不要 `GetMcpTools`

## 依赖规则

```
features/{domain} (切片)
 ├──→ {domain}_contract ← 自己的公共表面
 └──→ 其他 *_contract（只读 Port）

cross_domain/（共享业务件）
 ├──→ 被 ≥2 个 feature 依赖的**业务规则**
 └──→ 现状：approval / doc_numbering / costing / inventory_ledger

禁止：contract 互依；切片依赖其他 features/{other} runtime；往 cross_domain/ 塞技术件（归 infrastructure/）
```

验证：`cargo test -p server arch_test`

## 编码摘要

- **Endpoint**：`features/{domain}/endpoint/{resource}_{action}.rs`（DTO + utoipa + handler + execute + tests）；新文件对照 `features/identity/endpoint/account_create.rs`（写）/ `account_search.rs`（列表）；禁止 `endpoint/{action}/` 子目录
- **handler / execute**：`#[tracing::instrument]`；execute 另加 `#[inline]`；有 `pg_pool` 则 `skip(pg_pool)`
- **Port** 名词（`by_id`）/ **Repository** 动词（`create`）；`conn` 第一参数；Port 单文件 `{domain}_contract/port.rs`；Repository 仅当 ≥2 个写端点共用才抽
- **错误**：`rootcause::Result` + thiserror；`#[error("snake_case_key")]`；key 必须有 en-US/zh-CN 翻译；共享 key 只放 `shared.ftl`；禁止参数化 Display、禁止 `report!()` / `from_msg()`
- **列表**：SeaQuery，禁止 NULL 哨兵；搜索用 `paginate*`（LEFT JOIN 用 `paginate_with` + 限定列；方向固定 DESC；tuple 列序 = select 列序）
- **可筛字段**：后端 `FILTER_SCHEMA`（RSQL 单参数 `filter`）→ `pnpm gen:api` → 前端 label；加实体另要 `bin/server/meta.rs` 一行
- **枚举**：`#[repr(i16)]` 必须 `serde_repr`；Contract Entity 不承载 created_at/updated_at；migration 应用后不可编辑
- **领域**：完成 / 检验结论 / 批准 不同；审批流状态 / 生命周期状态 两条时间线。术语以 `CONTEXT.md` 为准，不要每次先读

## 测试摘要

端点同文件 `mod tests`；`#[sqlx::test]` → `migration::run_migrations` → `appctx::testing::build`；先测 execute。Hurl：`just e2e`，jsonpath 用 `$.id`。

## 常用命令

| 命令 | 用途 |
|------|------|
| `cargo check -p <crate>` / `cargo test -p <crate> --quiet` | 单 crate |
| `cargo test -p server arch_test` | 架构边界（无需 DB） |
| `just e2e` / `just sqlx_up` / `just pre_commit` | E2E / 迁移 / 提交前 |
| `cd frontend && pnpm run dev` | 前端（端口 3000） |
