//! 审批盘点单。

use crate::repository::inventory_check_repository::InventoryCheckRepository;
use axum::extract::State;
use db::PgPool;
use inventory_ledger::InventoryLedger;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CheckActionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CheckActionResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/inventory-checks/{id}/approve",
    operation_id = "inventory_check_approve", tag = "inventory-check",
    params(CheckActionPath),
    responses((status = 200, body = JsonResponse<CheckActionResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<CheckActionPath>,
) -> JsonResponseType<CheckActionResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: CheckActionPath,
) -> rootcause::Result<CheckActionResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    // 锁定读 + 状态机校验 + 写状态（含 approved_at），返回仓库快照供库存台账副作用使用
    let locked = InventoryCheckRepository::approve(&mut txn, &path.id).await?;

    let items = sqlx::query!(
        r#"SELECT item_id, actual_qty
           FROM inventory_check_items WHERE check_id = $1"#,
        &*path.id
    )
    .fetch_all(&mut *txn)
    .await?;

    for item in &items {
        InventoryLedger::adjust(
            &mut txn,
            &ID::new_unchecked(item.item_id),
            &ID::new_unchecked(locked.warehouse_id),
            item.actual_qty,
            "inventory_check",
            &path.id,
        )
        .await?;
    }

    txn.commit().await?;
    Ok(CheckActionResponse { success: true })
}
