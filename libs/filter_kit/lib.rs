//! RSQL（FIQL 超集）风格筛选解析（rsql.dsmdclab.io 规范子集，Spring Data 同款）。
//!
//! 单个 `filter` query 参数承载整棵布尔树，支持 and / or / 括号分组
//! （优先级：**括号 > AND > OR**）：
//! ```text
//! filter=(name==张;amount=gt=1000),created_at=lt=2024-03-15
//! ```
//!
//! # 语法（本 crate 子集）
//!
//! | 元素 | 形式 |
//! |------|------|
//! | AND | `;` 或 `and`（大小写不敏感） |
//! | OR | `,` 或 `or`（大小写不敏感） |
//! | 分组 | `( ... )` |
//! | 等于 / 不等于 | `==` / `!=` |
//! | 大于 / 大于等于 | `=gt=` / `=ge=`（RSQL 标准；语义 = gte） |
//! | 小于 / 小于等于 | `=lt=` / `=le=` |
//! | 模糊 | `=ilike=`（值含通配符 `*`，任意字符；仅 text 列） |
//!
//! 值可用单引号包裹（`'` 转义为 `''`）；未包裹时遇到 `,` `;` `(` `)` 或空白即结束。
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
//! **非法「类型 × 操作符」组合（如 `created_at=ilike=*`）在解析期直接拒绝**——旧
//! PostgREST 实现是 SQL 生成期静默忽略，但树形结构下忽略一侧会改变布尔语义，故改为硬错误。
//!
//! ⚠️ 陷阱：sea_query 1.0 的 `impl<T> ExprTrait for T where T: Into<Expr>` blanket
//! 会让作用域内的 `String` 获得 `ExprTrait::contains`（返回 Expr），方法解析优先于
//! `str::contains`。本 crate 因 `use ExprTrait as _` 必然中招——**不要在本 crate
//! 任何地方写 `String::contains(...)`**，用 `str::find(...)` / `find(...).is_some()`。

use sea_query::extension::postgres::PgExpr;
use sea_query::{Alias, Expr, ExprTrait as _, SimpleExpr};

/// 筛选操作符（语义枚举；wire 形式见 [`Op::rsql_str`]）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `==` 等于
    Eq,
    /// `!=` 不等于
    Neq,
    /// `=gt=` 大于
    Gt,
    /// `=ge=` 大于等于（RSQL 标准形式；语义 = gte）
    Gte,
    /// `=lt=` 小于
    Lt,
    /// `=le=` 小于等于
    Lte,
    /// `=ilike=` 大小写不敏感模糊匹配（值含通配符 `*`；仅 text 列）
    Ilike,
}

impl Op {
    /// 协议名（操作符矩阵 / meta 端点导出 / 前端操作符 id）：`eq` / `ilike` 等。
    pub fn as_str(&self) -> &'static str {
        match self {
            Op::Eq => "eq",
            Op::Neq => "neq",
            Op::Gt => "gt",
            Op::Gte => "gte",
            Op::Lt => "lt",
            Op::Lte => "lte",
            Op::Ilike => "ilike",
        }
    }

    /// RSQL 比较操作符串（wire 形式，与 [`RSQL_OPERATORS`] 同源）。
    pub fn rsql_str(&self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Neq => "!=",
            Op::Gt => "=gt=",
            Op::Gte => "=ge=",
            Op::Lt => "=lt=",
            Op::Lte => "=le=",
            Op::Ilike => "=ilike=",
        }
    }
}

/// 一条已通过白名单校验的筛选条件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    /// 原始值（ilike 含通配符 `*`；int 字段已校验为合法整数串）
    pub value: String,
}

/// RSQL 布尔树节点（`parse` 产物；`to_sql` 与之一一对应）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Cmp(Condition),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
}

/// 解析错误（Display 即 locale key；细节进 Debug/日志）
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    /// RSQL 语法错误 / 非法「类型 × 操作符」组合 / int 值非整数
    #[error("invalid_filter_syntax")]
    Syntax,
    /// 字段不在白名单
    #[error("filter_field_not_allowed")]
    FieldNotAllowed,
}

