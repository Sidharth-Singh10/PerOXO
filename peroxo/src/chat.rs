use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    // Direct message to a specific user
    DirectMessage {
        from: i32,
        to: i32,
        content: String,
    },
    // Presence update (user online/offline)
    Presence {
        user: i32,
        status: PresenceStatus,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PresenceStatus {
    Online,
    Offline,
}
