use std::sync::Arc;

use scylla::{
    client::session::Session,
    statement::batch::{Batch, BatchType},
    value::{CqlTimestamp, CqlTimeuuid},
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
    // Bounded: a user with thousands of conversations must not force the
    // whole list into memory. Keyset pagination (conversation_id > cursor)
    // can be added to the RPC later if needed.
    let query = "SELECT conversation_id, last_message FROM affinity.user_conversations WHERE project_id = ? AND user_id = ? LIMIT 50";

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
    // Bounded: an unbounded conversation must not be materialized in memory.
    // Cap at 100 rows; clients should use the paginated RPC for more.
    let query = r#"
        SELECT message_id, message_text, sender_id, recipient_id, created_at 
        FROM direct_messages 
        WHERE project_id = ? AND conversation_id = ?
        LIMIT 100
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

    // Durability: default to LocalQuorum so a write is acknowledged only
    // after at least one replica besides the coordinator has it. Dev
    // single-node setups can override with SCYLLA_CONSISTENCY=One.
    batch.set_consistency(crate::utils::write_consistency());

    let batch_values = (
        // Statement 1: direct_messages
        (
            &message.project_id,
            &message.conversation_id,
            CqlTimeuuid::from(message.message_id),
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

    batch.set_consistency(crate::utils::write_consistency());

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

pub async fn getsert_conversation_id(
    session: &Session,
    project_id: &str,
    user_a: &str,
    user_b: &str,
) -> Result<(String, bool), Box<dyn std::error::Error + Send + Sync>> {
    let (user_id_1, user_id_2) = if user_a < user_b {
        (user_a, user_b)
    } else {
        (user_b, user_a)
    };

    let select_query = r#"
        SELECT conversation_id 
        FROM affinity.dm_lookup 
        WHERE project_id = ? AND user_id_1 = ? AND user_id_2 = ?
    "#;

    let result = session
        .query_unpaged(select_query, (project_id, user_id_1, user_id_2))
        .await?;

    if let Ok(rows_result) = result.into_rows_result() {
        let mut typed_rows = rows_result.rows::<(String,)>()?;

        if let Some(row_result) = typed_rows.next() {
            let (existing_conv_id,) = row_result?;
            insert_conversation_members(
                session,
                project_id,
                &existing_conv_id,
                user_id_1,
                user_id_2,
            )
            .await?;
            return Ok((existing_conv_id, false));
        }
    }

    let new_conversation_id = deterministic_conversation_id(project_id, user_id_1, user_id_2);

    let insert_query = r#"
        INSERT INTO affinity.dm_lookup (project_id, user_id_1, user_id_2, conversation_id, created_at)
        VALUES (?, ?, ?, ?, toTimestamp(now()))
    "#;

    session
        .query_unpaged(
            insert_query,
            (project_id, user_id_1, user_id_2, &new_conversation_id),
        )
        .await?;

    insert_conversation_members(
        session,
        project_id,
        &new_conversation_id,
        user_id_1,
        user_id_2,
    )
    .await?;

    Ok((new_conversation_id, true))
}

/// Records both participants for a conversation.
///
/// Writing these rows is what makes the read path's authorization check
/// possible: without them there is no way to answer "does this caller belong
/// to this conversation" from a conversation id alone.
async fn insert_conversation_members(
    session: &Session,
    project_id: &str,
    conversation_id: &str,
    user_a: &str,
    user_b: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut batch = Batch::new(BatchType::Unlogged);

    batch.append_statement(
        "INSERT INTO affinity.conversation_members (project_id, conversation_id, user_id) \
         VALUES (?, ?, ?)",
    );
    batch.append_statement(
        "INSERT INTO affinity.conversation_members (project_id, conversation_id, user_id) \
         VALUES (?, ?, ?)",
    );

    let values = (
        (project_id, conversation_id, user_a),
        (project_id, conversation_id, user_b),
    );

    session.batch(&batch, values).await?;

    Ok(())
}

/// Returns true when `user_id` is one of the recorded participants.
///
/// Missing membership rows are treated as "not a participant": a conversation
/// that predates the index must be backfilled
/// (`BACKFILL_CONVERSATION_MEMBERS=1`) rather than silently readable by anyone
/// who knows its id.
pub async fn is_conversation_participant(
    session: &Session,
    project_id: &str,
    conversation_id: &str,
    user_id: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let result = session
        .query_unpaged(
            "SELECT user_id FROM affinity.conversation_members \
             WHERE project_id = ? AND conversation_id = ?",
            (project_id, conversation_id),
        )
        .await?;

    let rows_result = result.into_rows_result()?;
    let rows = rows_result.rows::<(String,)>()?;

    let mut members = Vec::new();
    for row in rows {
        members.push(row?.0);
    }

    Ok(membership_contains(&members, user_id))
}

/// Pure helper, split out so the authorization rule is unit-testable without a
/// database.
pub fn membership_contains(members: &[String], user_id: &str) -> bool {
    members.iter().any(|member| member == user_id)
}

/// Derives a conversation id from the (already sorted) user pair instead of
/// a random UUID. Two callers racing between the SELECT and the INSERT for
/// the same pair always converge on the same id, so a conversation can never
/// be split across differently-id'd rows. Existing rows created with
/// random ids are still returned by the SELECT, keeping them consistent.
fn deterministic_conversation_id(project_id: &str, user_id_1: &str, user_id_2: &str) -> String {
    let key = format!("{}|{}|{}", project_id, user_id_1, user_id_2);
    Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_id_is_deterministic() {
        let first = deterministic_conversation_id("proj", "alice", "bob");
        let second = deterministic_conversation_id("proj", "alice", "bob");
        assert_eq!(first, second);
        assert!(Uuid::parse_str(&first).is_ok());
    }

    #[test]
    fn conversation_id_is_scoped_to_project() {
        let a = deterministic_conversation_id("project-a", "alice", "bob");
        let b = deterministic_conversation_id("project-b", "alice", "bob");
        assert_ne!(a, b);
    }

    #[test]
    fn membership_requires_an_exact_match() {
        let members = vec!["alice".to_string(), "bob".to_string()];

        assert!(membership_contains(&members, "alice"));
        assert!(membership_contains(&members, "bob"));
        assert!(!membership_contains(&members, "carol"));
        assert!(!membership_contains(&members, ""));
        assert!(!membership_contains(&[], "alice"));
    }
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
