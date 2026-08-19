use serde::{Deserialize, Serialize};
use sqlx::Type;

/// 销售订单审批流状态（CONTEXT.md「审批流状态」时间线）。
///
/// 草稿 -> 已提交 -> 主管审批中 -> 已批准。
/// 与生命周期状态（发货 / 退货 / 开票）是独立的两条时间线；无驳回。
///
/// 流转规则见 `features/sales/shared/flow.rs` 的 `SO_FLOW`。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum SalesOrderStatus {
    /// 草稿：可编辑 / 可提交。
    Draft = 0,
    /// 已提交：进入审批流，等待主管审批。
    Submitted = 1,
    /// 主管审批中：第一次审批通过后的中间态。
    PendingSupervisor = 2,
    /// 已批准：审批流终态，可发货。
    Approved = 3,
}

/// 销售发货生命周期状态（CONTEXT.md「生命周期状态」时间线）。
///
/// 发货单创建即过账（出库已发生），无草稿。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum SalesDeliveryStatus {
    /// 已过账：发货生效，库存已出账。
    Posted = 1,
}

/// 销售退货生命周期状态（CONTEXT.md「生命周期状态」时间线）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum SalesReturnStatus {
    /// 开放：已创建（表默认值）。
    Open = 0,
}

/// 销售发票生命周期状态（CONTEXT.md「生命周期状态」时间线）。
///
/// 收款进度由 `paid_amount` 跟踪，本枚举只区分发票自身是否仍开放。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "SMALLINT")]
#[repr(i16)]
pub enum SalesInvoiceStatus {
    /// 开放：已开票，款项未结清（表默认值）。
    Open = 0,
}
