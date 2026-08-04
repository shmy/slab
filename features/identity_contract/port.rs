use rootcause::{Report, Result};
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::entity::account::Account;
use crate::error::IdentityError;

pub struct AccountPort;

impl AccountPort {
    #[tracing::instrument]
    pub async fn by_id(conn: &mut PgConnection, id: &ID) -> Result<Account> {
        let row = sqlx::query_as!(
            Account,
            r#"
            SELECT
                id as "id: ID",
                name,
                phone as "phone: shared_contract::value_object::phone_number::PhoneNumber",
                password as "password: crate::value_object::hashed_password::HashedPassword",
                privileged,
                version
            FROM accounts
            WHERE id = $1
            "#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else::<Report, _>(|| IdentityError::AccountNotFound.into())?;
        Ok(row)
    }
}
