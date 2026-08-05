//! 变更历史（Audit Logs）公共表面：跨域写入入口 [`AuditService`] 与动作枚举 [`AuditAction`]。
//!
//! 与只读 Port（`{Domain}Port`，方法名词）不同，`AuditService` 是**跨域同事务写**入口：
//! 各业务切片在自己的写事务内调用（传 `&mut txn`），与业务写同事务原子提交——
//! 回滚即消失，提交即可见（无 Outbox、无异步延迟）。SQL 实现放 contract 内
//! （与 Port 默认方法自带读 SQL 的先例一致），供 `features/audit` 之外的域直接调用。
//!
//! 查询侧在 `features/audit`（读 `audit_logs` 表 + 读时算字段级 diff）。

use rootcause::Result;
use serde::Serialize;
use serde_repr::Serialize_repr;
use shared_contract::value_object::id::ID;
use shared_contract::value_object::operator::Operator;
use sqlx::PgConnection;

/// 变更动作：落库为 `audit_logs.action` SMALLINT。
///
/// 当前仅 CRUD 三态（查询端 `change_type` 亦由 before/after 快照推断，两者一致）；
/// 需要承载业务动作（如 approve / submit）时扩展变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize_repr)]
#[repr(i16)]
pub enum AuditAction {
    Created = 1,
    Updated = 2,
    Deleted = 3,
}

/// 变更历史写入口（跨域同事务写；动词命名以区分只读 Port 的名词方法）。
pub struct AuditService;

impl AuditService {
    /// 记录创建：`after` 为写后实体（借用），before 恒为 `None`。
    pub async fn record_create<S: Serialize>(
        executor: &mut PgConnection,
        entity: &str,
        entity_id: &ID,
        operator: &Operator,
        after: &S,
    ) -> Result<()> {
        Self::insert(
            executor,
            entity,
            AuditAction::Created,
            entity_id,
            operator,
            None,
            Some(after),
        )
        .await
    }

    /// 记录更新：`before` / `after` 为写前 / 写后实体（借用，写前值由调用方锁读提供）。
    pub async fn record_updated<S: Serialize>(
        executor: &mut PgConnection,
        entity: &str,
        entity_id: &ID,
        operator: &Operator,
        before: &S,
        after: &S,
    ) -> Result<()> {
        Self::insert(
            executor,
            entity,
            AuditAction::Updated,
            entity_id,
            operator,
            Some(before),
            Some(after),
        )
        .await
    }

    /// 记录删除：`before` 为删除前实体（借用，锁读提供），after 恒为 `None`。
    pub async fn record_deleted<S: Serialize>(
        executor: &mut PgConnection,
        entity: &str,
        entity_id: &ID,
        operator: &Operator,
        before: &S,
    ) -> Result<()> {
        Self::insert(
            executor,
            entity,
            AuditAction::Deleted,
            entity_id,
            operator,
            Some(before),
            None,
        )
        .await
    }

    /// 同事务 INSERT 一条变更记录。序列化失败传播错误，不静默降级。
    async fn insert<S: Serialize>(
        executor: &mut PgConnection,
        entity: &str,
        action: AuditAction,
        entity_id: &ID,
        operator: &Operator,
        before: Option<&S>,
        after: Option<&S>,
    ) -> Result<()> {
        let before = before.map(serde_json::to_value).transpose()?;
        let after = after.map(serde_json::to_value).transpose()?;
        sqlx::query!(
            r#"
            INSERT INTO audit_logs (id, operator_id, action, entity, entity_id, before, after, ip, user_agent)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            ID::new() as _,
            operator.operator_id as _,
            action as i16, // sqlx 宏断言要求精确 i16（枚举的 sqlx::Type 不满足宏检查）
            entity,
            entity_id as _,
            before,
            after,
            operator.ip.map(ipnetwork::IpNetwork::from),
            operator.user_agent,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}
