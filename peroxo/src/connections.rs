use crate::actors::chat_service::chat_service_client::ChatServiceClient;
use tonic::transport::Channel;

pub async fn connect_chat_service_client()
-> Result<ChatServiceClient<Channel>, Box<dyn std::error::Error>> {
    let chat_service_addr = std::env::var("CHAT_SERVICE_ADDR")?;
    let client = ChatServiceClient::connect(chat_service_addr).await?;
    Ok(client)
}
