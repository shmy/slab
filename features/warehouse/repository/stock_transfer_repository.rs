//! 调拨单聚合的持久化变更。

use crate::shared::flow::TR_FLOW;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;
use warehouse_contract::error::WarehouseError;

/// 调拨单聚合写库。
///
/// 动作方法（`submit` / `approve`）把「锁定读 + 状态机规则 + 写状态」一步完成，
/// 调用方只负责事务边界；审批后的库存台账副作用由调用方在同一事务内执行。
pub struct StockTransferRepository;

/// 审批动作返回的锁定快照：供调用方在同一事务内执行库存台账副作用。
#[derive(Debug)]
pub(crate) struct LockedStockTransfer {
    pub from_warehouse_id: i64,
    pub to_warehouse_id: i64,
}

impl StockTransferRepository {
    /// 提交：锁定读 + 状态机校验 + 写入新状态。
    pub async fn submit(conn: &mut PgConnection, id: &ID) -> Result<()> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = TR_FLOW.submit_status(status)?;
        sqlx::query!(
            r#"UPDATE stock_transfers SET status = $1 WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// 审批：锁定读 + 状态机校验 + 写入新状态并记录审批时间；
    /// 返回锁定快照，调用方据此在同一事务内执行库存台账副作用。
    pub async fn approve(conn: &mut PgConnection, id: &ID) -> Result<LockedStockTransfer> {
        let row = sqlx::query!(
            r#"SELECT status, from_warehouse_id, to_warehouse_id
               FROM stock_transfers WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(WarehouseError::NotFound)?;

        let new_status = TR_FLOW.approve_status(row.status)?;
        sqlx::query!(
            r#"UPDATE stock_transfers SET status = $1, approved_at = NOW() WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(LockedStockTransfer {
            from_warehouse_id: row.from_warehouse_id,
            to_warehouse_id: row.to_warehouse_id,
        })
    }

    async fn lock_status(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let row = sqlx::query!(
            r#"SELECT status FROM stock_transfers WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(WarehouseError::NotFound)?;
        Ok(row.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
use crate::tests::insert_test_warehouse;
use migration::run_migrations;
use sqlx::Acquire;
use warehouse_contract::value_object::StockTransferStatus;

    async fn seed_transfer(pool: &sqlx::PgPool, code: &str, status: i16) -> ID {
        let from_wh = insert_test_warehouse(pool, &format!("W-{code}-A")).await;
        let to_wh = insert_test_warehouse(pool, &format!("W-{code}-B")).await;
        let id = ID::new();
        sqlx::query!(
            r#"INSERT INTO stock_transfers (id, code, from_warehouse_id, to_warehouse_id, status, transfer_date)
               VALUES ($1, $2, $3, $4, $5, CURRENT_DATE)"#,
            &*id,
            code,
            &*from_wh,
            &*to_wh,
            status,
        )
        .execute(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        id
    }

    #[sqlx::test]
    async fn test_submit_draft_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_transfer(&pool, "TR-T1", StockTransferStatus::Draft as i16).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        StockTransferRepository::submit(&mut txn, &id)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let status = sqlx::query_scalar!("SELECT status FROM stock_transfers WHERE id = $1", &*id)
            .fetch_one(&mut *pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, StockTransferStatus::Submitted as i16);
    }

    #[sqlx::test]
    async fn test_submit_already_submitted_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_transfer(&pool, "TR-T2", StockTransferStatus::Submitted as i16).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let err = StockTransferRepository::submit(&mut txn, &id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_approve_pending_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_transfer(&pool, "TR-T3", StockTransferStatus::Submitted as i16).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let locked = StockTransferRepository::approve(&mut txn, &id)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let row = sqlx::query!(
            "SELECT status, approved_at FROM stock_transfers WHERE id = $1",
            &*id
        )
        .fetch_one(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.status, StockTransferStatus::Approved as i16);
        assert!(row.approved_at.is_some());
        assert!(locked.from_warehouse_id != locked.to_warehouse_id);
    }

    #[sqlx::test]
    async fn test_approve_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_transfer(&pool, "TR-T4", StockTransferStatus::Draft as i16).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let err = StockTransferRepository::approve(&mut txn, &id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let err = StockTransferRepository::submit(&mut txn, &ID::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("warehouse_not_found"));
    }
}
