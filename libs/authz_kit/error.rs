use thiserror::Error;

/// 授权系统错误
#[derive(Debug, Error)]
pub enum AuthzError {
    #[error("policy: {0}")]
    Policy(#[from] Box<cedar_policy::PolicySetError>),

    #[error("entity set: {0}")]
    EntitySet(String),

    #[error("request: {0}")]
    Request(String),
}
