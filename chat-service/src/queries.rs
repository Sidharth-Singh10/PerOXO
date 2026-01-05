use std::sync::Arc;

use scylla::{
    client::session::Session,
    statement::{
        Consistency,
        batch::{Batch, BatchType},
    },
    value::CqlTimestamp,
};
use uuid::Uuid;

use crate::{
    Queries,
    utils::{DbMessage, DbRoomMessage, DbRoomMessageEx},
};

pub struct DirectMessage {
    pub project_id: String,
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: String,
    pub recipient_id: String,
    pub message_text: String,
    pub created_at: CqlTimestamp,
}

pub async fn fetch_user_conversations(
    session: &Session,
    project_id: &str,
    user_id: &str,
) -> Result<Vec<(String, CqlTimestamp)>, Box<dyn std::error::Error>> {
    let query = "SELECT conversation_id, last_message FROM affinity.user_conversations WHERE project_id = ? AND user_id = ?";

    let result = session.query_unpaged(query, (project_id, user_id)).await?;

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
    project_id: &str,
    conversation_id: &str,
) -> Result<Vec<(Uuid, String, String, String, CqlTimestamp)>, Box<dyn std::error::Error>> {
    let query = r#"
        SELECT message_id, message_text, sender_id, recipient_id, created_at 
        FROM direct_messages 
        WHERE project_id = ? AND conversation_id = ?
    "#;

    let result = session
        .query_unpaged(query, (project_id, conversation_id))
        .await?;

    let rows_result = result.into_rows_result()?;

    let typed_rows = rows_result.rows::<(Uuid, String, String, String, CqlTimestamp)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        messages.push(row_result?);
    }

    Ok(messages)
}

