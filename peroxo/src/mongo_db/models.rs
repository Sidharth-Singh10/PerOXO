use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DirectMessage {
    #[serde(rename = "_id")]
    pub id: DirectMessageId,
    pub sender_id: i32,
    pub recipient_id: i32,
    pub message_text: String,
    pub created_at: DateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DirectMessageId {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserConversation {
    #[serde(rename = "_id")]
    pub id: UserConversationId,
    pub last_message: DateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserConversationId {
    pub user_id: i32,
    pub conversation_id: String,
}
