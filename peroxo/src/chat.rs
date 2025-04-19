use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    // Direct message to a specific user
    DirectMessage {
        from: String,
        to: String,
        content: String,
    },
    // Presence update (user online/offline)
    Presence {
        user: String,
        status: PresenceStatus,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PresenceStatus {
    Online,
    Offline,
}
