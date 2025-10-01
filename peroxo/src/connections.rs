use crate::actors::chat_service::chat_service_client::ChatServiceClient;
use tonic::transport::Channel;

pub async fn connect_chat_service_client(
    chat_service_addr: String,
) -> Result<ChatServiceClient<Channel>, Box<dyn std::error::Error>> {
    let client = ChatServiceClient::connect(chat_service_addr).await?;
    Ok(client)
}

#[cfg(feature = "mongo_db")]
pub async fn connect_mongo_db_client(
    mongo_db_url: impl Into<String>,
) -> Result<mongodb::Client, Box<dyn std::error::Error>> {
    let options = mongodb::options::ClientOptions::parse(&mongo_db_url.into()).await?;
    let client = mongodb::Client::with_options(options)?;
    Ok(client)
}
