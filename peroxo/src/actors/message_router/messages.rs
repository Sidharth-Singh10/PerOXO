#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::PaginatedMessagesResponse;
use crate::chat::{ChatMessage, MessageAckResponse};

use tokio::sync::oneshot;

#[derive(Debug)]
pub enum RouterMessage {
    RegisterUser {
        user_id: i32,
        sender: tokio::sync::mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    UnregisterUser {
        user_id: i32,
    },
    SendDirectMessage {
        from: i32,
        to: i32,
        content: String,
        message_id: uuid::Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    },
    GetOnlineUsers {
        respond_to: oneshot::Sender<Vec<i32>>,
    },
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    GetPaginatedMessages {
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}
