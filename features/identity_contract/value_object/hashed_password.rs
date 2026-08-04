use argon2::password_hash::SaltString;
use argon2::{Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, password_hash};
use rootcause::Result;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::LazyLock;

use crate::error::IdentityError;
use crate::value_object::password::Password;

static ARGON2: LazyLock<Argon2> = LazyLock::new(|| {
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        Params::default(),
    )
});

#[derive(Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct HashedPassword(String);

impl HashedPassword {
    #[tracing::instrument]
    pub fn new_unchecked(hash: String) -> Self {
        Self(hash)
    }

    #[tracing::instrument(skip(password))]
    pub fn try_new(password: &str) -> Result<Self> {
        let password = Password::try_new(password)?;
        let password_hash = Self::argon2_hash_password(&password)?;
        Ok(Self(password_hash))
    }

    #[tracing::instrument(skip(password))]
    fn argon2_hash_password(password: &str) -> Result<String> {
        let password = password.to_owned();

        let salt = SaltString::generate(&mut password_hash::rand_core::OsRng);
        let hash = ARGON2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| IdentityError::AccountPasswordEncodeFailed)?;
        Ok(hash.to_string())
    }

    #[tracing::instrument(skip(password))]
    pub fn verify(&self, password: &str) -> Result<()> {
        let parsed_hash =
            PasswordHash::new(&self.0).map_err(|_| IdentityError::AccountPasswordDecodeFailed)?;
        if ARGON2
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_err()
        {
            Err(IdentityError::AccountPasswordIncorrect)?;
        }
        Ok(())
    }
}

impl Debug for HashedPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("HashedPassword")
            .field(&"<Redacted>")
            .finish()
    }
}

impl Deref for HashedPassword {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_new_valid_and_verify() {
        let hp = HashedPassword::try_new("abcd").unwrap();
        assert!(hp.verify("abcd").is_ok());
    }

    #[test]
    fn test_try_new_too_short() {
        let err = HashedPassword::try_new("abc").unwrap_err();
        assert!(err.to_string().contains("account_password_too_short"));
    }

    #[test]
    fn test_try_new_too_long() {
        let long = "a".repeat(65);
        let err = HashedPassword::try_new(&long).unwrap_err();
        assert!(err.to_string().contains("account_password_too_long"));
    }

    #[test]
    fn test_verify_wrong_password() {
        let hp = HashedPassword::try_new("correct_password").unwrap();
        let err = hp.verify("wrong_password").unwrap_err();
        assert!(err.to_string().contains("account_password_incorrect"));
    }

    #[test]
    fn test_debug_redacted() {
        let hp = HashedPassword::try_new("abcd").unwrap();
        let debug = format!("{:?}", hp);
        assert!(debug.contains("<Redacted>"));
        assert!(!debug.contains("$argon2"));
    }
}
