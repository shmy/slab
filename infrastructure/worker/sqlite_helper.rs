use std::str::FromStr;

use rootcause::Result;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};

pub async fn new_sqlite_pool(path: impl AsRef<str>) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(path.as_ref())?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePool::connect_with(options).await?;
    Ok(pool)
}
