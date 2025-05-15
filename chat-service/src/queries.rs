use chrono::{DateTime, Utc};
use scylla::{
    client::session::Session,
    statement::{
        batch::{Batch, BatchType}, Consistency
    }, value::{CqlDate, CqlTimestamp},
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
    // Query to get all conversations for a user
    let query = "SELECT conversation_id, last_message FROM user_conversations WHERE user_id = ?";
    let result = session.query_unpaged(query, (user_id,)).await?;
    
    // Convert QueryResult to QueryRowsResult
    let rows_result = result.into_rows_result()?;
    
    // Use the rows<T>() method with the correct types
    let typed_rows = rows_result.rows::<(String, CqlTimestamp)>()?;
    
    // Collect the rows, handling potential deserialization errors
    let mut conversations = Vec::new();
    for row_result in typed_rows {
        conversations.push(row_result?);
    }
    
    Ok(conversations)
}
pub async fn write_direct_message(
    session: &Session,
    message: DirectMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut batch = Batch::new(BatchType::Logged);

    batch.append_statement("INSERT INTO direct_messages (conversation_id, message_id, sender_id, recipient_id, message_text, created_at) VALUES (?, ?, ?, ?, ?, ?)");
    batch.append_statement(
        "UPDATE user_conversations SET last_message = ? WHERE user_id = ? AND conversation_id = ?",
    );
    batch.append_statement(
        "UPDATE user_conversations SET last_message = ? WHERE user_id = ? AND conversation_id = ?",
    );

    batch.set_consistency(Consistency::Quorum);

    let batch_values = (
        // Values for INSERT INTO direct_messages
        (
            &message.conversation_id,
            message.message_id,
            message.sender_id,
            message.recipient_id,
            &message.message_text,
            message.created_at,
        ),
        // Values for first UPDATE user_conversations (sender)
        (
            message.created_at,
            message.sender_id,
            &message.conversation_id,
        ),
        // Values for second UPDATE user_conversations (recipient)
        (
            message.created_at,
            message.recipient_id,
            &message.conversation_id,
        ),
    );

    // Execute the batch with all values
    session.batch(&batch, batch_values).await?;

    Ok(())
}
