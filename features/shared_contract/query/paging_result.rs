use crate::value_object::id::ID;
use serde::Serialize;
use utoipa::ToSchema;

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
    pub next_cursor: Option<ID>,
}