/// 可筛字段声明：`text_fields`（eq/neq/ilike）、`date_fields`（eq/neq/gt/gte/lt/lte，
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

    /// 字段类型名（`text` / `date` / `int`，与 [`OPERATOR_MATRIX`] 键一致），供 meta 端点导出。
    pub fn field_kind(&self, field: &str) -> Option<&'static str> {
        match self.col_kind(field) {
            Some(ColKind::Text) => Some("text"),
            Some(ColKind::Date) => Some("date"),
            Some(ColKind::Int) => Some("int"),
            None => None,
        }
    }

    /// 日期比较值：显式 cast，避免 `timestamptz > text` 报错
    fn date_value(&self, value: &str) -> SimpleExpr {
        Expr::val(value).cast_as(Alias::new("timestamptz"))
    }
}

/// 操作符矩阵（协议事实源）：列类型名 → 支持的操作符集。
///
/// 与 [`FilterSchema`] 的列类型对应（text / date / int）；`parse` 按此集校验
/// 组合，非法组合（如 date/int 的 ilike）在解析期拒绝。
pub const OPERATOR_MATRIX: &[(&str, &[Op])] = &[
    ("text", &[Op::Eq, Op::Neq, Op::Ilike]),
    ("date", &[Op::Eq, Op::Neq, Op::Gt, Op::Gte, Op::Lt, Op::Lte]),
    ("int", &[Op::Eq, Op::Neq, Op::Gt, Op::Gte, Op::Lt, Op::Lte]),
];

/// 全部操作符（枚举序）：meta 端点导出「协议名 → RSQL 比较串」映射的事实源。
pub const ALL_OPS: [Op; 7] = [Op::Eq, Op::Neq, Op::Gt, Op::Gte, Op::Lt, Op::Lte, Op::Ilike];

/// RSQL 比较操作符串（长→短，lexer 最长匹配）——协议事实源，供 meta 端点导出。
/// `=ge=` / `=gt=` 同长不互为前缀，但 `=ilike=` 必须排最前（`=ilike=` 含 `=` 前缀）；
/// 保持从长到短，未来加操作符时务必插在正确位置。
pub const RSQL_OPERATORS: &[(&str, Op)] = &[
    ("=ilike=", Op::Ilike),
    ("=ge=", Op::Gte),
    ("=le=", Op::Lte),
    ("=gt=", Op::Gt),
    ("=lt=", Op::Lt),
    ("!=", Op::Neq),
    ("==", Op::Eq),
];

/// 协议名 → RSQL 比较操作符串（meta 端点导出，前端序列化事实源）。
pub fn comparison_ops() -> Vec<(&'static str, &'static str)> {
    ALL_OPS
        .iter()
        .map(|op| (op.as_str(), op.rsql_str()))
        .collect()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// 列类型 → 操作符集（与 [`OPERATOR_MATRIX`] 同源；未知类型防御性返回空集）
fn kind_ops(kind: ColKind) -> &'static [Op] {
    let name = match kind {
        ColKind::Text => "text",
        ColKind::Date => "date",
        ColKind::Int => "int",
    };
    OPERATOR_MATRIX
        .iter()
        .find(|(k, _)| *k == name)
        .map_or(&[], |(_, ops)| *ops)
}

