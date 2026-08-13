use serde::Deserialize;

use serde_with::{NoneAsEmptyString, PickFirst, Same, serde_as};

#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

/// 分页数字字段：支持 JSON 数字；查询串里常见 `page=` / `limit=`（空字符串）视为未传。
///
/// `PickFirst` 先试标准 `Option<i64>`（数字等），再试 [`NoneAsEmptyString`]（`""` → `None`）。
#[serde_as]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams, ToSchema))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct PagingQuery {
    /// 页码
    #[cfg_attr(feature = "openapi", param(value_type = i64, example = 1))]
    #[cfg_attr(feature = "openapi", schema(default = 1, example = 1))]
    #[serde_as(as = "PickFirst<(Same, NoneAsEmptyString)>")]
    #[serde(default)]
    page: Option<i64>,
    /// 每页条数
    #[cfg_attr(feature = "openapi", param(value_type = i64, example = 10))]
    #[cfg_attr(feature = "openapi", schema(default = 10, example = 10))]
    #[serde_as(as = "PickFirst<(Same, NoneAsEmptyString)>")]
    #[serde(default)]
    page_size: Option<i64>,
}

impl PagingQuery {
    const DEFAULT_PAGE: i64 = 1;
    const DEFAULT_PAGE_SIZE: i64 = 10;

    fn page(&self) -> i64 {
        self.page.unwrap_or(Self::DEFAULT_PAGE).max(1)
    }

    fn page_size(&self) -> i64 {
        self.page_size
            .unwrap_or(Self::DEFAULT_PAGE_SIZE)
            .clamp(1, 100)
    }

    pub fn offset(&self) -> i64 {
        (self.page() - 1) * self.page_size()
    }

    pub fn limit(&self) -> i64 {
        self.page_size()
    }

    pub fn offset_u64(&self) -> u64 {
        self.offset() as u64
    }

    pub fn limit_u64(&self) -> u64 {
        self.limit() as u64
    }
}

#[serde_as]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams, ToSchema))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct CursorPagingQuery {
    /// 游标（上一页最后一条；数字 = id 游标，JSON = 排序复合游标）
    #[cfg_attr(feature = "openapi", param(value_type = Option<String>, example = "1983507123456789012"))]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>, example = "1983507123456789012"))]
    #[serde_as(as = "NoneAsEmptyString")]
    #[serde(default)]
    next_cursor: Option<String>,
    /// 每页条数（1-100）
    #[cfg_attr(feature = "openapi", param(value_type = i64, example = 10))]
    #[cfg_attr(feature = "openapi", schema(default = 10, example = 10))]
    #[serde_as(as = "PickFirst<(Same, NoneAsEmptyString)>")]
    #[serde(default)]
    limit: Option<i64>,
}

impl CursorPagingQuery {
    const DEFAULT_LIMIT: i64 = 10;

    /// 数字游标（旧端点：id 分页）；非数字（复合游标）返回 None
    pub fn next_cursor_id(&self) -> Option<i64> {
        self.next_cursor.as_deref().and_then(|s| s.parse().ok())
    }

    /// 原始游标字符串（排序复合游标等场景直接用）
    pub fn next_cursor_str(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    pub fn limit(&self) -> u64 {
        self.limit.unwrap_or(Self::DEFAULT_LIMIT).clamp(1, 100) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_paging_empty_limit_string() {
        let q: CursorPagingQuery =
            serde_json::from_str(r#"{"next_cursor":"","limit":""}"#).unwrap();
        assert_eq!(q.limit(), 10);
        assert!(q.next_cursor_str().is_none());
    }

    #[test]
    fn cursor_paging_limit_string() {
        let q: CursorPagingQuery =
            serde_json::from_str(r#"{"next_cursor":"","limit":"12"}"#).unwrap();
        assert_eq!(q.limit(), 12);
        assert!(q.next_cursor_str().is_none());
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
    fn paging_empty_page_size_json() {
        let q: PagingQuery = serde_json::from_str(r#"{"page":"","page_size":""}"#).unwrap();
        assert_eq!(q.page(), 1);
        assert_eq!(q.page_size(), 10);
    }
}
