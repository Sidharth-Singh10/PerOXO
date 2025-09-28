use crate::actors::chat_service::chat_service_client::ChatServiceClient;
use tonic::transport::Channel;

pub async fn connect_chat_service_client(
    chat_service_addr: String,
) -> Result<ChatServiceClient<Channel>, Box<dyn std::error::Error>> {
    let client = ChatServiceClient::connect(chat_service_addr).await?;
    Ok(client)
}
