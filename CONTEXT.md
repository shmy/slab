# 游标分页深度化 —— shared_contract::query 接口设计

## 0. 现状与问题

9 个搜索端点（customer / audit / item / account / purchase_order / supplier / payment / inventory / inventory_transaction）各自内联同一段 keyset 分页中间件：

1. 游标条件 `paging.cursor_id().map(|c| Expr::col("id").lt(*c))`
2. `order_by("id", Order::Desc)`
3. `limit(paging.fetch_limit())`（limit+1 多取一条）
4. `build_sqlx(PostgresQueryBuilder)` → acquire → `fetch_all`
5. `finalize_cursor_page(items, page_limit, |item| item.id)`

共享层（`features/shared_contract/query/`，共 141 行）只做 parse（`paging_query.rs`）与 finalize（`cursor_page.rs`），中间三段 SQL 全部留在端点。拼写已经漂移：

| 漂移点 | 位置 | 写法 |
|---|---|---|
| 限定列 | `features/audit/endpoint/audit_search.rs:114-121` | `Expr::col(("audit_logs","id"))`（LEFT JOIN 必需） |
| 本地变量 | `features/identity/endpoint/account_search.rs:78-80`、`features/item/endpoint/item_search.rs:78-80` | `input_next_cursor` |
| 手工 cast | `features/warehouse/endpoint/inventory_search.rs:58-59` | `Expr::cust("CAST(quantity AS DOUBLE PRECISION)")` |
| 映射形态 | `audit_search.rs:103-128` | tuple 行 + diff 映射；其余 8 处 FromRow + identity |

每个端点约 8-9 行分页样板，合起来是 9 份几乎相同、细节各异的拷贝。

## 1. 接口设计（两阶入口）

模块：`shared_contract::query::cursor_page`（扩展现有 `cursor_page.rs`，`finalize_cursor_page` 删除/内部化；`CursorPagingResult` 留在 `paging_result.rs` 不动）。

```rust
/// 快路径（7/8 调用点）：单表 + FromRow 行类型 + 游标列 = "id"。mapper 为内置恒等
/// 映射：每行按名反序列化 T，游标 id 从追加的别名列读取。
pub async fn paginate<T, C>(
    conn: &mut PgConnection,        // 仓库参数约定：conn → 业务参数
    mut select: SelectStatement,    // 调用方已建好 FROM/JOIN/列/业务筛选；模块按值接管
    paging: &CursorPagingQuery,     // 解析好的分页参数
    cursor_col: C,                  // IntoColumnRef："id" 或 ("audit_logs","id")
) -> rootcause::Result<CursorPagingResult<T>>
where
    T: for<'r> FromRow<'r, PgRow> + Serialize + ToSchema,
    C: IntoColumnRef,

/// 显式路径（audit）：原始行 R（tuple 等）+ 映射闭包 → (T, ID)。ID 由映射器给出，
/// 模块据此算 next_cursor，不碰 R 内部结构。
pub async fn paginate_with<T, R, C, F>(
    conn: &mut PgConnection,
    mut select: SelectStatement,
    paging: &CursorPagingQuery,
    cursor_col: C,
    map: F,                         // FnMut(R) -> rootcause::Result<(T, ID)>
) -> rootcause::Result<CursorPagingResult<T>>
where
    T: Serialize + ToSchema,
    R: for<'r> FromRow<'r, PgRow>,
    C: IntoColumnRef,
    F: FnMut(R) -> rootcause::Result<(T, ID)>,
```

两条入口共享私有 `build_keyset`（append 游标条件 + 排序 + limit + 别名列 + build_sqlx）与私有 `finalize`（has_more + 弹行 + next_cursor）。

### 参数与不变量

| 参数 | 语义 | 不变量 |
|---|---|---|
| `conn` | 执行入口 `&mut PgConnection` | 与 Repository 参数顺序约定一致（conn → 业务参数），可参与事务 |
| `select` | 调用方的查询（列 + FROM/JOIN + 业务 WHERE） | `paginate`：必须含 T 的全部列（sqlx 按名匹配）；`paginate_with`：R 的列序 = select 列序（tuple 按位解码） |
| `paging` | 已解析分页参数 | `limit` clamp 1..=100（复用 `CursorPagingQuery::limit()`） |
| `cursor_col` | 游标列，`IntoColumnRef` | 必须 FROM/JOIN 可达；LEFT JOIN 场景必须传限定列 `(table, col)` 防列歧义；值须单调递增（tsid 语义，keyset 正确性前置，注释写明，不强制） |
| `map` | `R -> (T, ID)` | 返回的 ID 用于 next_cursor，须与游标列值一致（约定；audit 的 `id` 即 `audit_logs.id`） |

方向固定 **Desc**（无参数）。升序是未来另一个函数，不是本函数的一个参数——接口不为此膨胀。

### 错误模式

- 返回 `rootcause::Result<CursorPagingResult<T>>`；sqlx 错误直接透传（`web::error` → 500 `internal_server_error`）
- 映射器闭包签名保留 `Result` 通道（audit 的 diff 映射当前不抛错，但领域错误可经此上抛 → 按 key 映射 HTTP）
- parse 层错误不归本模块：仍由 `CursorPagingQuery` 反序列化 + `web::ValidQuery` 处理（400）

