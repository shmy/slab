# 游标分页统一为 ID keyset（shared_contract::query 深模块）

Status: accepted

> **2026-08-15**：将 keyset 分页生命周期收编进 `shared_contract::query::cursor_page`（`paginate` / `paginate_with`），删除 8 个搜索端点内联的重复 SQL 中间段与 `finalize_cursor_page`。

## 背景

2026-07 至 2026-08 期间，游标分页接缝被连锁修改 5 次：引入排序复合游标（f40d451）→ 移除复合游标统一 ID 游标（198069f / 11da03a）→ 参数 `next_cursor` 改名 `cursor`（198069f）→ 抽取 `fetch_limit`（3eaab3c）→ 移除列表排序收敛单字段 keyset（163b4ea）。每次改动都要同步编辑 8 个搜索端点，因为共享层（`CursorPagingQuery` 解析 + `finalize_cursor_page` 收尾，共 141 行）只覆盖「两头」，真实的 keyset 语义（游标条件 / `ORDER BY id DESC` / `LIMIT limit+1` / has_more）全部内联在端点里，且拼写已漂移（audit 限定列 `("audit_logs","id")`、account/item 局部变量、inventory 手工 cast）。

## 决策要点

1. **深模块归 `shared_contract::query::cursor_page`**：`paginate`（FromRow 快路径）+ `paginate_with`（任意行形态 + 映射闭包 → `(T, ID)`）两个入口，共享私有 `apply_keyset` / `finalize`。模块拥有全生命周期：追加 keyset 子句 → build_sqlx → fetch → 行映射 → has_more 判定 → next_cursor 提取。新增 `sea-query` / `sea-query-sqlx` 依赖（libs 类，arch_test 允许 contract 依赖）。
2. **游标列显式参数化，方向固定 `DESC`**：调用方传 `"id"`（单表）或限定列 `("table", "id")`（LEFT JOIN 防歧义，audit 必需）。方向不进接口——升序/复合键若未来需要，是新的函数而不是本函数的参数（2026-07 已否定复合排序游标，见背景）。
3. **游标 id 提取不依赖调用方 select**：`paginate` 由模块追加别名列 `__cursor_id` 按名读取（`T::from_row` 按名解码忽略多余列）；`paginate_with` 由映射闭包返回 `(T, ID)`（tuple 按位解码，游标 id 与列标签解耦）。不为 `T` 引入任何游标 trait。
4. **执行入口 `&mut PgConnection`**（仓库参数约定）；错误统一 `rootcause::Result`，sqlx 错误透传 → 500（内部故障语义，无新 locale key）。
5. **`CursorPagingQuery` 表面收紧**：`cursor_id()` / `limit()` 降 `pub(crate)`，删除 `fetch_limit()`——「怎么翻页」完全锁进模块，公共表面只留解析（DTO 内嵌反序列化）与 `paginate*`。
6. **测试面 = 模块接口**：`#[sqlx::test]` 自建表（contract 不得依赖 infrastructure::migration），覆盖首页 has_more / 翻页 / 恰满 limit / 空表 / 业务 WHERE 保真 / limit clamp / 限定列 + tuple 行（镜像 audit）/ mapper 错误传播；8 个端点测试保留分页冒烟断言守护接线，不再重复语义断言。
7. **模板与约定同步**：`docs/templates/endpoint_template.rs` Pattern B 改为 `paginate` 写法；新搜索端点一律走 `paginate*`（软约束，靠模板 + code review 兜底，不设 arch_test）。

## Considered Options

- **收进 `libs/filter_kit`（被否）**：libs 语义上更「干净」，但引入 lib → `features/shared_contract` 新依赖方向，且拆散 `CursorPagingQuery` 与行为的局部性。
- **装配器形态（被否）**：只追加 keyset 子句、把 build/fetch/finalize 留给端点——删除测试不过关（复杂度弹回端点），浅。
- **快路径隐藏游标列默认 `"id"`（被否）**：audit 证明列必须显式（LEFT JOIN 歧义），默认值掩盖「为什么需要参数化」；两个入口统一显式传列。
- **双闭包（map + id_of）（被否）**：接口多一个维度；单闭包返回 `(T, ID)` 使 next_cursor 与行映射同处显式对齐。

## Consequences

- 端点文件只表达「查什么」（列 + 业务筛选），不再表达「怎么翻页」；新搜索端点 = 3 行示例可抄。
- 未来 keyset 语义变更（多列游标、升序、total 计数、换执行后端）只改 `cursor_page.rs` 一个文件，9 个调用点零改动。
- 删除 `finalize_cursor_page`（公共表面净变化：+2 函数，-1 函数）；`fetch_limit` 不再存在。
- 别名列 `__cursor_id` 是模块内部约定（与真实表列撞名概率可忽略，注释已声明保留字）。
- `paginate_with` 的 tuple 行按位解码，列序 = select 列序——改列序会静默错位，文档已写明（audit 保持 tuple 顺序）。
