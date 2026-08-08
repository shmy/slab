//! web 层产出的全部 l10n key（单一真相源）。
//!
//! locale 结构性测试扫描本文件中值为纯 snake_case 的 `pub const` 字符串常量行，
//! 校验每个 key 在 en-US / zh-CN bundle 都有翻译——**新增 key 必须在此登记**，
//! 忘登记即测试失败。

pub const INVALID_REQUEST_BODY: &str = "invalid_request_body";
pub const INVALID_PATH_PARAMS: &str = "invalid_path_params";

pub const JSON_BODY_SYNTAX: &str = "json_body_syntax";
pub const JSON_BODY_MISSING_FIELD: &str = "json_body_missing_field";
pub const JSON_BODY_INVALID_TYPE: &str = "json_body_invalid_type";
pub const JSON_BODY_UNKNOWN_FIELD: &str = "json_body_unknown_field";
pub const JSON_BODY_DUPLICATE_FIELD: &str = "json_body_duplicate_field";
pub const JSON_BODY_TRAILING: &str = "json_body_trailing";

pub const QUERY_MISSING_FIELD: &str = "query_missing_field";
pub const QUERY_INVALID_TYPE: &str = "query_invalid_type";
pub const QUERY_UNKNOWN_FIELD: &str = "query_unknown_field";
pub const QUERY_DUPLICATE_FIELD: &str = "query_duplicate_field";
pub const QUERY_INVALID: &str = "query_invalid";

pub const PATH_PARAMS_INVALID_TYPE: &str = "path_params_invalid_type";
pub const PATH_PARAMS_PARSE_ERROR: &str = "path_params_parse_error";
pub const PATH_PARAMS_WRONG_COUNT: &str = "path_params_wrong_count";

pub const MULTIPART_MISSING_FIELD: &str = "multipart_missing_field";
pub const MULTIPART_WRONG_FIELD_TYPE: &str = "multipart_wrong_field_type";
pub const MULTIPART_DUPLICATE_FIELD: &str = "multipart_duplicate_field";
pub const MULTIPART_UNKNOWN_FIELD: &str = "multipart_unknown_field";
pub const MULTIPART_INVALID_ENUM_VALUE: &str = "multipart_invalid_enum_value";
pub const MULTIPART_FIELD_TOO_LARGE: &str = "multipart_field_too_large";
