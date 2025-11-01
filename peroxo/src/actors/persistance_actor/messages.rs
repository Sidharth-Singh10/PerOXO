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
    PersistRoomMessage {
        room_id: String,
        sender_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    // the idea is to return a list of messages after the given message id
    SyncMessages {
        conversation_id: String,
        message_id: uuid::Uuid,
        respond_to: oneshot::Sender<Result<Vec<crate::chat::ResponseDirectMessage>, String>>,
    },
}
