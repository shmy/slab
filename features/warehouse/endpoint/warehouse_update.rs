use axum::extract::State;
use db::PgPool;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use warehouse_contract::entity::WarehouseType;
use warehouse_contract::error::WarehouseError;
use web::extract::{valid_json::ValidJson, valid_path::ValidPath};
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct UpdateWarehousePath {
    pub id: ID,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct UpdateWarehouseRequest {
    pub name: Option<String>,
    pub r#type: Option<WarehouseType>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UpdateWarehouseResponse {
    pub updated: bool,
}

#[utoipa::path(
    patch, path = "/api/v1/warehouses/{id}", operation_id = "warehouse_update", tag = "warehouse",
    params(UpdateWarehousePath), request_body = UpdateWarehouseRequest,
    responses((status = 200, body = JsonResponse<UpdateWarehouseResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<UpdateWarehousePath>,
    ValidJson(request): ValidJson<UpdateWarehouseRequest>,
) -> JsonResponseType<UpdateWarehouseResponse> {
    let response = execute(&pg_pool, path, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    path: UpdateWarehousePath,
    request: UpdateWarehouseRequest,
) -> rootcause::Result<UpdateWarehouseResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let current = sqlx::query!(
        r#"SELECT name, type, is_active FROM warehouses WHERE id = $1"#,
        &*path.id
    )
    .fetch_optional(txn.as_mut())
    .await?
    .ok_or(WarehouseError::NotFound)?;

    let name = request.name.unwrap_or(current.name);
    let wh_type = request.r#type.map(|t| t as i16).unwrap_or(current.r#type);
    let is_active = request.is_active.unwrap_or(current.is_active);

    sqlx::query!(
        r#"UPDATE warehouses SET name = $1, type = $2, is_active = $3 WHERE id = $4"#,
        name,
        wh_type,
        is_active,
        &*path.id
    )
    .execute(txn.as_mut())
    .await?;
    txn.commit().await?;
    Ok(UpdateWarehouseResponse { updated: true })
}
