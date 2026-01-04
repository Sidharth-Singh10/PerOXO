#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::PaginatedMessagesResponse;
use crate::{
    chat::{ChatMessage, MessageAckResponse},
    tenant::TenantUserId,
};

use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub enum RouterMessage {
    RegisterUser {
        tenant_user_id: TenantUserId,
        sender: tokio::sync::mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    UnregisterUser {
        tenant_user_id: TenantUserId,
    },
    SendDirectMessage {
        from: TenantUserId,
        to: TenantUserId,
        content: String,
        message_id: uuid::Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    },
    GetOnlineUsers {
        respond_to: oneshot::Sender<Vec<TenantUserId>>,
    },
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    GetPaginatedMessages {
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
    JoinRoom {
        tenant_user_id: TenantUserId,
        room_id: String,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    LeaveRoom {
        tenant_user_id: TenantUserId,
        room_id: String,
    },
    SendRoomMessage {
        room_id: String,
        from: TenantUserId,
        content: String,
        message_id: uuid::Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    },
    GetRoomMembers {
        room_id: String,
        respond_to: oneshot::Sender<Option<Vec<TenantUserId>>>,
    },

    #[cfg(feature = "persistence")]
    SyncMessages {
        conversation_id: String,
        message_id: uuid::Uuid,
        respond_to: oneshot::Sender<Result<Vec<crate::chat::ResponseDirectMessage>, String>>,
    },
}
