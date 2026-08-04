use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::entity::InspectionOrder;

/// 质检跨域只读 Port
pub struct QualityPort;

impl QualityPort {
    pub async fn inspection_by_id(conn: &mut PgConnection, id: &ID) -> Result<InspectionOrder> {
        let row = sqlx::query!(
            r#"SELECT id, code, template_id, source_type, source_id,
                      item_id, lot_qty, sample_qty, inspector,
                      result, status, inspected_at
               FROM inspection_orders WHERE id = $1"#,
            id as _
        )
        .fetch_optional(conn)
        .await?
        .ok_or(crate::error::QualityError::InspectionNotFound)?;
        Ok(InspectionOrder {
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
        })
    }
}
