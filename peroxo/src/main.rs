use per_oxo::{config::Config, startup, telemetry};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let config = Config::from_env().expect("invalid configuration");

    telemetry::init_tracing(&config.rust_log);

    let state = match startup::build_state(&config).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("Failed to build PerOxoState: {:?}", e);
            return;
        }
    };

    if let Err(e) = startup::run(&config, state).await {
        tracing::error!("Server error: {:?}", e);
    }
}
