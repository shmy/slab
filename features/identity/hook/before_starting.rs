use appctx::AppCtx;
use identity_contract::{entity::account::Account, value_object::hashed_password::HashedPassword};
use rootcause::Result;
use shared_contract::value_object::{id::ID, phone_number::PhoneNumber};
use sqlx::Connection as _;

use crate::repository::account_repository::AccountRepository;

pub async fn before_starting(state: &AppCtx) -> Result<()> {
    let mut conn = state.pg_pool.acquire().await?;
    let mut txn = conn.begin().await?;
    let exists = AccountRepository::is_privileged_exists(&mut txn).await?;
    if !exists {
        let phone = std::env::var("SEED_ADMIN_PHONE").unwrap_or_else(|_| "13888888888".to_string());
        let password =
            std::env::var("SEED_ADMIN_PASSWORD").unwrap_or_else(|_| "admin123!".to_string());
        tracing::info!("No privileged account exists, creating with phone: {phone}");
        let account = Account {
            id: ID::default(),
            name: "admin".to_string(),
            phone: PhoneNumber::try_new(&phone)?,
            password: HashedPassword::try_new(&password)?,
            privileged: true,
            version: 1,
        };
        AccountRepository::create(&mut txn, &account).await?;
    } else {
        tracing::info!("Privileged account already exists, skipping creation");
    }
    txn.commit().await?;
    Ok(())
}
