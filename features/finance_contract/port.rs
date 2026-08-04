//! 发票只读 Port — 供付款模块验证发票信息。

use std::str::FromStr;

use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::error::FinanceError;

/// 发票类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceType {
    Sales,
    Purchase,
}

impl FromStr for InvoiceType {
    type Err = rootcause::Report;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "sales_invoice" => Ok(Self::Sales),
            "purchase_invoice" => Ok(Self::Purchase),
            _ => Err(FinanceError::InvalidInvoiceType.into()),
        }
    }
}

impl InvoiceType {
    /// 转为数据库表名及 invoice_type 字段值。
    pub fn table_name(&self) -> &'static str {
        match self {
            Self::Sales => "sales_invoices",
            Self::Purchase => "purchase_invoices",
        }
    }

    /// 转为 payments.invoice_type 列的值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sales => "sales_invoice",
            Self::Purchase => "purchase_invoice",
        }
    }
}

/// 发票只读数据。
#[derive(Debug, Clone)]
pub struct InvoiceInfo {
    pub total_amount: i64,
    pub paid_amount: i64,
}

/// 未付发票账龄桶：账龄区间 + 张数 + 未付金额。
///
/// bucket 标签："0-30" / "31-60" / "61-90" / "90+"。
#[derive(Debug, Clone)]
pub struct UnpaidBucket {
    pub bucket: String,
    pub count: i64,
    pub amount: i64,
}

/// 发票只读 Port。
pub struct InvoicePort;

impl InvoicePort {
    /// 根据发票 ID 和类型查询发票信息（FOR UPDATE）。
    pub async fn by_id(
        conn: &mut PgConnection,
        invoice_type: InvoiceType,
        invoice_id: &ID,
    ) -> Result<InvoiceInfo> {
        match invoice_type {
            InvoiceType::Sales => Self::by_sales_id(conn, invoice_id).await,
            InvoiceType::Purchase => Self::by_purchase_id(conn, invoice_id).await,
        }
    }

    /// 未付发票账龄聚合（未付清且有开票日期的发票按账龄分桶）。
    ///
    /// 账龄桶与未付金额口径的单一事实来源：账龄报表按桶呈现。
    pub async fn unpaid_aging(
        conn: &mut PgConnection,
        invoice_type: InvoiceType,
    ) -> Result<Vec<UnpaidBucket>> {
        match invoice_type {
            InvoiceType::Sales => Self::unpaid_sales_aging(conn).await,
            InvoiceType::Purchase => Self::unpaid_purchase_aging(conn).await,
        }
    }

    /// 未付总额（所有未付清发票，不要求开票日期）。
    ///
    /// 与 `unpaid_aging` 的区别：总额口径包含无开票日期的发票——
    /// 它们进不了账龄桶，但仍是未付金额。余额等报表用本方法。
    pub async fn unpaid_total(conn: &mut PgConnection, invoice_type: InvoiceType) -> Result<i64> {
        match invoice_type {
            InvoiceType::Sales => Self::unpaid_sales_total(conn).await,
            InvoiceType::Purchase => Self::unpaid_purchase_total(conn).await,
        }
    }

    async fn unpaid_sales_total(conn: &mut PgConnection) -> Result<i64> {
        let total = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(total_amount - paid_amount), 0)::BIGINT AS "total!"
               FROM sales_invoices"#
        )
        .fetch_one(conn)
        .await?;
        Ok(total)
    }

    async fn unpaid_purchase_total(conn: &mut PgConnection) -> Result<i64> {
        let total = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(total_amount - paid_amount), 0)::BIGINT AS "total!"
               FROM purchase_invoices"#
        )
        .fetch_one(conn)
        .await?;
        Ok(total)
    }

    async fn unpaid_sales_aging(conn: &mut PgConnection) -> Result<Vec<UnpaidBucket>> {
        let rows = sqlx::query!(
            r#"SELECT
                   CASE
                     WHEN CURRENT_DATE - invoice_date <= 30 THEN '0-30'
                     WHEN CURRENT_DATE - invoice_date <= 60 THEN '31-60'
                     WHEN CURRENT_DATE - invoice_date <= 90 THEN '61-90'
                     ELSE '90+'
                   END AS bucket,
                   COUNT(*)::BIGINT AS "count!",
                   COALESCE(SUM(total_amount - paid_amount), 0)::BIGINT AS "amount!"
               FROM sales_invoices
               WHERE total_amount > paid_amount AND invoice_date IS NOT NULL
               GROUP BY bucket
               ORDER BY bucket"#
        )
        .fetch_all(conn)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| UnpaidBucket {
                bucket: r.bucket.unwrap_or_default(),
                count: r.count,
                amount: r.amount,
            })
            .collect())
    }

    async fn unpaid_purchase_aging(conn: &mut PgConnection) -> Result<Vec<UnpaidBucket>> {
        let rows = sqlx::query!(
            r#"SELECT
                   CASE
                     WHEN CURRENT_DATE - invoice_date <= 30 THEN '0-30'
                     WHEN CURRENT_DATE - invoice_date <= 60 THEN '31-60'
                     WHEN CURRENT_DATE - invoice_date <= 90 THEN '61-90'
                     ELSE '90+'
                   END AS bucket,
                   COUNT(*)::BIGINT AS "count!",
                   COALESCE(SUM(total_amount - paid_amount), 0)::BIGINT AS "amount!"
               FROM purchase_invoices
               WHERE total_amount > paid_amount AND invoice_date IS NOT NULL
               GROUP BY bucket
               ORDER BY bucket"#
        )
        .fetch_all(conn)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| UnpaidBucket {
                bucket: r.bucket.unwrap_or_default(),
                count: r.count,
                amount: r.amount,
            })
            .collect())
    }

    async fn by_sales_id(conn: &mut PgConnection, invoice_id: &ID) -> Result<InvoiceInfo> {
        let row = sqlx::query!(
            r#"SELECT total_amount, paid_amount FROM sales_invoices WHERE id = $1 FOR UPDATE"#,
            invoice_id as _,
        )
        .fetch_optional(conn)
        .await?
        .ok_or(FinanceError::InvoiceNotFound)?;
        Ok(InvoiceInfo {
            total_amount: row.total_amount,
            paid_amount: row.paid_amount,
        })
    }

    async fn by_purchase_id(conn: &mut PgConnection, invoice_id: &ID) -> Result<InvoiceInfo> {
        let row = sqlx::query!(
            r#"SELECT total_amount, paid_amount FROM purchase_invoices WHERE id = $1 FOR UPDATE"#,
            invoice_id as _,
        )
        .fetch_optional(conn)
        .await?
        .ok_or(FinanceError::InvoiceNotFound)?;
        Ok(InvoiceInfo {
            total_amount: row.total_amount,
            paid_amount: row.paid_amount,
        })
    }
}
