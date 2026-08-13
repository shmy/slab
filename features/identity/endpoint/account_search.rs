use axum::extract::State;
use db::PgPool;
use web::extract::valid_query::ValidQuery;
use web::response::json_response::{JsonResponse, JsonResponseType};

use sea_query::extension::postgres::PgExpr;
use sea_query::{Expr, ExprTrait as _, Order, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder as _;
use serde::{Deserialize, Serialize};
use serde_with::{NoneAsEmptyString, serde_as};
use shared_contract::query::cursor_page::finalize_cursor_page;
use shared_contract::query::paging_query::CursorPagingQuery;
use shared_contract::query::paging_result::CursorPagingResult;
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::prelude::FromRow;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;

#[serde_as]
#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchAccountQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    #[param(example = "test")]
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Serialize, FromRow, ToSchema)]
pub(crate) struct SearchAccountItem {
    pub id: ID,
    #[schema(example = "Tom")]
    pub name: String,
    #[schema(example = "13888888888")]
    pub phone: PhoneNumber,
    pub privileged: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/accounts",
    operation_id = "account_search",
    tag = "account",
    params(SearchAccountQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<SearchAccountItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchAccountQuery>,
) -> JsonResponseType<CursorPagingResult<SearchAccountItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchAccountQuery,
) -> rootcause::Result<CursorPagingResult<SearchAccountItem>> {
    let SearchAccountQuery { paging, q, .. } = query;
    let q = q.filter(|s| !s.is_empty());
    let input_next_cursor = paging.next_cursor_id();
    let page_limit = paging.limit();
    let fetch_limit = page_limit + 1;

    let (sql, values) = {
        Query::select()
            .from("accounts")
            .column("id")
            .column("name")
            .column("phone")
            .column("privileged")
            .and_where_option(q.map(|q| {
                Expr::col("phone")
                    .ilike(format!("%{q}%"))
                    .or(Expr::col("name").ilike(format!("%{q}%")))
            }))
            .and_where_option(input_next_cursor.map(|next_cursor| Expr::col("id").lt(next_cursor)))
            .order_by("id", Order::Desc)
            .limit(fetch_limit)
            .build_sqlx(PostgresQueryBuilder)
    };

    let mut conn = pg_pool.acquire().await?;
    let items: Vec<SearchAccountItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;
    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use serde_json::json;

    #[sqlx::test]
    async fn test_search_by_phone(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        tests::insert_test_account(&state.pg_pool, "13900002001").await;
        let query: SearchAccountQuery = serde_json::from_value(json!({
            "q": "13900002001"
        }))
        .unwrap();
        let result = execute(&state.pg_pool, query).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(&*result.items[0].phone, "13900002001");
        assert!(result.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn test_search_by_name(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        tests::insert_test_account(&state.pg_pool, "13900002002").await;
        let query: SearchAccountQuery = serde_json::from_value(json!({
            "q": "test-"
        }))
        .unwrap();
        let result = execute(&state.pg_pool, query).await.unwrap();
        assert!(result.items.len() >= 1);
    }

    #[sqlx::test]
    async fn test_search_cursor_pagination(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        // 插入 3 条，每页 2 条
        tests::insert_test_account(&state.pg_pool, "13900002201").await;
        tests::insert_test_account(&state.pg_pool, "13900002202").await;
        tests::insert_test_account(&state.pg_pool, "13900002203").await;

        // 第一页
        let query: SearchAccountQuery = serde_json::from_value(json!({
            "limit": 2
        }))
        .unwrap();
        let page1 = execute(&state.pg_pool, query).await.unwrap();
        assert_eq!(page1.items.len(), 2);
        assert!(page1.next_cursor.is_some());

        // 第二页
        let cursor = page1.next_cursor.unwrap();
        let query: SearchAccountQuery = serde_json::from_value(json!({
            "limit": 2,
            "next_cursor": cursor.to_string()
        }))
        .unwrap();
        let page2 = execute(&state.pg_pool, query).await.unwrap();
        assert!(page2.items.len() >= 1);
        assert!(page2.next_cursor.is_none());
    }
}
