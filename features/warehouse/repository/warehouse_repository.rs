use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;
use warehouse_contract::entity::Warehouse;

pub struct WarehouseRepository;

impl WarehouseRepository {
    pub async fn create(conn: &mut PgConnection, warehouse: &Warehouse) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO warehouses (id, code, name, type, is_active)
               VALUES ($1, $2, $3, $4, $5)"#,
            &*warehouse.id,
            warehouse.code,
            warehouse.name,
            warehouse.r#type as i16,
            warehouse.is_active,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn delete(conn: &mut PgConnection, id: &ID) -> Result<bool> {
        let affected = sqlx::query!(
            r#"UPDATE warehouses SET is_active = FALSE WHERE id = $1"#,
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(affected.rows_affected() > 0)
    }
}
