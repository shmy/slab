use std::fmt::Display;

use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode, get_current_timestamp,
};
use rootcause::Result;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::token_realm::TokenRealm;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: u64,
    pub iat: u64,
    pub jti: String,
}

#[derive(Clone)]
pub struct TokenHelper {
    realm: TokenRealm,
    jwt_secret: SecretString,
    access_ttl_secs: u64,
    refresh_ttl_secs: u64,
}

impl Debug for TokenHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenHelper").finish()
    }
}

impl TokenHelper {
    pub fn new(
        realm: TokenRealm,
        jwt_secret: SecretString,
        access_ttl_secs: u64,
        refresh_ttl_secs: u64,
    ) -> Self {
        Self {
            realm,
            jwt_secret,
            access_ttl_secs,
            refresh_ttl_secs,
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn new_for_test_with_realm(realm: TokenRealm) -> Self {
        Self {
            realm,
            jwt_secret: SecretString::from("unit-test-jwt-secret-at-least-32-bytes-long!!"),
            access_ttl_secs: 3600,
            refresh_ttl_secs: 86_400,
        }
    }

    pub fn encode_access_token<T: Display>(&self, user_id: T, jti: &str) -> Result<(String, u64)> {
        let key = EncodingKey::from_secret(self.jwt_secret.expose_secret().as_bytes());
        let now = get_current_timestamp();
        let claims = Claims {
            sub: user_id.to_string(),
            exp: now + self.access_ttl_secs,
            iat: now,
            jti: jti.to_string(),
        };
        let token = encode(&Header::default(), &claims, &key)?;
        Ok((token, self.access_ttl_secs))
    }

    pub fn decode_access_token(&self, token: &str) -> Result<Claims> {
        let key = DecodingKey::from_secret(self.jwt_secret.expose_secret().as_bytes());
        let validation = Validation::new(Algorithm::HS256);
        let data = decode::<Claims>(token, &key, &validation)?;
        Ok(data.claims)
    }

    pub fn realm(&self) -> &str {
        &self.realm
    }

    pub fn access_ttl_secs(&self) -> u64 {
        self.access_ttl_secs
    }

    pub fn refresh_ttl_secs(&self) -> u64 {
        self.refresh_ttl_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, get_current_timestamp};

    fn helper(access_ttl: u64) -> TokenHelper {
        TokenHelper::new(
            TokenRealm::Account,
            SecretString::from("unit-test-secret-at-least-32-bytes-long!!"),
            access_ttl,
            86_400,
        )
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let h = helper(3600);
        let (token, expires_in) = h.encode_access_token(42u64, "jti-1").unwrap();
        assert_eq!(expires_in, 3600);

        let claims = h.decode_access_token(&token).unwrap();
        assert_eq!(claims.sub, "42");
        assert_eq!(claims.jti, "jti-1");
        assert!(claims.exp > claims.iat);
        assert_eq!(claims.exp, claims.iat + 3600);
    }

    #[test]
    fn test_decode_with_wrong_secret_fails() {
        let h = helper(3600);
        let other = TokenHelper::new(
            TokenRealm::Account,
            SecretString::from("another-secret-at-least-32-bytes-long!!"),
            3600,
            86_400,
        );
        let (token, _) = h.encode_access_token(1u64, "j").unwrap();
        assert!(other.decode_access_token(&token).is_err());
    }

    #[test]
    fn test_decode_expired_token_fails() {
        let h = helper(3600);
        let key = EncodingKey::from_secret(h.jwt_secret.expose_secret().as_bytes());
        let now = get_current_timestamp();
        let expired = Claims {
            sub: "1".into(),
            exp: now.saturating_sub(100), // 100 秒前过期
            iat: now.saturating_sub(3600),
            jti: "j".into(),
        };
        let token = encode(&Header::default(), &expired, &key).unwrap();
        assert!(h.decode_access_token(&token).is_err());
    }

    #[test]
    fn test_decode_garbage_fails() {
        let h = helper(3600);
        assert!(h.decode_access_token("not-a-jwt").is_err());
    }

    #[test]
    fn test_realm_and_ttls() {
        let h = helper(3600);
        assert_eq!(h.realm(), "account");
        assert_eq!(h.access_ttl_secs(), 3600);
        assert_eq!(h.refresh_ttl_secs(), 86_400);
    }
}
