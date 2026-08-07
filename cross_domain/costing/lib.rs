//! 成本核算 — 加权平均等成本计算逻辑。
//!
//! 供采购收货等跨域写操作调用。与 `inventory_ledger` 同为 infrastructure 层深度模块。

use item_contract::entity::{CostType, ItemCost};
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

pub struct CostCalculator;

impl CostCalculator {
    /// 采购收货后调用：按加权平均法重算物料均价。
    /// 在同一事务内执行，返回新的 unit_cost（分）。
    #[tracing::instrument(skip_all)]
    #[inline]
    pub async fn recalc_weighted_average(
        conn: &mut PgConnection,
        item_id: &ID,
        receipt_qty: i64,
        receipt_unit_cost: i64,
    ) -> Result<i64> {
        // 1. 查当前均价（可能无记录，库存为 0 或首次收货）
        let current = sqlx::query!(
            r#"SELECT id, unit_cost FROM item_costs
               WHERE item_id = $1 AND cost_type = $2 AND is_current = TRUE"#,
            item_id as _,
            CostType::WeightedAverage as i16,
        )
        .fetch_optional(&mut *conn)
        .await?;

        // 2. 查当前总库存（跨仓库求和——库存更新后）
        let total_stock = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(quantity), 0)::BIGINT AS "total!"
               FROM inventories WHERE item_id = $1"#,
            item_id as _
        )
        .fetch_one(&mut *conn)
        .await?;

        // 3. 计算新均价
        //    新均价 = (旧总库存 × 旧均价 + 收货量 × 收货价) / (旧总库存 + 收货量)
        //    但 旧总库存 = total_stock - receipt_qty（库存已更新）
        let old_stock = total_stock - receipt_qty;
        let new_avg = match current {
            Some(ref r) if old_stock > 0 => {
                (old_stock * r.unit_cost + receipt_qty * receipt_unit_cost)
                    / (old_stock + receipt_qty)
            }
            _ => receipt_unit_cost, // 首次或无库存
        };

        // 4. 翻转旧记录的 is_current
        if let Some(r) = current {
            sqlx::query!(
                "UPDATE item_costs SET is_current = FALSE WHERE id = $1",
                r.id
            )
            .execute(&mut *conn)
            .await?;
        }

        // 5. 插入新均价记录
        let cost = ItemCost {
            id: ID::new(),
            item_id: *item_id,
            cost_type: CostType::WeightedAverage,
            unit_cost: new_avg,
            currency: "CNY".to_string(),
            effective_at: chrono::Utc::now(),
            is_current: true,
            remark: None,
        };
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
        Ok(new_avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::run_migrations;
    use sqlx::PgPool;

    async fn seed_item(pool: &PgPool, code: &str) -> ID {
        let id = ID::new();
        sqlx::query!(
            "INSERT INTO items (id, code, name, item_type, base_unit) VALUES ($1, $2, 'TestItem', 1, 'kg')",
            &*id,
            code,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        id
    }

    async fn seed_warehouse(pool: &PgPool, code: &str) -> ID {
        let id = ID::new();
        sqlx::query!(
            "INSERT INTO warehouses (id, code, name, type, is_active) VALUES ($1, $2, 'TestWH', 3, true)",
            &*id,
            code,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        id
    }

    async fn seed_inventory(pool: &PgPool, item_id: &ID, quantity: i64) {
        let wh = seed_warehouse(pool, "WH-CST").await;
        sqlx::query!(
            "INSERT INTO inventories (id, item_id, warehouse_id, quantity, locked_qty, version) VALUES ($1, $2, $3, $4, 0, 1)",
            &*ID::new(),
            item_id as _,
            &*wh,
            quantity,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
    }

    async fn seed_cost(pool: &PgPool, item_id: &ID, unit_cost: i64) {
        sqlx::query!(
            r#"INSERT INTO item_costs (id, item_id, cost_type, unit_cost, currency, effective_at, is_current)
               VALUES ($1, $2, $3, $4, 'CNY', CURRENT_TIMESTAMP, TRUE)"#,
            &*ID::new(),
            item_id as _,
            CostType::WeightedAverage as i16,
            unit_cost,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
    }

    async fn current_cost(pool: &PgPool, item_id: &ID) -> (i64, bool) {
        let row = sqlx::query!(
            r#"SELECT unit_cost, is_current FROM item_costs
               WHERE item_id = $1 AND cost_type = $2 AND is_current = TRUE"#,
            item_id as _,
            CostType::WeightedAverage as i16,
        )
        .fetch_one(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        (row.unit_cost, row.is_current)
    }

    #[sqlx::test]
    async fn test_first_receipt_uses_receipt_price(pool: PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item = seed_item(&pool, "CST-1").await;
        seed_inventory(&pool, &item, 100).await;

        let new_avg = CostCalculator::recalc_weighted_average(
            &mut *pool.acquire().await.unwrap(),
            &item,
            100,
            500,
        )
        .await
        .unwrap();
        assert_eq!(new_avg, 500);

        let (unit_cost, _) = current_cost(&pool, &item).await;
        assert_eq!(unit_cost, 500);
    }

    #[sqlx::test]
    async fn test_weighted_average_blends_old_and_new(pool: PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item = seed_item(&pool, "CST-2").await;
        // 前置条件：收货已入账（total = 200，其中本次收货 100）
        // 旧库存 100 @ 100 分；本次收货 100 @ 200 分 → (100×100 + 100×200)/200 = 150
        seed_inventory(&pool, &item, 200).await;
        seed_cost(&pool, &item, 100).await;

        let new_avg = CostCalculator::recalc_weighted_average(
            &mut *pool.acquire().await.unwrap(),
            &item,
            100,
            200,
        )
        .await
        .unwrap();
        assert_eq!(new_avg, 150);
    }

    #[sqlx::test]
    async fn test_weighted_average_non_divisible_rounds_down(pool: PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item = seed_item(&pool, "CST-3").await;
        // 前置条件：收货已入账（total = 5，其中本次收货 2）
        // 旧库存 3 @ 100；收货 2 @ 1000 → (300 + 2000)/5 = 460（整数除法）
        seed_inventory(&pool, &item, 5).await;
        seed_cost(&pool, &item, 100).await;

        let new_avg = CostCalculator::recalc_weighted_average(
            &mut *pool.acquire().await.unwrap(),
            &item,
            2,
            1000,
        )
        .await
        .unwrap();
        assert_eq!(new_avg, 460);
    }

    #[sqlx::test]
    async fn test_receipt_with_zero_old_stock_uses_receipt_price(pool: PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item = seed_item(&pool, "CST-4").await;
        // 有成本记录但库存为 0（如全部出库后再次收货）→ 新均价 = 收货价
        seed_cost(&pool, &item, 999).await;

        let new_avg = CostCalculator::recalc_weighted_average(
            &mut *pool.acquire().await.unwrap(),
            &item,
            50,
            300,
        )
        .await
        .unwrap();
        assert_eq!(new_avg, 300);
    }

    #[sqlx::test]
    async fn test_old_cost_record_flipped_to_not_current(pool: PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let item = seed_item(&pool, "CST-5").await;
        // 前置条件：收货已入账（total = 20，其中本次收货 10）
        seed_inventory(&pool, &item, 20).await;
        seed_cost(&pool, &item, 100).await;

        CostCalculator::recalc_weighted_average(
            &mut *pool.acquire().await.unwrap(),
            &item,
            10,
            200,
        )
        .await
        .unwrap();

        // 旧记录被翻转，只有一条 is_current
        let rows = sqlx::query!(
            r#"SELECT unit_cost, is_current FROM item_costs
               WHERE item_id = $1 AND cost_type = $2 ORDER BY id"#,
            &*item,
            CostType::WeightedAverage as i16,
        )
        .fetch_all(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().filter(|r| r.is_current).count(), 1);
        // 新均价 = (10×100 + 10×200)/20 = 150
        let current = rows.iter().find(|r| r.is_current).unwrap();
        assert_eq!(current.unit_cost, 150);
    }
}
