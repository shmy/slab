use appctx::TokenHelper;
use authn_kit::{access_jti_key, refresh_key, subject_refresh_key};
use identity_contract::error::IdentityError;
use kv::KvBackend;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use std::time::Duration;
use tempoid::TempoId;

pub(crate) struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

/// 签发令牌：写入缓存后端（可丢辅助数据）。
///
/// 不再参与调用方 PG 事务——缓存与业务主存解耦（见 `infrastructure/kv`）；
/// 调用方保证先提交业务事务再调用本函数，缓存失败不影响登录（吊销延迟到 TTL 过期）。
#[tracing::instrument(skip(kv, token_helper))]
pub async fn issue_tokens(
    kv: &KvBackend,
    token_helper: &TokenHelper,
    user_id: &ID,
) -> Result<TokenPair> {
    let jti = TempoId::generate().to_string();
    let refresh_token = TempoId::generate().to_string();
    let (access_token, expires_in) = token_helper.encode_access_token(user_id, &jti)?;

    let realm = token_helper.realm();
    let user_refresh_key = subject_refresh_key(realm, user_id);
    if let Some(old_refresh_token) = kv.get::<String>(&user_refresh_key).await? {
        kv.del(&refresh_key(realm, &old_refresh_token)).await?;
    }
    let user_id_str = user_id.to_string();
    kv.set_ex(
        &refresh_key(realm, &refresh_token),
        &user_id_str,
        Duration::from_secs(token_helper.refresh_ttl_secs()),
    )
    .await?;
    kv.set_ex(
        &user_refresh_key,
        &refresh_token,
        Duration::from_secs(token_helper.refresh_ttl_secs()),
    )
    .await?;
    kv.set_ex(
        &access_jti_key(realm, user_id),
        &jti,
        Duration::from_secs(token_helper.access_ttl_secs()),
    )
    .await?;

    Ok(TokenPair {
        access_token,
        refresh_token,
        expires_in,
    })
}

/// 消费 refresh token（原子 take），返回对应用户 ID。
#[tracing::instrument(skip(kv, refresh_token))]
pub async fn consume_refresh_token(
    kv: &KvBackend,
    token_helper: &TokenHelper,
    refresh_token: &str,
) -> Result<ID> {
    let realm = token_helper.realm();
    let user_id = kv
        .take::<ID>(&refresh_key(realm, refresh_token))
        .await?
        .ok_or(IdentityError::RefreshTokenInvalid)?;
    Ok(user_id)
}

/// 吊销用户全部令牌（refresh token 轮换 + access jti 清除）。
#[tracing::instrument(skip(kv))]
pub async fn revoke_tokens(kv: &KvBackend, token_helper: &TokenHelper, user_id: ID) -> Result<()> {
    let realm = token_helper.realm();
    let user_refresh_key = subject_refresh_key(realm, user_id);
    if let Some(old_refresh_token) = kv.get::<String>(&user_refresh_key).await? {
        kv.del(&refresh_key(realm, &old_refresh_token)).await?;
    }
    kv.del(&user_refresh_key).await?;
    kv.del(&access_jti_key(realm, user_id)).await?;

    Ok(())
}
