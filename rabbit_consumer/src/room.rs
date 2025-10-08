use scylla::{
    client::session::Session,
    statement::{
        Consistency,
        batch::{Batch, BatchType},
    },
    value::CqlTimestamp,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message_handler::MessageHandler;

pub struct RoomMessage {
    pub room_id: String,
    pub message_id: Uuid,
    pub from: i32,
    pub content: String,
    pub created_at: CqlTimestamp,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableRoomMessage {
    pub room_id: String,
    pub message_id: Uuid,
    pub from: i32,
    pub content: String,
    pub created_at: i64,
}

impl From<RoomMessage> for SerializableRoomMessage {
    fn from(rm: RoomMessage) -> Self {
        Self {
            room_id: rm.room_id,
            message_id: rm.message_id,
            from: rm.from,
            content: rm.content,
            created_at: rm.created_at.0,
        }
    }
}

impl From<SerializableRoomMessage> for RoomMessage {
    fn from(srm: SerializableRoomMessage) -> Self {
        Self {
            room_id: srm.room_id,
            message_id: srm.message_id,
            from: srm.from,
            content: srm.content,
            created_at: CqlTimestamp(srm.created_at),
        }
    }
}

async fn write_room_message(
    session: &Session,
    message: RoomMessage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut batch = Batch::new(BatchType::Logged);

    batch.append_statement(
        "INSERT INTO affinity.room_messages (room_id, message_id, sender_id, content, created_at) \
         VALUES (?, ?, ?, ?, ?)",
    );

    batch.set_consistency(Consistency::One);

    let batch_values = ((
        &message.room_id,
        message.message_id,
        message.from,
        &message.content,
        message.created_at,
    ),);

    session.batch(&batch, batch_values).await?;

    Ok(())
}

pub struct RoomMessageHandler<'a> {
    pub session: &'a Session,
}

#[async_trait::async_trait]
impl<'a> MessageHandler for RoomMessageHandler<'a> {
    async fn process_message(&self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let srm: SerializableRoomMessage = serde_json::from_slice(data)?;
        let rm: RoomMessage = srm.into();

        // Use the result
        if let Err(e) = write_room_message(self.session, rm).await {
            tracing::error!("Failed to write room message for room_queue: {}", e);
        }
        Ok(())
    }
}
