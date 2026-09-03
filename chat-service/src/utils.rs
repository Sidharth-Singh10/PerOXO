use scylla::statement::Consistency;
use scylla::value::CqlTimestamp;
use std::sync::OnceLock;
use uuid::Uuid;

/// Consistency level for writes, configurable via `SCYLLA_CONSISTENCY`
/// (default: `LocalQuorum` for durability). Single-node dev setups should
/// set it to `One`.
pub fn write_consistency() -> Consistency {
    static CONSISTENCY: OnceLock<Consistency> = OnceLock::new();
    *CONSISTENCY.get_or_init(|| match std::env::var("SCYLLA_CONSISTENCY").as_deref() {
        Ok("One") => Consistency::One,
        Ok("Quorum") => Consistency::Quorum,
        Ok("LocalQuorum") => Consistency::LocalQuorum,
        Ok("EachQuorum") => Consistency::EachQuorum,
        Ok("All") => Consistency::All,
        _ => Consistency::LocalQuorum,
    })
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
