use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 调拨单审批流状态（CONTEXT.md「审批流状态」时间线）。
///
/// 草稿 -> 已提交 -> 已批准（单步审批，无主管中间态）。
///
/// 流转规则见 `features/warehouse/shared/flow.rs` 的 `TR_FLOW`。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum StockTransferStatus {
    /// 草稿：可编辑 / 可提交。
    Draft = 0,
    /// 已提交：等待审批。
    Submitted = 1,
    /// 已批准：审批流终态，调拨生效。
    Approved = 3,
}

/// 盘点单审批流状态（CONTEXT.md「审批流状态」时间线）。
///
/// 草稿 -> 已提交 -> 已批准（单步审批，无主管中间态）。
///
/// 流转规则见 `features/warehouse/shared/flow.rs` 的 `CH_FLOW`。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum InventoryCheckStatus {
    /// 草稿：可编辑 / 可提交。
    Draft = 0,
    /// 已提交：等待审批。
    Submitted = 1,
    /// 已批准：审批流终态，盘点调整已入账。
    Approved = 3,
}
