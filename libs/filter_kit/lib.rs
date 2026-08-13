//! PostgREST 风格筛选/排序解析（PostgreSQL 生态惯例，Supabase 同款）。
//!
//! 筛选：每个字段一个 query 参数，值 = `{op}.{value}`，多参数天然 AND：
//! ```text
//! name=ilike.*张*&amount=gte.1000&created_at=gt.2024-03-15
//! ```
//!
//! 排序：`order` 参数 = `{field}.{asc|desc}`，逗号分隔多级：
//! ```text
//! order=name.asc,created_at.desc
//! ```
//!
//! 分页游标：统一数字 id 游标（`id < cursor`，配合默认 `ORDER BY id DESC`）；
//! 有排序时用 [`cursor_where`] 生成单字段 keyset 谓词（排序键值以标量子查询现取，
//! 游标仍是不带排序信息的数字 id）。
//!
//! # 操作符矩阵（按 [`FilterSchema`] 列类型）
//!
//! | 列类型 | 支持操作符 |
//! |--------|-----------|
//! | text   | `eq` / `neq` / `ilike`（`*` 通配任意字符） |
//! | date   | `eq` / `neq` / `gt` / `gte` / `lt` / `lte`（自动 cast timestamptz） |
//! | int    | `eq` / `neq` / `gt` / `gte` / `lt` / `lte`（解析期校验 i64 格式） |
//!
//! 字段白名单与 SQL 生成同源（[`FilterSchema`] 三数组并集），解析期校验防注入；
//! 未覆盖的类型×操作符组合（如 `created_at=ilike.*`）在 SQL 生成期静默忽略。
//!
//! ⚠️ 陷阱：sea_query 1.0 的 `impl<T> ExprTrait for T where T: Into<Expr>` blanket
//! 会让作用域内的 `String` 获得 `ExprTrait::contains`（返回 Expr），方法解析优先于
//! `str::contains`。本 crate 因 `use ExprTrait as _` 必然中招——**不要在本 crate
//! 任何地方写 `String::contains(...)`**，用 `str::find(...)` / `find(...).is_some()`。

use std::collections::HashMap;
use std::fmt;

use sea_query::extension::postgres::PgExpr;
use sea_query::{Alias, Expr, ExprTrait as _, Order, Query, SelectStatement, SimpleExpr};

/// 筛选操作符（PostgREST 风格，值前缀）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `eq.` 等于
    Eq,
    /// `neq.` 不等于
    Neq,
    /// `gt.` 大于
    Gt,
    /// `gte.` 大于等于
    Gte,
    /// `lt.` 小于
    Lt,
    /// `lte.` 小于等于
    Lte,
    /// `ilike.` 大小写不敏感模糊匹配（值含通配符 `*` / `_`；仅 text 列）
    Ilike,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Op::Eq => "eq.",
            Op::Neq => "neq.",
            Op::Gt => "gt.",
            Op::Gte => "gte.",
            Op::Lt => "lt.",
            Op::Lte => "lte.",
            Op::Ilike => "ilike.",
        };
        f.write_str(s)
    }
}

/// 一条已通过白名单校验的筛选条件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    /// 原始值（ilike 含通配符 `*` / `_`；int 字段已校验为合法整数串）
    pub value: String,
}

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDir {
    Asc,
    Desc,
}

/// 一条排序项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderItem {
    pub field: String,
    pub dir: OrderDir,
}

/// 解析错误（Display 即 locale key；细节进 Debug/日志）
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    /// 值不是 `{op}.{value}` 格式 / int 值非整数 / 排序格式错误 / 操作符未知
    #[error("invalid_filter_syntax")]
    Syntax,
    /// 字段不在白名单
    #[error("filter_field_not_allowed")]
    FieldNotAllowed,
}

/// 可筛/可排字段声明：`text_fields`（eq/neq/ilike）、`date_fields`（eq/neq/gt/gte/lt/lte，
/// 自动 cast timestamptz）、`int_fields`（eq/neq/gt/gte/lt/lte，BIGINT/INTEGER 列）。
/// 白名单 = 三数组并集，与 SQL 生成同源，不会不一致。
#[derive(Debug, Clone, Copy)]
pub struct FilterSchema {
    pub text_fields: &'static [&'static str],
    pub date_fields: &'static [&'static str],
    pub int_fields: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColKind {
    Text,
    Date,
    Int,
}

