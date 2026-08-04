#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("account_not_found")]
    AccountNotFound,
    #[error("account_duplicated")]
    AccountDuplicated,
    #[error("account_version_conflict")]
    AccountVersionConflict,
    #[error("account_password_encode_failed")]
    AccountPasswordEncodeFailed,
    #[error("account_password_decode_failed")]
    AccountPasswordDecodeFailed,
    #[error("account_password_incorrect")]
    AccountPasswordIncorrect,
    #[error("account_invalid_credentials")]
    AccountInvalidCredentials,
    #[error("refresh_token_invalid")]
    RefreshTokenInvalid,
}
