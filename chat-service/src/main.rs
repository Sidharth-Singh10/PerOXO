use queries::DirectMessage;
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::value::CqlTimestamp;
use std::error::Error;
use uuid::Uuid;
mod queries;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let session: Session = SessionBuilder::new()
        .known_node("127.0.0.1:9042")
        .known_node("1.2.3.4:9876")
        .build()
        .await?;

    Ok(())
}


pub fn create_message(sender_id: i32, recipient_id: i32, message_text: String) -> DirectMessage {
    // Sort user IDs to ensure consistent conversation_id format
    let (first_id, second_id) = if sender_id < recipient_id {
        (sender_id, recipient_id)
    } else {
        (recipient_id, sender_id)
    };

    // Create conversation_id in format "smaller_uuid_larger_uuid"
    let conversation_id = format!("{}_{}", first_id, second_id);

    // Generate a new UUID for the message
    let message_id = Uuid::new_v4();

    // Think about possibilty of managing timestamps in a more efficient way
    let current_time_millis = chrono::Utc::now().timestamp();
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
