use std::sync::Arc;

use scylla::client::session::Session;
use tonic::{Request, Response, Status};

use crate::chat_service::DirectMessage;
use crate::chat_service::GetPaginatedMessagesRequest;
use crate::chat_service::GetPaginatedMessagesResponse;
use crate::chat_service::GetPaginatedRoomMessagesRequest;
use crate::chat_service::GetPaginatedRoomMessagesResponse;
use crate::chat_service::RoomMessage;
use crate::chat_service::WriteRoomMessageRequest;
use crate::chat_service::WriteRoomMessageResponse;
use crate::queries::create_room_message;
use crate::queries::fetch_paginated_room_messages;
use crate::rabbit::SerializableRoomMessage;
use crate::{
    chat_service::{
        ConversationMessage, FetchConversationHistoryRequest, FetchConversationHistoryResponse,
        FetchUserConversationsRequest, FetchUserConversationsResponse, UserConversation,
        WriteDmRequest, WriteDmResponse, chat_service_server::ChatService,
    },
    queries::{
        create_dm, fetch_conversation_history, fetch_paginated_messages, fetch_user_conversations,
    },
    rabbit::{MessagePublisher, SerializableDirectMessage},
};
use uuid::Uuid;

pub struct ChatServiceImpl {
    session: Arc<Session>,
    dm_publisher: Arc<MessagePublisher>,
    room_publisher: Arc<MessagePublisher>,
}

impl ChatServiceImpl {
    pub fn new(
        session: Arc<Session>,
        dm_publisher: Arc<MessagePublisher>,
        room_publisher: Arc<MessagePublisher>,
    ) -> Self {
        Self {
            session,
            dm_publisher,
            room_publisher,
        }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServiceImpl {
    async fn write_dm(
        &self,
        request: Request<WriteDmRequest>,
    ) -> Result<Response<WriteDmResponse>, Status> {
        let req = request.into_inner();

        // Create the DirectMessage using your existing function
        let dm = match create_dm(
            req.sender_id,
            req.receiver_id,
            req.message,
            (req.message_id).as_str(),
            req.timestamp,
        ) {
            Ok(dm) => dm,
            Err(e) => {
                let response = WriteDmResponse {
                    success: false,
                    error_message: format!("Failed to create message: {}", e),
                };
                return Ok(Response::new(response));
            }
        };
        let serializable_dm: SerializableDirectMessage = dm.into();

        match self.dm_publisher.publish_message(&serializable_dm).await {
            Ok(()) => {
                let response = WriteDmResponse {
                    success: true,
                    error_message: String::new(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = WriteDmResponse {
                    success: false,
                    error_message: format!("Failed to queue message: {}", e),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn fetch_user_conversations(
        &self,
        request: Request<FetchUserConversationsRequest>,
    ) -> Result<Response<FetchUserConversationsResponse>, Status> {
        let req = request.into_inner();

        match fetch_user_conversations(&self.session, req.user_id).await {
            Ok(conversations_data) => {
                // Convert Vec<(String, CqlTimestamp)> to Vec<UserConversation>
                let conversations: Vec<UserConversation> = conversations_data
                    .into_iter()
                    .map(|(conversation_id, last_message)| UserConversation {
                        conversation_id,
                        last_message: last_message.0, // Extract i64 from CqlTimestamp
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
                let response = FetchUserConversationsResponse {
                    success: false,
                    error_message: e.to_string(),
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

        // Fetch conversation history using your existing function
        match fetch_conversation_history(&self.session, &req.conversation_id).await {
            Ok(messages_data) => {
                // Convert Vec<(Uuid, String, i32, i32)> to Vec<ConversationMessage>
                let messages: Vec<ConversationMessage> = messages_data
                    .into_iter()
                    .map(|(message_id, message_text, sender_id, recipient_id)| {
                        ConversationMessage {
                            message_id: message_id.to_string(), // Convert UUID to string
                            message_text,
                            sender_id,
                            recipient_id,
                        }
                    })
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
                    error_message: e.to_string(),
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

        match fetch_paginated_messages(&self.session, &req.conversation_id, cursor).await {
            Ok(messages_data) => {
                let next_cursor = messages_data
                    .last()
                    .map(|m| m.message_id.to_string())
                    .unwrap_or_default();

                let messages: Vec<DirectMessage> = messages_data
                    .into_iter()
                    .map(|m| DirectMessage {
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
                    error_message: e.to_string(),
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

        let rm = match create_room_message(
            req.room_id,
            req.from,
            req.content,
            req.message_id.as_str(),
            req.timestamp,
        ) {
            Ok(rm) => rm,
            Err(e) => {
                let response = WriteRoomMessageResponse {
                    success: false,
                    error_message: format!("Failed to create room message: {}", e),
                };
                return Ok(Response::new(response));
            }
        };
        let serializable_rm: SerializableRoomMessage = rm.into();

        match self.room_publisher.publish_message(&serializable_rm).await {
            Ok(()) => {
                let response = WriteRoomMessageResponse {
                    success: true,
                    error_message: String::new(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = WriteRoomMessageResponse {
                    success: false,
                    error_message: format!("Failed to queue room message: {}", e),
                };
                Ok(Response::new(response))
            }
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

        match fetch_paginated_room_messages(&self.session, &req.room_id, cursor).await {
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
                        from: m.from,
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
                    error_message: e.to_string(),
                    messages: Vec::new(),
                    next_cursor: String::new(),
                };
                Ok(Response::new(response))
            }
        }
    }
}
