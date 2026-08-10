//! 采购退货单聚合的持久化变更。

use crate::shared::flow::RET_FLOW;
use purchase_contract::error::PurchaseError;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

/// 采购退货单聚合写库。
///
/// 动作方法（`submit` / `approve`）把「锁定读 + 状态机规则 + 写状态」一步完成，
/// 调用方只负责事务边界；审批后的库存台账副作用由调用方在同一事务内执行。
pub struct PurchaseReturnRepository;

impl PurchaseReturnRepository {
    /// 提交：锁定读 + 状态机校验 + 写入新状态。
    pub async fn submit(conn: &mut PgConnection, id: &ID) -> Result<()> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = RET_FLOW.submit_status(status)?;
        sqlx::query!(
            r#"UPDATE purchase_returns SET status = $1 WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// 审批：锁定读 + 状态机校验 + 写入新状态并记录审批时间，返回新状态。
    pub async fn approve(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = RET_FLOW.approve_status(status)?;
        sqlx::query!(
            r#"UPDATE purchase_returns SET status = $1, approved_at = NOW() WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(new_status)
    }

    async fn lock_status(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let row = sqlx::query!(
            r#"SELECT status FROM purchase_returns WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(conn)
        .await?
        .ok_or(PurchaseError::NotFound)?;
        Ok(row.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::insert_test_purchase_order;
    use migration::run_migrations;
    use purchase_contract::value_object::PurchaseReturnStatus;
    use sqlx::Acquire;

    async fn seed_return(pool: &sqlx::PgPool, code: &str, status: i16) -> ID {
        let order_id = insert_test_purchase_order(pool, &format!("PO-{code}"), 0).await;
        let supplier_id = sqlx::query_scalar!(
            "SELECT supplier_id FROM purchase_orders WHERE id = $1",
            &*order_id
        )
        .fetch_one(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        let id = ID::new();
        sqlx::query!(
            r#"INSERT INTO purchase_returns (id, code, order_id, supplier_id, status)
               VALUES ($1, $2, $3, $4, $5)"#,
            &*id,
            code,
            &*order_id,
            supplier_id,
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
        let id = seed_return(&pool, "RET-T1", 0).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        PurchaseReturnRepository::submit(&mut txn, &id)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let status = sqlx::query_scalar!("SELECT status FROM purchase_returns WHERE id = $1", &*id)
            .fetch_one(&mut *pool.acquire().await.unwrap())
            .await
            .unwrap();
        assert_eq!(status, PurchaseReturnStatus::Submitted as i16);
    }

    #[sqlx::test]
    async fn test_submit_already_submitted_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_return(&pool, "RET-T2", 1).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let err = PurchaseReturnRepository::submit(&mut txn, &id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_approve_pending_success(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_return(&pool, "RET-T3", 1).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let new_status = PurchaseReturnRepository::approve(&mut txn, &id)
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert_eq!(new_status, PurchaseReturnStatus::Approved as i16);

        let row = sqlx::query!(
            "SELECT status, approved_at FROM purchase_returns WHERE id = $1",
            &*id
        )
        .fetch_one(&mut *pool.acquire().await.unwrap())
        .await
        .unwrap();
        assert_eq!(row.status, PurchaseReturnStatus::Approved as i16);
        assert!(row.approved_at.is_some());
    }

    #[sqlx::test]
    async fn test_approve_draft_rejected(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");
        let id = seed_return(&pool, "RET-T4", 0).await;

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let err = PurchaseReturnRepository::approve(&mut txn, &id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid_status_transition"));
    }

    #[sqlx::test]
    async fn test_not_found(pool: sqlx::PgPool) {
        run_migrations(&pool).await.expect("run migrations");

        let mut conn = pool.acquire().await.unwrap();
        let mut txn = conn.begin().await.unwrap();
        let err = PurchaseReturnRepository::submit(&mut txn, &ID::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("purchase_order_not_found"));
    }
}
