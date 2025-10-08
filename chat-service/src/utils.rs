use scylla::value::CqlTimestamp;
use uuid::Uuid;

pub struct RoomMessage {
    pub room_id: String,
    pub message_id: Uuid,
    pub from: i32,
    pub content: String,
    pub created_at: CqlTimestamp,
}
