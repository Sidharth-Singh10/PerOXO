use queries::{DirectMessage, fetch_user_conversations, write_direct_message};
use scylla::client::session_builder::SessionBuilder;
use scylla::client::{execution_profile::ExecutionProfile, session::Session};
use scylla::statement::Consistency;
use scylla::value::CqlTimestamp;
use std::error::Error;

use tonic::{Request, Response, Status, transport::Server};
use uuid::Uuid;

mod queries;

// Include the generated proto code
pub mod chat_service {
    tonic::include_proto!("chat_service");
}

use crate::chat_service::ConversationMessage;
use crate::chat_service::FetchConversationHistoryRequest;
use crate::chat_service::FetchConversationHistoryResponse;
use crate::queries::fetch_conversation_history;
use chat_service::{
    FetchUserConversationsRequest, FetchUserConversationsResponse, UserConversation,
    WriteDmRequest, WriteDmResponse,
    chat_service_server::{ChatService, ChatServiceServer},
};

// gRPC service implementation
pub struct ChatServiceImpl {
    session: Session,
}

impl ChatServiceImpl {
    pub fn new(session: Session) -> Self {
        Self { session }
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

        // Write to database using your existing function
        match write_direct_message(&self.session, dm).await {
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
                    error_message: e.to_string(),
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

        // Fetch conversations using your existing function
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize Scylla session

    let profile = ExecutionProfile::builder()
        .consistency(Consistency::One)
        .build();

    let session: Session = SessionBuilder::new()
        .known_node("172.17.0.2:9042")
        .default_execution_profile_handle(profile.into_handle())
        .build()
        .await?;

    // Create the gRPC service
    let chat_service = ChatServiceImpl::new(session);
    let service = ChatServiceServer::new(chat_service);

    // gRPC server address
    let addr = "[::1]:50051".parse()?;

    println!("ChatService gRPC server listening on {}", addr);

    // Start the gRPC server
    Server::builder().add_service(service).serve(addr).await?;

    Ok(())
}

pub fn create_dm(sender_id: i32, recipient_id: i32, message_text: String) -> DirectMessage {
    // Sort user IDs to ensure consistent conversation_id format
    let (first_id, second_id) = if sender_id < recipient_id {
        (sender_id, recipient_id)
    } else {
        (recipient_id, sender_id)
    };

    // Create conversation_id in format "smaller_id_larger_id"
    let conversation_id = format!("{}_{}", first_id, second_id);

    // Generate a new UUID for the message
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
