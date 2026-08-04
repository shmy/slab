use db::PgPool;
use rootcause::Result;

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./versions");
    MIGRATOR.run(pool).await?;
    Ok(())
}
