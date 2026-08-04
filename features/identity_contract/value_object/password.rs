use rootcause::Result;
use serde::Deserialize;
use std::{fmt::Debug, ops::Deref};
use utoipa::ToSchema;
use validify::{Modify, Validate};

const MAX_PASSWORD_LEN: usize = 64;

/// 密码
#[derive(Clone, Deserialize, ToSchema)]
#[schema(value_type = String, example = "admin123!")]
#[serde(transparent)]
pub struct Password(String);

impl Password {
    #[tracing::instrument(skip_all)]
    pub fn try_new(password: &str) -> Result<Self> {
        let mut instance = Self(password.to_owned());
        instance.modify();
        instance.validate()?;
        Ok(instance)
    }

    pub fn new_unchecked(password: String) -> Self {
        Self(password)
    }
}

impl Modify for Password {
    fn modify(&mut self) {
        self.0 = self.0.trim().to_string();
    }
}

impl Validate for Password {
    fn validate(&self) -> std::result::Result<(), validify::ValidationErrors> {
        let mut errors = validify::ValidationErrors::new();
        if self.0.chars().count() < 4 {
            errors.add(validify::field_err!(
                "too_short",
                "account_password_too_short",
                "password"
            ));
        }
        if self.0.chars().count() > MAX_PASSWORD_LEN {
            errors.add(validify::field_err!(
                "too_long",
                "account_password_too_long",
                "password"
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Password").field(&"<Redacted>").finish()
    }
}

impl Deref for Password {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
