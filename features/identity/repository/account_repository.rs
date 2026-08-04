use identity_contract::entity::account::Account;
use identity_contract::error::IdentityError;
use identity_contract::value_object::hashed_password::HashedPassword;
use rootcause::{Report, Result};
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

fn account_repository_pg_err(e: sqlx::Error) -> Report {
    if let sqlx::Error::Database(db) = &e
        && db.code().as_deref() == Some("23505")
    {
        IdentityError::AccountDuplicated.into()
    } else {
        e.into()
    }
}

pub struct AccountRepository;

impl AccountRepository {
    #[tracing::instrument]
    pub async fn create(conn: &mut PgConnection, account: &Account) -> Result<ID> {
        // TODO(多租户): Task 5+ 会从鉴权上下文注入真实 tenant_id
        let row = sqlx::query!(
            r#"
            INSERT INTO accounts (id, name, phone, password, privileged, version)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id as "id: ID"
            "#,
            &*account.id,
            &account.name,
            &*account.phone,
            &*account.password,
            account.privileged,
            account.version,
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(account_repository_pg_err)?;
        Ok(row.id)
    }

    #[tracing::instrument]
    pub async fn update(conn: &mut PgConnection, profile: &Account) -> Result<Account> {
        let row = sqlx::query_as!(
            Account,
            r#"
            WITH updated AS (
                UPDATE accounts
                SET name = $2, phone = $3, version = version + 1
                WHERE id = $1 AND version = $4
                RETURNING id, name, phone, password, privileged, version
            )
            SELECT
                id as "id: ID",
                name,
                phone as "phone: shared_contract::value_object::phone_number::PhoneNumber",
                password as "password: HashedPassword",
                privileged,
                version
            FROM updated
            "#,
            &*profile.id,
            &profile.name,
            &*profile.phone,
            profile.version
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(account_repository_pg_err)?;

        match row {
            Some(row) => Ok(row),
            None => {
                let exists = sqlx::query_scalar!(
                    r#"SELECT 1 as "one!" FROM accounts WHERE id = $1"#,
                    &*profile.id
                )
                .fetch_optional(&mut *conn)
                .await?;
                Err(if exists.is_some() {
                    IdentityError::AccountVersionConflict
                } else {
                    IdentityError::AccountNotFound
                }
                .into())
            }
        }
    }

    #[tracing::instrument]
    pub async fn delete(conn: &mut PgConnection, id: &ID) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM accounts
            WHERE id = $1
            "#,
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    #[tracing::instrument]
    pub async fn is_privileged_exists(conn: &mut PgConnection) -> Result<bool> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM accounts WHERE privileged = true")
            .fetch_one(&mut *conn)
            .await?;
        Ok(count > Some(0))
    }

    /// 更新密码（管理员重置）。账密不存在时返回 `AccountNotFound`。
    #[tracing::instrument]
    pub async fn update_password(
        conn: &mut PgConnection,
        id: &ID,
        password: &HashedPassword,
    ) -> Result<()> {
        let rows = sqlx::query!(
            r#"UPDATE accounts SET password = $2, version = version + 1 WHERE id = $1"#,
            id as _,
            password as _,
        )
        .execute(&mut *conn)
        .await?;
        if rows.rows_affected() == 0 {
            return Err(IdentityError::AccountNotFound.into());
        }
        Ok(())
    }

    /// 读取当前密码哈希（用于本人改密前的旧密码校验）。
    #[tracing::instrument]
    pub async fn get_password_hash(conn: &mut PgConnection, id: &ID) -> Result<HashedPassword> {
        let row = sqlx::query!(
            r#"SELECT password as "password: HashedPassword" FROM accounts WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(IdentityError::AccountNotFound)?;
        Ok(row.password)
    }
}
