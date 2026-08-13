//! PostgREST 风格筛选/排序解析（PostgreSQL 生态惯例，Supabase 同款）。
//!
//! 筛选：每个字段一个 query 参数，值 = `{op}.{value}`，多参数天然 AND：
//! ```text
//! name=ilike.*张*&created_at=gt.2024-03-15&code=eq.C-001
//! ```
//!
//! 排序：`order` 参数 = `{field}.{asc|desc}`，逗号分隔多级：
//! ```text
//! order=name.asc,created_at.desc
//! ```
//!
//! 分页游标：无排序时 id 游标（兼容现状）；有排序时复合游标
//! `(sort_value, id)`，与排序方向一致（PostgREST 同款语义）。
//!
//! 字段/操作符白名单在解析期校验（防 SQL 注入）；SQL 生成由
//! [`FilterSchema`] 声明驱动（text/date 列类型区分）。
//!
//! ⚠️ 陷阱：sea_query 1.0 的 `impl<T> ExprTrait for T where T: Into<Expr>` blanket
//! 会让作用域内的 `String` 获得 `ExprTrait::contains`（返回 Expr），方法解析优先于
//! `str::contains`。本 crate 因 `use ExprTrait as _` 必然中招——**不要在本 crate
//! 任何地方写 `String::contains(...)`**，用 `str::find(...)` / `find(...).is_some()`。

use std::collections::HashMap;
use std::fmt;

use sea_query::extension::postgres::PgExpr;
use sea_query::{Alias, Expr, ExprTrait as _, SimpleExpr};

/// 筛选操作符（PostgREST 风格，值前缀）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `eq.` 等于
    Eq,
    /// `gt.` 大于
    Gt,
    /// `gte.` 大于等于
    Gte,
    /// `lt.` 小于
    Lt,
    /// `lte.` 小于等于
    Lte,
    /// `ilike.` 大小写不敏感模糊匹配（值含通配符 `*` / `_`）
    Ilike,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Op::Eq => "eq.",
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
    /// 原始值（ilike 含通配符 `*` / `_`）
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
    /// 值不是 `{op}.{value}` 格式 / 排序格式错误 / 操作符未知
    #[error("invalid_filter_syntax")]
    Syntax,
    /// 字段不在白名单
    #[error("filter_field_not_allowed")]
    FieldNotAllowed,
}

/// 可筛/可排字段声明：text 支持 eq/ilike，date 支持 gt/gte/lt/lte（自动 cast）。
/// 白名单 = `text_fields ∪ date_fields`，与 SQL 生成同源，不会不一致。
#[derive(Debug, Clone, Copy)]
pub struct FilterSchema {
    pub text_fields: &'static [&'static str],
    pub date_fields: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColKind {
    Text,
    Date,
}

