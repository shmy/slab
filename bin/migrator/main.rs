use migration::run_migrations;

#[tokio::main]
async fn main() {
    dotenvy::dotenv_override().ok();
    tracing_subscriber::fmt().init();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");
    run_migrations(&pool).await.expect("Failed to migrate");
}
