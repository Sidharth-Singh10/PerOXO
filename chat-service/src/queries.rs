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

pub async fn fetch_user_conversations(
    session: &Session,
    user_id: i32,
) -> Result<Vec<(String, CqlTimestamp)>, Box<dyn std::error::Error>> {
    let query =
        "SELECT conversation_id, last_message FROM affinity.user_conversations WHERE user_id = ?";
    let result = session.query_unpaged(query, (user_id,)).await?;

    let rows_result = result.into_rows_result()?;

    let typed_rows = rows_result.rows::<(String, CqlTimestamp)>()?;

    let mut conversations = Vec::new();
    for row_result in typed_rows {
        conversations.push(row_result?);
    }

    Ok(conversations)
}
pub async fn fetch_conversation_history(
    session: &Session,
    conversation_id: &str,
) -> Result<Vec<(Uuid, String, i32, i32)>, Box<dyn std::error::Error>> {
    let query = "SELECT message_id, message_text, sender_id, recipient_id FROM affinity.direct_messages WHERE conversation_id = ? ORDER BY message_id ASC";

    let result = session.query_unpaged(query, (conversation_id,)).await?;
    let rows_result = result.into_rows_result()?;
    let typed_rows = rows_result.rows::<(Uuid, String, i32, i32)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        messages.push(row_result?);
    }

    Ok(messages)
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
pub fn create_dm(sender_id: i32, recipient_id: i32, message_text: String) -> DirectMessage {
    // Sort user IDs to ensure consistent conversation_id format
    let (first_id, second_id) = if sender_id < recipient_id {
        (sender_id, recipient_id)
    } else {
        (recipient_id, sender_id)
    };

    let conversation_id = format!("{}_{}", first_id, second_id);

    let message_id = Uuid::new_v4();

    // Think about possibility of managing timestamps in a more efficient way
    let current_time_millis = chrono::Utc::now().timestamp_millis();
    let created_at = CqlTimestamp(current_time_millis);

    DirectMessage {
        conversation_id,
        message_id,
        sender_id,
        recipient_id,
        message_text,
        created_at,
    }
}
