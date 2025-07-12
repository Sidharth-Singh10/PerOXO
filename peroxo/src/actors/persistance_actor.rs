use tokio::sync::{mpsc, oneshot};
use tonic::Request;
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

use crate::{
    actors::chat_service::{
        GetPaginatedMessagesRequest, WriteDmRequest, WriteDmResponse,
        chat_service_client::ChatServiceClient,
    },
    chat::ResponseDirectMessage,
};

#[derive(Debug)]
pub enum PersistenceMessage {
    PersistDirectMessage {
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    GetPaginatedMessages {
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}

pub struct PersistenceActor {
    receiver: mpsc::UnboundedReceiver<PersistenceMessage>,
    chat_service_client: ChatServiceClient<Channel>,
}

impl PersistenceActor {
    pub async fn new(
        chat_service_client: ChatServiceClient<Channel>,
    ) -> Result<(Self, mpsc::UnboundedSender<PersistenceMessage>), Box<dyn std::error::Error>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let actor = Self {
            receiver,
            chat_service_client,
        };

        Ok((actor, sender))
    }

    pub async fn run(mut self) {
        info!("Persistence actor started");

        while let Some(message) = self.receiver.recv().await {
            match message {
                PersistenceMessage::PersistDirectMessage {
                    sender_id,
                    receiver_id,
                    message_content,
                    message_id,
                    timestamp,
                    respond_to,
                } => {
                    let result = self
                        .handle_persist_direct_message(
                            sender_id,
                            receiver_id,
                            message_content,
                            message_id,
                            timestamp,
                        )
                        .await;

                    let _ = respond_to.send(result);
                }
                PersistenceMessage::GetPaginatedMessages {
                    message_id,
                    conversation_id,
                    respond_to,
                } => {
                    let result = self
                        .handle_get_paginated_messages(message_id, conversation_id)
                        .await;
                    let _ = respond_to.send(result);
                }
            }
        }

        info!("Persistence actor stopped");
    }

    async fn handle_persist_direct_message(
        &mut self,
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
    ) -> Result<(), String> {
        // Make the gRPC call with retry logic
        match self
            .write_dm_with_retry(
                sender_id,
                receiver_id,
                message_content,
                message_id,
                timestamp,
                3,
            )
            .await
        {
            Ok(response) => {
                let write_dm_response = response.into_inner();
                if write_dm_response.success {
                    debug!(
                        "Successfully persisted message from {} to {}",
                        sender_id, receiver_id
                    );
                    Ok(())
                } else {
                    error!(
                        "Failed to persist message: {}",
                        write_dm_response.error_message
                    );
                    Err(write_dm_response.error_message)
                }
            }
            Err(e) => {
                error!("gRPC call failed: {}", e);
                Err(format!("gRPC call failed: {}", e))
            }
        }
    }

    async fn write_dm_with_retry(
        &mut self,
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        message_id: uuid::Uuid,
        timestamp: i64,
        max_retries: u32,
    ) -> Result<tonic::Response<WriteDmResponse>, tonic::Status> {
        let mut attempts = 0;
        let mut last_error = None;

        // Maybe a Better Retry Logic
        while attempts <= max_retries {
            let request = Request::new(WriteDmRequest {
                sender_id,
                receiver_id,
                message: message_content.clone(),
                message_id: message_id.to_string(),
                timestamp,
            });

            match self.chat_service_client.write_dm(request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);

                    if attempts <= max_retries {
                        let delay = std::time::Duration::from_millis(100 * attempts as u64);
                        warn!(
                            "gRPC call failed (attempt {}/{}), retrying in {:?}: {}",
                            attempts,
                            max_retries + 1,
                            delay,
                            last_error.as_ref().unwrap()
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    async fn handle_get_paginated_messages(
        &mut self,
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
    ) -> Result<PaginatedMessagesResponse, String> {
        let cursor_message_id = message_id.map(|id| id.to_string()).unwrap_or_default();

        let request = Request::new(GetPaginatedMessagesRequest {
            conversation_id,
            cursor_message_id,
        });

        match self
            .chat_service_client
            .get_paginated_messages(request)
            .await
        {
            Ok(response) => {
                let get_paginated_response = response.into_inner();
                if get_paginated_response.success {
                    let messages: Vec<ResponseDirectMessage> = get_paginated_response
                        .messages
                        .into_iter()
                        .map(|msg| ResponseDirectMessage {
                            conversation_id: msg.conversation_id,
                            message_id: uuid::Uuid::parse_str(&msg.message_id)
                                .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                            sender_id: msg.sender_id,
                            recipient_id: msg.recipient_id,
                            message_text: msg.message_text,
                            created_at: msg.created_at,
                        })
                        .collect::<Vec<ResponseDirectMessage>>();

                    let next_cursor = if get_paginated_response.next_cursor.is_empty() {
                        None
                    } else {
                        Some(get_paginated_response.next_cursor)
                    };

                    let has_more = next_cursor.is_some();

                    debug!("Successfully fetched {} paginated messages", messages.len());
                    Ok(PaginatedMessagesResponse {
                        messages,
                        next_cursor,
                        has_more,
                    })
                } else {
                    error!(
                        "Failed to fetch paginated messages: {}",
                        get_paginated_response.error_message
                    );
                    Err(get_paginated_response.error_message)
                }
            }
            Err(e) => {
                error!("gRPC call failed: {}", e);
                Err(format!("gRPC call failed: {}", e))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct PaginatedMessagesResponse {
    pub messages: Vec<ResponseDirectMessage>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
