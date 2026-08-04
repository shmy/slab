use item_contract::entity::ItemUnit;
use rootcause::Result;
use sqlx::PgConnection;

pub struct ItemUnitRepository;

impl ItemUnitRepository {
    pub async fn create(conn: &mut PgConnection, unit: &ItemUnit) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO item_units (id, item_id, unit, rate) VALUES ($1, $2, $3, $4)"#,
            &*unit.id,
            &*unit.item_id,
            unit.unit,
            unit.rate,
        )
        .execute(conn)
        .await?;
        Ok(())
    }
}
