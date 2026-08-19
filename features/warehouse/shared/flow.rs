//! 仓库域审批流定义。

use approval::StateTransitions;
use warehouse_contract::value_object::{InventoryCheckStatus, StockTransferStatus};

/// 调拨单审批流：草稿 -> 已提交 -> 已批准（单步审批）。
pub(crate) const TR_FLOW: StateTransitions = StateTransitions {
    submit: (
        StockTransferStatus::Draft as i16,
        StockTransferStatus::Submitted as i16,
    ),
    approvals: &[(
        StockTransferStatus::Submitted as i16,
        StockTransferStatus::Approved as i16,
    )],
    reject: None,
};

/// 盘点单审批流：草稿 -> 已提交 -> 已批准（单步审批）。
pub(crate) const CH_FLOW: StateTransitions = StateTransitions {
    submit: (
        InventoryCheckStatus::Draft as i16,
        InventoryCheckStatus::Submitted as i16,
    ),
    approvals: &[(
        InventoryCheckStatus::Submitted as i16,
        InventoryCheckStatus::Approved as i16,
    )],
    reject: None,
};
