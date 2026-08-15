use axum::extract::State;
use db::PgPool;
use sea_query::extension::postgres::PgExpr;
use sea_query::{Expr, ExprTrait as _, Query};
use serde::{Deserialize, Serialize};
use serde_with::{NoneAsEmptyString, serde_as};
use shared_contract::query::cursor_page::paginate;
use shared_contract::query::paging_query::CursorPagingQuery;
use shared_contract::query::paging_result::CursorPagingResult;
use shared_contract::value_object::id::ID;
use sqlx::FromRow;
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;

use web::extract::valid_query::ValidQuery;
use web::response::json_response::{JsonResponse, JsonResponseType};

/// 可筛/可排字段声明（text 支持 eq/ilike；date 支持 gt/gte/lt/lte，自动 cast）。
/// 白名单与 SQL 生成同源（filter_kit::FilterSchema），不会不一致。
/// `pub` 供 bin/server meta 端点收集（筛选协议事实源，勿改可见性）。
pub const FILTER_SCHEMA: filter_kit::FilterSchema = filter_kit::FilterSchema {
    text_fields: &["code", "name", "phone", "contact_person"],
    date_fields: &["created_at"],
    int_fields: &[],
};

#[serde_as]
#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchCustomerQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    pub q: Option<String>,
    /// PostgREST 风格筛选（flatten 收集除分页/搜索词外的所有参数）：
    /// `name=ilike.*张*&created_at=gt.2024-03-15`（多参数天然 AND）
    #[serde(flatten)]
    #[serde(default)]
    pub filters: HashMap<String, String>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub(crate) struct SearchCustomerItem {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub is_active: bool,
    /// 以下字段供排序游标取「最后一条的排序键值」（列表页详情抽屉也需要）
    pub phone: Option<String>,
    pub contact_person: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get, path = "/api/v1/customers", operation_id = "customer_search", tag = "customer",
    params(SearchCustomerQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<SearchCustomerItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchCustomerQuery>,
) -> JsonResponseType<CursorPagingResult<SearchCustomerItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchCustomerQuery,
) -> rootcause::Result<CursorPagingResult<SearchCustomerItem>> {
    let q = query.q.filter(|s| !s.is_empty());

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
        // 软删除（delete 置 is_active=false）不出现在列表
        .and_where(Expr::col("is_active").eq(true))
        .to_owned();

    // 筛选（PostgREST 风格，白名单校验 + 类型映射由 filter_kit 保证）
    for expr in filter_kit::filter_where(&query.filters, &FILTER_SCHEMA)? {
        select.and_where(expr);
    }

    let mut conn = pg_pool.acquire().await?;
    paginate(&mut conn, select, &query.paging, "id").await
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use shared_contract::query::paging_query::CursorPagingQuery;

    async fn seed(pool: &sqlx::PgPool) {
        run_migrations(pool).await.expect("run migrations");
        sqlx::query(
            "INSERT INTO customers (id, code, name, contact_person, phone, is_active) VALUES
             (1, 'C-001', '张伟', '张三', '13800138000', true),
             (2, 'C-002', '李娜', '李四', '13900139000', true),
             (3, 'C-003', '张三丰', '王五', '13700137000', false)",
        )
        .execute(pool)
        .await
        .expect("seed customers");
    }

    fn query_with(filters: &[(&str, &str)]) -> SearchCustomerQuery {
        SearchCustomerQuery {
            paging: CursorPagingQuery::default(),
            q: None,
            filters: filters
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[sqlx::test]
    async fn test_filter_name_contains(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        let result = execute(&state.pg_pool, query_with(&[("name", "ilike.*张*")]))
            .await
            .unwrap();
        // 张三丰 is_active=false（软删除）被排除，只剩张伟
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "张伟");
    }

    #[sqlx::test]
    async fn test_filter_created_after(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        let result = execute(
            &state.pg_pool,
            query_with(&[("created_at", "gt.2000-01-01")]),
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 2); // 两条 active
    }

    #[sqlx::test]
    async fn test_unknown_field_rejected(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        // 未知字段被白名单拒绝 → 400（filter_field_not_allowed），注入串进不了 SQL
        let err = match execute(&state.pg_pool, query_with(&[("hack", "ilike.' OR 1=1 --")])).await
        {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        let msg = err.to_string();
        assert!(msg.find("filter_field_not_allowed").is_some());
    }

    #[sqlx::test]
    async fn test_invalid_syntax_rejected(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        let err = match execute(&state.pg_pool, query_with(&[("name", "foo.张")])).await {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        let msg = err.to_string();
        assert!(msg.find("invalid_filter_syntax").is_some());
    }

    #[sqlx::test]
    async fn test_q_matches_phone(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        let result = execute(
            &state.pg_pool,
            SearchCustomerQuery {
                paging: CursorPagingQuery::default(),
                q: Some("13900139000".into()),
                filters: HashMap::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "李娜");
    }
}
