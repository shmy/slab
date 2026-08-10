use axum::extract::State;
use db::PgPool;
use quality_contract::entity::{InspectionOrder, InspectionResult};
use quality_contract::error::QualityError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetInspectionPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct InspectionDetail {
    pub order: InspectionOrder,
    pub results: Vec<InspectionResult>,
}

#[utoipa::path(get, path = "/api/v1/inspection-orders/{id}", operation_id = "inspection_order_get", tag = "inspection-order",
    params(GetInspectionPath), responses((status = 200, body = JsonResponse<InspectionDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetInspectionPath>,
) -> JsonResponseType<InspectionDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetInspectionPath) -> rootcause::Result<InspectionDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!("SELECT id, code, template_id, source_type, source_id, item_id, lot_qty, sample_qty, inspector, result, status, inspected_at FROM inspection_orders WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn).await?.ok_or(QualityError::InspectionNotFound)?;
    let results = sqlx::query!("SELECT id, inspection_id, template_item_id, result, actual_value, remark FROM inspection_results WHERE inspection_id = $1", &*path.id).fetch_all(&mut *conn).await?;
    Ok(InspectionDetail {
        order: InspectionOrder {
            id: ID::new_unchecked(row.id),
            code: row.code,
            template_id: row.template_id.map(ID::new_unchecked),
            source_type: row.source_type,
            source_id: row.source_id,
            item_id: ID::new_unchecked(row.item_id),
            lot_qty: row.lot_qty,
            sample_qty: row.sample_qty,
            inspector: row.inspector,
            result: row.result,
            status: row.status,
            inspected_at: row.inspected_at,
        },
        results: results
            .into_iter()
            .map(|r| InspectionResult {
                id: ID::new_unchecked(r.id),
                inspection_id: ID::new_unchecked(r.inspection_id),
                template_item_id: ID::new_unchecked(r.template_item_id),
                result: r.result,
                actual_value: r.actual_value,
                remark: r.remark,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;
    use quality_contract::value_object::InspectionOrderStatus;

    #[sqlx::test]
    async fn test_inspection_order_get_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let template_id = tests::insert_test_template(&state.pg_pool, "TPL-GO-1").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-GO-1").await;
        let order_id =
            tests::insert_test_inspection_order(&state.pg_pool, "IQ-GO-1", &template_id, &item_id)
                .await;

        let detail = execute(&state.pg_pool, GetInspectionPath { id: order_id })
            .await
            .unwrap();
        assert_eq!(detail.order.code, "IQ-GO-1");
        assert_eq!(detail.order.status, InspectionOrderStatus::Pending as i16);
        assert_eq!(detail.order.lot_qty, 100);
        assert!(detail.results.is_empty());
    }

    #[sqlx::test]
    async fn test_inspection_order_get_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, GetInspectionPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("inspection_order_not_found"));
    }
}
