use std::any::type_name;

use scylla::{
    client::session::Session,
    statement::{
        Consistency,
        batch::{Batch, BatchType},
    },
    value::CqlTimestamp,
};
use uuid::Uuid;

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
