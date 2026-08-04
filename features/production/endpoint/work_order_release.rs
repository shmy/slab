use axum::extract::State;
use db::PgPool;
use production_contract::error::ProductionError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct WOPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WOResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{id}/release",
    operation_id = "work_order_release", tag = "work-order",
    params(WOPath),
    responses((status = 200, body = JsonResponse<WOResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<WOPath>,
) -> JsonResponseType<WOResponse> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: WOPath) -> rootcause::Result<WOResponse> {
    let mut conn = pg_pool.acquire().await?;
    let wo = sqlx::query!("SELECT status FROM work_orders WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ProductionError::NotFound)?;
    if wo.status != 0 {
        return Err(ProductionError::InvalidStatus.into());
    }
    sqlx::query!("UPDATE work_orders SET status = 1 WHERE id = $1", &*path.id)
        .execute(&mut *conn)
        .await?;
    Ok(WOResponse { success: true })
}
