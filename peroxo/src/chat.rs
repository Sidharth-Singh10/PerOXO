use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    // client to server (naming from client POV, can be improved)
    SendDirectMessage {
        to: i32,
        content: String,
        client_message_id: uuid::Uuid,
        // message_id: Option<uuid::Uuid>,
    },
    // server to client
    DirectMessage {
        from: i32,
        content: String,
        server_message_id: uuid::Uuid,
        timestamp: i64,
    },
    // Presence update (user online/offline)
    Presence {
        user: i32,
        status: PresenceStatus,
    },

    MessageAck {
        client_message_id: uuid::Uuid,
        message_id: uuid::Uuid,
        timestamp: i64,
        status: MessageStatus,
    },

    GetPaginatedMessages {
        // paginataion cursor
        message_id: Option<uuid::Uuid>,
        // feat: add limit for pagination
        // limit: Option<u32>,
        conversation_id: String,
    },
    ChatHistoryResponse {
        messages: Vec<ResponseDirectMessage>,
        has_more: bool,
        next_cursor: Option<uuid::Uuid>,
    },
    RoomMessage {
        room_id: String,
        from: i32,
        content: String,
        message_id: uuid::Uuid,
    },
    JoinRoom {
        room_id: String,
    },

    LeaveRoom {
        room_id: String,
    },

    #[cfg(feature = "persistence")]
    SyncMessages {
        conversation_id: String,
        message_id: uuid::Uuid,
    },
    #[cfg(feature = "persistence")]
    SyncMessagesResponse {
        messages: Vec<ResponseDirectMessage>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MessageStatus {
    Delivered,
    Persisted,
    Failed(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageAckResponse {
    pub message_id: uuid::Uuid,
    pub timestamp: i64,
    pub status: MessageStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PresenceStatus {
    Online,
    Offline,
}

// Rename Struct to something more appropriate
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseDirectMessage {
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: i32,
    pub recipient_id: i32,
    pub message_text: String,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct PaginatedMessagesResponse {
    pub messages: Vec<ResponseDirectMessage>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
