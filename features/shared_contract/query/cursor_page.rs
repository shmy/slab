//! ID keyset 游标分页的深模块：追加 keyset 子句 → build_sqlx → fetch → 行映射 →
//! has_more 判定 → next_cursor 提取，全生命周期收在接缝后。
//!
//! 调用方只声明「分页什么」：列 + FROM/JOIN + 业务 WHERE 建好 [`SelectStatement`]，
//! 传入游标列（`"id"`，或 LEFT JOIN 场景的限定列 `("table", "id")`）与
//! [`CursorPagingQuery`]。方向固定 `DESC`、多取一条判定 has_more、游标提取全部在模块内部。
//!
//! # 不变量
//! - `select` 不得自带 ORDER BY / LIMIT / OFFSET（模块追加 keyset 子句，重复会叠条件）。
//! - 游标列必须 FROM/JOIN 可达且值单调递增（tsid 语义，keyset 正确性前置）。
//! - [`paginate`] 的 `T` 按列名解码，游标 id 从模块追加的 `__cursor_id` 别名列读取，
//!   不依赖调用方 select 是否含游标列，也不要求 `T` 实现任何游标 trait。
//! - [`paginate_with`] 的行类型按位解码（tuple 等），游标 id 由映射闭包返回的
//!   `(T, ID)` 提供；闭包内的 ID 须与游标列值一致（约定，不做运行时校验）。

use sea_query::{
    Alias, ColumnRef, Expr, ExprTrait as _, IntoColumnRef, Order, PostgresQueryBuilder,
    SelectStatement,
};
use sea_query_sqlx::SqlxBinder as _;
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgConnection, Row};
use utoipa::ToSchema;

use super::paging_query::CursorPagingQuery;
use super::paging_result::CursorPagingResult;
use crate::value_object::id::ID;

/// 模块注入的游标 id 别名列（SELECT 尾部追加，仅模块内部读取，对调用方不可见；
/// `T::from_row` 按名解码、tuple 按位解码都会忽略多余列）。
const CURSOR_ID_ALIAS: &str = "__cursor_id";

/// 快路径：单表 + [`FromRow`] 行类型 + 显式游标列。
pub async fn paginate<T, C>(
    conn: &mut PgConnection,
    mut select: SelectStatement,
    paging: &CursorPagingQuery,
    cursor_col: C,
) -> rootcause::Result<CursorPagingResult<T>>
where
    T: for<'r> FromRow<'r, PgRow> + Serialize + ToSchema,
    C: IntoColumnRef,
{
    let col: ColumnRef = cursor_col.into_column_ref();
    apply_keyset(&mut select, &col, paging);
    // 游标 id 从别名列按名读取，与 select 是否已含该列解耦
    select.expr_as(Expr::col(col), Alias::new(CURSOR_ID_ALIAS));

    let (sql, values) = select.build_sqlx(PostgresQueryBuilder);
    let rows: Vec<PgRow> = sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;

    let mut mapped: Vec<(T, ID)> = Vec::with_capacity(rows.len());
    for row in rows {
        let item = T::from_row(&row)?;
        let cursor_id = row.try_get::<ID, _>(CURSOR_ID_ALIAS)?;
        mapped.push((item, cursor_id));
    }
    Ok(finalize(mapped, paging))
}

/// 显式路径：任意行形态（tuple / 自定义 [`FromRow`] 结构）+ 映射闭包 → `(T, ID)`。
///
/// 适用于 LEFT JOIN + 派生字段（如变更历史的 diff 计算）；映射闭包返回的 ID 用于
/// next_cursor，须与游标列值一致（约定，不做运行时校验）。
pub async fn paginate_with<T, R, C, F>(
    conn: &mut PgConnection,
    mut select: SelectStatement,
    paging: &CursorPagingQuery,
    cursor_col: C,
    mut map: F,
) -> rootcause::Result<CursorPagingResult<T>>
where
    T: Serialize + ToSchema,
    R: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    C: IntoColumnRef,
    F: FnMut(R) -> rootcause::Result<(T, ID)>,
{
    let col: ColumnRef = cursor_col.into_column_ref();
    apply_keyset(&mut select, &col, paging);

    let (sql, values) = select.build_sqlx(PostgresQueryBuilder);
    let rows: Vec<R> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;

    let mut mapped: Vec<(T, ID)> = Vec::with_capacity(rows.len());
    for row in rows {
        mapped.push(map(row)?);
    }
    Ok(finalize(mapped, paging))
}

