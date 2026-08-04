use axum::extract::State;
use db::PgPool;
use product_contract::entity::Mold;
use product_contract::error::ProductError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetMoldPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MoldDetail {
    pub data: Mold,
}

#[utoipa::path(get, path = "/api/v1/molds/{id}", operation_id = "mold_get", tag = "mold",
    params(GetMoldPath), responses((status = 200, body = JsonResponse<MoldDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetMoldPath>,
) -> JsonResponseType<MoldDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetMoldPath) -> rootcause::Result<MoldDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!("SELECT id, code, name, item_id, cavity_count, life_expectancy, life_used, status, maintenance_cycle, remark FROM molds WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn).await?.ok_or(ProductError::MoldNotFound)?;
    Ok(MoldDetail {
        data: Mold {
            id: ID::new_unchecked(row.id),
            code: row.code,
            name: row.name,
            item_id: ID::new_unchecked(row.item_id),
            cavity_count: row.cavity_count,
            life_expectancy: row.life_expectancy,
            life_used: row.life_used,
            status: row.status,
            maintenance_cycle: row.maintenance_cycle,
            remark: row.remark,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_mold_get_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-MOLDG-1").await;

        let mold_id = ID::new();
        let mut conn = state.pg_pool.acquire().await.unwrap();
        sqlx::query!(
            "INSERT INTO molds (id, code, name, item_id, cavity_count, status) VALUES ($1, 'MOLD-GET-1', 'Mold', $2, 4, 0)",
            &*mold_id,
            &*item_id,
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let detail = execute(&state.pg_pool, GetMoldPath { id: mold_id })
            .await
            .unwrap();
        assert_eq!(detail.data.code, "MOLD-GET-1");
        assert_eq!(detail.data.cavity_count, 4);
        assert_eq!(detail.data.life_used.unwrap_or(-1), 0);
    }

    #[sqlx::test]
    async fn test_mold_get_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, GetMoldPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("mold_not_found"));
    }
}
