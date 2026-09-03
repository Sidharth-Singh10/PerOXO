use std::time::Duration;
use tonic::transport::Channel;

use crate::chat_service_client::ChatServiceClient;
use crate::auth_service_client::AuthServiceClient;

const CONNECT_RETRIES: u32 = 5;
const CONNECT_BACKOFF: Duration = Duration::from_secs(2);

/// Connects to a gRPC service with bounded retries so startup survives a
/// dependency that is briefly unavailable, and configures HTTP/2 keep-alive
/// so idle long-lived channels detect dead peers instead of hanging.
async fn connect_with_retry(
    addr: &str,
    service: &str,
) -> Result<Channel, Box<dyn std::error::Error>> {
    let endpoint = tonic::transport::Endpoint::from_shared(addr.to_string())
        .map_err(|e| format!("invalid {service} address {addr}: {e}"))?
        .connect_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true)
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_timeout(Duration::from_secs(20));

    let mut attempt = 0;
    loop {
        match endpoint.connect().await {
            Ok(channel) => return Ok(channel),
            Err(e) if attempt < CONNECT_RETRIES => {
                attempt += 1;
                tracing::warn!(
                    "Failed to connect to {service} (attempt {attempt}/{}), retrying in {:?}: {e}",
                    CONNECT_RETRIES + 1,
                    CONNECT_BACKOFF
                );
                tokio::time::sleep(CONNECT_BACKOFF).await;
            }
            Err(e) => {
                return Err(format!(
                    "failed to connect to {service} after {} attempts: {e}",
                    CONNECT_RETRIES + 1
                )
                .into());
            }
        }
    }
}

pub async fn connect_chat_service_client(
    chat_service_addr: String,
) -> Result<ChatServiceClient<Channel>, Box<dyn std::error::Error>> {
    let channel = connect_with_retry(&chat_service_addr, "chat-service").await?;
    Ok(ChatServiceClient::new(channel))
}

pub async fn connect_auth_service_client(
    auth_service_addr: String,
) -> Result<AuthServiceClient<Channel>, Box<dyn std::error::Error>> {
    let channel = connect_with_retry(&auth_service_addr, "auth-service").await?;
    Ok(AuthServiceClient::new(channel))
}

#[cfg(feature = "mongo_db")]
pub async fn connect_mongo_db_client(
    mongo_db_url: impl Into<String>,
) -> Result<mongodb::Client, Box<dyn std::error::Error>> {
    let options = mongodb::options::ClientOptions::parse(mongo_db_url.into()).await?;
    let client = mongodb::Client::with_options(options)?;
    Ok(client)
}