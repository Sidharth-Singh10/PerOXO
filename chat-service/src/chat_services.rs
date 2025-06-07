use std::sync::Arc;

use scylla::client::session::Session;
use tonic::{Request, Response, Status};

use crate::{
    chat_service::{
        ConversationMessage, FetchConversationHistoryRequest, FetchConversationHistoryResponse,
        FetchUserConversationsRequest, FetchUserConversationsResponse, UserConversation,
        WriteDmRequest, WriteDmResponse, chat_service_server::ChatService,
    },
    queries::{create_dm, fetch_conversation_history, fetch_user_conversations},
    rabbit::{MessagePublisher, SerializableDirectMessage},
};

pub struct ChatServiceImpl {
    session: Session,
    publisher: Arc<MessagePublisher>,
}

impl ChatServiceImpl {
    pub fn new(session: Session, publisher: Arc<MessagePublisher>) -> Self {
        Self { session, publisher }
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
        let dm = create_dm(req.sender_id, req.receiver_id, req.message);
        let serializable_dm: SerializableDirectMessage = dm.into();

        match self.publisher.publish_message(&serializable_dm).await {
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
}
