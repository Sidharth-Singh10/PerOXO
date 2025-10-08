use scylla::{client::session::Session, value::CqlTimestamp};
use uuid::Uuid;

use crate::utils::RoomMessage;

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

pub async fn fetch_paginated_messages(
    session: &Session,
    conversation_id: &str,
    cursor: Option<Uuid>,
) -> Result<Vec<DirectMessage>, Box<dyn std::error::Error>> {
    // Define queries
    let query_without_cursor = "SELECT conversation_id, message_id, sender_id, recipient_id, message_text, created_at \
                                FROM affinity.direct_messages \
                                WHERE conversation_id = ? \
                                ORDER BY message_id DESC \
                                LIMIT 50";

    let query_with_cursor = "SELECT conversation_id, message_id, sender_id, recipient_id, message_text, created_at \
                             FROM affinity.direct_messages \
                             WHERE conversation_id = ? AND message_id < ? \
                             ORDER BY message_id DESC \
                             LIMIT 50";

    // Execute query
    let result = match cursor {
        Some(message_id) => {
            session
                .query_unpaged(query_with_cursor, (conversation_id, message_id))
                .await?
        }
        None => {
            session
                .query_unpaged(query_without_cursor, (conversation_id,))
                .await?
        }
    };

    let rows_result = result.into_rows_result()?;
    let typed_rows = rows_result.rows::<(String, Uuid, i32, i32, String, CqlTimestamp)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        let (conv_id, msg_id, sender_id, recipient_id, msg_text, created_at) = row_result?;
        messages.push(DirectMessage {
            conversation_id: conv_id,
            message_id: msg_id,
            sender_id,
            recipient_id,
            message_text: msg_text,
            created_at,
        });
    }

    Ok(messages)
}

pub async fn fetch_paginated_room_messages(
    session: &Session,
    room_id: &str,
    cursor: Option<Uuid>,
) -> Result<Vec<RoomMessage>, Box<dyn std::error::Error>> {
    // Define queries
    let query_without_cursor = "SELECT room_id, message_id, from, content, created_at \
                                FROM affinity.room_messages \
                                WHERE room_id = ? \
                                ORDER BY message_id DESC \
                                LIMIT 50";

    let query_with_cursor = "SELECT room_id, message_id, from, content, created_at \
                             FROM affinity.room_messages \
                             WHERE room_id = ? AND message_id < ? \
                             ORDER BY message_id DESC \
                             LIMIT 50";

    // Execute query
    let result = match cursor {
        Some(message_id) => {
            session
                .query_unpaged(query_with_cursor, (room_id, message_id))
                .await?
        }
        None => {
            session
                .query_unpaged(query_without_cursor, (room_id,))
                .await?
        }
    };

    let rows_result = result.into_rows_result()?;
    let typed_rows = rows_result.rows::<(String, Uuid, i32, String, CqlTimestamp)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        let (rm_id, msg_id, frm, cnt, created_at) = row_result?;
        messages.push(RoomMessage {
            room_id: rm_id,
            message_id: msg_id,
            from: frm,
            content: cnt,
            created_at,
        });
    }

    Ok(messages)
}

pub fn create_dm(
    sender_id: i32,
    recipient_id: i32,
    message_text: String,
    message_id: &str,
    timestamp: i64,
) -> Result<DirectMessage, Box<dyn std::error::Error>> {
    // Sort user IDs to ensure consistent conversation_id format
    let (first_id, second_id) = if sender_id < recipient_id {
        (sender_id, recipient_id)
    } else {
        (recipient_id, sender_id)
    };

    let conversation_id = format!("{}_{}", first_id, second_id);

    let message_id = match Uuid::parse_str(message_id) {
        Ok(uuid) => uuid,
        Err(e) => return Err(Box::new(e)),
    };

    // Think about possibility of managing timestamps in a more efficient way
    let created_at = CqlTimestamp(timestamp);

    Ok(DirectMessage {
        conversation_id,
        message_id,
        sender_id,
        recipient_id,
        message_text,
        created_at,
    })
}

pub fn create_room_message(
    room_id: String,
    from: i32,
    content: String,
    message_id: &str,
    timestamp: i64,
) -> Result<RoomMessage, Box<dyn std::error::Error>> {
    let message_id = match Uuid::parse_str(message_id) {
        Ok(uuid) => uuid,
        Err(e) => return Err(Box::new(e)),
    };

    let created_at = CqlTimestamp(timestamp);

    Ok(RoomMessage {
        room_id,
        message_id,
        from,
        content,
        created_at,
    })
}
