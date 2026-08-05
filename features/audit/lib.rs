//! 变更历史（Audit Logs）查询域。
//!
//! 薄切片：只挂查询路由；记录能力在 `audit_contract::AuditService`（各业务切片在自己的
//! 写事务内调用 `record_create` / `record_updated` / `record_deleted`）。读时字段级 diff
//! （git diff 风格展示的输入）见 [`diff`]。

use appctx::AppCtx;
use feature::FeatureModule;
use utoipa_axum::{router::OpenApiRouter, routes};

mod diff;
mod endpoint;

pub struct Module;

impl FeatureModule for Module {
    fn name(&self) -> &'static str {
        "audit"
    }

    fn protected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new().routes(routes!(endpoint::audit_search::handler))
    }
}
