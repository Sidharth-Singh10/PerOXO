use tokio::sync::oneshot;

use crate::{chat::PaginatedMessagesResponse, tenant::TenantUserId};

#[derive(Debug)]
pub enum PersistenceMessage {
    PersistDirectMessage {
        conversation_id: String,
        sender_id: TenantUserId,
        receiver_id: TenantUserId,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    GetPaginatedMessages {
        project_id: String,
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
    PersistRoomMessage {
        room_id: String,
        sender_id: TenantUserId,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    // the idea is to return a list of messages after the given message id
    SyncMessages {
        project_id: String,
        conversation_id: String,
        message_id: uuid::Uuid,
        respond_to: oneshot::Sender<Result<Vec<crate::chat::ResponseDirectMessage>, String>>,
    },
}
