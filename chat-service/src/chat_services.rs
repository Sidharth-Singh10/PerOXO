use std::sync::Arc;

use scylla::client::session::Session;
use tonic::{Request, Response, Status};

use crate::chat_service::GetPaginatedMessagesRequest;
use crate::chat_service::GetPaginatedMessagesResponse;
use crate::chat_service::GetPaginatedRoomMessagesRequest;
use crate::chat_service::GetPaginatedRoomMessagesResponse;
use crate::chat_service::GetSertConversationRequest;
use crate::chat_service::GetSertConversationResponse;
use crate::chat_service::RoomMessage;
use crate::chat_service::SyncMessagesRequest;
use crate::chat_service::SyncMessagesResponse;
use crate::chat_service::WriteRoomMessageRequest;
use crate::chat_service::WriteRoomMessageResponse;
use crate::queries::fetch_messages_after;
use crate::queries::fetch_paginated_room_messages;
use crate::queries::getsert_conversation_id;
use crate::queries::is_conversation_participant;
use crate::queries::write_direct_message;
use crate::queries::write_room_message;
use crate::utils::DbRoomMessageEx;
use crate::{
    chat_service::{
        ConversationMessage, FetchConversationHistoryRequest, FetchConversationHistoryResponse,
        FetchUserConversationsRequest, FetchUserConversationsResponse, UserConversation,
        WriteDmRequest, WriteDmResponse, chat_service_server::ChatService,
    },
    queries::{fetch_conversation_history, fetch_paginated_messages, fetch_user_conversations},
};
use scylla::value::CqlTimestamp;
use uuid::Uuid;

pub struct ChatServiceImpl {
    session: Arc<Session>,
    queries: Arc<crate::Queries>,
}

impl ChatServiceImpl {
    pub fn new(session: Arc<Session>, queries: Arc<crate::Queries>) -> Self {
        Self { session, queries }
    }
}

/// Logs the real cause and returns a generic message for the caller.
fn internal_error(context: &str, error: &dyn std::fmt::Display) -> String {
    tracing::error!("{context}: {error}");
    "Internal error".to_string()
}

/// Bounds enforced again here, not only at the gateway: a caller holding the
/// service secret must not be able to write unbounded rows either.
const MAX_ID_LEN: usize = 128;
const MAX_CONTENT_CHARS: usize = 4_000;

fn within_id_bound(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LEN
}

fn within_content_bound(value: &str) -> bool {
    value.chars().count() <= MAX_CONTENT_CHARS
}

/// A conversation belongs to exactly two users; the caller must be one of them.
fn caller_is_participant(caller_user_id: &str, user_id_1: &str, user_id_2: &str) -> bool {
    !caller_user_id.is_empty() && (caller_user_id == user_id_1 || caller_user_id == user_id_2)
}

