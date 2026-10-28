//! Boundary validation for client-supplied WebSocket payloads.
//!
//! Every value here arrives unauthenticated in the sense that the client
//! controls it; the token only proves *who* is sending, not that the payload is
//! sane. Reject early, before the value reaches the router, the database, or
//! another user's queue.

/// Chat messages are short; 4000 chars is generous for a DM and caps the write
/// amplification of a single frame.
pub const MAX_CONTENT_CHARS: usize = 4_000;

/// Opaque tenant identifiers are bounded so they cannot grow the keyspace or
/// the persistence layer without limit.
pub const MAX_ID_LEN: usize = 128;

/// A DM payload is valid when the recipient id is well formed and the content
/// is non-empty, bounded, and free of control characters other than newlines
/// and tabs.
pub fn validate_direct_message(recipient_user_id: &str, content: &str) -> Result<(), &'static str> {
    validate_id(recipient_user_id)?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("message content is empty");
    }

    if content.chars().count() > MAX_CONTENT_CHARS {
        return Err("message content is too long");
    }

    if content
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err("message content contains control characters");
    }

    Ok(())
}

/// Room payloads are validated like DMs, but the room id replaces the recipient.
pub fn validate_room_message(room_id: &str, content: &str) -> Result<(), &'static str> {
    validate_id(room_id)?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("message content is empty");
    }

    if content.chars().count() > MAX_CONTENT_CHARS {
        return Err("message content is too long");
    }

    if content
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err("message content contains control characters");
    }

    Ok(())
}

/// Identifiers are opaque, but they must be printable ASCII of bounded length.
/// Whitespace and control characters are rejected so ids cannot be used to
/// smuggle formatting or payloads into downstream systems.
pub fn validate_id(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("identifier is empty");
    }

    if value.len() > MAX_ID_LEN {
        return Err("identifier is too long");
    }

    if !value.chars().all(|c| c.is_ascii_graphic()) {
        return Err("identifier contains invalid characters");
    }

    Ok(())
}

/// Conversation ids must be UUIDs. New conversations are UUIDv5 and legacy
/// ones were random UUIDv4, so both parse.
pub fn validate_conversation_id(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("conversation_id is required");
    }

    if uuid::Uuid::parse_str(value).is_err() {
        return Err("conversation_id must be a UUID");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normal_direct_message() {
        assert!(validate_direct_message("user-1", "hello\nthere").is_ok());
    }

    #[test]
    fn rejects_empty_or_oversized_content() {
        assert!(validate_direct_message("user-1", "   ").is_err());
        assert!(validate_direct_message("user-1", &"a".repeat(MAX_CONTENT_CHARS + 1)).is_err());
    }

    #[test]
    fn rejects_control_characters() {
        assert!(validate_direct_message("user-1", "hello\u{0}world").is_err());
    }

    #[test]
    fn rejects_malformed_ids() {
        assert!(validate_id("").is_err());
        assert!(validate_id(&"a".repeat(MAX_ID_LEN + 1)).is_err());
        assert!(validate_id("has space").is_err());
        assert!(validate_id("line\nbreak").is_err());
        assert!(validate_id("ok.id_1:2").is_ok());
    }

    #[test]
    fn conversation_ids_must_parse_as_uuid() {
        assert!(validate_conversation_id("8f14e45f-ceea-5678-9abc-def012345678").is_ok());
        assert!(validate_conversation_id("not-a-uuid").is_err());
        assert!(validate_conversation_id("").is_err());
    }
}
