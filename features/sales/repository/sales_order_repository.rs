//! 销售订单聚合的持久化变更。

use crate::shared::flow::SO_FLOW;
use rootcause::Result;
use sales_contract::error::SalesError;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

/// 销售订单聚合写库。
///
/// 动作方法（`submit` / `approve`）把「锁定读 + 状态机规则 + 写状态」一步完成，
/// 调用方只负责事务边界（锁行与写入必须处于同一事务，保证并发安全）。
pub struct SalesOrderRepository;

impl SalesOrderRepository {
    /// 提交：锁定读 + 状态机校验 + 写入新状态。
    pub async fn submit(conn: &mut PgConnection, id: &ID) -> Result<()> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = SO_FLOW.submit_status(status)?;
        sqlx::query!(
            r#"UPDATE sales_orders SET status = $1 WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// 审批：锁定读 + 状态机校验 + 写入新状态并记录审批时间。
    pub async fn approve(conn: &mut PgConnection, id: &ID) -> Result<()> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = SO_FLOW.approve_status(status)?;
        sqlx::query!(
            r#"UPDATE sales_orders SET status = $1, approved_at = NOW() WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    async fn lock_status(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let row = sqlx::query!(
            r#"SELECT status FROM sales_orders WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(conn)
        .await?
        .ok_or(SalesError::NotFound)?;
        Ok(row.status)
    }
}
