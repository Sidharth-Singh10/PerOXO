use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub chat_service_addr: String,
    pub auth_service_addr: String,
    pub service_addr: String,
    pub rust_log: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    MissingEnvVar(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            chat_service_addr: require_env("CHAT_SERVICE_ADDR")?,
            auth_service_addr: require_env("AUTH_SERVICE_ADDR")?,
            service_addr: require_env("PER_OXO_SERVICE_ADDR")?,
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "debug".into()),
        })
    }
}

fn require_env(key: &str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::MissingEnvVar(key.to_string()))
}
