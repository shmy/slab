use std::fmt;
use std::ops::Deref;

use rootcause::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validify::{Modify, Validate};

/// 手机号：规范化后为 11 位中国大陆手机号（`1[3-9]` + 9 位数字）。
///
/// `Debug` 输出打码（如 `PhoneNumber("138****8080")`），避免日志 / tracing span /
/// rootcause 报告泄露明文；序列化（serde）与 `Deref` 仍暴露明文供业务使用。
#[derive(Clone, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[schema(value_type = String, example = "13888888888")]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct PhoneNumber(String);

fn is_mainland_china_mobile(s: &str) -> bool {
    if s.len() != 11 {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let b = s.as_bytes();
    b.first() == Some(&b'1') && b.get(1).is_some_and(|c| (b'3'..=b'9').contains(c))
}

impl PhoneNumber {
    pub fn try_new(phone_number: &str) -> Result<Self> {
        let mut instance = Self(phone_number.to_owned());
        instance.modify();
        instance.validate()?;
        Ok(instance)
    }

    /// 打码展示：`13880808080` → `138****8080`。
    ///
    /// 长度不足 8 位时全量打码（`****`），保证日志路径绝不泄露明文。
    pub fn masked(&self) -> String {
        let s = self.0.as_str();
        let n = s.chars().count();
        if n < 8 {
            return "****".to_string();
        }
        let head: String = s.chars().take(3).collect();
        let tail: String = s.chars().skip(n - 4).collect();
        format!("{head}****{tail}")
    }
}

impl fmt::Debug for PhoneNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PhoneNumber").field(&self.masked()).finish()
    }
}

impl Modify for PhoneNumber {
    fn modify(&mut self) {
        self.0 = self.0.trim().to_string();
    }
}

impl Validate for PhoneNumber {
    fn validate(&self) -> std::result::Result<(), validify::ValidationErrors> {
        let mut errors = validify::ValidationErrors::new();
        let s = self.0.as_str();
        let len = s.len();
        if len < 11 {
            errors.add(validify::field_err!(
                "too_short",
                "phone_number_too_short",
                "phone_number"
            ));
        }
        if len > 11 {
            errors.add(validify::field_err!(
                "too_long",
                "phone_number_too_long",
                "phone_number"
            ));
        }
        if len == 11 && !is_mainland_china_mobile(s) {
            errors.add(validify::field_err!(
                "invalid_cn_mobile",
                "phone_number_invalid_cn_mobile",
                "phone_number"
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Deref for PhoneNumber {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_new_valid() {
        let phone = PhoneNumber::try_new("13888888888").unwrap();
        assert_eq!(&*phone, "13888888888");
    }

    #[test]
    fn test_try_new_trims_whitespace() {
        let phone = PhoneNumber::try_new("  13888888888  ").unwrap();
        assert_eq!(&*phone, "13888888888");
    }

    #[test]
    fn test_try_new_empty() {
        let err = PhoneNumber::try_new("").unwrap_err();
        assert!(err.to_string().contains("phone_number_too_short"));
    }

    #[test]
    fn test_try_new_invalid_cn_mobile() {
        let err = PhoneNumber::try_new("12812345678").unwrap_err();
        assert!(err.to_string().contains("phone_number_invalid_cn_mobile"));
    }

    #[test]
    fn test_masked_masks_middle() {
        let phone = PhoneNumber::try_new("13880808080").unwrap();
        assert_eq!(phone.masked(), "138****8080");
    }

    #[test]
    fn test_masked_short_fully_hidden() {
        // 绕过 try_new 校验直接构造短值，验证防御路径
        let phone = PhoneNumber("123".to_string());
        assert_eq!(phone.masked(), "****");
    }

    #[test]
    fn test_debug_is_masked() {
        let phone = PhoneNumber::try_new("13880808080").unwrap();
        let debug = format!("{phone:?}");
        assert_eq!(debug, "PhoneNumber(\"138****8080\")");
        assert!(!debug.contains("13880808080"));
    }

    #[test]
    fn test_deref_still_exposes_plaintext() {
        let phone = PhoneNumber::try_new("13880808080").unwrap();
        assert_eq!(&*phone, "13880808080");
    }
}
