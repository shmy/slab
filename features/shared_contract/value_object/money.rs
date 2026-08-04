use std::{
    fmt,
    iter::Sum,
    ops::{Add, Deref, Mul},
};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use sqlx::prelude::FromRow;
use utoipa::ToSchema;

/// 金额，单位：分
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, ToSchema, FromRow, sqlx::Type,
)]
#[schema(value_type = i64, example = 100)]
#[sqlx(transparent)]
pub struct Money(i64);

impl Money {
    pub fn new(cents: i64) -> Self {
        Self(cents)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for Money {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Money> for i64 {
    fn from(value: Money) -> Self {
        value.0
    }
}

impl Serialize for Money {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

struct MoneyVisitor;

impl Visitor<'_> for MoneyVisitor {
    type Value = Money;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an integer money in cents")
    }

    fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Money(v))
    }

    fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        let v = i64::try_from(v).map_err(E::custom)?;
        Ok(Money(v))
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_i64(MoneyVisitor)
    }
}

impl Add for Money {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Money(self.0 + rhs.0)
    }
}

impl Mul<i64> for Money {
    type Output = Self;

    fn mul(self, rhs: i64) -> Self {
        Money(self.0 * rhs)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Money(0), |a, b| a + b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_roundtrip() {
        let money = Money::new(500);
        let json = serde_json::to_string(&money).unwrap();
        assert_eq!(json, "500");
        let deserialized: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, money);
    }

    #[test]
    fn test_display() {
        assert_eq!(Money::new(100).to_string(), "100");
    }
}
