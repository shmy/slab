use serde::Serialize;
use utoipa::ToSchema;

use crate::value_object::id::ID;

#[derive(Clone, Serialize, ToSchema)]
#[schema(bound = "T: ToSchema")]
pub struct PagingResult<T>
where
    T: Serialize + ToSchema,
{
    pub total: u64,
    pub items: Vec<T>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct CursorPagingResult<T>
where
    T: Serialize + ToSchema,
{
    pub items: Vec<T>,
    /// 上一页最后一条的数字 id（keyset：`id < next_cursor`）
    pub next_cursor: Option<ID>,
}
