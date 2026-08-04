use axum::extract::State;
use db::PgPool;
use serde::Serialize;
use shared_contract::value_object::id::ID;
use utoipa::ToSchema;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WarehouseItem {
    pub id: ID,
    pub code: String,
    pub name: String,
    pub r#type: i16,
    pub is_active: bool,
}

#[utoipa::path(
    get, path = "/api/v1/warehouses", operation_id = "warehouse_list", tag = "warehouse",
    responses((status = 200, body = JsonResponse<Vec<WarehouseItem>>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(State(pg_pool): State<PgPool>) -> JsonResponseType<Vec<WarehouseItem>> {
    let response = execute(&pg_pool).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip(pg_pool))]
#[inline]
async fn execute(pg_pool: &PgPool) -> rootcause::Result<Vec<WarehouseItem>> {
    let mut conn = pg_pool.acquire().await?;
    let rows =
        sqlx::query!(r#"SELECT id, code, name, type, is_active FROM warehouses ORDER BY code"#)
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| WarehouseItem {
            id: ID::new_unchecked(r.id),
            code: r.code,
            name: r.name,
            r#type: r.r#type,
            is_active: r.is_active,
        })
        .collect())
}
