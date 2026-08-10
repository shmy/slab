//! 采购域审批流定义。

use approval::StateTransitions;
use purchase_contract::value_object::{PurchaseOrderStatus, PurchaseReturnStatus};

/// 采购订单审批流：草稿 -> 已提交 -> 主管审批中 -> 已批准，可驳回。
pub(crate) const PO_FLOW: StateTransitions = StateTransitions {
    submit: (
        PurchaseOrderStatus::Draft as i16,
        PurchaseOrderStatus::Submitted as i16,
    ),
    approvals: &[
        (
            PurchaseOrderStatus::Submitted as i16,
            PurchaseOrderStatus::PendingSupervisor as i16,
        ),
        (
            PurchaseOrderStatus::PendingSupervisor as i16,
            PurchaseOrderStatus::Approved as i16,
        ),
    ],
    reject: Some((
        &[
            PurchaseOrderStatus::Submitted as i16,
            PurchaseOrderStatus::PendingSupervisor as i16,
        ],
        PurchaseOrderStatus::Rejected as i16,
    )),
};

/// 采购退货审批流：草稿 -> 已提交 -> 已批准（单步审批）。
pub(crate) const RET_FLOW: StateTransitions = StateTransitions {
    submit: (
        PurchaseReturnStatus::Draft as i16,
        PurchaseReturnStatus::Submitted as i16,
    ),
    approvals: &[(
        PurchaseReturnStatus::Submitted as i16,
        PurchaseReturnStatus::Approved as i16,
    )],
    reject: None,
};
