# 编码细则（AI 按需）

根目录 `AGENTS.md` 是 always-on 摘要。**本文件不要每轮先读**——写错误 key、HTTP 方法、可筛字段、或踩到下列陷阱时打开对应小节。

切片落位、新建域/Job/流程：见 [backend.md](backend.md)。

## Endpoint

- 单文件 `features/{domain}/endpoint/{resource}_{action}.rs`：DTO + `#[utoipa::path]` + handler + execute + tests
- 新端点对照 `features/identity/endpoint/account_create.rs`（写）/ `account_search.rs`（列表），不要从别的域抄带领域噪音的端点
- handler + execute 加 `#[tracing::instrument]`，execute 另加 `#[inline]`
- 含 `pg_pool` 的 handler/execute 统一 `#[tracing::instrument(skip(pg_pool))]`（保留 path/request 等业务字段）
- 禁止拆 `endpoint/{action}/` 子目录
- 部分字段更新默认 **`PATCH`**；`PUT` 仅整体替换。状态动作用 `POST .../{id}/submit` 等子路径

## Port / Repository

- Port（跨域只读）：`{Domain}Port`，方法名词（`by_id`）
- Repository（本域写库）：`{Aggregate}Repository`，方法动词（`create`、`update_status`）
- 参数顺序：`conn: &mut PgConnection`（或 `tx.as_mut()`）→ 业务参数
- Port 统一放单文件 `{domain}_contract/port.rs`（与专属值对象同文件），**不建** `port/` 子目录
- Repository **按需创建**：同一域 ≥2 个写端点共用同类 SQL / 变更逻辑时才抽；禁止空占位

## 错误

- `rootcause::Result<T>`
- 领域错误用 `thiserror` 枚举，**禁止** `report!()` / `from_msg()` 等 ad-hoc 错误
- `#[error("...")]` 必须是 **snake_case key**（`^[a-z0-9_]+$`），如 `#[error("purchase_order_not_found")]`；句子、空格、冒号一律禁止（locale 测试结构性扫描强制）
- 每个 key 必须在 `infrastructure/locale/locales/{en-US,zh-CN}/` 有翻译；跨域共享 key（如 `invalid_status_transition`）只放 `shared.ftl`，禁止多 ftl 重复定义（Fluent bundle 重复 key 会 panic）
- **禁止参数化消息**（含 `{`）：细节进字段供日志，不进 Display。仅 `libs/image_kit`、`libs/authz_kit` 豁免（内部故障永远 500，不进 locale）
  - 例外：web 层参数解析 rejection 的 detail 允许 Fluent 参数（`{ $field }`，路径来自 `serde_path_to_error` / axum `ErrorKind` / 结构化 multipart），属 locale 渲染层，不违反 Rust Display 禁令
- **禁止字符串参数当错误区分器**：`InvalidStatus("need at least one line")` 是反模式——一个语义一个变体
- HTTP：key → 400（`access_token_*` → 401、`*_version_conflict` → 409、`internal_server_error` → 500）；非 key → 500
- **禁止** `#[allow(clippy::expect_used)]` / `#[allow(clippy::unwrap_used)]`

## 陷阱

- `#[repr(i16)]` 枚举 → 必须 `serde_repr`，不是普通 `Deserialize`
- 自引用 `children: Vec<Self>` → `#[schema(no_recursion)]`，否则 utoipa 栈溢出
- 单 `routes!()` 内不能有两个相同 HTTP method 的 handler
- 列表查询用 SeaQuery，禁止 NULL 哨兵
- 搜索：`paginate(conn, select, paging, "id")`；LEFT JOIN 用 `paginate_with` + 限定列 `("table", "id")`。方向固定 DESC——升序/复合键是新函数，不要给 `paginate` 加参数。`paginate_with` 的 tuple 列序必须等于 select 列序。不要在端点内联 keyset / limit+1
- 筛选事实源在后端：`FILTER_SCHEMA`（`pub`）+ `filter_kit`（RSQL 单参数 `filter`）。前端不要手抄操作符矩阵。加字段：`FILTER_SCHEMA` → `pnpm gen:api` → 前端 label 映射一行（`satisfies Record<XxxFilterField, ...>`）；加可筛实体另要 `bin/server/meta.rs` 注册。FilterBar 用生成契约，不要运行时拉 meta
- Contract Entity 不承载 created_at / updated_at
- `#[derive(Validify)]` 文件不要写 `use rootcause::Result`
- migration 应用后不可编辑，改 schema 新建下一个版本
- 响应扁平 JSON，Hurl jsonpath 用 `$.id` 不是 `$.data.id`

## 测试

- 端点同文件 `#[cfg(test)] mod tests`
- `#[sqlx::test]` → `migration::run_migrations(&pool)` → `appctx::testing::build(pool)`
- 先测 execute 再测 handler
- Hurl：`just e2e`，间隔 2s 防 429；变量 `e2e/env`。编写规范见 [E2E_HURL.md](../E2E_HURL.md)

## 领域语言

术语以根目录 `CONTEXT.md` 为准，不要每次先读。完成 / 检验结论 / 批准 是三个不同概念；审批流状态 / 生命周期状态 是两条独立时间线。架构讨论用深模块词汇（`.agents/optional-skills/codebase-design`）。
