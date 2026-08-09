use std::str::FromStr;

use rootcause::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool};

/// 打开（不存在则创建）本地 sqlite 队列文件，WAL 模式。
///
/// 仅支持单进程消费（sqlite 文件锁语义）；供单机部署 / 测试使用。
pub async fn new_sqlite_pool(path: impl AsRef<str>) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(path.as_ref())?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePool::connect_with(options).await?;
    Ok(pool)
}