impl FilterSchema {
    /// 白名单字段（text + date + int 并集）
    pub fn allowed_fields(&self) -> Vec<&'static str> {
        self.text_fields
            .iter()
            .chain(self.date_fields.iter())
            .chain(self.int_fields.iter())
            .copied()
            .collect()
    }

    fn col_kind(&self, field: &str) -> Option<ColKind> {
        if self.text_fields.contains(&field) {
            Some(ColKind::Text)
        } else if self.date_fields.contains(&field) {
            Some(ColKind::Date)
        } else if self.int_fields.contains(&field) {
            Some(ColKind::Int)
        } else {
            None
        }
    }

    /// 日期比较值：显式 cast，避免 `timestamptz > text` 报错
    fn date_value(&self, value: &str) -> SimpleExpr {
        Expr::val(value).cast_as(Alias::new("timestamptz"))
    }
}

/// 筛选操作符表：从长到短匹配前缀（`ilike.` 先于 `eq.` 等，避免子串误配）
const FILTER_OPERATORS: [(&str, Op); 7] = [
    ("ilike.", Op::Ilike),
    ("neq.", Op::Neq),
    ("gte.", Op::Gte),
    ("lte.", Op::Lte),
    ("gt.", Op::Gt),
    ("lt.", Op::Lt),
    ("eq.", Op::Eq),
];

/// 解析 PostgREST 风格筛选参数（query string 中除分页/搜索词之外的字段参数）。
///
/// int 字段的值在解析期校验为合法 i64（失败 → [`FilterError::Syntax`]），
/// SQL 生成期不再有类型失败路径。
pub fn parse(
    filters: &HashMap<String, String>,
    schema: &FilterSchema,
) -> Result<Vec<Condition>, FilterError> {
    let allowed = schema.allowed_fields();
    let mut conditions = Vec::new();
    for (field, raw) in filters {
        if !allowed.contains(&field.as_str()) {
            return Err(FilterError::FieldNotAllowed);
        }
        let mut parsed = None;
        for (op_str, op) in FILTER_OPERATORS {
            if let Some(value) = raw.strip_prefix(op_str) {
                if schema.col_kind(field) == Some(ColKind::Int) && value.parse::<i64>().is_err() {
                    return Err(FilterError::Syntax);
                }
                parsed = Some(Condition {
                    field: field.clone(),
                    op,
                    value: value.to_owned(),
                });
                break;
            }
        }
        conditions.push(parsed.ok_or(FilterError::Syntax)?);
    }
    // HashMap 迭代无序：按字段排序保证确定性（AND 交换律下语义不变，SQL 条件顺序稳定）
    conditions.sort_by(|a, b| a.field.cmp(&b.field));
    Ok(conditions)
}

/// 条件 → SeaQuery 表达式（字段白名单与 int 值格式由 [`parse`] 校验；此处按 schema
/// 类型安全映射，未覆盖的组合如 `created_at=ilike.*` 直接忽略）。
pub fn to_sql(conds: &[Condition], schema: &FilterSchema) -> Vec<SimpleExpr> {
    conds
        .iter()
        .filter_map(|c| {
            let col = Expr::col(c.field.to_string());
            match (schema.col_kind(&c.field), c.op) {
                (Some(ColKind::Text), Op::Eq) => Some(col.eq(c.value.as_str())),
                (Some(ColKind::Text), Op::Neq) => Some(col.ne(c.value.as_str())),
                // PostgREST 通配符语义：* = 任意字符（→ SQL %），_ 单字符保留
                (Some(ColKind::Text), Op::Ilike) => Some(col.ilike(c.value.replace('*', "%"))),
                (Some(ColKind::Date), op) => {
                    let v = schema.date_value(&c.value);
                    Some(match op {
                        Op::Eq => col.eq(v),
                        Op::Neq => col.ne(v),
                        Op::Gt => col.gt(v),
                        Op::Gte => col.gte(v),
                        Op::Lt => col.lt(v),
                        Op::Lte => col.lte(v),
                        Op::Ilike => return None,
                    })
                }
                (Some(ColKind::Int), op) => {
                    // parse 已保证 i64 格式；此处防御性忽略（永不失败路径）
                    let v = c.value.parse::<i64>().ok()?;
                    Some(match op {
                        Op::Eq => col.eq(v),
                        Op::Neq => col.ne(v),
                        Op::Gt => col.gt(v),
                        Op::Gte => col.gte(v),
                        Op::Lt => col.lt(v),
                        Op::Lte => col.lte(v),
                        Op::Ilike => return None,
                    })
                }
                _ => None,
            }
        })
        .collect()
}

