use sqlx::postgres::{PgPool, PgPoolOptions};

pub struct DatabaseConfig;

impl DatabaseConfig {
    pub async fn connect(database_url: &str) -> PgPool {
        PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await
            .expect("Failed to connect to PostgreSQL")
    }
}
