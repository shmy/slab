//! 采购域审批流定义。

use approval::StateTransitions;

/// 采购订单审批流：Draft(0) → Submit(1) → Supervisor(2) → Approved(3)，可驳回(4)。
pub(crate) const PO_FLOW: StateTransitions = StateTransitions {
    submit: (0, 1),
    approvals: &[(1, 2), (2, 3)],
    reject: Some((&[1, 2], 4)),
};

/// 采购退货审批流：Draft(0) → Submit(1) → Approved(3)。
pub(crate) const RET_FLOW: StateTransitions = StateTransitions {
    submit: (0, 1),
    approvals: &[(1, 3)],
    reject: None,
};