/// 筛选参数 → WHERE 表达式（[`parse`] + [`to_sql`] 一步；空筛选返回空 Vec）。
pub fn filter_where(
    filters: &HashMap<String, String>,
    schema: &FilterSchema,
) -> Result<Vec<SimpleExpr>, FilterError> {
    Ok(to_sql(&parse(filters, schema)?, schema))
}

/// 排序子句应用到 select（OrderDir → sea_query [`Order`] 映射样板收敛在此）。
pub fn apply_order(select: &mut SelectStatement, clauses: &[(String, OrderDir)]) {
    for (field, dir) in clauses {
        select.order_by(
            field.to_string(),
            match dir {
                OrderDir::Asc => Order::Asc,
                OrderDir::Desc => Order::Desc,
            },
        );
    }
}

/// 搜索一站式：游标 keyset 谓词 + 筛选 WHERE + ORDER BY 一次应用到 select。
/// 无排序默认 `ORDER BY id DESC`；有排序单字段 keyset（[`cursor_where`]，
/// 标量子查询现取排序键值）。与 [`filter_where`] / [`apply_order`] / [`order_and_cursor`]
/// 同属 mutating 风格，调用方不再自行装配表达式。
pub fn apply_search(
    select: &mut SelectStatement,
    filters: &HashMap<String, String>,
    order: Option<&str>,
    schema: &FilterSchema,
    table: &str,
    cursor_id: Option<i64>,
) -> Result<(), FilterError> {
    let (clauses, cursor) = order_and_cursor(order, schema, table, cursor_id)?;
    select.and_where_option(cursor);
    for expr in filter_where(filters, schema)? {
        select.and_where(expr);
    }
    apply_order(select, &clauses);
    Ok(())
}

/// 排序解析：`"name.asc,created_at.desc"`（逗号分隔；省略方向默认 asc）。
pub fn parse_order(orders: &str, schema: &FilterSchema) -> Result<Vec<OrderItem>, FilterError> {
    let allowed = schema.allowed_fields();
    let mut out = Vec::new();
    for part in orders.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (field, dir) = match part.rsplit_once('.') {
            Some((f, "asc")) => (f, OrderDir::Asc),
            Some((f, "desc")) => (f, OrderDir::Desc),
            Some((f, _)) => {
                // 未知方向后缀：字段合法则方向语法错误，否则字段错误
                return if allowed.contains(&f) {
                    Err(FilterError::Syntax)
                } else {
                    Err(FilterError::FieldNotAllowed)
                };
            }
            None => (part, OrderDir::Asc),
        };
        if !allowed.contains(&field) {
            return Err(FilterError::FieldNotAllowed);
        }
        out.push(OrderItem {
            field: field.to_string(),
            dir,
        });
    }
    Ok(out)
}

/// 排序子句：`(列名, 方向)` 列表（供 ORDER BY 与 keyset 游标使用）
pub type OrderClauses = Vec<(String, OrderDir)>;

/// 排序子句：用户排序 + id 稳定二级排序（游标分页必须稳定）。
/// 返回 `(列名, 方向)` 列表，由调用方转 SeaQuery `order_by`。
pub fn order_clauses(orders: &[OrderItem]) -> OrderClauses {
    let mut clauses: Vec<(String, OrderDir)> =
        orders.iter().map(|o| (o.field.clone(), o.dir)).collect();
    // 用户未显式排 id 时，附加 id DESC 二级排序：同值字段内最新在前
    if !orders.iter().any(|o| o.field == "id") {
        clauses.push(("id".to_string(), OrderDir::Desc));
    }
    clauses
}