## 2. 用法示例

### 2.1 客户列表（common case，改写 `features/customer/endpoint/customer_search.rs`）

```rust
#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchCustomerQuery,
) -> rootcause::Result<CursorPagingResult<SearchCustomerItem>> {
    let q = query.q.filter(|s| !s.is_empty());

    // 业务查询：列 + 搜索词 + 软删除。分页中间件不再出现在这里。
    let mut select = Query::select()
        .from("customers")
        .column("id")
        .column("code")
        .column("name")
        .column("is_active")
        .column("phone")
        .column("contact_person")
        .column("created_at")
        .and_where_option(q.map(|q| {
            Expr::col("code")
                .ilike(format!("%{q}%"))
                .or(Expr::col("name").ilike(format!("%{q}%")))
                .or(Expr::col("phone").ilike(format!("%{q}%")))
                .or(Expr::col("contact_person").ilike(format!("%{q}%")))
        }))
        .and_where(Expr::col("is_active").eq(true))
        .to_owned();

    if let Some(expr) = filter_kit::filter_where(query.filter.as_deref(), &FILTER_SCHEMA)? {
        select.and_where(expr);
    }

    let mut conn = pg_pool.acquire().await?;
    cursor_page::paginate(&mut conn, select, &query.paging, "id").await
}
```

消失的样板：`cursor_id().map(...)` 游标子句、`order_by("id", Desc)`、`limit(fetch_limit())`、`build_sqlx`、`fetch_all`、`finalize_cursor_page(...)`、`let page_limit = query.paging.limit();` —— 共 8-9 行，换成 1 行 `paginate(...)`。`T` 由 execute 返回类型推断，无需 turbofish。

### 2.2 变更历史（exotic，改写 `features/audit/endpoint/audit_search.rs`）

```rust
#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchAuditQuery,
) -> rootcause::Result<CursorPagingResult<AuditLogItem>> {
    let select = Query::select()
        .column(("audit_logs", "id"))
        .column(("audit_logs", "before"))
        .column(("audit_logs", "after"))
        .column(("audit_logs", "operator_id"))
        .column(("accounts", "name"))
        .column(("audit_logs", "created_at"))
        .from("audit_logs")
        .left_join(
            "accounts",
            Expr::col(("audit_logs", "operator_id")).equals(("accounts", "id")),
        )
        .and_where(Expr::col(("audit_logs", "entity")).eq(&query.entity))
        .and_where(Expr::col(("audit_logs", "entity_id")).eq(*query.entity_id))
        .to_owned();

    let mut conn = pg_pool.acquire().await?;
    cursor_page::paginate_with(
        &mut conn,
        select,
        &query.paging,
        ("audit_logs", "id"),
        |(id, before, after, operator_id, operator_name, created_at)| {
            Ok((
                AuditLogItem {
                    id: ID::from(id),
                    change_type: match (&before, &after) {
                        (None, Some(_)) => "create",
                        (Some(_), None) => "delete",
                        _ => "update",
                    }
                    .to_string(),
                    diff: json_diff(before.as_ref(), after.as_ref()),
                    operator_id: ID::from(operator_id),
                    operator_name,
                    created_at,
                },
                ID::from(id),
            ))
        },
    )
    .await
}
```

消失的样板：限定列游标子句（3 行）、`order_by(("audit_logs","id"), Desc)`、`limit`、`build_sqlx`、acquire/fetch、独立的 `rows.into_iter().map(...).collect()`、`finalize_cursor_page(...)`。audit 付出的唯一额外代价：mapper 闭包返回 `(T, ID)` 元组（2 个字符组件的差别）。**diff 映射留在端点**——它是业务，模块不该拥有。

新端点学习模式：3 行（`select` 建查询 → `acquire` → `paginate(&mut conn, select, &query.paging, "id")`）。

## 3. 接缝后隐藏的行为清单

模块拥有 keyset 分页的**全生命周期**：

1. 游标条件：`paging.cursor_id()` → `and_where_option(cursor.map(|c| Expr::col(col.clone()).lt(*c)))`
2. 排序：`order_by(col, Order::Desc)`（固定方向）
3. 条数：内部 `limit = paging.limit()`（clamp 1..=100），`LIMIT limit + 1`
4. 追加游标读取列：`expr_as(Expr::col(col.clone()), "__cursor_id")` —— SELECT 尾部多一列别名，`paginate` 据此按名取 id，**不依赖调用方 select 是否含 id 列、也不需要 T 上任何 trait**
5. `build_sqlx(PostgresQueryBuilder)` → `(String, PgArguments)`（`AssertSqlSafe` 包装）
6. 执行：
   - `paginate`：`query_with` 取 `Vec<PgRow>` → 每行 `T::from_row(&row)` + `row.try_get::<ID, _>("__cursor_id")` → `(T, ID)`
   - `paginate_with`：`query_as_with::<_, R>` 取 `Vec<R>` → `map(R)` → `(T, ID)`
