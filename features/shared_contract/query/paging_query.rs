use serde::Deserialize;

use serde_with::{NoneAsEmptyString, PickFirst, Same, serde_as};

#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

use crate::value_object::id::ID;

#[serde_as]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams, ToSchema))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct CursorPagingQuery {
    /// 游标：无排序 = 上一页最后一条的数字 id；有排序 = 复合游标（排序字段值 + id 的 JSON 串）
    #[cfg_attr(feature = "openapi", param(value_type = Option<String>, example = "1983507123456789012"))]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>, example = "1983507123456789012"))]
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    cursor: Option<String>,
    /// 每页条数（1-100）
    #[cfg_attr(feature = "openapi", param(value_type = i64, example = 10))]
    #[cfg_attr(feature = "openapi", schema(default = 10, example = 10))]
    #[serde_as(as = "PickFirst<(Same, NoneAsEmptyString)>")]
    #[serde(default)]
    limit: Option<i64>,
}

impl CursorPagingQuery {
    const DEFAULT_LIMIT: i64 = 10;

    /// 游标原始串（空串/缺省 → None）
    pub fn cursor_str(&self) -> Option<&str> {
        self.cursor.as_deref().filter(|s| !s.is_empty())
    }

    /// 数字游标（id 分页）；复合游标（非数字）返回 None
    pub fn cursor_id(&self) -> Option<ID> {
        self.cursor_str().and_then(|s| s.parse().ok())
    }

    pub fn limit(&self) -> u64 {
        self.limit.unwrap_or(Self::DEFAULT_LIMIT).clamp(1, 100) as u64
    }

    /// 每页实际取 limit+1 条：多取一条判定 has_more（[`finalize_cursor_page`] 弹掉多余行）
    pub fn fetch_limit(&self) -> u64 {
        self.limit() + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_limit_is_limit_plus_one() {
        let q: CursorPagingQuery = serde_json::from_str(r#"{"limit":"12"}"#).unwrap();
        assert_eq!(q.limit(), 12);
        assert_eq!(q.fetch_limit(), 13);
    }

    #[test]
    fn cursor_paging_empty_limit_string() {
        let q: CursorPagingQuery = serde_json::from_str(r#"{"cursor":"","limit":""}"#).unwrap();
        assert_eq!(q.limit(), 10);
        assert!(q.cursor_str().is_none());
        assert!(q.cursor_id().is_none());
    }

    #[test]
    fn cursor_paging_limit_string() {
        let q: CursorPagingQuery = serde_json::from_str(r#"{"cursor":"","limit":"12"}"#).unwrap();
        assert_eq!(q.limit(), 12);
        assert!(q.cursor_id().is_none());
    }

    #[test]
    fn cursor_paging_limit_numeric_json() {
        let q: CursorPagingQuery = serde_json::from_value(serde_json::json!({
            "limit": 25,
        }))
        .unwrap();
        assert_eq!(q.limit(), 25);
    }

    #[test]
    fn cursor_id_parses_number_only() {
        let q: CursorPagingQuery =
            serde_json::from_str(r#"{"cursor":"1983507123456789012"}"#).unwrap();
        assert_eq!(q.cursor_id().map(|c| i64::from(c)), Some(1983507123456789012));
        // 复合游标（JSON 串）不是数字 id
        let q: CursorPagingQuery =
            serde_json::from_str(r#"{"cursor":"{\"f\":\"name\",\"v\":\"李娜\",\"id\":\"2\"}"}"#)
                .unwrap();
        assert!(q.cursor_id().is_none());
        assert!(q.cursor_str().is_some());
    }
}
