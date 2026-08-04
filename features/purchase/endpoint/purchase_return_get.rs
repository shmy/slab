use axum::extract::State;
use db::PgPool;
use purchase_contract::entity::PurchaseReturn;
use purchase_contract::error::PurchaseError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetReturnPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PurchaseReturnDetail {
    pub data: PurchaseReturn,
}

#[utoipa::path(get, path = "/api/v1/purchase-returns/{id}",
    operation_id = "purchase_return_get", tag = "purchase-return",
    params(GetReturnPath),
    responses((status = 200, body = JsonResponse<PurchaseReturnDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetReturnPath>,
) -> JsonResponseType<PurchaseReturnDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetReturnPath) -> rootcause::Result<PurchaseReturnDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!(
        r#"SELECT id, code, order_id, supplier_id, return_date, status, reason, remark
           FROM purchase_returns WHERE id = $1"#,
        &*path.id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(PurchaseError::NotFound)?;
    Ok(PurchaseReturnDetail {
        data: PurchaseReturn {
            id: ID::new_unchecked(row.id),
            code: row.code,
            order_id: ID::new_unchecked(row.order_id),
            supplier_id: ID::new_unchecked(row.supplier_id),
            return_date: row.return_date,
            status: row.status,
            reason: row.reason,
            remark: row.remark,
        },
    })
}
