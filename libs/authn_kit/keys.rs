use std::fmt::Display;

use axum::http::{HeaderMap, Uri, header::AUTHORIZATION};
use serde::Deserialize;

use crate::error::AuthnError;

pub fn access_token_from_parts(headers: &HeaderMap, uri: &Uri) -> Result<String, AuthnError> {
    get_access_token_from_header(headers)
        .or_else(|| get_access_token_from_query(uri))
        .ok_or(AuthnError::AccessTokenMissing)
}

pub fn refresh_key(realm: &str, refresh_token: &str) -> String {
    format!("auth:{realm}:refresh:{refresh_token}")
}

pub fn subject_refresh_key(realm: &str, subject: impl Display) -> String {
    format!("auth:{realm}:subject_refresh:{subject}")
}

pub fn access_jti_key(realm: &str, subject: impl Display) -> String {
    format!("auth:{realm}:access_jti:{subject}")
}

fn get_access_token_from_header(header_map: &HeaderMap) -> Option<String> {
    header_map
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn get_access_token_from_query(uri: &Uri) -> Option<String> {
    uri.query()
        .and_then(|query| serde_urlencoded::from_str::<AccessTokenInQuery>(query).ok())
        .and_then(|query| query.access_token.or(query.token))
}

#[derive(Deserialize)]
struct AccessTokenInQuery {
    access_token: Option<String>,
    token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Uri;

    fn headers_with_bearer(token: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        map
    }

    #[test]
    fn test_token_from_bearer_header() {
        let uri: Uri = "/api/v1/orders".parse().unwrap();
        let h = headers_with_bearer("abc.def");
        assert_eq!(access_token_from_parts(&h, &uri).unwrap(), "abc.def");
    }

    #[test]
    fn test_token_from_query_access_token() {
        let uri: Uri = "/api/v1/orders?access_token=q1w2".parse().unwrap();
        assert_eq!(
            access_token_from_parts(&HeaderMap::new(), &uri).unwrap(),
            "q1w2"
        );
    }

    #[test]
    fn test_token_from_query_token_alias() {
        let uri: Uri = "/api/v1/orders?token=q1w2".parse().unwrap();
        assert_eq!(
            access_token_from_parts(&HeaderMap::new(), &uri).unwrap(),
            "q1w2"
        );
    }

    #[test]
    fn test_header_wins_over_query() {
        let uri: Uri = "/api/v1/orders?access_token=from_query".parse().unwrap();
        let h = headers_with_bearer("from_header");
        assert_eq!(access_token_from_parts(&h, &uri).unwrap(), "from_header");
    }

    #[test]
    fn test_missing_token_is_error() {
        let uri: Uri = "/api/v1/orders".parse().unwrap();
        assert!(matches!(
            access_token_from_parts(&HeaderMap::new(), &uri),
            Err(AuthnError::AccessTokenMissing)
        ));
    }

    #[test]
    fn test_non_bearer_prefix_falls_back_to_query() {
        // 只认精确的 "Bearer " 前缀（大小写敏感），否则回退 query
        let uri: Uri = "/api/v1/orders?access_token=q1w2".parse().unwrap();
        let mut map = HeaderMap::new();
        map.insert(AUTHORIZATION, "bearer lowercase".parse().unwrap());
        assert_eq!(access_token_from_parts(&map, &uri).unwrap(), "q1w2");
    }

    #[test]
    fn test_key_builders() {
        assert_eq!(refresh_key("account", "rt"), "auth:account:refresh:rt");
        assert_eq!(
            subject_refresh_key("account", 42),
            "auth:account:subject_refresh:42"
        );
        assert_eq!(access_jti_key("account", 7), "auth:account:access_jti:7");
    }
}
