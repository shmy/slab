use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use inventory_ledger::{InventoryLedger, LedgerCommand, TransactionType};
use sales_contract::error::SalesError;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct DeliveryLineInput {
    pub order_line_id: ID,
    pub item_id: ID,
    pub warehouse_id: ID,
    pub quantity: i64,
    pub batch_number: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateDeliveryRequest {
    pub order_id: ID,
    pub remark: Option<String>,
    pub lines: Vec<DeliveryLineInput>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateDeliveryResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(post, path = "/api/v1/sales-deliveries",
    operation_id = "sales_delivery_create", tag = "sales-delivery",
    request_body = CreateDeliveryRequest,
    responses((status = 200, body = JsonResponse<CreateDeliveryResponse>)),
    security(("bearerAuth" = [])))]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidJson(request): ValidJson<CreateDeliveryRequest>,
) -> JsonResponseType<CreateDeliveryResponse> {
    let response = execute(&pg_pool, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    request: CreateDeliveryRequest,
) -> rootcause::Result<CreateDeliveryResponse> {
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;

    let order = sqlx::query!(
        "SELECT customer_id, status FROM sales_orders WHERE id = $1 FOR UPDATE",
        &*request.order_id
    )
    .fetch_optional(&mut *txn)
    .await?
    .ok_or(SalesError::NotFound)?;
    if order.status != 3 {
        return Err(SalesError::OrderNotApproved.into());
    }

    let code = CodeGen::next_code(&mut txn, "seq_sales_delivery", "DLV").await?;
    let delivery_id = ID::new();

    sqlx::query!(
        r#"INSERT INTO sales_deliveries (id, code, order_id, customer_id, remark, status)
           VALUES ($1, $2, $3, $4, $5, 1)"#,
        &*delivery_id,
        code,
        &*request.order_id,
        &*ID::new_unchecked(order.customer_id),
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    for line in &request.lines {
        let line_id = ID::new();
        let ol = sqlx::query!(
            r#"SELECT quantity, delivered_qty FROM sales_order_lines WHERE id = $1 AND order_id = $2 FOR UPDATE"#,
            &*line.order_line_id, &*request.order_id,
        ).fetch_optional(&mut *txn).await?.ok_or(SalesError::LineNotFound)?;

        let new_delivered = ol.delivered_qty + line.quantity;
        if new_delivered > ol.quantity {
            return Err(SalesError::OverDelivery.into());
        }

        sqlx::query!(
            r#"INSERT INTO sales_delivery_lines (id, delivery_id, order_line_id, item_id, warehouse_id, quantity, batch_number)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            &*line_id, &*delivery_id, &*line.order_line_id, &*line.item_id, &*line.warehouse_id, line.quantity, line.batch_number,
        ).execute(&mut *txn).await?;

        sqlx::query!("UPDATE sales_order_lines SET delivered_qty = $1, closed = ($1 >= quantity) WHERE id = $2", new_delivered, &*line.order_line_id)
            .execute(&mut *txn).await?;

        InventoryLedger::issue(
            &mut txn,
            &LedgerCommand {
                item_id: &line.item_id,
                warehouse_id: &line.warehouse_id,
                quantity: line.quantity,
                tx_type: TransactionType::Outbound,
                reference_type: "sales_delivery",
                reference_id: &line_id,
                batch_number: line.batch_number.as_deref(),
            },
        )
        .await?;
    }
    txn.commit().await?;
    Ok(CreateDeliveryResponse {
        id: delivery_id,
        code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    /// 造一个已审批订单（status=3）+ 一行 100kg 明细 + 1000kg 库存。
    async fn seed_approved_order(state: &appctx::AppCtx, code: &str) -> (ID, ID, ID, ID) {
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-DLV-1").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-DLV-1").await;
        let warehouse_id = tests::insert_test_warehouse(&state.pg_pool, "WH-DLV-1").await;
        tests::insert_test_inventory(&state.pg_pool, &item_id, &warehouse_id, 1000).await;
        let order_id = tests::insert_test_sales_order(&state.pg_pool, code, &customer_id, 3).await;
        let line_id =
            tests::insert_test_sales_order_line(&state.pg_pool, &order_id, &item_id, 100, 0).await;
        (order_id, line_id, item_id, warehouse_id)
    }

    #[sqlx::test]
    async fn test_delivery_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let (order_id, line_id, item_id, warehouse_id) =
            seed_approved_order(&state, "SO-DLV-1").await;

        let req = CreateDeliveryRequest {
            order_id,
            remark: None,
            lines: vec![DeliveryLineInput {
                order_line_id: line_id,
                item_id,
                warehouse_id,
                quantity: 30,
                batch_number: None,
            }],
        };
        let resp = execute(&state.pg_pool, req).await.unwrap();
        assert!(resp.code.starts_with("DLV-"));

        // 订单行累计发货量 + 库存扣减
        let row = sqlx::query!(
            "SELECT delivered_qty, closed FROM sales_order_lines WHERE id = $1",
            &*line_id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.delivered_qty, 30);
        assert!(!row.closed);

        let inv = sqlx::query!(
            "SELECT quantity FROM inventories WHERE item_id = $1 AND warehouse_id = $2",
            &*item_id,
            &*warehouse_id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(inv.quantity, 970);
    }

    #[sqlx::test]
    async fn test_delivery_order_not_approved_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-DLV-2").await;
        let order_id =
            tests::insert_test_sales_order(&state.pg_pool, "SO-DLV-2", &customer_id, 0).await;

        let req = CreateDeliveryRequest {
            order_id,
            remark: None,
            lines: vec![],
        };
        let err = execute(&state.pg_pool, req).await.unwrap_err();
        assert!(err.to_string().contains("sales_order_not_approved"));
    }

    #[sqlx::test]
    async fn test_delivery_over_deliver_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let (order_id, line_id, item_id, warehouse_id) =
            seed_approved_order(&state, "SO-DLV-3").await;

        let req = CreateDeliveryRequest {
            order_id,
            remark: None,
            lines: vec![DeliveryLineInput {
                order_line_id: line_id,
                item_id,
                warehouse_id,
                quantity: 150, // 超出订单行 100
                batch_number: None,
            }],
        };
        let err = execute(&state.pg_pool, req).await.unwrap_err();
        assert!(err.to_string().contains("sales_over_delivery"));
    }

    #[sqlx::test]
    async fn test_delivery_insufficient_inventory(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-DLV-4").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-DLV-4").await;
        let warehouse_id = tests::insert_test_warehouse(&state.pg_pool, "WH-DLV-4").await;
        tests::insert_test_inventory(&state.pg_pool, &item_id, &warehouse_id, 10).await;
        let order_id =
            tests::insert_test_sales_order(&state.pg_pool, "SO-DLV-4", &customer_id, 3).await;
        let line_id =
            tests::insert_test_sales_order_line(&state.pg_pool, &order_id, &item_id, 100, 0).await;

        let req = CreateDeliveryRequest {
            order_id,
            remark: None,
            lines: vec![DeliveryLineInput {
                order_line_id: line_id,
                item_id,
                warehouse_id,
                quantity: 30, // 库存只有 10
                batch_number: None,
            }],
        };
        let err = execute(&state.pg_pool, req).await.unwrap_err();
        assert!(err.to_string().contains("insufficient_inventory"));
    }

    #[sqlx::test]
    async fn test_delivery_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;

        let req = CreateDeliveryRequest {
            order_id: ID::new(),
            remark: None,
            lines: vec![],
        };
        let err = execute(&state.pg_pool, req).await.unwrap_err();
        assert!(err.to_string().contains("sales_document_not_found"));
    }
}