/// Rejects reads from callers who are not participants of the conversation.
///
/// The gateway identifies the caller from the verified token; chat-service
/// enforces the rule again so that a direct gRPC call cannot bypass it.
async fn authorize_conversation(
    session: &Session,
    project_id: &str,
    conversation_id: &str,
    caller_user_id: &str,
) -> Result<(), String> {
    if caller_user_id.is_empty() {
        return Err("caller_user_id is required".to_string());
    }

    match is_conversation_participant(session, project_id, conversation_id, caller_user_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Not a participant of this conversation".to_string()),
        Err(e) => {
            tracing::error!("conversation membership lookup failed: {e}");
            Err("Internal error".to_string())
        }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServiceImpl {
    async fn get_sert_conversation(
        &self,
        request: Request<GetSertConversationRequest>,
    ) -> Result<Response<GetSertConversationResponse>, Status> {
        let req = request.into_inner();

        // 1. Validation
        if req.project_id.is_empty() || req.user_id_1.is_empty() || req.user_id_2.is_empty() {
            return Ok(Response::new(GetSertConversationResponse {
                success: false,
                error_message: "project_id, user_id_1, and user_id_2 are required".to_string(),
                conversation_id: String::new(),
                created_new: false,
            }));
        }

        if req.user_id_1 == req.user_id_2 {
            return Ok(Response::new(GetSertConversationResponse {
                success: false,
                error_message: "Cannot create conversation with self".to_string(),
                conversation_id: String::new(),
                created_new: false,
            }));
        }

        // Only a participant may ask for (or create) the conversation. Without
        // this, any tenant member could enumerate the conversation ids of any
        // other pair in the project.
        if !caller_is_participant(&req.caller_user_id, &req.user_id_1, &req.user_id_2) {
            return Ok(Response::new(GetSertConversationResponse {
                success: false,
                error_message: "Not a participant of this conversation".to_string(),
                conversation_id: String::new(),
                created_new: false,
            }));
        }

        match getsert_conversation_id(
            &self.session,
            &req.project_id,
            &req.user_id_1,
            &req.user_id_2,
        )
        .await
        {
            Ok((conversation_id, created_new)) => Ok(Response::new(GetSertConversationResponse {
                success: true,
                error_message: String::new(),
                conversation_id,
                created_new,
            })),
            Err(e) => Ok(Response::new(GetSertConversationResponse {
                success: false,
                error_message: internal_error("get_sert_conversation failed", &e),
                conversation_id: String::new(),
                created_new: false,
            })),
        }
    }

    async fn write_dm(
        &self,
        request: Request<WriteDmRequest>,
    ) -> Result<Response<WriteDmResponse>, Status> {
        let req = request.into_inner();

        // 1. Input Validation
        if !within_id_bound(&req.project_id)
            || !within_id_bound(&req.sender_id)
            || !within_id_bound(&req.receiver_id)
        {
            return Ok(Response::new(WriteDmResponse {
                success: false,
                error_message: "project_id, sender_id and receiver_id are required".to_string(),
            }));
        }

        if !within_content_bound(&req.message) {
            return Ok(Response::new(WriteDmResponse {
                success: false,
                error_message: "message is too long".to_string(),
            }));
        }

        // 2. Parse UUID
        let message_id = match Uuid::parse_str(&req.message_id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return Ok(Response::new(WriteDmResponse {
                    success: false,
                    error_message: "Invalid message_id UUID".to_string(),
                }));
            }
        };

        // A self-conversation is not a meaningful thread and the lookup would
        // record the same user twice.
        if req.sender_id == req.receiver_id {
            return Ok(Response::new(WriteDmResponse {
                success: false,
                error_message: "Cannot message self".to_string(),
            }));
        }

        // 3. Resolve the conversation server-side from the authenticated
        //    sender and the recipient. The client no longer supplies a
        //    conversation id, so it cannot write into a thread it is not part
        //    of. Existing conversations keep their id via `dm_lookup`, so
        //    older threads are not split.
        let conversation_id = match getsert_conversation_id(
            &self.session,
            &req.project_id,
            &req.sender_id,
            &req.receiver_id,
        )
        .await
        {
            Ok((conversation_id, _)) => conversation_id,
            Err(e) => {
                tracing::error!("failed to resolve conversation for write: {e}");
                return Ok(Response::new(WriteDmResponse {
                    success: false,
                    error_message: "Internal error".to_string(),
                }));
            }
        };

        // 4. Construct the internal struct
        let message = crate::queries::DirectMessage {
            project_id: req.project_id,
            conversation_id,
            message_id,
            sender_id: req.sender_id,
            recipient_id: req.receiver_id, // Map receiver_id (proto) to recipient_id (db)
            message_text: req.message,
            created_at: CqlTimestamp(req.timestamp),
        };

        // 5. Execute Batch Write
        match write_direct_message(&self.session, message).await {
            Ok(_) => Ok(Response::new(WriteDmResponse {
                success: true,
                error_message: String::new(),
            })),
            Err(e) => {
                tracing::error!("failed to write direct message: {e}");
                Ok(Response::new(WriteDmResponse {
                    success: false,
                    error_message: "Internal error".to_string(),
                }))
            }
        }
    }

    async fn fetch_user_conversations(
        &self,
        request: Request<FetchUserConversationsRequest>,
    ) -> Result<Response<FetchUserConversationsResponse>, Status> {
        let req = request.into_inner();

        let tenant_id = match req.tenant_user_id {
            Some(id) => id,
            None => {
                return Ok(Response::new(FetchUserConversationsResponse {
                    success: false,
                    error_message: "Missing tenant_user_id".to_string(),
                    conversations: Vec::new(),
                }));
            }
        };

        if tenant_id.project_id.is_empty() || tenant_id.user_id.is_empty() {
            return Ok(Response::new(FetchUserConversationsResponse {
                success: false,
                error_message: "project_id and user_id are required".to_string(),
                conversations: Vec::new(),
            }));
        }

        match fetch_user_conversations(&self.session, &tenant_id.project_id, &tenant_id.user_id)
            .await
        {
            Ok(conversations_data) => {
                let conversations: Vec<UserConversation> = conversations_data
                    .into_iter()
                    .map(|(conversation_id, last_message)| UserConversation {
                        conversation_id,
                        last_message: last_message.0,
                    })
                    .collect();

                let response = FetchUserConversationsResponse {
                    success: true,
                    error_message: String::new(),
                    conversations,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                // ... error handling
                let response = FetchUserConversationsResponse {
                    success: false,
                    error_message: internal_error("fetch_user_conversations failed", &e),
                    conversations: Vec::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn fetch_conversation_history(
        &self,
        request: Request<FetchConversationHistoryRequest>,
    ) -> Result<Response<FetchConversationHistoryResponse>, Status> {
        let req = request.into_inner();

        if req.project_id.is_empty() || req.conversation_id.is_empty() {
            return Ok(Response::new(FetchConversationHistoryResponse {
                success: false,
                error_message: "project_id and conversation_id are required".to_string(),
                messages: Vec::new(),
            }));
        }

        if let Err(reason) = authorize_conversation(
            &self.session,
            &req.project_id,
            &req.conversation_id,
            &req.caller_user_id,
        )
        .await
        {
            return Ok(Response::new(FetchConversationHistoryResponse {
                success: false,
                error_message: reason,
                messages: Vec::new(),
            }));
        }

        match fetch_conversation_history(&self.session, &req.project_id, &req.conversation_id).await
        {
            Ok(messages_data) => {
                let messages: Vec<ConversationMessage> = messages_data
                    .into_iter()
                    .map(
                        |(message_id, message_text, sender_id, recipient_id, created_at)| {
                            ConversationMessage {
                                message_id: message_id.to_string(),
                                message_text,
                                sender_id,
                                recipient_id,
                                created_at: created_at.0,
                            }
                        },
                    )
                    .collect();

                let response = FetchConversationHistoryResponse {
                    success: true,
                    error_message: String::new(),
                    messages,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = FetchConversationHistoryResponse {
                    success: false,
                    error_message: internal_error("fetch_conversation_history failed", &e),
                    messages: Vec::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn get_paginated_messages(
        &self,
        request: Request<GetPaginatedMessagesRequest>,
    ) -> Result<Response<GetPaginatedMessagesResponse>, Status> {
        let req = request.into_inner();

        let cursor = if req.cursor_message_id.trim().is_empty() {
            None
        } else {
            match Uuid::parse_str(&req.cursor_message_id) {
                Ok(uuid) => Some(uuid),
                Err(_) => {
                    return Ok(Response::new(GetPaginatedMessagesResponse {
                        success: false,
                        error_message: "Invalid cursor_message_id".to_string(),
                        messages: Vec::new(),
                        next_cursor: String::new(),
                    }));
                }
            }
        };

        if req.project_id.is_empty() || req.conversation_id.is_empty() {
            return Ok(Response::new(GetPaginatedMessagesResponse {
                success: false,
                error_message: "project_id and conversation_id are required".to_string(),
                messages: Vec::new(),
                next_cursor: String::new(),
            }));
        }

        if let Err(reason) = authorize_conversation(
            &self.session,
            &req.project_id,
            &req.conversation_id,
            &req.caller_user_id,
        )
        .await
        {
            return Ok(Response::new(GetPaginatedMessagesResponse {
                success: false,
                error_message: reason,
                messages: Vec::new(),
                next_cursor: String::new(),
            }));
        }

        match fetch_paginated_messages(&self.session, &req.project_id, &req.conversation_id, cursor)
            .await
        {
            Ok(messages_data) => {
                // Get next cursor from last message
                let next_cursor = messages_data
                    .last()
                    .map(|m| m.message_id.to_string())
                    .unwrap_or_default();

                // Map DB result to Proto message
                let messages: Vec<crate::chat_service::DirectMessage> = messages_data
                    .into_iter()
                    .map(|m| crate::chat_service::DirectMessage {
                        conversation_id: m.conversation_id,
                        message_id: m.message_id.to_string(),
                        sender_id: m.sender_id,
                        recipient_id: m.recipient_id,
                        message_text: m.message_text,
                        created_at: m.created_at.0,
                    })
                    .collect();

                let response = GetPaginatedMessagesResponse {
                    success: true,
                    error_message: String::new(),
                    messages,
                    next_cursor,
                };

                Ok(Response::new(response))
            }
            Err(e) => {
                let response = GetPaginatedMessagesResponse {
                    success: false,
                    error_message: internal_error("get_paginated_messages failed", &e),
                    messages: Vec::new(),
                    next_cursor: String::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn write_room_message(
        &self,
        request: Request<WriteRoomMessageRequest>,
    ) -> Result<Response<WriteRoomMessageResponse>, Status> {
        let req = request.into_inner();

        // 1. Validate Inputs
        if !within_id_bound(&req.project_id)
            || !within_id_bound(&req.room_id)
            || !within_id_bound(&req.sender_id)
        {
            return Ok(Response::new(WriteRoomMessageResponse {
                success: false,
                error_message: "project_id and room_id are required".to_string(),
            }));
        }

        if !within_content_bound(&req.content) {
            return Ok(Response::new(WriteRoomMessageResponse {
                success: false,
                error_message: "message is too long".to_string(),
            }));
        }

        // 2. Parse UUID
        let message_id = match Uuid::parse_str(&req.message_id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return Ok(Response::new(WriteRoomMessageResponse {
                    success: false,
                    error_message: "Invalid message_id UUID".to_string(),
                }));
            }
        };

        // 3. Construct DB Struct
        let message = DbRoomMessageEx {
            project_id: req.project_id,
            room_id: req.room_id,
            message_id,
            sender_id: req.sender_id,
            content: req.content,
            created_at: CqlTimestamp(req.timestamp),
        };

        // 4. Execute Batch
        match write_room_message(&self.session, message).await {
            Ok(_) => Ok(Response::new(WriteRoomMessageResponse {
                success: true,
                error_message: String::new(),
            })),
            Err(e) => Ok(Response::new(WriteRoomMessageResponse {
                success: false,
                error_message: internal_error("write_room_message failed", &e),
            })),
        }
    }

    async fn get_paginated_room_messages(
        &self,
        request: Request<GetPaginatedRoomMessagesRequest>,
    ) -> Result<Response<GetPaginatedRoomMessagesResponse>, Status> {
        let req = request.into_inner();

        let cursor = if req.cursor_message_id.trim().is_empty() {
            None
        } else {
            match Uuid::parse_str(&req.cursor_message_id) {
                Ok(uuid) => Some(uuid),
                Err(_) => {
                    return Ok(Response::new(GetPaginatedRoomMessagesResponse {
                        success: false,
                        error_message: "Invalid cursor_message_id".to_string(),
                        messages: Vec::new(),
                        next_cursor: String::new(),
                    }));
                }
            }
        };

        if req.project_id.is_empty() || req.room_id.is_empty() {
            return Ok(Response::new(GetPaginatedRoomMessagesResponse {
                success: false,
                error_message: "project_id and room_id are required".to_string(),
                messages: Vec::new(),
                next_cursor: String::new(),
            }));
        }

        match fetch_paginated_room_messages(&self.session, &req.project_id, &req.room_id, cursor)
            .await
        {
            Ok(messages_data) => {
                let next_cursor = messages_data
                    .last()
                    .map(|m| m.message_id.to_string())
                    .unwrap_or_default();

                let messages: Vec<RoomMessage> = messages_data
                    .into_iter()
                    .map(|m| RoomMessage {
                        room_id: m.room_id,
                        message_id: m.message_id.to_string(),
                        sender_id: m.sender_id,
                        content: m.content,
                        created_at: m.created_at.0,
                    })
                    .collect();

                let response = GetPaginatedRoomMessagesResponse {
                    success: true,
                    error_message: String::new(),
                    messages,
                    next_cursor,
                };

                Ok(Response::new(response))
            }
            Err(e) => {
                let response = GetPaginatedRoomMessagesResponse {
                    success: false,
                    error_message: internal_error("get_paginated_room_messages failed", &e),
                    messages: Vec::new(),
                    next_cursor: String::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn sync_messages(
        &self,
        request: Request<SyncMessagesRequest>,
    ) -> Result<Response<SyncMessagesResponse>, Status> {
        let req = request.into_inner();

        // 1. Parse UUID
        let last_message_id = match Uuid::parse_str(&req.last_message_id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return Ok(Response::new(SyncMessagesResponse {
                    success: false,
                    error_message: "Invalid last_message_id".to_string(),
                    messages: Vec::new(),
                }));
            }
        };

        // 2. Validate Inputs
        if req.project_id.is_empty() || req.conversation_id.is_empty() {
            return Ok(Response::new(SyncMessagesResponse {
                success: false,
                error_message: "project_id and conversation_id are required".to_string(),
                messages: Vec::new(),
            }));
        }

        if let Err(reason) = authorize_conversation(
            &self.session,
            &req.project_id,
            &req.conversation_id,
            &req.caller_user_id,
        )
        .await
        {
            return Ok(Response::new(SyncMessagesResponse {
                success: false,
                error_message: reason,
                messages: Vec::new(),
            }));
        }

        // 3. Fetch Messages
        match fetch_messages_after(
            &self.session,
            Arc::clone(&self.queries),
            &req.project_id, // Pass project_id
            &req.conversation_id,
            last_message_id,
        )
        .await
        {
            Ok(messages_data) => {
                let messages: Vec<crate::chat_service::DirectMessage> = messages_data
                    .into_iter()
                    .map(|m| crate::chat_service::DirectMessage {
                        conversation_id: m.conversation_id,
                        message_id: m.message_id.to_string(),
                        sender_id: m.sender_id,
                        recipient_id: m.recipient_id,
                        message_text: m.message_text,
                        created_at: m.created_at.0,
                    })
                    .collect();

                let response = SyncMessagesResponse {
                    success: true,
                    error_message: String::new(),
                    messages,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = SyncMessagesResponse {
                    success: false,
                    error_message: internal_error("sync_messages failed", &e),
                    messages: Vec::new(),
                };
                Ok(Response::new(response))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_participants_may_resolve_a_conversation() {
        assert!(caller_is_participant("alice", "alice", "bob"));
        assert!(caller_is_participant("bob", "alice", "bob"));
    }

    #[test]
    fn third_parties_and_empty_callers_are_rejected() {
        assert!(!caller_is_participant("carol", "alice", "bob"));
        assert!(!caller_is_participant("", "alice", "bob"));
    }

    #[test]
    fn identifiers_and_content_are_bounded() {
        assert!(within_id_bound("user-1"));
        assert!(!within_id_bound(""));
        assert!(!within_id_bound(&"a".repeat(MAX_ID_LEN + 1)));

        assert!(within_content_bound("hello"));
        assert!(within_content_bound(&"a".repeat(MAX_CONTENT_CHARS)));
        assert!(!within_content_bound(&"a".repeat(MAX_CONTENT_CHARS + 1)));
    }
}
