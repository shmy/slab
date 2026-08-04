use item_contract::entity::ItemCost;
use rootcause::Result;
use sqlx::PgConnection;

pub struct ItemCostRepository;

impl ItemCostRepository {
    pub async fn create(conn: &mut PgConnection, cost: &ItemCost) -> Result<()> {
        if cost.is_current {
            sqlx::query!(
                r#"UPDATE item_costs SET is_current = FALSE
                   WHERE item_id = $1 AND cost_type = $2 AND is_current = TRUE"#,
                &*cost.item_id,
                cost.cost_type as i16,
            )
            .execute(&mut *conn)
            .await?;
        }
        sqlx::query!(
            r#"INSERT INTO item_costs (id, item_id, cost_type, unit_cost, currency, effective_at, is_current, remark)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            &*cost.id,
            &*cost.item_id,
            cost.cost_type as i16,
            cost.unit_cost,
            cost.currency,
            cost.effective_at,
            cost.is_current,
            cost.remark,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
}
