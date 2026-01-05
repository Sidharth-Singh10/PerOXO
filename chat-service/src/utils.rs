use scylla::value::CqlTimestamp;
use uuid::Uuid;

#[cfg(feature = "rabbit")]

pub struct RoomMessage {
    pub room_id: String,
    pub message_id: Uuid,
    pub from: i32,
    pub content: String,
    pub created_at: CqlTimestamp,
}

pub struct DbMessage {
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: String,
    pub recipient_id: String,
    pub message_text: String,
    pub created_at: CqlTimestamp,
}

pub struct DbRoomMessage {
    pub room_id: String,
    pub message_id: Uuid,
    pub sender_id: String,
    pub content: String,
    pub created_at: CqlTimestamp,
}

pub struct DbRoomMessageEx {
    pub project_id: String,
    pub room_id: String,
    pub message_id: Uuid,
    pub sender_id: String,
    pub content: String,
    pub created_at: CqlTimestamp,
}
