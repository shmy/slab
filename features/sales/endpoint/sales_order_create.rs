use audit_contract::AuditService;
use axum::extract::State;
use code_gen::CodeGen;
use db::PgPool;
use http_auth::extract::operator::OperatorContext;
use sales_contract::entity::SalesOrder;
use serde::{Deserialize, Serialize};
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use utoipa::ToSchema;
use validify::Validify;
use web::extract::valid_json::ValidJson;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateOrderLine {
    pub item_id: ID,
    pub quantity: i64,
    pub unit: String,
    pub unit_price: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validify, ToSchema)]
pub(crate) struct CreateSalesOrderRequest {
    pub customer_id: ID,
    pub remark: Option<String>,
    pub lines: Vec<CreateOrderLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CreateSalesOrderResponse {
    pub id: ID,
    pub code: String,
}

#[utoipa::path(
    post, path = "/api/v1/sales-orders",
    operation_id = "sales_order_create", tag = "sales-order",
    request_body = CreateSalesOrderRequest,
    responses((status = 200, body = JsonResponse<CreateSalesOrderResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ctx: OperatorContext,
    ValidJson(request): ValidJson<CreateSalesOrderRequest>,
) -> JsonResponseType<CreateSalesOrderResponse> {
    let response = execute(&pg_pool, ctx, request).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    pg_pool: &PgPool,
    ctx: OperatorContext,
    request: CreateSalesOrderRequest,
) -> rootcause::Result<CreateSalesOrderResponse> {
    if request.lines.is_empty() {
        return Err(sales_contract::error::SalesError::EmptyOrder.into());
    }

    let id = ID::new();
    let mut conn = pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let code = CodeGen::next_code(&mut txn, "seq_sales_order", "SO").await?;

    let mut total_amount = 0i64;
    let mut lines: Vec<(ID, &CreateOrderLine)> = Vec::new();
    for line in &request.lines {
        let line_id = ID::new();
        total_amount += line.quantity * line.unit_price;
        lines.push((line_id, line));
    }

    sqlx::query!(
        r#"INSERT INTO sales_orders (id, code, customer_id, status, total_amount, remark)
           VALUES ($1, $2, $3, 0, $4, $5)"#,
        &*id,
        code,
        &*request.customer_id,
        total_amount,
        request.remark,
    )
    .execute(&mut *txn)
    .await?;

    for (i, (line_id, line)) in lines.into_iter().enumerate() {
        let line_total = line.quantity * line.unit_price;
        sqlx::query!(
            r#"INSERT INTO sales_order_lines (id, order_id, line_no, item_id, quantity, unit,
                unit_price, line_total, remark) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            &*line_id,
            &*id,
            i as i16 + 1,
            &*line.item_id,
            line.quantity,
            line.unit,
            line.unit_price,
            line_total,
            line.remark,
        )
        .execute(&mut *txn)
        .await?;
    }

    // 变更历史：同事务回读整行作为创建快照
    let row = sqlx::query!(
        r#"SELECT id, code, customer_id, status, order_date, currency, total_amount, remark, created_by
           FROM sales_orders WHERE id = $1"#,
        &*id
    )
    .fetch_one(&mut *txn)
    .await?;
    let order = SalesOrder {
        id: ID::new_unchecked(row.id),
        code: row.code,
        customer_id: ID::new_unchecked(row.customer_id),
        status: row.status,
        order_date: row.order_date,
        currency: row.currency,
        total_amount: row.total_amount,
        remark: row.remark,
        created_by: row.created_by.map(ID::new_unchecked),
    };
    AuditService::record_create(&mut txn, "sales_order", &id, &ctx, &order).await?;

    txn.commit().await?;
    Ok(CreateSalesOrderResponse { id, code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_create_order_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-SO-1").await;
        let item_id = tests::insert_test_item(&state.pg_pool, "I-SO-1").await;

        let req = CreateSalesOrderRequest {
            customer_id,
            remark: None,
            lines: vec![
                CreateOrderLine {
                    item_id,
                    quantity: 10,
                    unit: "kg".into(),
                    unit_price: 100,
                    remark: None,
                },
                CreateOrderLine {
                    item_id,
                    quantity: 5,
                    unit: "kg".into(),
                    unit_price: 200,
                    remark: None,
                },
            ],
        };

        let resp = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap();
        assert!(resp.code.starts_with("SO-"));

        let row = sqlx::query!(
            "SELECT status, total_amount FROM sales_orders WHERE id = $1",
            &*resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.status, 0);
        // 10*100 + 5*200 = 2000
        assert_eq!(row.total_amount, 2000);

        // 变更历史：create 类型，before 为空
        let audit_row = sqlx::query!(
            r#"SELECT action, entity, before, after FROM audit_logs WHERE entity_id = $1"#,
            *resp.id
        )
        .fetch_one(&mut *state.pg_pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(audit_row.action, 1); // Created
        assert_eq!(audit_row.entity, "sales_order");
        assert!(audit_row.before.is_none());
        let after: serde_json::Value = audit_row.after.unwrap();
        assert_eq!(after["code"], resp.code);
        assert_eq!(after["total_amount"], 2000);
        assert_eq!(after["status"], 0);
    }

    #[sqlx::test]
    async fn test_create_order_empty_lines_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let state = testing::build(pool.clone()).await;
        let customer_id = tests::insert_test_customer(&state.pg_pool, "C-SO-2").await;

        let req = CreateSalesOrderRequest {
            customer_id,
            remark: None,
            lines: vec![],
        };
        let err = execute(&state.pg_pool, tests::test_operator_context(), req)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("sales_order_empty"));
    }
}
