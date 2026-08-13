//! PostgREST 风格筛选解析（PostgreSQL 生态惯例，Supabase 同款）。
//!
//! 语法：每个字段一个 query 参数，值 = `{op}.{value}`，多参数天然 AND：
//! ```text
//! name=ilike.*张*&created_at=gt.2024-03-15&code=eq.C-001
//! ```
//!
//! 操作符：`eq` / `gt` / `gte` / `lt` / `lte` / `ilike`。
//! `ilike` 的值是通配模式（`*` = 任意字符、`_` = 单字符），SQL 转换时 `*` → `%`。
//! 字段白名单在解析期校验（拒绝未知字段，防 SQL 注入）。

use std::collections::HashMap;
use std::fmt;

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

/// 解析错误（Display 即 locale key；细节进 Debug/日志）
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    /// 值不是 `{op}.{value}` 格式或操作符未知
    #[error("invalid_filter_syntax")]
    Syntax,
    /// 字段不在白名单
    #[error("filter_field_not_allowed")]
    FieldNotAllowed,
}

/// 操作符表：从长到短匹配前缀（`ilike.` 先于 `eq.` 等，避免子串误配）
const OPERATORS: [(&str, Op); 6] = [
    ("ilike.", Op::Ilike),
    ("gte.", Op::Gte),
    ("lte.", Op::Lte),
    ("gt.", Op::Gt),
    ("lt.", Op::Lt),
    ("eq.", Op::Eq),
];

/// 解析 PostgREST 风格筛选参数（query string 中除分页/搜索词之外的字段参数）。
///
/// - 每个 (field, value) 一条条件；`value` 必须为 `{op}.{value}` 格式
/// - 字段必须 ∈ `allowed_fields`，否则返回 [`FilterError::FieldNotAllowed`]
/// - 未知操作符 / 空值的参数返回 [`FilterError::Syntax`]
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
        for (op_str, op) in OPERATORS {
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWED: [&str; 5] = ["code", "name", "phone", "contact_person", "created_at"];

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_multiple_conditions() {
        let conds = parse(
            &map(&[("name", "ilike.*张*"), ("created_at", "gt.2024-03-15")]),
            &ALLOWED,
        )
        .unwrap();
        assert_eq!(conds.len(), 2);
        // 按字段名排序：created_at 在前
        assert_eq!(conds[0].field, "created_at");
        assert_eq!(conds[0].op, Op::Gt);
        assert_eq!(conds[0].value, "2024-03-15");
        assert_eq!(conds[1].field, "name");
        assert_eq!(conds[1].op, Op::Ilike);
        assert_eq!(conds[1].value, "*张*");
    }

    #[test]
    fn eq_operator() {
        let conds = parse(&map(&[("code", "eq.C-001")]), &ALLOWED).unwrap();
        assert_eq!(conds[0].op, Op::Eq);
        assert_eq!(conds[0].value, "C-001");
    }

    #[test]
    fn gte_lte_operators() {
        let conds = parse(
            &map(&[("created_at", "gte.2024-01-01"), ("code", "lte.Z")]),
            &ALLOWED,
        )
        .unwrap();
        // 排序后：code 在前
        assert_eq!(conds[0].op, Op::Lte);
        assert_eq!(conds[1].op, Op::Gte);
    }

    #[test]
    fn unknown_field_rejected() {
        assert_eq!(
            parse(&map(&[("secret", "eq.x")]), &ALLOWED)
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn unknown_operator_rejected() {
        assert_eq!(
            parse(&map(&[("name", "foo.张")]), &ALLOWED)
                .unwrap_err()
                .to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn empty_filters_returns_empty() {
        assert!(parse(&HashMap::new(), &ALLOWED).unwrap().is_empty());
    }

    #[test]
    fn value_may_contain_dots_and_special_chars() {
        // 值里的 . / * / _ 原样保留（通配符由 SQL 层解释）
        let conds = parse(&map(&[("name", "ilike.*1。\"\\'4*")]), &ALLOWED).unwrap();
        assert_eq!(conds[0].value, "*1。\"\\'4*");
    }

    #[test]
    fn empty_value_allowed_but_noop() {
        // "name=ilike." 无值：操作符匹配、值为空——SQL 层 ILIKE '%%' 匹配全部，无害
        let conds = parse(&map(&[("name", "ilike.")]), &ALLOWED).unwrap();
        assert!(conds[0].value.is_empty());
    }

    #[test]
    fn op_prefix_matching_longest_first() {
        // "gte." 不被 "gt." 误匹配
        let conds = parse(&map(&[("created_at", "gte.2024-01-01")]), &ALLOWED).unwrap();
        assert_eq!(conds[0].op, Op::Gte);
    }
}
