use redis::Client;
use redis::aio::ConnectionManager;

pub struct RedisConfig;

impl RedisConfig {
    pub async fn connect(redis_url: &str) -> ConnectionManager {
        let client = Client::open(redis_url).expect("Invalid Redis URL");
        ConnectionManager::new(client)
            .await
            .expect("Failed to connect to Redis")
    }
}
