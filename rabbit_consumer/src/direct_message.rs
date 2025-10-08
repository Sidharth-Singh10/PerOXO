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

pub struct DirectMessage {
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: i32,
    pub recipient_id: i32,
    pub message_text: String,
    pub created_at: CqlTimestamp,
}

pub async fn write_direct_message(
    session: &Session,
    message: DirectMessage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut batch = Batch::new(BatchType::Logged);

    batch.append_statement("INSERT INTO affinity.direct_messages (conversation_id, message_id, sender_id, recipient_id, message_text, created_at) VALUES (?, ?, ?, ?, ?, ?)");
    batch.append_statement(
        "UPDATE affinity.user_conversations SET last_message = ? WHERE user_id = ? AND conversation_id = ?",
    );
    batch.append_statement(
        "UPDATE affinity.user_conversations SET last_message = ? WHERE user_id = ? AND conversation_id = ?",
    );

    // for development purposes, setting consistency to One
    batch.set_consistency(Consistency::One);

    let batch_values = (
        (
            &message.conversation_id,
            message.message_id,
            message.sender_id,
            message.recipient_id,
            &message.message_text,
            message.created_at,
        ),
        (
            message.created_at,
            message.sender_id,
            &message.conversation_id,
        ),
        (
            message.created_at,
            message.recipient_id,
            &message.conversation_id,
        ),
    );

    session.batch(&batch, batch_values).await?;

    Ok(())
}

pub struct DirectMessageHandler<'a> {
    pub session: &'a Session,
}

#[async_trait::async_trait]
impl<'a> MessageHandler for DirectMessageHandler<'a> {
    async fn process_message(&self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let msg: SerializableDirectMessage = serde_json::from_slice(data)?;
        let dm: DirectMessage = msg.into();

        if let Err(e) = write_direct_message(self.session, dm).await {
            tracing::error!("Failed to write room message for room_queue: {}", e);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableDirectMessage {
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: i32,
    pub recipient_id: i32,
    pub message_text: String,
    pub created_at: i64,
}

impl From<DirectMessage> for SerializableDirectMessage {
    fn from(dm: DirectMessage) -> Self {
        Self {
            conversation_id: dm.conversation_id,
            message_id: dm.message_id,
            sender_id: dm.sender_id,
            recipient_id: dm.recipient_id,
            message_text: dm.message_text,
            created_at: dm.created_at.0,
        }
    }
}

impl From<SerializableDirectMessage> for DirectMessage {
    fn from(sdm: SerializableDirectMessage) -> Self {
        Self {
            conversation_id: sdm.conversation_id,
            message_id: sdm.message_id,
            sender_id: sdm.sender_id,
            recipient_id: sdm.recipient_id,
            message_text: sdm.message_text,
            created_at: CqlTimestamp(sdm.created_at),
        }
    }
}
