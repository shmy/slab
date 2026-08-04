use std::{fmt, ops::Deref, str::FromStr};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

use tsid::create_tsid;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// ID
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, sqlx::Type)]
#[sqlx(transparent)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(value_type = String, example = "1234567890123456789"))]
pub struct ID(i64);

impl ID {
    pub fn new() -> Self {
        let id = create_tsid();
        Self(id.number() as i64)
    }

    pub fn new_unchecked(id: i64) -> Self {
        Self(id)
    }
}

impl Default for ID {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for ID {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<i64> for ID {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<ID> for i64 {
    fn from(value: ID) -> Self {
        value.0
    }
}

impl FromStr for ID {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(i64::from_str(s)?))
    }
}

impl Serialize for ID {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

struct IdVisitor;

impl<'de> Visitor<'de> for IdVisitor {
    type Value = ID;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string or integer id")
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        let id = v.parse::<i64>().map_err(E::custom)?;
        Ok(ID(id))
    }

    fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ID(v))
    }

    fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        let id = i64::try_from(v).map_err(E::custom)?;
        Ok(ID(id))
    }
}

impl<'de> Deserialize<'de> for ID {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(IdVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_new_positive() {
        let id = ID::new();
        assert!(i64::from(id) > 0);
    }

    #[test]
    fn test_from_str_valid() {
        let id: ID = "12345".parse().unwrap();
        assert_eq!(i64::from(id), 12345);
    }

    #[test]
    fn test_from_str_invalid() {
        assert!("abc".parse::<ID>().is_err());
    }

    #[test]
    fn test_serde_serialize_as_string() {
        let id = ID::from(123i64);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""123""#);
    }

    #[test]
    fn test_serde_deserialize_from_string() {
        let id: ID = serde_json::from_str(r#""456""#).unwrap();
        assert_eq!(i64::from(id), 456);
    }

    #[test]
    fn test_serde_deserialize_from_number() {
        let id: ID = serde_json::from_str("789").unwrap();
        assert_eq!(i64::from(id), 789);
    }
}
