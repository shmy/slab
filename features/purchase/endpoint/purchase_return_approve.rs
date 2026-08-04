use crate::repository::purchase_return_repository::PurchaseReturnRepository;
use axum::extract::State;
use db::PgPool;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ReturnActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReturnActionResponse {
    pub success: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-returns/{id}/submit",
    operation_id = "purchase_return_submit",
    tag = "purchase-return",
    params(ReturnActionPath),
    responses((status = 200, body = JsonResponse<ReturnActionResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn submit_handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<ReturnActionPath>,
) -> JsonResponseType<ReturnActionResponse> {
    let response = submit_execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[utoipa::path(
    post,
    path = "/api/v1/purchase-returns/{id}/approve",
    operation_id = "purchase_return_approve",
    tag = "purchase-return",
    params(ReturnActionPath),
    responses((status = 200, body = JsonResponse<ReturnActionResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn approve_handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<ReturnActionPath>,
) -> JsonResponseType<ReturnActionResponse> {
    let response = approve_execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn submit_execute(
    pg_pool: &PgPool,
    path: ReturnActionPath,
) -> rootcause::Result<ReturnActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    PurchaseReturnRepository::submit(&mut txn, &path.id).await?;

    txn.commit().await?;
    Ok(ReturnActionResponse { success: true })
}

#[tracing::instrument(skip_all)]
#[inline]
async fn approve_execute(
    pg_pool: &PgPool,
    path: ReturnActionPath,
) -> rootcause::Result<ReturnActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 锁定读 + 状态机校验 + 写状态（含 approved_at），返回新状态
    let _ = PurchaseReturnRepository::approve(&mut txn, &path.id).await?;

    // Get lines with warehouse + order_line via join
    let lines = sqlx::query!(
        r#"SELECT prl.id, prl.item_id, prl.quantity,
                  prl2.warehouse_id, prl2.order_line_id
           FROM purchase_return_lines prl
           JOIN purchase_receipt_lines prl2 ON prl2.id = prl.receipt_line_id
           WHERE prl.return_id = $1"#,
        &*path.id
    )
    .fetch_all(&mut *txn)
    .await?;

    for line in &lines {
        // 库存台账统一处理（允许负库存）
        InventoryLedger::force_issue(
            &mut txn,
            &LedgerCommand {
                item_id: &ID::new_unchecked(line.item_id),
                warehouse_id: &ID::new_unchecked(line.warehouse_id),
                quantity: line.quantity,
                tx_type: TransactionType::PurchaseReturn,
                reference_type: "purchase_return",
                reference_id: &ID::new_unchecked(line.id),
                batch_number: None,
            },
        )
        .await?;

        // Update order_line returned_qty (order_line_id is NOT NULL via JOIN)
        sqlx::query!(
            r#"UPDATE purchase_order_lines SET returned_qty = returned_qty + $1 WHERE id = $2"#,
            line.quantity,
            line.order_line_id,
        )
        .execute(&mut *txn)
        .await?;
    }

    txn.commit().await?;
    Ok(ReturnActionResponse { success: true })
}
