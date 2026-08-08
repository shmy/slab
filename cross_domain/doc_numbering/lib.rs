//! 单据编码生成：统一封装 PG 序列号 + 日期 + 前缀格式。
//!
//! 所有创建端点均调用 `DocNumberer` 替代重复的内联 SQL。
//!
//! # 格式
//!
//! - `next_number` → `{prefix}-{yyyymmdd}-{seq:06}`（最常见）
//! - `next_seq` → 仅返回序列值，由调用方自行格式化（少数自定义格式）

use rootcause::Result;
use sqlx::Row as _;

/// 单据编码生成器。
pub struct DocNumberer;

impl DocNumberer {
    /// 生成标准编码：`{prefix}-{yyyymmdd}-{seq:06}`。
    ///
    /// 查询 PG 序列 `{seq_name}` 获取下一值。
    #[tracing::instrument(skip_all)]
    #[inline]
    pub async fn next_number(
        conn: &mut sqlx::PgConnection,
        seq_name: &str,
        prefix: &str,
    ) -> Result<String> {
        let seq = Self::next_seq(conn, seq_name).await?;
        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        Ok(format!("{}-{}-{:06}", prefix, today, seq))
    }

    /// 获取 PG 序列的下一个值。
    #[tracing::instrument(skip_all)]
    #[inline]
    pub async fn next_seq(conn: &mut sqlx::PgConnection, seq_name: &str) -> Result<i64> {
        // SAFETY: seq_name 始终来自编译期常量，非用户输入。
        let sql = format!("SELECT nextval('{}')::BIGINT AS v", seq_name);
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_one(conn)
            .await?;
        Ok(row.try_get("v").unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn test_next_number(pool: sqlx::PgPool) {
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query("CREATE TEMP SEQUENCE IF NOT EXISTS test_code_seq START 1")
            .execute(&mut *conn)
            .await
            .unwrap();

        let code = DocNumberer::next_number(&mut conn, "test_code_seq", "TEST")
            .await
            .unwrap();
        assert!(code.starts_with("TEST-"));
        assert!(code.len() > 15);

        let code2 = DocNumberer::next_number(&mut conn, "test_code_seq", "TEST")
            .await
            .unwrap();
        assert_ne!(code, code2);
    }

    #[sqlx::test]
    async fn test_next_seq(pool: sqlx::PgPool) {
        let mut conn = pool.acquire().await.unwrap();

        sqlx::query("CREATE TEMP SEQUENCE IF NOT EXISTS test_seq_seq START 100")
            .execute(&mut *conn)
            .await
            .unwrap();

        let seq = DocNumberer::next_seq(&mut conn, "test_seq_seq")
            .await
            .unwrap();
        assert_eq!(seq, 100);

        let seq2 = DocNumberer::next_seq(&mut conn, "test_seq_seq")
            .await
            .unwrap();
        assert_eq!(seq2, 101);
    }
}
