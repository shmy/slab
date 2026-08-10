use audit_contract::AuditService;
use axum::extract::State;
use db::PgPool;
use doc_numbering::DocNumberer;
use http_auth::extract::operator::OperatorContext;
use purchase_contract::entity::PurchaseReturn;
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReturnLine {
    pub receipt_line_id: ID,
    pub item_id: ID,
    pub quantity: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateReturnRequest {
    pub order_id: ID,
    pub reason: Option<String>,
    pub lines: Vec<CreateReturnLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateReturnResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-returns",
    operation_id = "purchase_return_create",
    tag = "purchase-return",
    request_body = CreateReturnRequest,
    responses((status = 200, body = JsonResponse<CreateReturnResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateReturnRequest>,
) -> JsonResponseType<CreateReturnResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateReturnRequest,
) -> rootcause::Result<CreateReturnResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 验证订单存在
    let order = sqlx::query!(
        r#"SELECT supplier_id, status FROM purchase_orders WHERE id = $1"#,
        &*request.order_id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(PurchaseError::NotFound)?;

    let code = DocNumberer::next_number(&mut txn, "seq_purchase_return", "RET").await?;

    let return_id = ID::new();
    sqlx::query!(
        r#"INSERT INTO purchase_returns
               (id, code, order_id, supplier_id, reason, status)
           VALUES ($1, $2, $3, $4, $5, 0)"#,
        &*return_id,
        code,
        &*request.order_id,
        &*ID::new_unchecked(order.supplier_id),
        request.reason,
    )
    .execute(&mut *txn)
    .await?;

    for line in &request.lines {
        let line_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO purchase_return_lines
                   (id, return_id, receipt_line_id, item_id, quantity, reason)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            &*line_id,
            &*return_id,
            &*line.receipt_line_id,
            &*line.item_id,
            line.quantity,
            line.reason,
        )
        .execute(&mut *txn)
        .await?;
    }

    // 变更历史：同事务读回写入后的退货单作为快照
    let ret = sqlx::query_as!(
        PurchaseReturn,
        r#"SELECT id, code, order_id, supplier_id, return_date, status, reason, remark
           FROM purchase_returns WHERE id = $1"#,
        &*return_id
    )
    .fetch_one(&mut *txn)
    .await?;
    AuditService::record_create(&mut txn, "purchase_return", &return_id, &ctx, &ret).await?;

    txn.commit().await?;
    Ok(CreateReturnResponse {
        id: return_id,
        code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;
    use purchase_contract::value_object::PurchaseReturnStatus;

    #[sqlx::test]
    async fn test_create_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool).await;
        let order_id =
            crate::tests::insert_test_purchase_order(&state.pg_pool, "PO-RETC1", 0).await;

        let resp = execute(
            &state.pg_pool,
            crate::tests::test_operator_context(),
            CreateReturnRequest {
                order_id,
                reason: Some("defective".to_string()),
                lines: vec![],
            },
        )
        .await
        .unwrap();
        assert!(i64::from(resp.id) > 0);
        assert!(resp.code.starts_with("RET"));

        // 变更历史：create 类型，before 为空
        let mut conn = state.pg_pool.acquire().await.unwrap();
        let audit_row = sqlx::query!(
            r#"SELECT action, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["status"], PurchaseReturnStatus::Draft as i16);
    }
}
