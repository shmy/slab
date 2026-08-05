//! 操作人上下文提取器：操作人 + 客户端 IP + User-Agent。

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{header::USER_AGENT, request::Parts},
};
use shared_contract::value_object::id::ID;
use std::net::{IpAddr, SocketAddr};
use web::error::WebError;

use crate::extract::authed_account::AuthedAccount;

/// 操作人上下文：操作人 + 客户端 IP + User-Agent。
///
/// 变更历史 `audit_contract::record` 需要这三件套，写端点 handler 里几乎总是成组出现，
/// 合并为一个提取器，省去每个 handler 重复声明 `AuthedAccount` / `ConnectInfo` / 取头。
/// 按内容（谁在操作、从哪来、什么客户端）而非消费方命名：登录历史、安全日志等场景
/// 同样需要这份上下文。
///
/// 放在 `http_auth` 而非 `audit_contract`：contract 不得依赖 infrastructure
/// （否则会把鉴权中间件栈拖进所有消费方）；`web` 依赖 `http_auth` 的反向会成环。
#[derive(Clone, Debug)]
pub struct Operator {
    /// 操作人（当前登录账户）
    pub operator_id: ID,
    /// 客户端 IP（ConnectInfo 未配置时服务端 500，不静默降级为 None）
    pub ip: Option<IpAddr>,
    /// 客户端 User-Agent（未携带时为 None）
    pub user_agent: Option<String>,
}

impl<S> FromRequestParts<S> for Operator
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
        Ok(Self {
            operator_id,
            ip: Some(addr.ip()),
            user_agent,
        })
    }
}
