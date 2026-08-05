//! 调拨单 / 盘点单的审计快照：`audit_logs.before / after` 的序列化载体。
//!
//! 领域合约中没有对应实体结构（`warehouse_contract::entity` 仅含 Warehouse / Inventory），
//! 状态流转（submit / approve）与创建都需在同事务内读写整行做快照，因此在这里
//! 定义轻量 `Serialize` 结构 + 锁读 / 普通读辅助方法，供各 endpoint 复用。

use chrono::{DateTime, NaiveDate, Utc};
use rootcause::Result;
use serde::Serialize;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

/// 调拨单审计快照（对应 `stock_transfers` 表）。
#[derive(Debug, Serialize)]
pub(crate) struct StockTransferSnapshot {
    pub id: ID,
    pub code: String,
    pub from_warehouse_id: ID,
    pub to_warehouse_id: ID,
    pub status: i16,
    pub transfer_date: NaiveDate,
    pub remark: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// 盘点单审计快照（对应 `inventory_checks` 表）。
#[derive(Debug, Serialize)]
pub(crate) struct InventoryCheckSnapshot {
    pub id: ID,
    pub code: String,
    pub warehouse_id: ID,
    pub status: i16,
    pub plan_date: NaiveDate,
    pub remark: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

impl StockTransferSnapshot {
    /// 锁读调拨单整行（供变更前快照；仓库方法内部的 FOR UPDATE 为同行的可重入锁）。
    pub(crate) async fn read_locked(conn: &mut PgConnection, id: &ID) -> Result<Option<Self>> {
        let row = sqlx::query!(
            r#"SELECT code, from_warehouse_id, to_warehouse_id, status, transfer_date, remark, approved_at
               FROM stock_transfers WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| Self {
            id: *id,
            code: r.code,
            from_warehouse_id: ID::new_unchecked(r.from_warehouse_id),
            to_warehouse_id: ID::new_unchecked(r.to_warehouse_id),
            status: r.status,
            transfer_date: r.transfer_date,
            remark: r.remark,
            approved_at: r.approved_at,
        }))
    }

    /// 普通读调拨单整行（同事务内读回写后快照，可见本事务未提交写入）。
    pub(crate) async fn read(conn: &mut PgConnection, id: &ID) -> Result<Option<Self>> {
        let row = sqlx::query!(
            r#"SELECT code, from_warehouse_id, to_warehouse_id, status, transfer_date, remark, approved_at
               FROM stock_transfers WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| Self {
            id: *id,
            code: r.code,
            from_warehouse_id: ID::new_unchecked(r.from_warehouse_id),
            to_warehouse_id: ID::new_unchecked(r.to_warehouse_id),
            status: r.status,
            transfer_date: r.transfer_date,
            remark: r.remark,
            approved_at: r.approved_at,
        }))
    }
}

impl InventoryCheckSnapshot {
    /// 锁读盘点单整行（供变更前快照；仓库方法内部的 FOR UPDATE 为同行的可重入锁）。
    pub(crate) async fn read_locked(conn: &mut PgConnection, id: &ID) -> Result<Option<Self>> {
        let row = sqlx::query!(
            r#"SELECT code, warehouse_id, status, plan_date, remark, approved_at
               FROM inventory_checks WHERE id = $1 FOR UPDATE"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| Self {
            id: *id,
            code: r.code,
            warehouse_id: ID::new_unchecked(r.warehouse_id),
            status: r.status,
            plan_date: r.plan_date,
            remark: r.remark,
            approved_at: r.approved_at,
        }))
    }

    /// 普通读盘点单整行（同事务内读回写后快照，可见本事务未提交写入）。
    pub(crate) async fn read(conn: &mut PgConnection, id: &ID) -> Result<Option<Self>> {
        let row = sqlx::query!(
            r#"SELECT code, warehouse_id, status, plan_date, remark, approved_at
               FROM inventory_checks WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row.map(|r| Self {
            id: *id,
            code: r.code,
            warehouse_id: ID::new_unchecked(r.warehouse_id),
            status: r.status,
            plan_date: r.plan_date,
            remark: r.remark,
            approved_at: r.approved_at,
        }))
    }
}