/// 单字段 keyset 游标谓词（配合 [`order_clauses`] 使用；游标仍是统一数字 id）。
///
/// 排序键值用标量子查询现取，调用方无需回查游标行：
/// ```text
/// asc:  f >  (SELECT f FROM {table} WHERE id = c)
///       OR (f = (SELECT f FROM {table} WHERE id = c) AND id < c)
/// desc: 同上，首比较换 `<`
/// ```
///
/// 语义 = 「游标行之后」继续分页，与 `ORDER BY f, id DESC` 完全一致。
/// 列对列比较自动匹配类型（无需 cast）；排序键为 NULL（可空列）时该行不参与分页比较。
pub fn cursor_where(table: &str, field: &str, dir: OrderDir, cursor_id: i64) -> SimpleExpr {
    // 游标行的排序键值：`SELECT f FROM {table} WHERE id = cursor`
    let key = || -> SimpleExpr {
        Query::select()
            .column(field.to_string())
            .from(table.to_string())
            .and_where(Expr::col("id").eq(cursor_id))
            .to_owned()
            .into()
    };
    let col = Expr::col(field.to_string());
    let after = match dir {
        OrderDir::Asc => col.clone().gt(key()),
        OrderDir::Desc => col.clone().lt(key()),
    };
    after.or(col.eq(key()).and(Expr::col("id").lt(cursor_id)))
}

/// 排序 + 游标谓词一站式：`order` 串（白名单校验，只取首字段）→ 排序子句与 keyset 游标 WHERE。
///
/// 返回 `(排序子句, 游标谓词)`：前者供 `order_by`，后者供 `and_where_option`。
/// 无排序 → 默认 `ORDER BY id DESC` + `id < cursor`；有排序 → 单字段 keyset
/// （[`cursor_where`]，标量子查询现取排序键值）。
pub fn order_and_cursor(
    order: Option<&str>,
    schema: &FilterSchema,
    table: &str,
    cursor_id: Option<i64>,
) -> Result<(OrderClauses, Option<SimpleExpr>), FilterError> {
    let orders = match order {
        Some(o) if !o.trim().is_empty() => parse_order(o, schema)?,
        _ => Vec::new(),
    };
    // 多级 order 只取首字段（keyset 游标以首字段为基准；表头排序即单字段）
    let orders = orders.into_iter().take(1).collect::<Vec<_>>();
    let clauses = if orders.is_empty() {
        vec![("id".to_string(), OrderDir::Desc)]
    } else {
        order_clauses(&orders)
    };
    let cursor = match (cursor_id, orders.first()) {
        (Some(c), None) => Some(Expr::col("id").lt(c)),
        (Some(c), Some(order)) => Some(cursor_where(table, &order.field, order.dir, c)),
        (None, _) => None,
    };
    Ok((clauses, cursor))
}

#[cfg(test)]
mod tests {

    use super::*;
    use sea_query::{PostgresQueryBuilder, Query};

    const SCHEMA: FilterSchema = FilterSchema {
        text_fields: &["code", "name", "phone", "contact_person"],
        date_fields: &["created_at"],
        int_fields: &["amount", "quantity"],
    };

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ---- 筛选解析 ----

    #[test]
    fn parses_multiple_conditions() {
        let conds = parse(
            &map(&[("name", "ilike.*张*"), ("created_at", "gt.2024-03-15")]),
            &SCHEMA,
        )
        .unwrap();
        assert_eq!(conds.len(), 2);
        // 按字段名排序：created_at 在前
        assert_eq!(conds[0].field, "created_at");
        assert_eq!(conds[0].op, Op::Gt);
        assert_eq!(conds[1].field, "name");
        assert_eq!(conds[1].op, Op::Ilike);
        assert_eq!(conds[1].value, "*张*");
    }

    #[test]
    fn eq_operator() {
        let conds = parse(&map(&[("code", "eq.C-001")]), &SCHEMA).unwrap();
        assert_eq!(conds[0].op, Op::Eq);
        assert_eq!(conds[0].value, "C-001");
    }

    #[test]
    fn neq_operator() {
        let conds = parse(&map(&[("code", "neq.C-001")]), &SCHEMA).unwrap();
        assert_eq!(conds[0].op, Op::Neq);
        assert_eq!(conds[0].value, "C-001");
    }

