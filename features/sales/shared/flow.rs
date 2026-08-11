//! 销售域审批流定义。

use approval::StateTransitions;
use sales_contract::value_object::SalesOrderStatus;

/// 销售订单审批流：草稿 -> 已提交 -> 主管审批中 -> 已批准。
pub(crate) const SO_FLOW: StateTransitions = StateTransitions {
    submit: (
        SalesOrderStatus::Draft as i16,
        SalesOrderStatus::Submitted as i16,
    ),
    approvals: &[
        (
            SalesOrderStatus::Submitted as i16,
            SalesOrderStatus::PendingSupervisor as i16,
        ),
        (
            SalesOrderStatus::PendingSupervisor as i16,
            SalesOrderStatus::Approved as i16,
        ),
    ],
    reject: None,
};
