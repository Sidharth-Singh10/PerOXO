use tokio::sync::oneshot;

use crate::chat::PaginatedMessagesResponse;

#[derive(Debug)]
pub enum PersistenceMessage {
    PersistDirectMessage {
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    GetPaginatedMessages {
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}