7. has_more：`items.len() > limit`，弹掉多余一行
8. next_cursor：`items.last()` 的 ID（keyset 语义 `id < next_cursor`）
9. 组装 `CursorPagingResult<T>`

sqlx tuple `FromRow` 是按位解码（`sqlx-core-0.9.0/src/from_row.rs:326-340`），所以 `paginate_with` 追加的 `__cursor_id` 别名列对 R 的按位解码无干扰（多余尾列被忽略）；`paginate` 的 `T::from_row` 按名解码，别名列同样无干扰。

## 4. 依赖策略（local-substitutable，无 port）

- `shared_contract/Cargo.toml` 增：`sea-query`、`sea-query-sqlx`（均 workspace dep；arch_test 只禁 contract → infrastructure/* 与 feature runtime，libs 类依赖允许）
- `sqlx` 已在依赖；workspace 特性含 `postgres` + `macros` + `derive`。`ID` 的 `#[derive(sqlx::Type)]` 实际展开 Encode + Decode + Type（`sqlx-macros-0.9.0/src/lib.rs:60-64`），故 `row.try_get::<ID, _>("__cursor_id")` 可用
- 测试：`#[sqlx::test]`（macros 特性已有）+ 测试内 `CREATE TABLE` 自建表，**不依赖** `infrastructure::migration`（contract crate 红线）；dev-deps 视需要补 `tokio`（`#[sqlx::test]` 用 `sqlx::test_block_on`，workspace 已含 runtime-tokio，通常不需要）
- **无 port、无 trait 抽象**：`paginate` / `paginate_with` 是纯函数入口，函数签名即接口。底层 builder（sea_query → 其他）或执行细节替换只动 `shared_contract` 内部，调用方零感知——本地可替换性靠"模块内私有实现 + 公共函数表面"达成，不是靠接口 trait
- 不触碰跨域通道：本模块无业务词汇，不新增 `{Domain}Port`，不参与事件/Outbox；它只是 shared_contract 的公共查询表面
- 收紧面：迁移完成后删除 `finalize_cursor_page` 与 `fetch_limit()`；`CursorPagingQuery::limit()` / `cursor_id()` 降级 `pub(crate)`，把"怎么翻页"完全锁进模块（公共表面只留 parse 与 `paginate*`）

## 5. 权衡：杠杆在哪高、哪里薄

**杠杆高（这是深模块的教科书场景——9 个调用点共享同一段演化中的逻辑）：**

- 9 处 × ~9 行 keyset 中间 → 0；模块投入 ~70 行实现 + ~120 行测试。一次投入，9 处减负，且未来 keyset 语义变更（如多列游标、升序、WHERE 优化）只改一个地方
- 拼写漂移全部消失：audit 的限定列、account/item 的 `input_next_cursor`、inventory 的 `Expr::cust` cast、各自的 `page_limit`/`fetch_limit` 局部变量——接口把它们吸收为一个 `IntoColumnRef` 参数
- **局部性**：端点文件只表达"查什么"（列 + 业务筛选），不再表达"怎么翻页"；新端点 3 行示例，AI 与人都能照抄
- 方向固定 Desc、游标列显式参数化——把"唯一的分歧点"变成显式参数，把"共同点"（Desc、limit+1、别名列机制）锁进实现

**薄处（诚实列出的代价）：**

- `paginate` 按行 `T::from_row` + 别名列读取，比 `query_as` 多一层间接——实际无性能差（`query_as` 内部同样逐行 from_row），SELECT 多一列 `__cursor_id` 传输，可忽略
- audit 被挤到 `paginate_with`：多付一个闭包签名 + `(T, ID)` 元组返回（~2 行），但 keyset 中间 10 行消失，净省 ~8 行；diff 映射留在端点是对的——那是业务，不是分页
- **接口膨胀代价**：两个入口 + `IntoColumnRef`。`IntoColumnRef` 是 sea_query 既有 trait，`"id"` 与 `("audit_logs","id")` 写法与现状 `Expr::col(...)` 完全一致，零学习成本；没有为 T 引入任何 trait（无 Keyed/CursorId 机制）；`CursorPagingQuery` 原样复用，`CursorPagingResult` 原样返回。公共表面净变化：+2 函数，-1 函数（finalize），更小更聚焦
- 迁移触碰面：9 个端点文件机械改写（删中间件、换调用），架构测试不强制"必须用 paginate"——软约束，靠 code review 兜底；一次迁移后不可逆（旧样板消失）

## 风险与开放问题

- `__cursor_id` 别名与真实表列名撞名：保留字约定，注释写明
- `paginate_with` 的 R 列序 = select 列序，改列序会静默错位：文档写明，audit 保持 tuple 顺序
- 游标列非单调递增（未来若出现非 tsid 主键）：keyset 语义不成立，模块注释声明前置条件，不强制
- 测试基建：`#[sqlx::test]` 需要测试库 DATABASE_URL（仓库已有 `sqlx_up`/sqlx.toml 环境，runtime crate 已在使用），contract crate 首次使用需确认 dev 环境一致
