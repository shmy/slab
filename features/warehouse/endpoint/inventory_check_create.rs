//! 创建盘点单。

use crate::shared::snapshot::InventoryCheckSnapshot;
use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use warehouse_contract::error::WarehouseError;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CheckItemInput {
    pub item_id: ID,
    pub actual_qty: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateCheckRequest {
    pub warehouse_id: ID,
    pub plan_date: chrono::NaiveDate,
    pub remark: Option<String>,
    pub items: Vec<CheckItemInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateCheckResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/inventory-checks",
    operation_id = "inventory_check_create", tag = "inventory-check",
    request_body = CreateCheckRequest,
    responses((status = 200, body = JsonResponse<CreateCheckResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateCheckRequest>,
) -> JsonResponseType<CreateCheckResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateCheckRequest,
) -> rootcause::Result<CreateCheckResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let code = DocNumberer::next_number(&mut txn, "seq_inventory_check", "CHK").await?;

    let id = ID::new();
    sqlx::query!(
        r#"INSERT INTO inventory_checks (id, code, warehouse_id, plan_date, remark, status)
           VALUES ($1, $2, $3, $4, $5, 0)"#,
        &*id,
        code,
        &*request.warehouse_id,
        request.plan_date,
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    for item in &request.items {
        let book = sqlx::query!(
            r#"SELECT quantity FROM inventories WHERE item_id = $1 AND warehouse_id = $2"#,
            &*item.item_id,
            &*request.warehouse_id,
        )
        .fetch_optional(&mut *txn)
        .await?
        .map(|r| r.quantity)
        .unwrap_or(0);

        let diff = item.actual_qty - book;
        let line_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO inventory_check_items
                   (id, check_id, item_id, book_qty, actual_qty, diff_qty, remark)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            &*line_id,
            &*id,
            &*item.item_id,
            book,
            item.actual_qty,
            diff,
            item.remark,
        )
        .execute(&mut *txn)
        .await?;
    }

    // 写入未返回实体，同事务读回整行作为 after 快照
    let after = InventoryCheckSnapshot::read(&mut txn, &id)
        .await?
        .ok_or(WarehouseError::NotFound)?;
    AuditService::record_create(&mut txn, "inventory_check", &id, &ctx, &after).await?;
    txn.commit().await?;
    Ok(CreateCheckResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let wh = tests::insert_test_warehouse(&state.pg_pool, "CHK-C").await;
        let request = CreateCheckRequest {
            warehouse_id: wh,
            plan_date: chrono::Utc::now().date_naive(),
            remark: Some("test check".into()),
            items: vec![],
        };
        let response = execute(&state.pg_pool, tests::test_operator_context(), request)
            .await
            .unwrap();
        assert!(i64::from(response.id) > 0);
        assert!(response.code.starts_with("CHK-"));

        // 变更历史：create 类型，before 为空，快照为草稿状态
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *response.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["status"], 0);
        assert_eq!(after["code"], response.code.as_str());
        assert_eq!(after["remark"], "test check");
    }
}
