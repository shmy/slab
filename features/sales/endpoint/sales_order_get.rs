use axum::extract::State;
use db::PgPool;
use sales_contract::entity::{SalesOrder, SalesOrderLine};
use sales_contract::error::SalesError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use utoipa::{IntoParams, ToSchema};
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetSOPath {
    pub id: ID,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SalesOrderDetail {
    pub order: SalesOrder,
    pub lines: Vec<SalesOrderLine>,
}

#[utoipa::path(get, path = "/api/v1/sales-orders/{id}", operation_id = "sales_order_get", tag = "sales-order",
    params(GetSOPath), responses((status = 200, body = JsonResponse<SalesOrderDetail>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetSOPath>,
) -> JsonResponseType<SalesOrderDetail> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetSOPath) -> rootcause::Result<SalesOrderDetail> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!("SELECT id, code, customer_id, status, order_date, currency, total_amount, remark, created_by FROM sales_orders WHERE id = $1", &*path.id)
        .fetch_optional(&mut *conn).await?.ok_or(SalesError::NotFound)?;
    let lines = sqlx::query!("SELECT id, order_id, line_no, item_id, quantity, unit, unit_price, line_total, delivered_qty, returned_qty, closed, remark FROM sales_order_lines WHERE order_id = $1 ORDER BY line_no", &*path.id)
        .fetch_all(&mut *conn).await?;
    Ok(SalesOrderDetail {
        order: SalesOrder {
            id: ID::new_unchecked(row.id),
            code: row.code,
            customer_id: ID::new_unchecked(row.customer_id),
            status: row.status,
            order_date: row.order_date,
            currency: row.currency,
            total_amount: row.total_amount,
            remark: row.remark,
            created_by: row.created_by.map(ID::new_unchecked),
        },
        lines: lines
            .into_iter()
            .map(|r| SalesOrderLine {
                id: ID::new_unchecked(r.id),
                order_id: ID::new_unchecked(r.order_id),
                line_no: r.line_no,
                item_id: ID::new_unchecked(r.item_id),
                quantity: r.quantity,
                unit: r.unit,
                unit_price: r.unit_price,
                line_total: r.line_total,
                delivered_qty: r.delivered_qty,
                returned_qty: r.returned_qty,
                closed: r.closed,
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
    use sales_contract::value_object::SalesOrderStatus;

    #[sqlx::test]
    async fn test_get_order_with_lines(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-GET-1").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-GET-1").await;
        let order_id =
            tests::insert_test_sales_order(&state.pg_pool, "SO-GET-1", &customer_id, 0).await;
        tests::insert_test_sales_order_line(&state.pg_pool, &order_id, &item_id, 10, 3).await;

        let detail = execute(&state.pg_pool, GetSOPath { id: order_id })
            .await
            .unwrap();
        assert_eq!(detail.order.code, "SO-GET-1");
        assert_eq!(detail.order.status, SalesOrderStatus::Draft as i16);
        assert_eq!(detail.lines.len(), 1);
        assert_eq!(detail.lines[0].quantity, 10);
        assert_eq!(detail.lines[0].delivered_qty, 3);
        assert!(!detail.lines[0].closed);
    }

    #[sqlx::test]
    async fn test_get_order_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let err = execute(&state.pg_pool, GetSOPath { id: ID::new() })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("sales_document_not_found"));
    }
}
