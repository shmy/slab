use http_auth::extract::operator::Operator;
use rootcause::Result;
use serde::Serialize;
use serde_repr::Serialize_repr;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

#[derive(sqlx::Type, Serialize_repr)]
#[repr(i16)]
pub enum AuditAction {
    Created = 1,
    Updated = 2,
    Deleted = 3,
}
pub struct AuditService;

impl AuditService {
    pub async fn on_created<S>(
        executor: &mut PgConnection,
        entity: &str,
        entity_id: &ID,
        operator: &Operator,
        after: S,
    ) -> Result<()>
    where
        S: Serialize,
    {
        Self::insert(
            executor,
            entity,
            &AuditAction::Created,
            entity_id,
            operator,
            None,
            Some(after),
        )
        .await?;
        Ok(())
    }

    pub async fn on_updated<S>(
        executor: &mut PgConnection,
        entity: &str,
        entity_id: &ID,
        operator: &Operator,
        before: S,
        after: S,
    ) -> Result<()>
    where
        S: Serialize,
    {
        Self::insert(
            executor,
            entity,
            &AuditAction::Updated,
            entity_id,
            operator,
            Some(before),
            Some(after),
        )
        .await?;
        Ok(())
    }

    pub async fn on_deleted<S>(
        executor: &mut PgConnection,
        entity: &str,
        entity_id: &ID,
        operator: &Operator,
        before: S,
    ) -> Result<()>
    where
        S: Serialize,
    {
        Self::insert(
            executor,
            entity,
            &AuditAction::Deleted,
            entity_id,
            operator,
            Some(before),
            None,
        )
        .await?;
        Ok(())
    }

    async fn insert<S>(
        executor: &mut PgConnection,
        entity: &str,
        action: &AuditAction,
        entity_id: &ID,
        operator: &Operator,
        before: Option<S>,
        after: Option<S>,
    ) -> Result<()>
    where
        S: Serialize,
    {
        sqlx::query!(
        r#"
        INSERT INTO audit_logs (id, operator_id, action, entity, entity_id, before, after, ip, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        ID::new() as _,
        operator.operator_id as _,
        action as _,
        entity,
        entity_id as _,
        before.and_then(|s| serde_json::to_value(&s).ok()),
        after.and_then(|s| serde_json::to_value(&s).ok()),
        operator.ip.map(ipnetwork::IpNetwork::from),
        operator.user_agent,
    )
    .execute(executor)
    .await?;
        Ok(())
    }
}
