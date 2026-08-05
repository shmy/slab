//! 操作人上下文提取器：从请求解析「操作人 + 客户端 IP + User-Agent」，
//! 产出跨域共享值对象 `shared_contract::value_object::operator::Operator`。

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{header::USER_AGENT, request::Parts},
};
use shared_contract::value_object::operator::Operator;
use std::net::SocketAddr;
use web::error::WebError;

use crate::extract::authed_account::AuthedAccount;

/// 操作人上下文提取器：`Operator` 的 HTTP 适配器。
///
/// 变更历史 `audit_contract::AuditService` 需要「操作人 + IP + UA」三件套，
/// 写端点 handler 里几乎总是成组出现，合并为一个提取器。
/// 按内容（谁在操作、从哪来、什么客户端）而非消费方命名：登录历史、安全日志等场景
/// 同样需要这份上下文。
///
/// `Deref` 到 [`Operator`]：消费方（如 `audit_contract`）只依赖 shared_contract 的值对象，
/// 不依赖本 crate；调用点 `&ctx` 自动 deref coercion。
#[derive(Clone, Debug)]
pub struct OperatorContext(pub Operator);

impl std::ops::Deref for OperatorContext {
    type Target = Operator;

    fn deref(&self) -> &Operator {
        &self.0
    }
}

impl<S> FromRequestParts<S> for OperatorContext
where
    S: Send + Sync + 'static,
{
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthedAccount(operator_id) = AuthedAccount::from_request_parts(parts, state).await?;
        // ConnectInfo 缺失说明 server 未配置 connect_info（部署配置错误），按内部错误 500
        let ConnectInfo(addr) = ConnectInfo::<SocketAddr>::from_request_parts(parts, state)
            .await
            .map_err(|_| WebError::L10n("internal_server_error".to_string()))?;
        // HeaderMap 恒可用，直接从 parts 读取即可
        let user_agent = parts
            .headers
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        Ok(OperatorContext(Operator {
            operator_id,
            ip: Some(addr.ip()),
            user_agent,
        }))
    }
}
