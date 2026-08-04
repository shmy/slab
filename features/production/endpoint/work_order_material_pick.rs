use axum::extract::State;
use db::PgPool;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct PickPath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct PickItem {
    pub material_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct PickRequest {
    pub items: Vec<PickItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PickResponse {
    pub picked: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{id}/pick",
    operation_id = "work_order_pick", tag = "work-order",
    params(PickPath), request_body = PickRequest,
    responses((status = 200, body = JsonResponse<PickResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<PickPath>,
    ValidJson(request): ValidJson<PickRequest>,
) -> JsonResponseType<PickResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: PickPath,
    request: PickRequest,
) -> rootcause::Result<PickResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let wo = sqlx::query!(
        "SELECT status FROM work_orders WHERE id = $1 FOR UPDATE",
        &*path.id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(ProductionError::NotFound)?;
    if wo.status < 1 {
        return Err(ProductionError::InvalidStatus.into());
    }

    for item in &request.items {
        let mat = sqlx::query!(
            r#"SELECT required_qty, picked_qty FROM work_order_materials WHERE id = $1 AND work_order_id = $2 FOR UPDATE"#,
            &*item.material_id, &*path.id,
        ).fetch_optional(&mut *txn).await?.ok_or(ProductionError::NotFound)?;

        let current_picked = mat.picked_qty.unwrap_or(0);
        let new_picked = current_picked + item.quantity;
        if new_picked > mat.required_qty {
            return Err(ProductionError::InsufficientMaterials.into());
        }

        sqlx::query!(
            "UPDATE work_order_materials SET picked_qty = $1 WHERE id = $2",
            new_picked,
            &*item.material_id
        )
        .execute(&mut *txn)
        .await?;

        // 库存台账统一处理
        InventoryLedger::issue(
            &mut txn,
            &LedgerCommand {
                item_id: &item.item_id,
                warehouse_id: &item.warehouse_id,
                quantity: item.quantity,
                tx_type: TransactionType::MaterialPick,
                reference_type: "work_order_pick",
                reference_id: &path.id,
                batch_number: None,
            },
        )
        .await?;
    }
    txn.commit().await?;
    Ok(PickResponse { picked: true })
}
