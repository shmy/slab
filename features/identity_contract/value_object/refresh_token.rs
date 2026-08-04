use std::{fmt::Debug, ops::Deref};

use serde::Deserialize;
use utoipa::ToSchema;

/// 刷新令牌
#[derive(Deserialize, ToSchema)]
#[schema(value_type = String, example = "1234567890123456789")]
#[serde(transparent)]
pub struct RefreshToken(String);

impl RefreshToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }
}

impl Debug for RefreshToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RefreshToken").field(&"<Redacted>").finish()
    }
}

impl Deref for RefreshToken {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