impl FilterSchema {
    /// 白名单字段（text + date 并集）
    pub fn allowed_fields(&self) -> Vec<&'static str> {
        self.text_fields
            .iter()
            .chain(self.date_fields.iter())
            .copied()
            .collect()
    }

    fn col_kind(&self, field: &str) -> Option<ColKind> {
        if self.text_fields.contains(&field) {
            Some(ColKind::Text)
        } else if self.date_fields.contains(&field) {
            Some(ColKind::Date)
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
const FILTER_OPERATORS: [(&str, Op); 6] = [
    ("ilike.", Op::Ilike),
    ("gte.", Op::Gte),
    ("lte.", Op::Lte),
    ("gt.", Op::Gt),
    ("lt.", Op::Lt),
    ("eq.", Op::Eq),
];

/// 解析 PostgREST 风格筛选参数（query string 中除分页/搜索词之外的字段参数）。
pub fn parse(
    filters: &HashMap<String, String>,
    allowed_fields: &[&str],
) -> Result<Vec<Condition>, FilterError> {
    let mut conditions = Vec::new();
    for (field, raw) in filters {
        if !allowed_fields.contains(&field.as_str()) {
            return Err(FilterError::FieldNotAllowed);
        }
        let mut parsed = None;
        for (op_str, op) in FILTER_OPERATORS {
            if let Some(value) = raw.strip_prefix(op_str) {
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

/// 条件 → SeaQuery 表达式（字段白名单由 parse 校验；此处按 schema 类型安全映射，
/// 未覆盖的组合如 `created_at=ilike.*` 直接忽略）。
pub fn to_sql(conds: &[Condition], schema: &FilterSchema) -> Vec<SimpleExpr> {
    conds
        .iter()
        .filter_map(|c| match (schema.col_kind(&c.field), c.op) {
            (Some(ColKind::Text), Op::Eq) => {
                Some(Expr::col(c.field.to_string()).eq(c.value.as_str()))
            }
            // PostgREST 通配符语义：* = 任意字符（→ SQL %），_ 单字符保留
            (Some(ColKind::Text), Op::Ilike) => {
                Some(Expr::col(c.field.to_string()).ilike(c.value.replace('*', "%")))
            }
            (Some(ColKind::Date), Op::Gt) => {
                Some(Expr::col(c.field.to_string()).gt(schema.date_value(&c.value)))
            }
            (Some(ColKind::Date), Op::Gte) => {
                Some(Expr::col(c.field.to_string()).gte(schema.date_value(&c.value)))
            }
            (Some(ColKind::Date), Op::Lt) => {
                Some(Expr::col(c.field.to_string()).lt(schema.date_value(&c.value)))
            }
            (Some(ColKind::Date), Op::Lte) => {
                Some(Expr::col(c.field.to_string()).lte(schema.date_value(&c.value)))
            }
            _ => None,
        })
        .collect()
}

/// 排序解析：`"name.asc,created_at.desc"`（逗号分隔；省略方向默认 asc）。
pub fn parse_order(orders: &str, allowed: &[&str]) -> Result<Vec<OrderItem>, FilterError> {
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

/// 排序子句：用户排序 + id 稳定二级排序（游标分页必须稳定）。
/// 返回 `(列名, 方向)` 列表，由调用方转 SeaQuery `order_by`。
pub fn order_clauses(orders: &[OrderItem]) -> Vec<(String, OrderDir)> {
    let mut clauses: Vec<(String, OrderDir)> =
        orders.iter().map(|o| (o.field.clone(), o.dir)).collect();
    // 用户未显式排 id 时，附加 id DESC 二级排序：同值字段内最新在前
    if !orders.iter().any(|o| o.field == "id") {
        clauses.push(("id".to_string(), OrderDir::Desc));
    }
    clauses
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::{PostgresQueryBuilder, Query};

    const SCHEMA: FilterSchema = FilterSchema {
        text_fields: &["code", "name", "phone", "contact_person"],
        date_fields: &["created_at"],
    };

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ---- 筛选 ----

    #[test]
    fn parses_multiple_conditions() {
        let conds = parse(
            &map(&[("name", "ilike.*张*"), ("created_at", "gt.2024-03-15")]),
            &SCHEMA.allowed_fields(),
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
        let conds = parse(&map(&[("code", "eq.C-001")]), &SCHEMA.allowed_fields()).unwrap();
        assert_eq!(conds[0].op, Op::Eq);
        assert_eq!(conds[0].value, "C-001");
    }

    #[test]
    fn gte_lte_operators() {
        let conds = parse(
            &map(&[("created_at", "gte.2024-01-01"), ("code", "lte.Z")]),
            &SCHEMA.allowed_fields(),
        )
        .unwrap();
        assert_eq!(conds[0].op, Op::Lte);
        assert_eq!(conds[1].op, Op::Gte);
    }

    #[test]
    fn unknown_field_rejected() {
        assert_eq!(
            parse(&map(&[("secret", "eq.x")]), &SCHEMA.allowed_fields())
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn unknown_operator_rejected() {
        assert_eq!(
            parse(&map(&[("name", "foo.张")]), &SCHEMA.allowed_fields())
                .unwrap_err()
                .to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn empty_filters_returns_empty() {
        assert!(
            parse(&HashMap::new(), &SCHEMA.allowed_fields())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn value_may_contain_dots_and_special_chars() {
        let conds = parse(
            &map(&[("name", "ilike.*1。\"\\'4*")]),
            &SCHEMA.allowed_fields(),
        )
        .unwrap();
        assert_eq!(conds[0].value, "*1。\"\\'4*");
    }

    #[test]
    fn to_sql_generates_typed_expressions() {
        let conds = parse(
            &map(&[
                ("name", "ilike.*张*"),
                ("created_at", "gt.2024-03-15"),
                ("code", "eq.C-001"),
            ]),
            &SCHEMA.allowed_fields(),
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
    fn to_sql_ignores_unsupported_combinations() {
        // created_at=ilike.* 非法组合（date 不支持模糊）→ 忽略
        let conds = parse(
            &map(&[("created_at", "ilike.*张*")]),
            &SCHEMA.allowed_fields(),
        )
        .unwrap();
        assert!(to_sql(&conds, &SCHEMA).is_empty());
    }

    // ---- 排序 ----

    #[test]
    fn parses_order_with_direction() {
        let orders = parse_order("name.asc,created_at.desc", &SCHEMA.allowed_fields()).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].field, "name");
        assert_eq!(orders[0].dir, OrderDir::Asc);
        assert_eq!(orders[1].dir, OrderDir::Desc);
    }

    #[test]
    fn order_defaults_to_asc() {
        let orders = parse_order("name", &SCHEMA.allowed_fields()).unwrap();
        assert_eq!(orders[0].dir, OrderDir::Asc);
    }

    #[test]
    fn order_unknown_field_rejected() {
        assert_eq!(
            parse_order("hack.asc", &SCHEMA.allowed_fields())
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn order_unknown_direction_rejected() {
        assert_eq!(
            parse_order("name.foo", &SCHEMA.allowed_fields())
                .unwrap_err()
                .to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn order_clauses_appends_id_stability() {
        let orders = parse_order("name.asc", &SCHEMA.allowed_fields()).unwrap();
        let clauses = order_clauses(&orders);
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0], ("name".to_string(), OrderDir::Asc));
        assert_eq!(clauses[1], ("id".to_string(), OrderDir::Desc));
    }

    #[test]
    fn to_sql_builds_select_with_conditions() {
        // 集成冒烟：完整 select 构建（sea_query 链）
        let conds = parse(&map(&[("name", "ilike.*张*")]), &SCHEMA.allowed_fields()).unwrap();
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
