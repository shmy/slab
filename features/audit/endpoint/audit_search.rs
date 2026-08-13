//! GET /api/v1/audit-logs — 按资源查询变更历史（时间倒序，游标分页，读时 diff）。

use crate::diff::{ChangeField, json_diff};
use axum::extract::State;
use chrono::{DateTime, Utc};
use db::PgPool;
use sea_query::{Expr, ExprTrait as _, Order, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder as _;
use serde::{Deserialize, Serialize};
use shared_contract::query::cursor_page::finalize_cursor_page;
use shared_contract::query::paging_query::CursorPagingQuery;
use shared_contract::query::paging_result::CursorPagingResult;
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_query::ValidQuery;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchAuditQuery {
    #[serde(flatten)]
    #[param(inline)]
    pub paging: CursorPagingQuery,
    #[param(example = "account")]
    pub entity: String,
    #[param(example = "1234567890123456789")]
    pub entity_id: ID,
}

/// `audit_logs` × `accounts` LEFT JOIN 的查询行（中间形态，映射为 [`AuditLogItem`]）。
type AuditLogRow = (
    i64,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    i64,
    Option<String>,
    DateTime<Utc>,
);

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AuditLogItem {
    pub id: ID,
    /// 变更类型（由 before/after 快照推断）：create / update / delete
    pub change_type: String,
    /// 字段级变更明细（读时计算，git diff 风格展示的输入）
    pub diff: Vec<ChangeField>,
    /// 操作人
    pub operator_id: ID,
    /// 操作人姓名（冗余展示，账户被删除时为 null）
    pub operator_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[utoipa::path(
    get,
    path = "/api/v1/audit-logs",
    operation_id = "audit_search",
    tag = "audit",
    params(SearchAuditQuery),
    responses((status = 200, body = JsonResponse<CursorPagingResult<AuditLogItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidQuery(query): ValidQuery<SearchAuditQuery>,
) -> JsonResponseType<CursorPagingResult<AuditLogItem>> {
    let response = execute(&pg_pool, query).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    query: SearchAuditQuery,
) -> rootcause::Result<CursorPagingResult<AuditLogItem>> {
    let page_limit = query.paging.limit();

    let (sql, values) = Query::select()
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
        .and_where_option(
            query
                .paging
                .cursor_id()
                .map(|cursor| Expr::col(("audit_logs", "id")).lt(*cursor)),
        )
        // 单键排序：id 是应用生成的 tsid，单调递增，天然等同时序
        .order_by(("audit_logs", "id"), Order::Desc)
        .limit(query.paging.fetch_limit())
        .build_sqlx(PostgresQueryBuilder);

    let mut conn = pg_pool.acquire().await?;
    let rows: Vec<AuditLogRow> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(&mut *conn)
        .await?;

    let items = rows
        .into_iter()
        .map(
            |(id, before, after, operator_id, operator_name, created_at)| {
                rootcause::Result::<AuditLogItem>::Ok(AuditLogItem {
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
                })
            },
        )
        .collect::<rootcause::Result<Vec<_>>>()?;

    Ok(finalize_cursor_page(items, page_limit, |item| item.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use serde_json::json;

    /// 插入一个测试账户，返回其 ID（作为变更历史操作人）。
    async fn insert_actor(pool: &sqlx::PgPool) -> i64 {
        let id = ID::new();
        let name = format!("actor-{id}");
        // 固定 11 位手机号：13 + 9 位数字（取 tsid 的低 9 位）
        let digits = i64::from(id).rem_euclid(1_000_000_000);
        let phone = format!("13{digits:09}");
        sqlx::query!(
            r#"INSERT INTO accounts (id, name, phone, password, version)
               VALUES ($1, $2, $3, 'unused', 1)"#,
            *id,
            name,
            phone,
        )
        .execute(pool)
        .await
        .expect("insert actor");
        *id
    }

    async fn insert_audit_log(
        pool: &sqlx::PgPool,
        id: i64,
        operator_id: i64,
        action: i16,
        entity: &str,
        entity_id: i64,
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO audit_logs (id, operator_id, action, entity, entity_id, before, after, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            id,
            operator_id,
            action,
            entity,
            entity_id,
            before,
            after,
            Utc::now(),
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn search_query(entity: &str, entity_id: i64) -> SearchAuditQuery {
        serde_json::from_value(json!({
            "entity": entity,
            "entity_id": entity_id.to_string()
        }))
        .unwrap()
    }

    #[sqlx::test]
    async fn test_search_empty(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let result = execute(&state.pg_pool, search_query("account", 9001))
            .await
            .unwrap();
        assert!(result.items.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[sqlx::test]
    async fn test_search_returns_rows_with_diff_and_actor_name(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let actor_id = insert_actor(&state.pg_pool).await;

        // create：before 为空，全部字段记 after
        insert_audit_log(
            &state.pg_pool,
            1001,
            actor_id,
            1, // Created
            "account",
            9001,
            None,
            Some(json!({"name": "Tom", "phone": "13900000001"})),
        )
        .await;
        // update：只记变化字段
        insert_audit_log(
            &state.pg_pool,
            1002,
            actor_id,
            2, // Updated
            "account",
            9001,
            Some(json!({"name": "Tom", "phone": "13900000001"})),
            Some(json!({"name": "Tom", "phone": "13900000002"})),
        )
        .await;
        // 另一个资源的变更不应出现
        insert_audit_log(
            &state.pg_pool,
            1003,
            actor_id,
            2, // Updated
            "account",
            7001,
            Some(json!({"name": "Tom"})),
            Some(json!({"name": "Tomas"})),
        )
        .await;

        let result = execute(&state.pg_pool, search_query("account", 9001))
            .await
            .unwrap();
        assert_eq!(result.items.len(), 2);
        assert!(result.next_cursor.is_none());

        // 时间倒序：1002 在前
        assert_eq!(result.items[0].id, ID::from(1002));
        assert_eq!(result.items[0].change_type, "update");
        assert_eq!(result.items[0].diff.len(), 1);
        assert_eq!(result.items[0].diff[0].field, "phone");
        assert_eq!(result.items[0].diff[0].before, json!("13900000001"));
        assert_eq!(result.items[0].diff[0].after, json!("13900000002"));
        assert_eq!(result.items[0].operator_id, ID::from(actor_id));
        assert!(result.items[0].operator_name.is_some());

        assert_eq!(result.items[1].id, ID::from(1001));
        assert_eq!(result.items[1].change_type, "create");
        assert_eq!(result.items[1].diff.len(), 2);
        assert_eq!(result.items[1].diff[0].before, serde_json::Value::Null);
    }

    #[sqlx::test]
    async fn test_search_cursor_pagination(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let actor_id = insert_actor(&state.pg_pool).await;

        // 3 条同资源变更，每页 2 条
        for i in 1..=3 {
            insert_audit_log(
                &state.pg_pool,
                2000 + i,
                actor_id,
                2, // Updated
                "account",
                9002,
                Some(json!({"name": "Tom"})),
                Some(json!({"name": format!("Tom{i}")})),
            )
            .await;
        }

        let query: SearchAuditQuery = serde_json::from_value(json!({
            "entity": "account",
            "entity_id": "9002",
            "limit": 2
        }))
        .unwrap();
        let page1 = execute(&state.pg_pool, query).await.unwrap();
        assert_eq!(page1.items.len(), 2);
        assert!(page1.next_cursor.is_some());

        let cursor = page1.next_cursor.unwrap();
        let query: SearchAuditQuery = serde_json::from_value(json!({
            "entity": "account",
            "entity_id": "9002",
            "limit": 2,
            "cursor": cursor.to_string()
        }))
        .unwrap();
        let page2 = execute(&state.pg_pool, query).await.unwrap();
        assert_eq!(page2.items.len(), 1);
        assert!(page2.next_cursor.is_none());
    }
}