pub async fn fetch_paginated_messages(
    session: &Session,
    project_id: &str,
    conversation_id: &str,
    cursor: Option<Uuid>,
) -> Result<Vec<DbMessage>, Box<dyn std::error::Error>> {
    // Query 1: Initial fetch (no cursor)
    // Filter by project_id AND conversation_id
    let query_without_cursor = r#"
        SELECT conversation_id, message_id, sender_id, recipient_id, message_text, created_at 
        FROM direct_messages 
        WHERE project_id = ? AND conversation_id = ? 
        ORDER BY message_id DESC 
        LIMIT 50
    "#;

    // Query 2: Pagination fetch (with cursor)
    // Filter by project_id AND conversation_id AND message_id < cursor
    let query_with_cursor = r#"
        SELECT conversation_id, message_id, sender_id, recipient_id, message_text, created_at 
        FROM direct_messages 
        WHERE project_id = ? AND conversation_id = ? AND message_id < ? 
        ORDER BY message_id DESC 
        LIMIT 50
    "#;

    let result = match cursor {
        Some(message_id) => {
            session
                .query_unpaged(query_with_cursor, (project_id, conversation_id, message_id))
                .await?
        }
        None => {
            session
                .query_unpaged(query_without_cursor, (project_id, conversation_id))
                .await?
        }
    };

    let rows_result = result.into_rows_result()?;

    let typed_rows = rows_result.rows::<(String, Uuid, String, String, String, CqlTimestamp)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        let (conv_id, msg_id, sender_id, recipient_id, msg_text, created_at) = row_result?;
        messages.push(DbMessage {
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
    project_id: &str,
    room_id: &str,
    cursor: Option<Uuid>,
) -> Result<Vec<DbRoomMessage>, Box<dyn std::error::Error>> {
    // Query 1: Initial fetch (no cursor)
    // Filter by project_id AND room_id
    let query_without_cursor = r#"
        SELECT room_id, message_id, sender_id, content, created_at 
        FROM room_messages 
        WHERE project_id = ? AND room_id = ? 
        ORDER BY message_id DESC 
        LIMIT 50
    "#;

    // Query 2: Pagination fetch (with cursor)
    let query_with_cursor = r#"
        SELECT room_id, message_id, sender_id, content, created_at 
        FROM room_messages 
        WHERE project_id = ? AND room_id = ? AND message_id < ? 
        ORDER BY message_id DESC 
        LIMIT 50
    "#;

    let result = match cursor {
        Some(message_id) => {
            session
                .query_unpaged(query_with_cursor, (project_id, room_id, message_id))
                .await?
        }
        None => {
            session
                .query_unpaged(query_without_cursor, (project_id, room_id))
                .await?
        }
    };

    let rows_result = result.into_rows_result()?;

    let typed_rows = rows_result.rows::<(String, Uuid, String, String, CqlTimestamp)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        let (rm_id, msg_id, sender_id, content, created_at) = row_result?;
        messages.push(DbRoomMessage {
            room_id: rm_id,
            message_id: msg_id,
            sender_id,
            content,
            created_at,
        });
    }

    Ok(messages)
}

pub async fn fetch_messages_after(
    session: &Session,
    queries: Arc<Queries>,
    project_id: &str,
    conversation_id: &str,
    after_message_id: Uuid,
) -> Result<Vec<DbMessage>, Box<dyn std::error::Error>> {
    // OPTIMIZED: We skip the timestamp lookup.
    // We directly query 'message_id > ?' because TimeUUIDs are naturally sortable.

    let res = session
        .execute_unpaged(
            &queries.q_fetch_after_message_id, // Renamed for clarity
            (project_id, conversation_id, after_message_id),
        )
        .await?;

    let rows_result = res.into_rows_result()?;
    let typed_rows = rows_result.rows::<(String, Uuid, String, String, String, CqlTimestamp)>()?;

    let mut messages = Vec::new();
    for row_result in typed_rows {
        let (conv_id, msg_id, sender_id, recipient_id, msg_text, created_at) = row_result?;
        messages.push(DbMessage {
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

pub async fn write_direct_message(
    session: &Session,
    message: DirectMessage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut batch = Batch::new(BatchType::Logged);

    // 1. Insert into direct_messages
    // PK: ((project_id, conversation_id), message_id)
    batch.append_statement(
        "INSERT INTO affinity.direct_messages \
        (project_id, conversation_id, message_id, sender_id, recipient_id, message_text, created_at) \
        VALUES (?, ?, ?, ?, ?, ?, ?)"
    );

    // 2. Update Sender's conversation list
    // PK: ((project_id, user_id), conversation_id)
    batch.append_statement(
        "INSERT INTO affinity.user_conversations (project_id, user_id, conversation_id, last_message) \
        VALUES (?, ?, ?, ?)"
    );

    // 3. Update Recipient's conversation list
    // PK: ((project_id, user_id), conversation_id)
    batch.append_statement(
        "INSERT INTO affinity.user_conversations (project_id, user_id, conversation_id, last_message) \
        VALUES (?, ?, ?, ?)"
    );

    // Consistency::One is fine for dev; consider Quorum for prod
    batch.set_consistency(Consistency::One);

    let batch_values = (
        // Statement 1: direct_messages
        (
            &message.project_id,
            &message.conversation_id,
            message.message_id,
            &message.sender_id,
            &message.recipient_id,
            &message.message_text,
            message.created_at,
        ),
        // Statement 2: user_conversations (Sender)
        (
            &message.project_id,
            &message.sender_id,
            &message.conversation_id,
            message.created_at,
        ),
        // Statement 3: user_conversations (Recipient)
        (
            &message.project_id,
            &message.recipient_id,
            &message.conversation_id,
            message.created_at,
        ),
    );

    session.batch(&batch, batch_values).await?;

    Ok(())
}

pub async fn write_room_message(
    session: &Session,
    message: DbRoomMessageEx,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut batch = Batch::new(BatchType::Logged);

    // 1. Insert into room_messages
    // PK: ((project_id, room_id), message_id)
    batch.append_statement(
        "INSERT INTO affinity.room_messages \
        (project_id, room_id, message_id, sender_id, content, created_at) \
        VALUES (?, ?, ?, ?, ?, ?)",
    );

    // 2. Update project_rooms (last_activity)
    // PK: ((project_id), room_id)
    batch.append_statement(
        "UPDATE affinity.project_rooms SET last_activity = ? WHERE project_id = ? AND room_id = ?",
    );

    batch.set_consistency(Consistency::One);

    let batch_values = (
        // Statement 1: room_messages
        (
            &message.project_id,
            &message.room_id,
            message.message_id,
            &message.sender_id,
            &message.content,
            message.created_at,
        ),
        // Statement 2: project_rooms
        (message.created_at, &message.project_id, &message.room_id),
    );

    session.batch(&batch, batch_values).await?;

    Ok(())
}

// pub fn create_dm(
//     sender_id: i32,
//     recipient_id: i32,
//     message_text: String,
//     message_id: &str,
//     timestamp: i64,
// ) -> Result<DirectMessage, Box<dyn std::error::Error>> {
//     // Sort user IDs to ensure consistent conversation_id format
//     let (first_id, second_id) = if sender_id < recipient_id {
//         (sender_id, recipient_id)
//     } else {
//         (recipient_id, sender_id)
//     };

//     let conversation_id = format!("{}_{}", first_id, second_id);

//     let message_id = match Uuid::parse_str(message_id) {
//         Ok(uuid) => uuid,
//         Err(e) => return Err(Box::new(e)),
//     };

//     // Think about possibility of managing timestamps in a more efficient way
//     let created_at = CqlTimestamp(timestamp);

//     Ok(DirectMessage {
//         conversation_id,
//         message_id,
//         sender_id,
//         recipient_id,
//         message_text,
//         created_at,
//     })
// }
#[cfg(feature = "rabbit")]

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
