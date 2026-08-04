//! 采购订单聚合的持久化变更。

use crate::shared::flow::PO_FLOW;
use purchase_contract::error::PurchaseError;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

/// 采购订单聚合写库。
///
/// 动作方法（`submit` / `approve` / `reject`）把「锁定读 + 状态机规则 + 写状态」一步完成，
/// 调用方只负责事务边界；`lock_status` / `update_status` 供软删除等特殊场景使用。
pub struct PurchaseOrderRepository;

impl PurchaseOrderRepository {
    /// 锁定采购订单行并返回当前状态；不存在时返回 `NotFound`。
    pub async fn lock_status(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let row = sqlx::query!(
            r#"SELECT status FROM purchase_orders WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(conn)
        .await?
        .ok_or(PurchaseError::NotFound)?;
        Ok(row.status)
    }

    /// 仅更新状态（submit / 软删除等无时间戳动作）。
    pub async fn update_status(conn: &mut PgConnection, id: &ID, status: i16) -> Result<()> {
        sqlx::query!(
            r#"UPDATE purchase_orders SET status = $1 WHERE id = $2"#,
            status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(())
    }

    /// 提交：锁定读 + 状态机校验 + 写入新状态，返回新状态。
    pub async fn submit(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = PO_FLOW.submit_status(status)?;
        Self::update_status(conn, id, new_status).await?;
        Ok(new_status)
    }

    /// 审批：锁定读 + 状态机校验 + 写入新状态并记录审批时间，返回新状态。
    pub async fn approve(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = PO_FLOW.approve_status(status)?;
        sqlx::query!(
            r#"UPDATE purchase_orders SET status = $1, approved_at = NOW() WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(new_status)
    }

    /// 驳回：锁定读 + 状态机校验 + 写入新状态并记录驳回时间，返回新状态。
    pub async fn reject(conn: &mut PgConnection, id: &ID) -> Result<i16> {
        let status = Self::lock_status(conn, id).await?;
        let new_status = PO_FLOW.reject_status(status)?;
        sqlx::query!(
            r#"UPDATE purchase_orders SET status = $1, rejected_at = NOW() WHERE id = $2"#,
            new_status,
            id as _
        )
        .execute(conn)
        .await?;
        Ok(new_status)
    }
}
