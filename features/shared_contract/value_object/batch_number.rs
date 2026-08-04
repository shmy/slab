use std::{fmt, ops::Deref};

use serde::{Deserialize, Serialize};

/// 批号（P2 启用）
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct BatchNumber(String);

impl BatchNumber {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for BatchNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for BatchNumber {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
