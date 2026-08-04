use axum::extract::State;
use db::PgPool;
use finance_contract::entity::Payment;
use finance_contract::error::FinanceError;
use serde::Deserialize;
use shared_contract::value_object::id::ID;
use utoipa::IntoParams;
use validify::Validify;
use web::extract::valid_path::ValidPath;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(Debug, Deserialize, Validify, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct GetPaymentPath {
    pub id: ID,
}

#[utoipa::path(
    get, path = "/api/v1/payments/{id}",
    operation_id = "payment_get", tag = "payment",
    params(GetPaymentPath),
    responses((status = 200, body = JsonResponse<Payment>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(pg_pool))]
pub(crate) async fn handler(
    State(pg_pool): State<PgPool>,
    ValidPath(path): ValidPath<GetPaymentPath>,
) -> JsonResponseType<Payment> {
    let response = execute(&pg_pool, path).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(pg_pool: &PgPool, path: GetPaymentPath) -> rootcause::Result<Payment> {
    let mut conn = pg_pool.acquire().await?;
    let row = sqlx::query!(
        r#"SELECT id, code, payment_type, invoice_type, invoice_id, amount,
                  payment_date, payment_method, remark
           FROM payments WHERE id = $1"#,
        &*path.id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(FinanceError::PaymentNotFound)?;

    Ok(Payment {
        id: ID::new_unchecked(row.id),
        code: row.code,
        payment_type: row.payment_type,
        invoice_type: row.invoice_type,
        invoice_id: ID::new_unchecked(row.invoice_id),
        amount: row.amount,
        payment_date: row.payment_date,
        payment_method: row.payment_method,
        remark: row.remark,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use appctx::testing;
    use migration::run_migrations;

    #[sqlx::test]
    async fn test_get_payment(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;
        let mut conn = pool.acquire().await.unwrap();

        let id = ID::new();
        let inv_id = ID::new();
        sqlx::query!(
            r#"INSERT INTO payments (id, code, payment_type, invoice_type, invoice_id, amount, payment_date)
               VALUES ($1, 'PAY-20250101-000001', 1, 'sales_invoice', $2, 5000, '2025-01-01')"#,
            &*id, &*inv_id
        ).execute(&mut *conn).await.unwrap();

        let payment = execute(&pool, GetPaymentPath { id }).await.unwrap();
        assert_eq!(payment.code, "PAY-20250101-000001");
        assert_eq!(payment.amount, 5000);
        assert_eq!(payment.payment_type, 1);
        assert_eq!(payment.invoice_type, "sales_invoice");
    }

    #[sqlx::test]
    async fn test_get_payment_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let _state = testing::build(pool.clone()).await;

        let err = execute(&pool, GetPaymentPath { id: ID::new() })
            .await
            .unwrap_err();
        let msg = err.to_string();
        eprintln!("ERROR MSG: {msg}");
        eprintln!("ERROR DBG: {err:?}");
        assert!(
            msg.contains("payment_not_found"),
            "error message does not contain 'payment_not_found': {msg}"
        );
    }
}
