//! # 端点模板
//!
//! ## 使用方式
//!
//! 1. 复制本文件为 `features/{domain}/endpoint/{resource}_{action}.rs`
//! 2. 按场景选择下面一个 Pattern，删掉其他
//! 3. 完成 **注册清单**
//! 4. 运行 `just ai-check {domain}` 验证
//!
//! ## 注册清单（写完端点后必须做）
//!
//! - [ ] `features/{domain}/endpoint.rs` — 加 `pub(crate) mod {resource}_{action};`
//! - [ ] `features/{domain}/lib.rs` — 在 `protected_routing()` 或 `unprotected_routing()` 加路由
//! - [ ] 如果新建域：`bin/server/modules.rs` 的 `MODULES` 数组加一行
//! - [ ] 如果新建域：workspace `Cargo.toml` 加两个成员
//!
//! ## 常见问题
//!
//! - DTO 命名：`{Action}{Resource}{Request/Response/Path/Query}`
//! - `#[derive(Validify)]` 时**不要** `use rootcause::Result`（宏会遮蔽）
//! - `handler` 和 `execute` 都要 `#[tracing::instrument]`
//! - `execute` 额外加 `#[inline]`
//! - Contract Entity 不承载审计字段（created_at/updated_at）
//! - 列表查询用 SeaQuery，禁止可选参数 NULL 哨兵；**多列搜索时用 `.or()` 组合**
//! - 域内写库用 `crate::repository::{Aggregate}Repository`，跨域只读用 `{domain}_contract::port::{Domain}Port`
//! - 密码相关操作通过 `AccountRepository::update_password()` / `get_password_hash()`
//! - 写入类端点用 `pg_pool.begin().await?` 启动显式事务
//! - 需要 DB schema 时用 `psql describe <table>` — 运行中的表结构总是最新的

// ====================================================================
// Pattern A: 创建资源（POST）
// 参考: features/identity/endpoint/account_create.rs
// ====================================================================
// -- 取消下面的注释块使用 --
// use crate::repository::account_repository::AccountRepository;
// use event_bus::publish;
// #[derive(Debug, Deserialize, Validify, ToSchema)]
// pub(crate) struct CreateResourceRequest {
//     #[schema(example = "Tom")]
//     pub name: String,
//     #[validify]
//     pub phone: shared_contract::value_object::phone_number::PhoneNumber,
// }
// #[derive(Debug, Serialize, ToSchema)]
// pub(crate) struct CreateResourceResponse {
//     pub id: shared_contract::value_object::id::ID,
// }
// #[utoipa::path(
//     post,
//     path = "/api/v1/resources",
//     operation_id = "resource_create",
//     tag = "resource",
//     request_body = CreateResourceRequest,
//     responses((status = 200, body = JsonResponse<CreateResourceResponse>)),
//     security(("bearerAuth" = []))
// )]
// #[tracing::instrument]
// pub(crate) async fn handler(
//     State(pg_pool): State<PgPool>,
//     ValidJson(request): ValidJson<CreateResourceRequest>,
// ) -> JsonResponseType<CreateResourceResponse> {
//     let response = execute(&pg_pool, request).await?;
//     JsonResponse::ok(response)
// }
// #[tracing::instrument]
// #[inline]
// async fn execute(pg_pool: &PgPool, request: CreateResourceRequest)
//     -> rootcause::Result<CreateResourceResponse>
// {
//     use shared_contract::value_object::id::ID;
//     let id = ID::new();
//     let mut conn = pg_pool.acquire().await?;
//     let mut txn = conn.begin().await?;
//     // let entity = SomeEntity { id, ... };
//     // AccountRepository::create(&mut txn, &entity).await?;
//     // publish(txn.as_mut(), &SomeEvent { id }).await?;
//     txn.commit().await?;
//     Ok(CreateResourceResponse { id })
// }

// ====================================================================
// Pattern B: 列表+搜索（GET with SeaQuery + keyset 游标分页）
// 参考: features/identity/endpoint/account_search.rs（快路径）
//       features/audit/endpoint/audit_search.rs（LEFT JOIN + 映射闭包）
// 分页由 shared_contract::query::cursor_page 深模块接管：keyset 条件 / ORDER BY id
// DESC / LIMIT limit+1 / has_more / next_cursor 全部在接缝后，端点只声明列与业务筛选。
// ====================================================================
// -- 取消下面的注释块使用 --
// use sea_query::{Expr, ExprTrait as _, Query};
// use serde_with::{NoneAsEmptyString, serde_as};
// use shared_contract::query::cursor_page::paginate;
// use shared_contract::query::paging_query::CursorPagingQuery;
// use shared_contract::query::paging_result::CursorPagingResult;
// #[serde_as]
// #[derive(Debug, Deserialize, Validify, IntoParams)]
// #[into_params(parameter_in = Query)]
// pub(crate) struct SearchQuery {
//     #[serde(flatten)]
//     #[param(inline)]
//     pub paging: CursorPagingQuery,
//     #[param(example = "keyword")]
//     #[serde_as(as = "NoneAsEmptyString")]
//     #[serde(default)]
//     pub q: Option<String>,
// }
// #[derive(Serialize, FromRow, ToSchema)]
// pub(crate) struct SearchItem {
//     pub id: shared_contract::value_object::id::ID,
//     pub name: String,
// }
// #[utoipa::path(
//     get,
//     path = "/api/v1/resources",
//     operation_id = "resource_search",
//     tag = "resource",
//     params(SearchQuery),
//     responses((status = 200, body = JsonResponse<CursorPagingResult<SearchItem>>)),
//     security(("bearerAuth" = []))
// )]
// #[tracing::instrument]
// pub(crate) async fn handler(
//     State(pg_pool): State<PgPool>,
//     ValidQuery(query): ValidQuery<SearchQuery>,
// ) -> JsonResponseType<CursorPagingResult<SearchItem>> {
//     let response = execute(&pg_pool, query).await?;
//     JsonResponse::ok(response)
// }
// #[tracing::instrument]
// #[inline]
// async fn execute(pg_pool: &PgPool, query: SearchQuery)
//     -> rootcause::Result<CursorPagingResult<SearchItem>>
// {
//     let q = query.q.filter(|s| !s.is_empty());
//
//     let select = Query::select()
//         .from("resources")  // 替换表名
//         .columns([/* 列名 */])
//         .and_where_option(q.map(|q| {
//             Expr::col("name")
//                 .ilike(format!("%{q}%"))
//                 .or(Expr::col("phone").ilike(format!("%{q}%")))
//         }))
//         .to_owned();
//
//     let mut conn = pg_pool.acquire().await?;
//     paginate(&mut *conn, select, &query.paging, "id").await
// }
//
// // LEFT JOIN / 派生字段（如变更历史 diff）用 paginate_with，游标列须限定：
// //   paginate_with(&mut *conn, select, &query.paging, ("t", "id"), |row: (i64, ...)| Ok((item, ID::from(row.0))))


