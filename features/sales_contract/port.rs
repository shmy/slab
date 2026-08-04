use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::entity::SalesOrder;

/// 销售跨域只读 Port
pub struct SalesPort;

impl SalesPort {
    pub async fn order_by_id(conn: &mut PgConnection, id: &ID) -> Result<SalesOrder> {
        let row = sqlx::query!(
            r#"SELECT id, code, customer_id, status, order_date,
                      currency, total_amount, remark, created_by
               FROM sales_orders WHERE id = $1"#,
            id as _
        )
        .fetch_optional(conn)
        .await?
        .ok_or(crate::error::SalesError::NotFound)?;
        Ok(SalesOrder {
            id: ID::new_unchecked(row.id),
            code: row.code,
            customer_id: ID::new_unchecked(row.customer_id),
            status: row.status,
            order_date: row.order_date,
            currency: row.currency,
            total_amount: row.total_amount,
            remark: row.remark,
            created_by: row.created_by.map(ID::new_unchecked),
        })
    }
}