/// 追加 keyset 三件套：游标条件（`col < cursor`，可选）、`ORDER BY col DESC`、`LIMIT limit+1`。
fn apply_keyset(select: &mut SelectStatement, col: &ColumnRef, paging: &CursorPagingQuery) {
    if let Some(cursor) = paging.cursor_id() {
        select.and_where(Expr::col(col.clone()).lt(*cursor));
    }
    select
        .order_by(col.clone(), Order::Desc)
        .limit(paging.limit() + 1);
}

/// 多取一条判定 has_more；弹掉多余行后，next_cursor 取保留行末条的游标 id
/// （keyset：`id < next_cursor` 的下一页自然跳过多取行）。
fn finalize<T>(mut mapped: Vec<(T, ID)>, paging: &CursorPagingQuery) -> CursorPagingResult<T>
where
    T: Serialize + ToSchema,
{
    let limit = paging.limit() as usize;
    let has_more = mapped.len() > limit;
    let next_cursor = if has_more {
        mapped.pop();
        mapped.last().map(|(_, id)| *id)
    } else {
        None
    };
    CursorPagingResult {
        items: mapped.into_iter().map(|(item, _)| item).collect(),
        next_cursor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::{Expr, Query};
    use serde::Serialize;
    use sqlx::FromRow;
    use utoipa::ToSchema;

    #[derive(Debug, FromRow, Serialize, ToSchema)]
    struct Item {
        id: ID,
        name: String,
    }

    #[derive(Debug, Serialize, ToSchema)]
    struct JoinedItem {
        id: ID,
        name: String,
        operator_name: Option<String>,
    }

    /// 变更历史变体的行形态（tuple 按位解码，镜像 audit_search）：
    /// (id, operator_id, operator_name)。
    type JoinedRow = (i64, i64, Option<String>);

    async fn create_items(pool: &sqlx::PgPool) {
        sqlx::query(
            r#"
            CREATE TABLE items (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("create items table");
        for (id, name) in [(3i64, "c"), (2, "b"), (1, "a")] {
            sqlx::query("INSERT INTO items (id, name) VALUES ($1, $2)")
                .bind(id)
                .bind(name)
                .execute(pool)
                .await
                .expect("seed item");
        }
    }

    async fn create_joined(pool: &sqlx::PgPool) {
        sqlx::query(
            r#"
            CREATE TABLE logs (
                id BIGINT PRIMARY KEY,
                operator_id BIGINT
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("create logs table");
        sqlx::query(
            r#"
            CREATE TABLE accounts (
                id BIGINT PRIMARY KEY,
                name TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("create accounts table");
        for (id, operator_id) in [(3i64, 1), (2, 1), (1, 2)] {
            sqlx::query("INSERT INTO logs (id, operator_id) VALUES ($1, $2)")
                .bind(id)
                .bind(operator_id)
                .execute(pool)
                .await
                .expect("seed log");
        }
        sqlx::query("INSERT INTO accounts (id, name) VALUES (1, 'alice'), (2, 'bob')")
            .execute(pool)
            .await
            .expect("seed account");
    }

    fn paging(limit: u64) -> CursorPagingQuery {
        serde_json::from_value(serde_json::json!({ "limit": limit.to_string() })).unwrap()
    }

    // ---- paginate（快路径）----

    #[sqlx::test]
    async fn first_page_has_more_and_second_page_continues(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .to_owned();
        let page1: CursorPagingResult<Item> =
            paginate(&mut conn, select, &paging(2), "id").await.unwrap();
        assert_eq!(page1.items.len(), 2);
        // id DESC：3、2
        assert_eq!(page1.items[0].id, ID::from(3));
        assert_eq!(page1.items[1].id, ID::from(2));
        // 多取一条判定 has_more：next_cursor = 保留行末条（2）的游标 id
        assert_eq!(page1.next_cursor, Some(ID::from(2)));
    }

    #[sqlx::test]
    async fn cursor_continues_keyset(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let q: CursorPagingQuery = serde_json::from_str(r#"{"limit":"2","cursor":"2"}"#).unwrap();
        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .to_owned();
        let page: CursorPagingResult<Item> = paginate(&mut conn, select, &q, "id").await.unwrap();
        // cursor=2 → id < 2 → 只剩 1；limit 2 全取，无更多
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, ID::from(1));
        assert!(page.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn exact_limit_no_more(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .to_owned();
        let page: CursorPagingResult<Item> =
            paginate(&mut conn, select, &paging(3), "id").await.unwrap();
        assert_eq!(page.items.len(), 3);
        assert!(page.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn empty_table(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("DELETE FROM items")
            .execute(&mut *conn)
            .await
            .unwrap();

        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .to_owned();
        let page: CursorPagingResult<Item> =
            paginate(&mut conn, select, &paging(2), "id").await.unwrap();
        assert!(page.items.is_empty());
        assert!(page.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn business_where_preserved(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .and_where(Expr::col("name").ne("b"))
            .to_owned();
        let page: CursorPagingResult<Item> =
            paginate(&mut conn, select, &paging(2), "id").await.unwrap();
        // 过滤后只剩 c、a
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, ID::from(3));
        assert_eq!(page.items[1].id, ID::from(1));
        assert!(page.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn limit_zero_clamped_to_one(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .to_owned();
        let page: CursorPagingResult<Item> =
            paginate(&mut conn, select, &paging(0), "id").await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.next_cursor.is_some());
    }

    // ---- paginate_with（显式路径，镜像 audit）----

    #[sqlx::test]
    async fn mapped_qualified_column_and_tuple_rows(pool: sqlx::PgPool) {
        create_joined(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        let select = Query::select()
            .column(("logs", "id"))
            .column(("logs", "operator_id"))
            .column(("accounts", "name"))
            .from("logs")
            .left_join(
                "accounts",
                Expr::col(("logs", "operator_id")).equals(("accounts", "id")),
            )
            .to_owned();

        let page: CursorPagingResult<JoinedItem> = paginate_with(
            &mut conn,
            select,
            &paging(2),
            ("logs", "id"),
            |row: JoinedRow| {
                let (id, _operator_id, operator_name) = row;
                Ok((
                    JoinedItem {
                        id: ID::from(id),
                        name: format!("log-{id}"),
                        operator_name,
                    },
                    ID::from(id),
                ))
            },
        )
        .await
        .unwrap();

        // id DESC：3（alice）、2（alice）
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, ID::from(3));
        assert_eq!(page.items[0].operator_name.as_deref(), Some("alice"));
        assert_eq!(page.items[1].id, ID::from(2));
        assert_eq!(page.next_cursor, Some(ID::from(2)));
    }

    #[sqlx::test]
    async fn mapper_error_propagates(pool: sqlx::PgPool) {
        create_items(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        #[derive(Debug, thiserror::Error)]
        #[error("mapper_failed")]
        struct MapperError;

        let select = Query::select()
            .column("id")
            .column("name")
            .from("items")
            .to_owned();
        let result = paginate_with(
            &mut conn,
            select,
            &paging(2),
            "id",
            |_row: (i64, String)| -> rootcause::Result<(JoinedItem, ID)> {
                Err(MapperError.into())
            },
        )
        .await;
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        // rootcause Display 带位置前缀，断言只查 key 片段
        assert!(err.to_string().contains("mapper_failed"));
    }
}
