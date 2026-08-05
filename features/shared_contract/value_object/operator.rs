//! 操作人：跨域共享的操作上下文（变更历史、登录历史、安全日志共用）。

use super::id::ID;
use std::net::IpAddr;

/// 操作人：谁在操作 + 从哪来 + 什么客户端。
///
/// 纯值对象，零基础设施依赖（无 axum / http_auth 残留）。
/// HTTP 场景由 `http_auth` 的提取器（`OperatorContext`）构造；
/// 定时任务 / 队列消费者等非 HTTP 来源可直接构造（`ip` / `user_agent` 为 `None`）。
#[derive(Debug, Clone)]
pub struct Operator {
    /// 操作人（当前登录账户）
    pub operator_id: ID,
    /// 客户端 IP
    pub ip: Option<IpAddr>,
    /// 客户端 User-Agent
    pub user_agent: Option<String>,
}
