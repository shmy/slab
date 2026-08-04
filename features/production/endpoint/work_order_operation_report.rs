use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ReportPath {
    pub work_order_id: ID,
    pub operation_id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct ReportRequest {
    pub completed_qty: i64,
    pub scrap_qty: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReportResponse {
    pub success: bool,
}

#[utoipa::path(post, path = "/api/v1/work-orders/{work_order_id}/operations/{operation_id}/report",
    operation_id = "work_order_operation_report", tag = "work-order",
    params(ReportPath), request_body = ReportRequest,
    responses((status = 200, body = JsonResponse<ReportResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<ReportPath>,
    ValidJson(request): ValidJson<ReportRequest>,
) -> JsonResponseType<ReportResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: ReportPath,
    request: ReportRequest,
) -> rootcause::Result<ReportResponse> {
    let mut conn = pg_pool.acquire().await?;

    sqlx::query!(
        r#"UPDATE work_order_operations
           SET completed_qty = completed_qty + $1, scrap_qty = scrap_qty + $2, status = 2
           WHERE id = $3 AND work_order_id = $4"#,
        request.completed_qty,
        request.scrap_qty.unwrap_or(0),
        &*path.operation_id,
        &*path.work_order_id,
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query!(
        r#"UPDATE work_orders
           SET completed_qty = (SELECT COALESCE(SUM(completed_qty),0) FROM work_order_operations WHERE work_order_id = $1),
               scrap_qty = (SELECT COALESCE(SUM(scrap_qty),0) FROM work_order_operations WHERE work_order_id = $1)
           WHERE id = $1"#, &*path.work_order_id,
    ).execute(&mut *conn).await?;

    Ok(ReportResponse { success: true })
}
