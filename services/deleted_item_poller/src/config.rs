use anyhow::Context;
pub use macro_env::Environment;
use macro_env_var::env_vars;

env_vars! {
    struct DatabaseUrl;
    #[derive(Debug, Clone)]
    pub struct KafkaBrokers;
    #[derive(Debug, Clone)]
    pub struct RedisUri;
}

#[derive(Debug, Clone)]
pub struct Config {
    /// The connection URL for the Postgres database this application should use.
    pub database_url: String,

    /// Comma-separated Kafka bootstrap servers for document, project, and chat lifecycle events.
    pub kafka_brokers: KafkaBrokers,

    /// Redis endpoint used by the existing SHA count adapter.
    pub redis_uri: RedisUri,

    /// The environment we are in
    #[allow(dead_code)]
    pub environment: Environment,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = DatabaseUrl::new()
            .context("DATABASE_URL must be provided")?
            .to_string();
        let kafka_brokers = KafkaBrokers::new().context("KAFKA_BROKERS must be provided")?;
        let redis_uri = RedisUri::new().context("REDIS_URI must be provided")?;

        Ok(Config {
            database_url,
            kafka_brokers,
            redis_uri,
            environment: Environment::new_or_prod(),
        })
    }
}
