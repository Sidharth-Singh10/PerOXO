use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    // Direct message to a specific user
    DirectMessage {
        from: i32,
        to: i32,
        content: String,
        message_id: uuid::Uuid,
    },
    // Presence update (user online/offline)
    Presence {
        user: i32,
        status: PresenceStatus,
    },
    MessageAck {
        message_id: uuid::Uuid,
        timestamp: i64,
        status: MessageStatus,
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
