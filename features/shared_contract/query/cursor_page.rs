use serde::Serialize;
use utoipa::ToSchema;

use super::paging_result::CursorPagingResult;
use crate::value_object::id::ID;

/// 将「多取一条」的查询结果整理为 [`CursorPagingResult`]。
pub fn finalize_cursor_page<T>(
    mut items: Vec<T>,
    page_limit: u64,
    cursor_id: impl Fn(&T) -> ID,
) -> CursorPagingResult<T>
where
    T: Serialize + ToSchema,
{
    let limit = page_limit as usize;
    let has_more = items.len() > limit;
    let next_cursor = if has_more {
        items.pop();
        items.last().map(|item| cursor_id(item).to_string())
    } else {
        None
    };
    CursorPagingResult { items, next_cursor }
}
