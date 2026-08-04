use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use db::PgPool;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub(crate) struct ReadyzResponse {
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

async fn check(pg_pool: &PgPool) -> Result<(), &'static str> {
    let mut conn = pg_pool.acquire().await.map_err(|_| "postgres_pool")?;
    sqlx::query("SELECT 1")
        .execute(&mut *conn)
        .await
        .map_err(|_| "postgres")?;
    Ok(())
}

#[utoipa::path(
    get,
    path = "/readyz",
    operation_id = "readyz",
    tag = "health",
    responses(
        (status = 200, description = "依赖可用", body = ReadyzResponse),
        (status = 503, description = "依赖不可用", body = ReadyzResponse),
    ),
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(State(pg_pool): State<PgPool>) -> impl IntoResponse {
    match check(&pg_pool).await {
        Ok(()) => Json(ReadyzResponse {
            ready: true,
            reason: None,
        })
        .into_response(),
        Err(reason) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyzResponse {
                ready: false,
                reason: Some(reason.into()),
            }),
        )
            .into_response(),
    }
}
