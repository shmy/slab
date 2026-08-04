use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::entity::PurchaseOrder;

/// 采购跨域只读 Port
pub struct PurchasePort;

impl PurchasePort {
    pub async fn order_by_id(conn: &mut PgConnection, id: &ID) -> Result<PurchaseOrder> {
        let row = sqlx::query!(
            r#"SELECT id, code, supplier_id, status, order_date,
                      expected_delivery_date, currency, total_amount,
                      payment_terms, remark, created_by
               FROM purchase_orders WHERE id = $1"#,
            id as _
        )
        .fetch_optional(conn)
        .await?
        .ok_or(crate::error::PurchaseError::NotFound)?;
        Ok(PurchaseOrder {
            id: ID::new_unchecked(row.id),
            code: row.code,
            supplier_id: ID::new_unchecked(row.supplier_id),
            status: row.status,
            order_date: row.order_date,
            expected_delivery_date: row.expected_delivery_date,
            currency: row.currency,
            total_amount: row.total_amount,
            payment_terms: row.payment_terms,
            remark: row.remark,
            created_by: row.created_by.map(ID::new_unchecked),
        })
    }
}
