use appctx::{PgPool, TokenHelper};
use authn_kit::{access_jti_key, refresh_key, subject_refresh_key};
use cache as kv_cache;
use identity_contract::error::IdentityError;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::Acquire;
use tempoid::TempoId;

pub(crate) struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[tracing::instrument(skip(pg_pool, token_helper))]
pub async fn issue_tokens(
    pg_pool: &PgPool,
    token_helper: &TokenHelper,
    user_id: &ID,
) -> Result<TokenPair> {
    let jti = TempoId::generate().to_string();
    let refresh_token = TempoId::generate().to_string();
    let (access_token, expires_in) = token_helper.encode_access_token(user_id, &jti)?;

    let mut conn = pg_pool.acquire().await?;
    let mut tx = conn.begin().await?;
    let realm = token_helper.realm();
    let user_refresh_key = subject_refresh_key(realm, user_id);
    if let Some(old_refresh_token) = kv_cache::get::<String>(tx.as_mut(), &user_refresh_key).await?
    {
        kv_cache::del(tx.as_mut(), &refresh_key(realm, &old_refresh_token)).await?;
    }
    let user_id_str = user_id.to_string();
    kv_cache::set_ex(
        tx.as_mut(),
        &refresh_key(realm, &refresh_token),
        &user_id_str,
        token_helper.refresh_ttl_secs(),
    )
    .await?;
    kv_cache::set_ex(
        tx.as_mut(),
        &user_refresh_key,
        &refresh_token,
        token_helper.refresh_ttl_secs(),
    )
    .await?;
    kv_cache::set_ex(
        tx.as_mut(),
        &access_jti_key(realm, user_id),
        &jti,
        token_helper.access_ttl_secs(),
    )
    .await?;
    tx.commit().await?;

    Ok(TokenPair {
        access_token,
        refresh_token,
        expires_in,
    })
}

#[tracing::instrument(skip(pg_pool, refresh_token))]
pub async fn consume_refresh_token(
    pg_pool: &PgPool,
    token_helper: &TokenHelper,
    refresh_token: &str,
) -> Result<ID> {
    let realm = token_helper.realm();
    let mut conn = pg_pool.acquire().await?;
    let user_id = kv_cache::take::<ID>(&mut conn, &refresh_key(realm, refresh_token))
        .await?
        .ok_or(IdentityError::RefreshTokenInvalid)?;
    Ok(user_id)
}

#[tracing::instrument(skip(pg_pool))]
pub async fn revoke_tokens(
    pg_pool: &PgPool,
    token_helper: &TokenHelper,
    user_id: ID,
) -> Result<()> {
    let mut conn = pg_pool.acquire().await?;
    let mut tx = conn.begin().await?;
    let realm = token_helper.realm();
    let user_refresh_key = subject_refresh_key(realm, user_id);
    if let Some(old_refresh_token) = kv_cache::get::<String>(tx.as_mut(), &user_refresh_key).await?
    {
        kv_cache::del(tx.as_mut(), &refresh_key(realm, &old_refresh_token)).await?;
    }
    kv_cache::del(tx.as_mut(), &user_refresh_key).await?;
    kv_cache::del(tx.as_mut(), &access_jti_key(realm, user_id)).await?;
    tx.commit().await?;

    Ok(())
}
