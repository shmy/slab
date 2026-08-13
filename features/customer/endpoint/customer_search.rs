use axum::extract::State;
use db::PgPool;
use sea_query::extension::postgres::PgExpr;
use sea_query::{Expr, ExprTrait as _, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder as _;
use serde::{Deserialize, Serialize};
use serde_with::{NoneAsEmptyString, serde_as};
use shared_contract::query::cursor_page::finalize_cursor_page;
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
const FILTER_SCHEMA: filter_kit::FilterSchema = filter_kit::FilterSchema {
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
    /// PostgREST 风格排序：`name.asc,created_at.desc`（逗号分隔多级；含 id 稳定二级排序）
    #[serde(default)]
    pub order: Option<String>,
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
    let page_limit = query.paging.limit();

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
        .limit(query.paging.fetch_limit())
        .to_owned();

    // 筛选 + 排序 + 游标（keyset）一站式，全部由 filter_kit 装配
    filter_kit::apply_search(
        &mut select,
        &query.filters,
        query.order.as_deref(),
        &FILTER_SCHEMA,
        "customers",
        query.paging.cursor_id().map(|c| *c),
    )?;

    let (sql, values) = select.build_sqlx(PostgresQueryBuilder);
    println!("{}", sql);
    let mut conn = pg_pool.acquire().await?;
    let items: Vec<SearchCustomerItem> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;

    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
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
            order: None,
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

    fn paging_with(limit: u64, cursor: Option<&str>) -> CursorPagingQuery {
        let mut obj = serde_json::json!({ "limit": limit });
        if let Some(c) = cursor {
            obj["cursor"] = serde_json::json!(c);
        }
        serde_json::from_value(obj).unwrap()
    }

    #[sqlx::test]
    async fn test_order_by_name_asc(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        let result = execute(
            &state.pg_pool,
            SearchCustomerQuery {
                paging: paging_with(20, None),
                q: None,
                order: Some("name.asc".into()),
                filters: HashMap::new(),
            },
        )
        .await
        .unwrap();
        // 2 条 active：PG text 按字节序，张(e5) < 李(e6)，name asc → 张伟在前
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].name, "张伟");
        assert_eq!(result.items[1].name, "李娜");
    }

    #[sqlx::test]
    async fn test_order_cursor_keyset_pagination(pool: sqlx::PgPool) {
        seed(&pool).await;
        // 追加 id=4：tsid 时间序与名称序错位（id 越大 name 越靠后的真实场景）
        sqlx::query(
            "INSERT INTO customers (id, code, name, contact_person, phone, is_active) VALUES
             (4, 'C-004', '王五', '王五', '13700137001', true)",
        )
        .execute(&pool)
        .await
        .expect("seed extra customer");
        let state = testing::build(pool).await;
        // name asc（PG 字节序）：张伟(1) < 李娜(2) < 王五(4)
        let page1 = execute(
            &state.pg_pool,
            SearchCustomerQuery {
                paging: paging_with(1, None),
                q: None,
                order: Some("name.asc".into()),
                filters: HashMap::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(page1.items.len(), 1);
        assert_eq!(page1.items[0].name, "张伟");
        let cursor = page1.next_cursor.expect("has more");

        // 页 2（cursor=1）：keyset = name > '张伟' OR (name = '张伟' AND id < 1) → 李娜
        let page2 = execute(
            &state.pg_pool,
            SearchCustomerQuery {
                paging: paging_with(1, Some(&cursor.to_string())),
                q: None,
                order: Some("name.asc".into()),
                filters: HashMap::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(page2.items.len(), 1);
        assert_eq!(page2.items[0].name, "李娜");
        let cursor2 = page2.next_cursor.expect("has more");

        // 页 3：王五（id=4 > 页 1 游标 id=1，旧的 `id < cursor` 过滤会把它永久丢掉）
        let page3 = execute(
            &state.pg_pool,
            SearchCustomerQuery {
                paging: paging_with(1, Some(&cursor2.to_string())),
                q: None,
                order: Some("name.asc".into()),
                filters: HashMap::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(page3.items.len(), 1);
        assert_eq!(page3.items[0].name, "王五");
        assert!(page3.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn test_unknown_order_field_rejected(pool: sqlx::PgPool) {
        seed(&pool).await;
        let state = testing::build(pool).await;
        let err = match execute(
            &state.pg_pool,
            SearchCustomerQuery {
                paging: paging_with(20, None),
                q: None,
                order: Some("hack.asc".into()),
                filters: HashMap::new(),
            },
        )
        .await
        {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.to_string().find("filter_field_not_allowed").is_some());
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
                order: None,
                filters: HashMap::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "李娜");
    }
}