// ====================================================================
// Pattern C: 读单条（GET by ID，通过 Port）
// 参考: features/identity/endpoint/account_get.rs
// ====================================================================
// -- 取消下面的注释块使用 --
// use identity_contract::port::account_port::AccountPort;
// use shared_contract::value_object::id::ID;
// #[derive(Debug, Deserialize, Validify, IntoParams)]
// #[into_params(parameter_in = Path)]
// pub(crate) struct GetResourcePath {
//     pub id: ID,
// }
// #[derive(Debug, Serialize, ToSchema)]
// pub(crate) struct GetResourceResponse {
//     pub id: ID,
//     pub name: String,
// }
// #[utoipa::path(
//     get,
//     path = "/api/v1/resources/{id}",
//     operation_id = "resource_get",
//     tag = "resource",
//     params(GetResourcePath),
//     responses((status = 200, body = JsonResponse<GetResourceResponse>)),
//     security(("bearerAuth" = []))
// )]
// #[tracing::instrument]
// pub(crate) async fn handler(
//     State(pg_pool): State<PgPool>,
//     ValidPath(path): ValidPath<GetResourcePath>,
// ) -> JsonResponseType<GetResourceResponse> {
//     let response = execute(&pg_pool, path).await?;
//     JsonResponse::ok(response)
// }
// #[tracing::instrument]
// #[inline]
// async fn execute(pg_pool: &PgPool, path: GetResourcePath)
//     -> rootcause::Result<GetResourceResponse>
// {
//     let mut conn = pg_pool.acquire().await?;
//     let account = AccountPort::by_id(&mut conn, &path.id).await?;
//     Ok(GetResourceResponse { id: account.id, name: account.name })
// }

// ====================================================================
// Pattern D: 鉴权动作（当前用户操作，用 AuthedAccount）
// 参考: features/identity/endpoint/account_update_password.rs
// ====================================================================
// -- 取消下面的注释块使用 --
// use http_auth::extract::authed_account::AuthedAccount;
// #[derive(Debug, Deserialize, Validify, ToSchema)]
// pub(crate) struct ActionRequest {
//     pub field: String,
// }
// #[derive(Debug, Serialize, ToSchema)]
// pub(crate) struct ActionResponse {
//     pub updated: bool,
// }
// #[utoipa::path(
//     patch,
//     path = "/api/v1/resources/action",
//     operation_id = "resource_action",
//     tag = "resource",
//     request_body = ActionRequest,
//     responses((status = 200, body = JsonResponse<ActionResponse>)),
//     security(("bearerAuth" = []))
// )]
// #[tracing::instrument]
// pub(crate) async fn handler(
//     AuthedAccount(account_id): AuthedAccount,
//     State(pg_pool): State<PgPool>,
//     ValidJson(request): ValidJson<ActionRequest>,
// ) -> JsonResponseType<ActionResponse> {
//     let response = execute(&pg_pool, account_id, request).await?;
//     JsonResponse::ok(response)
// }
// #[tracing::instrument]
// #[inline]
// async fn execute(pg_pool: &PgPool, account_id: ID, request: ActionRequest)
//     -> rootcause::Result<ActionResponse>
// {
//     let mut conn = pg_pool.acquire().await?;
//     let mut txn = conn.begin().await?;
//     // 用 account_id 操作当前用户的数据
//     txn.commit().await?;
//     Ok(ActionResponse { updated: true })
// }

// ====================================================================
// 测试（所有 Pattern 通用）
// 参考: features/identity/endpoint/account_create.rs 末尾
// ====================================================================
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use migration::run_migrations;
//     use appctx::testing;
//
//     #[sqlx::test]
//     async fn test_xxx(pool: sqlx::PgPool) {
//         run_migrations(&pool).await.expect("run migrations");
//         let state = testing::build(pool).await;
//         // ...
//     }
// }
