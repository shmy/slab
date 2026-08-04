use validify::{ValidationError, ValidationErrors};

/// 从 validify 校验错误中提取首个 l10n key。
/// 优先取 `message`（若非空），其次退回 `code`。
pub(super) fn errors_to_key(errors: ValidationErrors) -> String {
    errors
        .errors()
        .iter()
        .find_map(l10n_key_from_validation_error)
        .unwrap_or_else(|| "invalid_request".to_string())
}

fn l10n_key_from_validation_error(err: &ValidationError) -> Option<String> {
    match err {
        ValidationError::Field { message, code, .. } => {
            if let Some(m) = message
                && !m.is_empty()
            {
                return Some(m.clone());
            }
            Some((*code).to_string())
        }
        ValidationError::Schema { message, code, .. } => {
            if let Some(m) = message
                && !m.is_empty()
            {
                return Some(m.clone());
            }
            Some((*code).to_string())
        }
    }
}
