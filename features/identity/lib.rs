use crate::{
    hook::before_starting::before_starting, subscriber::account_created::AccountCreatedSubscriber,
    subscriber::account_logged_in::AccountLoggedInSubscriber,
};
use appctx::AppCtx;
use futures_util::future::BoxFuture;
use module::{DomainModule, ModuleRegistrar};
use rootcause::Result;
use utoipa_axum::{router::OpenApiRouter, routes};

mod endpoint;
mod hook;
mod repository;
mod shared;
mod subscriber;

pub struct Module;

impl DomainModule for Module {
    fn name(&self) -> &'static str {
        "identity"
    }

    fn protected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(
                endpoint::account_search::handler,
                endpoint::account_create::handler
            ))
            .routes(routes!(
                endpoint::account_get::handler,
                endpoint::account_delete::handler,
                endpoint::account_update::handler
            ))
            .routes(routes!(endpoint::account_reset_password::handler))
            .routes(routes!(
                endpoint::account_logout::handler,
                endpoint::account_update_password::handler
            ))
    }

    fn unprotected_routing(&self) -> OpenApiRouter<AppCtx> {
        OpenApiRouter::new()
            .routes(routes!(endpoint::account_login::handler))
            .routes(routes!(endpoint::account_refresh_token::handler))
    }

    fn register(&self, registrar: &mut ModuleRegistrar) {
        registrar.bus.register(AccountCreatedSubscriber);
        registrar.bus.register(AccountLoggedInSubscriber);
    }

    fn on_start<'a>(&'a self, state: &'a AppCtx) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            before_starting(state).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::ops::Deref as _;

    use db::PgPool;
    use http_auth::extract::operator::OperatorContext;
    use shared_contract::value_object::id::ID;
    use shared_contract::value_object::operator::Operator;

    /// 测试用操作人上下文（操作人 42，无 IP / UA）。
    pub fn test_operator_context() -> OperatorContext {
        OperatorContext(Operator {
            operator_id: ID::from(42),
            ip: None,
            user_agent: None,
        })
    }

    pub async fn insert_test_account(
        pg_pool: &PgPool,
        phone: &str,
    ) -> shared_contract::value_object::id::ID {
        let id = shared_contract::value_object::id::ID::new();
        let password =
            identity_contract::value_object::hashed_password::HashedPassword::try_new("test1234")
                .unwrap();
        let mut conn = pg_pool.acquire().await.unwrap();
        sqlx::query!(
            r#"INSERT INTO accounts (id, name, phone, password, version) VALUES ($1, $2, $3, $4, 1)"#,
            &*id,
            format!("test-{phone}"),
            phone,
            password.deref(),
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        id
    }
}
