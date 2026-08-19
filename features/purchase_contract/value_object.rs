use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 采购订单审批流状态（CONTEXT.md「审批流状态」时间线）。
///
/// 草稿 -> 已提交 -> 主管审批中 -> 已批准 / 已驳回。
/// 与生命周期状态（收货 / 退货进程）是独立的两条时间线。
///
/// 流转规则见 `features/purchase/shared/flow.rs` 的 `PO_FLOW`。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum PurchaseOrderStatus {
    /// 草稿：可编辑 / 可提交 / 可删除。
    Draft = 0,
    /// 已提交：进入审批流，等待主管审批。
    Submitted = 1,
    /// 主管审批中：第一次审批通过后的中间态。
    PendingSupervisor = 2,
    /// 已批准：审批流终态，可收货。
    Approved = 3,
    /// 已驳回：审批流终态，可修改后重新提交。
    Rejected = 4,
    /// 软删除标记（**非**审批流状态）：仅 `purchase_order_delete` 使用。
    Deleted = -1,
}

/// 采购退货审批流状态（CONTEXT.md「审批流状态」时间线）。
///
/// 草稿 -> 已提交 -> 已批准（单步审批，无主管中间态）。
///
/// 流转规则见 `features/purchase/shared/flow.rs` 的 `RET_FLOW`。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum PurchaseReturnStatus {
    /// 草稿：可编辑 / 可提交。
    Draft = 0,
    /// 已提交：等待审批。
    Submitted = 1,
    /// 已批准：审批通过，退货生效。
    Approved = 3,
}

/// 采购收货生命周期状态（CONTEXT.md「生命周期状态」时间线）。
///
/// 收货单创建即过账（入库已发生），无草稿。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum PurchaseReceiptStatus {
    /// 已过账：收货生效，库存已入账。
    Posted = 1,
}

/// 采购发票生命周期状态（CONTEXT.md「生命周期状态」时间线）。
///
/// 付款进度由 `paid_amount` 跟踪，本枚举只区分发票自身是否仍开放。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum PurchaseInvoiceStatus {
    /// 开放：已开票，款项未结清（表默认值）。
    Open = 0,
}
