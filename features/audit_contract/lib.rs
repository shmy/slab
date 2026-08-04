//! 变更历史（Audit Logs）公共表面：跨域写入入口 [`record`] 与载荷 [`AuditEvent`]。
//!
//! 与只读 Port（`{Domain}Port`，方法名词）不同，`record` 是**跨域同事务写**入口：
//! 各业务切片在自己的写事务内调用（传 `txn.as_mut()`），与业务写同事务原子提交——
//! 回滚即消失，提交即可见（无 Outbox、无异步延迟）。SQL 实现放 contract 内
//! （与 Port 默认方法自带读 SQL 的先例一致），供 `features/audit` 之外的域直接调用。
//!
//! 查询侧在 `features/audit`（读 `audit_logs` 表 + 读时算字段级 diff）。

use serde::Serialize;
use serde_json::Value;
use shared_contract::value_object::id::ID;
use sqlx::{Executor, Postgres};

/// 一次变更记录：操作人 + 资源定位 + 前后快照。
///
/// `before` / `after` 是**审计视图**（实体序列化层已排除敏感字段，如密码哈希）：
/// 落库时原样存储 JSONB，查询端读取时现场计算字段级 diff（git diff 风格展示的输入）。
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    /// 操作人（当前登录账户）
    pub operator_id: ID,
    /// 业务动作，`{entity}.{动词}`，如 `account.create` / `purchase_order.approve`
    pub action: String,
    /// 资源类型（snake_case，如 `account` / `purchase_order`）
    pub entity: String,
    /// 资源 ID
    pub entity_id: ID,
    /// 变更前快照（创建时为 `None`）
    pub before: Option<Value>,
    /// 变更后快照（删除时为 `None`）
    pub after: Option<Value>,
    /// 客户端 IP（可选）
    pub ip: Option<std::net::IpAddr>,
    /// 客户端 User-Agent（可选）
    pub user_agent: Option<String>,
}

/// 写入一条变更记录（**必须在业务写事务内调用**，同步 INSERT，同事务原子）。
#[tracing::instrument(skip_all)]
pub async fn record<'e, E>(executor: E, event: &AuditEvent) -> rootcause::Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query!(
        r#"
        INSERT INTO audit_logs (id, operator_id, action, entity, entity_id, before, after, ip, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        *ID::new(),
        *event.operator_id,
        event.action.as_str(),
        event.entity.as_str(),
        *event.entity_id,
        event.before.as_ref(),
        event.after.as_ref(),
        event.ip.map(ipnetwork::IpNetwork::from),
        event.user_agent.as_deref(),
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// 变更历史域错误。
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("audit_invalid_entity")]
    InvalidEntity,
}