/// RSQL 递归下降解析器（游标 + 显式优先级：括号 > AND > OR）。
struct Parser<'a> {
    input: &'a str,
    pos: usize,
    schema: &'a FilterSchema,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    /// 关键字（`and` / `or`）须后跟非标识符字符（词边界），避免把 `oranges` 误当 `or`。
    fn eat_keyword(&mut self, kw: &str) -> bool {
        let rest = &self.input[self.pos..];
        let Some(head) = rest.get(..kw.len()) else {
            return false;
        };
        if !head.eq_ignore_ascii_case(kw) {
            return false;
        }
        let boundary = rest[kw.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_ident_char(c));
        if boundary {
            self.pos += kw.len();
            return true;
        }
        false
    }

    /// OR 层：优先级最低，`and_expr (',' | 'or') and_expr`*
    fn parse_or(&mut self) -> Result<Node, FilterError> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.eat(",") || self.eat_keyword("or") {
                let right = self.parse_and()?;
                left = Node::Or(Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    /// AND 层：`primary (';' | 'and') primary`*
    fn parse_and(&mut self) -> Result<Node, FilterError> {
        let mut left = self.parse_primary()?;
        loop {
            self.skip_ws();
            if self.eat(";") || self.eat_keyword("and") {
                let right = self.parse_primary()?;
                left = Node::And(Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    /// 原子：`'(' or_expr ')'` 或比较式
    fn parse_primary(&mut self) -> Result<Node, FilterError> {
        self.skip_ws();
        if self.eat("(") {
            let inner = self.parse_or()?;
            self.skip_ws();
            if !self.eat(")") {
                return Err(FilterError::Syntax);
            }
            Ok(inner)
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<Node, FilterError> {
        self.skip_ws();
        // 字段名：[A-Za-z_][A-Za-z0-9_]*
        let start = self.pos;
        while self.peek().is_some_and(is_ident_char) {
            self.bump();
        }
        if self.pos == start {
            return Err(FilterError::Syntax);
        }
        let field = &self.input[start..self.pos];

        self.skip_ws();
        let mut op = None;
        for (s, o) in RSQL_OPERATORS {
            if self.eat(s) {
                op = Some(*o);
                break;
            }
        }
        let op = op.ok_or(FilterError::Syntax)?;

        self.skip_ws();
        let value = self.parse_value()?;

        self.validate(field, op, &value)?;
        Ok(Node::Cmp(Condition {
            field: field.to_string(),
            op,
            value,
        }))
    }

    /// 值：单引号串（`''` 转义 `'`）或未包裹（遇到 `,` `;` `(` `)` 或空白即结束）。
    fn parse_value(&mut self) -> Result<String, FilterError> {
        if self.peek() == Some('\'') {
            self.bump();
            let mut out = String::new();
            loop {
                match self.bump() {
                    Some('\'') => {
                        if self.peek() == Some('\'') {
                            self.bump();
                            out.push('\'');
                        } else {
                            return Ok(out);
                        }
                    }
                    Some(c) => out.push(c),
                    None => return Err(FilterError::Syntax), // 未闭合引号
                }
            }
        } else {
            let start = self.pos;
            while let Some(c) = self.peek() {
                if matches!(c, ',' | ';' | '(' | ')') || c.is_whitespace() {
                    break;
                }
                self.bump();
            }
            if self.pos == start {
                return Err(FilterError::Syntax);
            }
            Ok(self.input[start..self.pos].to_string())
        }
    }

    /// 白名单 + 类型 × 操作符矩阵 + int 值格式，解析期一次校验。
    fn validate(&self, field: &str, op: Op, value: &str) -> Result<(), FilterError> {
        let kind = self
            .schema
            .col_kind(field)
            .ok_or(FilterError::FieldNotAllowed)?;
        if !kind_ops(kind).contains(&op) {
            return Err(FilterError::Syntax);
        }
        if kind == ColKind::Int && value.parse::<i64>().is_err() {
            return Err(FilterError::Syntax);
        }
        Ok(())
    }
}

/// 解析 RSQL 表达式为布尔树（and(`;`/`and`) / or(`,`/`or`) / 括号分组，优先级：括号 > AND > OR）。
///
/// 空串 / 纯空白 → [`FilterError::Syntax`]（「无筛选」用 [`filter_where`] 处理）。
pub fn parse(input: &str, schema: &FilterSchema) -> Result<Node, FilterError> {
    let mut parser = Parser {
        input,
        pos: 0,
        schema,
    };
    let node = parser.parse_or()?;
    // 尾部残留（未闭合括号、多余 token 等）→ 语法错误
    if parser.input[parser.pos..].trim().is_empty() {
        Ok(node)
    } else {
        Err(FilterError::Syntax)
    }
}

/// 布尔树 → SeaQuery 表达式（与 [`parse`] 1:1；非法组合在解析期已拒绝）。
///
/// SeaQuery 按优先级自动补括号：`(a OR b) AND c` 渲染为 `(a = $1 OR b = $2) AND c = $3`，
/// `a OR b AND c` 渲染为 `a = $1 OR (b = $2 AND c = $3)`（Postgres AND 优先，语义一致）。
pub fn to_sql(node: &Node, schema: &FilterSchema) -> SimpleExpr {
    match node {
        Node::Cmp(c) => comparison_expr(c, schema),
        Node::And(l, r) => to_sql(l, schema).and(to_sql(r, schema)),
        Node::Or(l, r) => to_sql(l, schema).or(to_sql(r, schema)),
    }
}

fn comparison_expr(c: &Condition, schema: &FilterSchema) -> SimpleExpr {
    let col = Expr::col(c.field.to_string());
    match (schema.col_kind(&c.field), c.op) {
        (Some(ColKind::Text), Op::Eq) => col.eq(c.value.as_str()),
        (Some(ColKind::Text), Op::Neq) => col.ne(c.value.as_str()),
        // RSQL 通配符语义：* = 任意字符（→ SQL %）
        (Some(ColKind::Text), Op::Ilike) => col.ilike(c.value.replace('*', "%")),
        (Some(ColKind::Date), op) => {
            let v = schema.date_value(&c.value);
            match op {
                Op::Eq => col.eq(v),
                Op::Neq => col.ne(v),
                Op::Gt => col.gt(v),
                Op::Gte => col.gte(v),
                Op::Lt => col.lt(v),
                Op::Lte => col.lte(v),
                // parse 期已拒绝 ilike on date；防御性回退（永不触发于 parse 产物）
                _ => col.eq(v),
            }
        }
        (Some(ColKind::Int), op) => {
            // parse 已保证 i64 格式；此处防御性回退 0（永不触发于 parse 产物）
            let v = c.value.parse::<i64>().unwrap_or(0);
            match op {
                Op::Eq => col.eq(v),
                Op::Neq => col.ne(v),
                Op::Gt => col.gt(v),
                Op::Gte => col.gte(v),
                Op::Lt => col.lt(v),
                Op::Lte => col.lte(v),
                _ => col.eq(v),
            }
        }
        // 未知字段防御性回退（parse 产物永不触发）
        _ => col.eq(c.value.as_str()),
    }
}

/// 筛选参数 → WHERE 表达式（[`parse`] + [`to_sql`] 一步；`None` / 空串 / 纯空白 → `None`）。
pub fn filter_where(
    filter: Option<&str>,
    schema: &FilterSchema,
) -> Result<Option<SimpleExpr>, FilterError> {
    match filter.map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => Ok(Some(to_sql(&parse(s, schema)?, schema))),
    }
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

    fn cmp(field: &str, op: Op, value: &str) -> Node {
        Node::Cmp(Condition {
            field: field.into(),
            op,
            value: value.into(),
        })
    }

    fn and(l: Node, r: Node) -> Node {
        Node::And(Box::new(l), Box::new(r))
    }

    fn or(l: Node, r: Node) -> Node {
        Node::Or(Box::new(l), Box::new(r))
    }

    fn build_sql(expr: SimpleExpr) -> (String, Vec<sea_query::Value>) {
        let select = Query::select()
            .column("id")
            .from("customers")
            .and_where(expr)
            .to_owned();
        let (sql, values) = select.build(PostgresQueryBuilder);
        (sql, values.into_iter().collect())
    }

    // ---- 解析：比较式 ----

    #[test]
    fn parses_all_comparison_operators() {
        assert_eq!(
            parse("code==C-001", &SCHEMA).unwrap(),
            cmp("code", Op::Eq, "C-001")
        );
        assert_eq!(
            parse("code!=C-001", &SCHEMA).unwrap(),
            cmp("code", Op::Neq, "C-001")
        );
        assert_eq!(
            parse("amount=gt=100", &SCHEMA).unwrap(),
            cmp("amount", Op::Gt, "100")
        );
        // RSQL 标准 >= / <= 形式（=ge= / =le=）
        assert_eq!(
            parse("amount=ge=100", &SCHEMA).unwrap(),
            cmp("amount", Op::Gte, "100")
        );
        assert_eq!(
            parse("amount=le=100", &SCHEMA).unwrap(),
            cmp("amount", Op::Lte, "100")
        );
        assert_eq!(
            parse("name=ilike=*张*", &SCHEMA).unwrap(),
            cmp("name", Op::Ilike, "*张*")
        );
    }

    // ---- 解析：and / or / 括号 ----

    #[test]
    fn parses_and_operators() {
        // `;` 与 `and` 关键字等价，大小写不敏感
        assert_eq!(
            parse("code==a;name==b", &SCHEMA).unwrap(),
            and(cmp("code", Op::Eq, "a"), cmp("name", Op::Eq, "b"))
        );
        assert_eq!(
            parse("code==a and name==b", &SCHEMA).unwrap(),
            and(cmp("code", Op::Eq, "a"), cmp("name", Op::Eq, "b"))
        );
        assert_eq!(
            parse("code==a AND name==b", &SCHEMA).unwrap(),
            and(cmp("code", Op::Eq, "a"), cmp("name", Op::Eq, "b"))
        );
        // 关键字前允许空白
        assert_eq!(
            parse("code==a ; name==b", &SCHEMA).unwrap(),
            and(cmp("code", Op::Eq, "a"), cmp("name", Op::Eq, "b"))
        );
    }

    #[test]
    fn parses_or_operators() {
        assert_eq!(
            parse("code==a,name==b", &SCHEMA).unwrap(),
            or(cmp("code", Op::Eq, "a"), cmp("name", Op::Eq, "b"))
        );
        assert_eq!(
            parse("code==a or name==b", &SCHEMA).unwrap(),
            or(cmp("code", Op::Eq, "a"), cmp("name", Op::Eq, "b"))
        );
    }

    #[test]
    fn precedence_and_binds_tighter_than_or() {
        // a, b; c → a OR (b AND c)
        assert_eq!(
            parse("name==a,code==b;amount=gt=5", &SCHEMA).unwrap(),
            or(
                cmp("name", Op::Eq, "a"),
                and(cmp("code", Op::Eq, "b"), cmp("amount", Op::Gt, "5"))
            )
        );
        // (a, b); c → (a OR b) AND c
        assert_eq!(
            parse("(name==a,code==b);amount=gt=5", &SCHEMA).unwrap(),
            and(
                or(cmp("name", Op::Eq, "a"), cmp("code", Op::Eq, "b")),
                cmp("amount", Op::Gt, "5")
            )
        );
    }

    #[test]
    fn parentheses_nest_and_repeat() {
        let node = parse("((name==a;code==b),amount=gt=5);quantity=lt=10", &SCHEMA).unwrap();
        assert_eq!(
            node,
            and(
                or(
                    and(cmp("name", Op::Eq, "a"), cmp("code", Op::Eq, "b")),
                    cmp("amount", Op::Gt, "5")
                ),
                cmp("quantity", Op::Lt, "10")
            )
        );
    }

    // ---- 解析：值 ----

    #[test]
    fn quoted_values_keep_delimiters_and_escape_quotes() {
        assert_eq!(
            parse("name=='a,b;c (d)''e'", &SCHEMA).unwrap(),
            cmp("name", Op::Eq, "a,b;c (d)'e")
        );
        assert_eq!(parse("name==''", &SCHEMA).unwrap(), cmp("name", Op::Eq, ""));
    }

    #[test]
    fn unquoted_value_stops_at_delimiter() {
        assert_eq!(
            parse("name==a,code==b", &SCHEMA).unwrap(),
            or(cmp("name", Op::Eq, "a"), cmp("code", Op::Eq, "b"))
        );
    }

    #[test]
    fn int_field_validates_format() {
        assert_eq!(
            parse("amount=gt=abc", &SCHEMA).unwrap_err().to_string(),
            "invalid_filter_syntax"
        );
        // 负数合法
        assert_eq!(
            parse("amount=ge=-500", &SCHEMA).unwrap(),
            cmp("amount", Op::Gte, "-500")
        );
    }

    // ---- 解析：错误路径 ----

    #[test]
    fn unknown_field_rejected() {
        assert_eq!(
            parse("secret==x", &SCHEMA).unwrap_err().to_string(),
            "filter_field_not_allowed"
        );
    }

    #[test]
    fn unknown_operator_rejected() {
        // `foo` 不是比较操作符
        assert_eq!(
            parse("name=foo=张", &SCHEMA).unwrap_err().to_string(),
            "invalid_filter_syntax"
        );
        // 缺值
        assert_eq!(
            parse("name=gt=", &SCHEMA).unwrap_err().to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn unsupported_kind_op_combination_rejected() {
        // date/int 不支持模糊（旧实现静默忽略；树形语义下改为硬错误）
        assert_eq!(
            parse("created_at=ilike=*张*", &SCHEMA)
                .unwrap_err()
                .to_string(),
            "invalid_filter_syntax"
        );
        assert_eq!(
            parse("amount=ilike=1", &SCHEMA).unwrap_err().to_string(),
            "invalid_filter_syntax"
        );
    }

    #[test]
    fn empty_input_rejected() {
        assert!(matches!(parse("", &SCHEMA), Err(FilterError::Syntax)));
        assert!(matches!(parse("   ", &SCHEMA), Err(FilterError::Syntax)));
    }

    #[test]
    fn unclosed_parenthesis_rejected() {
        assert!(matches!(
            parse("(name==a", &SCHEMA),
            Err(FilterError::Syntax)
        ));
        assert!(matches!(
            parse("name==a)", &SCHEMA),
            Err(FilterError::Syntax)
        ));
    }

    #[test]
    fn missing_separator_rejected() {
        assert!(matches!(
            parse("name==a code==b", &SCHEMA),
            Err(FilterError::Syntax)
        ));
    }

    #[test]
    fn unclosed_quote_rejected() {
        assert!(matches!(
            parse("name=='a", &SCHEMA),
            Err(FilterError::Syntax)
        ));
    }

    #[test]
    fn keyword_needs_word_boundary() {
        // `orb` / `or_b` 不是 `or` 关键字 → 尾部残留 → 语法错误
        assert!(matches!(
            parse("name==a orb==b", &SCHEMA),
            Err(FilterError::Syntax)
        ));
        assert!(matches!(
            parse("name==a or_b==b", &SCHEMA),
            Err(FilterError::Syntax)
        ));
        // 值以 or 开头不受影响（未包裹值消费到分隔符为止）
        assert_eq!(
            parse("name==oranges,code==b", &SCHEMA).unwrap(),
            or(cmp("name", Op::Eq, "oranges"), cmp("code", Op::Eq, "b"))
        );
    }

    // ---- SQL 生成 ----

    #[test]
    fn to_sql_parenthesizes_or_inside_and() {
        // (name==张,code==C-001);created_at=gt=2024-03-15
        let node = parse("(name==张,code==C-001);created_at=gt=2024-03-15", &SCHEMA).unwrap();
        let (sql, _) = build_sql(to_sql(&node, &SCHEMA));
        let lower = sql.to_lowercase();
        // OR 分支在 AND 左侧 → 必须带括号；date 条件带 timestamptz cast
        assert!(
            lower
                .find(
                    "(\"name\" = $1 or \"code\" = $2) and \"created_at\" > cast($3 as timestamptz)"
                )
                .is_some(),
            "got: {lower}"
        );
    }

    #[test]
    fn to_sql_and_inside_or_matches_sql_precedence() {
        // name==张,code==C-001;amount=gt=5 → 张 OR (C-001 AND amount>5)
        let node = parse("name==张,code==C-001;amount=gt=5", &SCHEMA).unwrap();
        let (sql, _) = build_sql(to_sql(&node, &SCHEMA));
        let lower = sql.to_lowercase();
        assert!(
            lower
                .find("\"name\" = $1 or (\"code\" = $2 and \"amount\" > $3)")
                .is_some(),
            "got: {lower}"
        );
    }

    #[test]
    fn to_sql_ilike_wildcard_and_params() {
        let node = parse("name=ilike=*张*", &SCHEMA).unwrap();
        let (sql, values) = build_sql(to_sql(&node, &SCHEMA));
        let lower = sql.to_lowercase();
        assert!(lower.find("\"name\" ilike $1").is_some(), "got: {lower}");
        assert!(format!("{values:?}").find("%张%").is_some());
    }

    #[test]
    fn to_sql_int_uses_numeric_params() {
        let node = parse("amount=ge=100", &SCHEMA).unwrap();
        let (sql, values) = build_sql(to_sql(&node, &SCHEMA));
        let lower = sql.to_lowercase();
        assert!(lower.find("\"amount\" >= $1").is_some(), "got: {lower}");
        assert!(format!("{values:?}").find("100").is_some());
    }

    #[test]
    fn to_sql_date_eq_cast() {
        let node = parse("created_at==2024-03-15", &SCHEMA).unwrap();
        let (sql, _) = build_sql(to_sql(&node, &SCHEMA));
        let lower = sql.to_lowercase();
        assert!(
            lower
                .find("\"created_at\" = cast($1 as timestamptz)")
                .is_some(),
            "got: {lower}"
        );
    }

    // ---- filter_where 一步式 ----

    #[test]
    fn filter_where_none_or_empty_returns_none() {
        assert!(filter_where(None, &SCHEMA).unwrap().is_none());
        assert!(filter_where(Some(""), &SCHEMA).unwrap().is_none());
        assert!(filter_where(Some("   "), &SCHEMA).unwrap().is_none());
    }

    #[test]
    fn filter_where_parses_and_generates() {
        let expr = filter_where(Some("name=ilike=*张*;amount=ge=100"), &SCHEMA)
            .unwrap()
            .expect("filter");
        let (sql, _) = build_sql(expr);
        let lower = sql.to_lowercase();
        // sea-query 会给 ilike 比较加括号（语义不变）
        assert!(lower.find("\"name\" ilike $1").is_some(), "got: {lower}");
        assert!(lower.find("\"amount\" >= $2").is_some(), "got: {lower}");
    }

    #[test]
    fn filter_where_error_propagates() {
        assert_eq!(
            filter_where(Some("hack==x"), &SCHEMA)
                .unwrap_err()
                .to_string(),
            "filter_field_not_allowed"
        );
    }

    // ---- 协议矩阵（meta 端点导出的事实源）----

    #[test]
    fn operator_matrix_kind_names_match_filter_schema() {
        for (kind, _ops) in OPERATOR_MATRIX {
            let field = match *kind {
                "text" => "code",
                "date" => "created_at",
                "int" => "amount",
                other => panic!("unknown matrix kind: {other}"),
            };
            assert_eq!(SCHEMA.field_kind(field), Some(*kind));
        }
        assert_eq!(OPERATOR_MATRIX.len(), 3);
    }

    #[test]
    fn operator_matrix_matches_parse_support() {
        // ilike 只出现在 text；矩阵内每个 (类型, 操作符) 组合 parse 都能产出 Cmp
        for (kind, ops) in OPERATOR_MATRIX {
            assert!(
                !ops.contains(&Op::Ilike) || *kind == "text",
                "ilike only for text, got {kind}"
            );
            for op in *ops {
                let field = match *kind {
                    "text" => "name",
                    "date" => "created_at",
                    "int" => "amount",
                    other => panic!("unknown kind {other}"),
                };
                let value = if *kind == "int" { "1" } else { "x" };
                let node = parse(&format!("{field}{}{value}", op.rsql_str()), &SCHEMA).unwrap();
                assert!(matches!(node, Node::Cmp(_)));
            }
        }
    }

    #[test]
    fn rsql_operators_cover_all_ops_longest_first() {
        // 每个 Op 恰好一个 wire 串，且从长到短（`=ilike=` 在前）
        let mut seen: Vec<Op> = Vec::new();
        let mut prev_len = usize::MAX;
        for (s, op) in RSQL_OPERATORS {
            assert!(s.len() <= prev_len, "operators must be longest-first: {s}");
            prev_len = s.len();
            assert!(!seen.contains(op), "duplicate op {op:?}");
            seen.push(*op);
            assert_eq!(op.rsql_str(), *s);
        }
        assert_eq!(seen.len(), ALL_OPS.len());
        for op in ALL_OPS {
            assert!(seen.contains(&op), "missing op {op:?}");
        }
    }

    #[test]
    fn comparison_ops_match_rsql_operators() {
        let mapped: Vec<(&str, &str)> = comparison_ops();
        for (name, wire) in &mapped {
            let op = RSQL_OPERATORS
                .iter()
                .find(|(s, _)| s == wire)
                .map(|(_, o)| *o)
                .unwrap();
            assert_eq!(op.as_str(), *name);
        }
        assert_eq!(mapped.len(), ALL_OPS.len());
    }
}
