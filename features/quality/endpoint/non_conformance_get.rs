use axum::extract::State;
use db::PgPool;
use quality_contract::entity::NonConformance;
use quality_contract::error::QualityError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetNCPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct NCDetail {
    pub data: NonConformance,
}

#[utoipa::path(get, path = "/api/v1/non-conformances/{id}", operation_id = "non_conformance_get", tag = "non-conformance",
    params(GetNCPath), responses((status = 200, body = JsonResponse<NCDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetNCPath>,
) -> JsonResponseType<NCDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetNCPath) -> rootcause::Result<NCDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!("SELECT id, code, inspection_id, item_id, quantity, severity, disposition, status, remark FROM non_conformances WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn).await?.ok_or(QualityError::NonConformanceNotFound)?;
    Ok(NCDetail {
        data: NonConformance {
            id: ID::new_unchecked(row.id),
            code: row.code,
            inspection_id: row.inspection_id.map(ID::new_unchecked),
            item_id: ID::new_unchecked(row.item_id),
            quantity: row.quantity,
            severity: row.severity,
            disposition: row.disposition,
            status: row.status,
            remark: row.remark,
        },
    })
}