    #[test]
    fn gte_lte_operators() {
        let conds = parse(
            &map(&[("created_at", "gte.2024-01-01"), ("code", "lte.Z")]),
            &SCHEMA,
        )
        .unwrap();
        assert_eq!(conds[0].op, Op::Lte);
        assert_eq!(conds[1].op, Op::Gte);
    }

    #[test]
    fn int_field_parses_negative_and_range_ops() {
        let conds = parse(
            &map(&[("amount", "gte.-500"), ("quantity", "lt.10")]),
            &SCHEMA,
        )
        .unwrap();
        assert_eq!(conds[0].op, Op::Gte);
        assert_eq!(conds[0].value, "-500");
        assert_eq!(conds[1].op, Op::Lt);
        assert_eq!(conds[1].value, "10");
    }

    #[test]
    fn int_field_rejects_non_integer() {
        assert_eq!(
            parse(&map(&[("amount", "gte.abc")]), &SCHEMA)
                .unwrap_err()
                .to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn unknown_field_rejected() {
        assert_eq!(
            parse(&map(&[("secret", "eq.x")]), &SCHEMA)
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn unknown_operator_rejected() {
        assert_eq!(
            parse(&map(&[("name", "foo.张")]), &SCHEMA)
                .unwrap_err()
                .to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn empty_filters_returns_empty() {
        assert!(parse(&HashMap::new(), &SCHEMA).unwrap().is_empty());
    }

    #[test]
    fn value_may_contain_dots_and_special_chars() {
        let conds = parse(&map(&[("name", "ilike.*1。\"\\'4*")]), &SCHEMA).unwrap();
        assert_eq!(conds[0].value, "*1。\"\\'4*");
    }

    // ---- SQL 生成 ----

    #[test]
    fn to_sql_generates_typed_expressions() {
        let conds = parse(
            &map(&[
                ("name", "ilike.*张*"),
                ("created_at", "gt.2024-03-15"),
                ("code", "eq.C-001"),
            ]),
            &SCHEMA,
        )
        .unwrap();
        let exprs = to_sql(&conds, &SCHEMA);
        assert_eq!(exprs.len(), 3);
        // 字段排序后 [code, created_at, name]；date 条件（index 1）带 timestamptz cast
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where(exprs[1].clone())
            .to_owned();
        let (sql, _) = select.build(PostgresQueryBuilder);
        let lower = sql.to_lowercase();
        assert!(lower.find("cast($1 as timestamptz)").is_some());
    }

    #[test]
    fn to_sql_text_neq_and_date_eq() {
        // 同字段多条件需分开 parse（HashMap 键唯一）后拼接；排序后 code < created_at×2
        let mut conds = Vec::new();
        conds.extend(parse(&map(&[("code", "neq.C-001")]), &SCHEMA).unwrap());
        conds.extend(parse(&map(&[("created_at", "eq.2024-03-15")]), &SCHEMA).unwrap());
        conds.extend(parse(&map(&[("created_at", "neq.2024-01-01")]), &SCHEMA).unwrap());
        let exprs = to_sql(&conds, &SCHEMA);
        assert_eq!(exprs.len(), 3);
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where(exprs[0].clone())
            .and_where(exprs[1].clone())
            .and_where(exprs[2].clone())
            .to_owned();
        let (sql, _) = select.build(PostgresQueryBuilder);
        let lower = sql.to_lowercase();
        // text neq → <>/!=，date eq/neq → 带 cast 的 = / <>
        assert!(lower.find("\"code\" <> $1").is_some());
        assert!(
            lower
                .find("\"created_at\" = cast($2 as timestamptz)")
                .is_some()
        );
        assert!(
            lower
                .find("\"created_at\" <> cast($3 as timestamptz)")
                .is_some()
        );
    }

    #[test]
    fn to_sql_int_operators() {
        // 同字段多条件需分开 parse（HashMap 键唯一）后拼接
        let mut conds = Vec::new();
        conds.extend(parse(&map(&[("amount", "eq.100")]), &SCHEMA).unwrap());
        conds.extend(parse(&map(&[("amount", "gte.50")]), &SCHEMA).unwrap());
        conds.extend(parse(&map(&[("quantity", "lt.10")]), &SCHEMA).unwrap());
        let exprs = to_sql(&conds, &SCHEMA);
        assert_eq!(exprs.len(), 3);
        let select = Query::select()
            .column("id")
            .from("payments")
            .and_where(exprs[0].clone())
            .and_where(exprs[1].clone())
            .and_where(exprs[2].clone())
            .to_owned();
        let (sql, values) = select.build(PostgresQueryBuilder);
        let lower = sql.to_lowercase();
        assert!(lower.find("\"amount\" = $1").is_some());
        assert!(lower.find("\"amount\" >= $2").is_some());
        assert!(lower.find("\"quantity\" < $3").is_some());
        // 数值参数（非文本串）
        assert!(format!("{values:?}").find("100").is_some());
    }

    #[test]
    fn to_sql_ignores_unsupported_combinations() {
        // created_at=ilike.* 与 amount=ilike.* 非法组合（date/int 不支持模糊）→ 忽略
        let conds = parse(
            &map(&[("created_at", "ilike.*张*"), ("amount", "ilike.1")]),
            &SCHEMA,
        )
        .unwrap();
        assert!(to_sql(&conds, &SCHEMA).is_empty());
    }

    // ---- 排序 ----

    #[test]
    fn parses_order_with_direction() {
        let orders = parse_order("name.asc,created_at.desc", &SCHEMA).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].field, "name");
        assert_eq!(orders[0].dir, OrderDir::Asc);
        assert_eq!(orders[1].dir, OrderDir::Desc);
    }

    #[test]
    fn order_defaults_to_asc() {
        let orders = parse_order("name", &SCHEMA).unwrap();
        assert_eq!(orders[0].dir, OrderDir::Asc);
    }

    #[test]
    fn order_int_field_allowed() {
        let orders = parse_order("amount.desc", &SCHEMA).unwrap();
        assert_eq!(orders[0].field, "amount");
        assert_eq!(orders[0].dir, OrderDir::Desc);
    }

    #[test]
    fn order_unknown_field_rejected() {
        assert_eq!(
            parse_order("hack.asc", &SCHEMA).unwrap_err().to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn order_unknown_direction_rejected() {
        assert_eq!(
            parse_order("name.foo", &SCHEMA).unwrap_err().to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn order_clauses_appends_id_stability() {
        let orders = parse_order("name.asc", &SCHEMA).unwrap();
        let clauses = order_clauses(&orders);
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0], ("name".to_string(), OrderDir::Asc));
        assert_eq!(clauses[1], ("id".to_string(), OrderDir::Desc));
    }

    #[test]
    fn order_and_cursor_no_order_defaults_id_desc() {
        // 无排序无游标：默认 id DESC 排序子句，无游标谓词
        let (clauses, cursor) = order_and_cursor(None, &SCHEMA, "customers", None).unwrap();
        assert_eq!(clauses, vec![("id".to_string(), OrderDir::Desc)]);
        assert!(cursor.is_none());

        // 无排序 + 游标：id < cursor
        let (clauses, cursor) = order_and_cursor(None, &SCHEMA, "customers", Some(42)).unwrap();
        assert_eq!(clauses, vec![("id".to_string(), OrderDir::Desc)]);
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where_option(cursor)
            .to_owned();
        let (sql, _) = select.build(PostgresQueryBuilder);
        assert!(sql.to_lowercase().find("\"id\" < $1").is_some());
    }

    #[test]
    fn order_and_cursor_single_field_keyset() {
        // 有排序 + 游标：首字段 keyset；多级 order 只取首字段
        let (clauses, cursor) = order_and_cursor(
            Some("name.asc,created_at.desc"),
            &SCHEMA,
            "customers",
            Some(7),
        )
        .unwrap();
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0], ("name".to_string(), OrderDir::Asc));
        assert_eq!(clauses[1], ("id".to_string(), OrderDir::Desc));
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where(cursor.unwrap())
            .to_owned();
        let (sql, _) = select.build(PostgresQueryBuilder);
        assert!(
            sql.to_lowercase()
                .find("\"name\" > (select \"name\"")
                .is_some()
        );
    }

    #[test]
    fn order_and_cursor_rejects_unknown_field() {
        assert_eq!(
            order_and_cursor(Some("hack.asc"), &SCHEMA, "customers", None)
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn filter_where_parses_and_generates_in_one_step() {
        // 空筛选 → 空 Vec
        assert!(filter_where(&HashMap::new(), &SCHEMA).unwrap().is_empty());
        // 非法字段 → Err
        assert_eq!(
            filter_where(&map(&[("hack", "eq.x")]), &SCHEMA)
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
        // 正常筛选 → 表达式数量与条件一致
        let exprs = filter_where(
            &map(&[("name", "ilike.*张*"), ("amount", "gte.100")]),
            &SCHEMA,
        )
        .unwrap();
        assert_eq!(exprs.len(), 2);
    }

    #[test]
    fn apply_order_maps_dirs_and_skips_empty() {
        // 空子句：无 ORDER BY
        let mut select = Query::select().column("id").from("customers").to_owned();
        apply_order(&mut select, &[]);
        let (sql, _) = select.build(PostgresQueryBuilder);
        assert!(sql.to_lowercase().find("order by").is_none());

        // 排序子句 → ORDER BY f1 ASC, f2 DESC
        let mut select = Query::select().column("id").from("customers").to_owned();
        apply_order(
            &mut select,
            &[
                ("name".to_string(), OrderDir::Asc),
                ("id".to_string(), OrderDir::Desc),
            ],
        );
        let (sql, _) = select.build(PostgresQueryBuilder);
        let lower = sql.to_lowercase();
        assert!(lower.find("order by \"name\" asc, \"id\" desc").is_some());
    }

    #[test]
    fn apply_search_builds_cursor_filters_and_order() {
        // 一站式：游标 keyset + 筛选 + 排序一次装配（mutating 风格，调用方不再自行拼接）
        let mut select = Query::select()
            .column("id")
            .from("customers")
            .limit(10)
            .to_owned();
        apply_search(
            &mut select,
            &map(&[("amount", "gte.100")]),
            Some("name.asc"),
            &SCHEMA,
            "customers",
            Some(42),
        )
        .unwrap();
        let (sql, values) = select.build(PostgresQueryBuilder);
        let lower = sql.to_lowercase();
        // 筛选
        assert!(lower.find("\"amount\" >= ").is_some());
        // 排序（name asc + id desc 尾缀）
        assert!(lower.find("order by \"name\" asc, \"id\" desc").is_some());
        // 游标 keyset（标量子查询）
        assert!(
            lower
                .find("\"name\" > (select \"name\" from \"customers\" where \"id\" = ")
                .is_some()
        );
        // 绑定参数：子查询键值×2 + 外层 id + 筛选值 + limit（sea-query limit 也走绑定）
        assert_eq!(values.0.len(), 5);
    }

    #[test]
    fn cursor_where_generates_keyset_predicate() {
        // asc：f > (SELECT f ...) OR (f = (SELECT f ...) AND id < c)
        let expr = cursor_where("customers", "name", OrderDir::Asc, 42);
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where(expr)
            .to_owned();
        let (sql, values) = select.build(PostgresQueryBuilder);
        let lower = sql.to_lowercase();
        assert!(
            lower
                .find("\"name\" > (select \"name\" from \"customers\" where \"id\" = $1)")
                .is_some()
        );
        assert!(lower
            .find("\"name\" = (select \"name\" from \"customers\" where \"id\" = $2) and \"id\" < $3")
            .is_some());
        // 三个参数：两个子查询键值 + 外层 id（列对列比较无需 cast）
        assert_eq!(values.0.len(), 3);

        // desc：首比较换 <
        let expr = cursor_where("customers", "name", OrderDir::Desc, 7);
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where(expr)
            .to_owned();
        let (sql, _) = select.build(PostgresQueryBuilder);
        assert!(
            sql.to_lowercase()
                .find("\"name\" < (select \"name\"")
                .is_some()
        );
    }

    #[test]
    fn to_sql_builds_select_with_conditions() {
        // 集成冒烟：完整 select 构建（sea_query 链）
        let conds = parse(&map(&[("name", "ilike.*张*")]), &SCHEMA).unwrap();
        let exprs = to_sql(&conds, &SCHEMA);
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where_option(exprs.first().cloned())
            .to_owned();
        let (sql, values) = select.build(PostgresQueryBuilder);
        assert!(sql.find("ILIKE").is_some());
        // 值参数绑定（不在 SQL 里）
        assert!(format!("{values:?}").find("%张%").is_some());
    }
}
