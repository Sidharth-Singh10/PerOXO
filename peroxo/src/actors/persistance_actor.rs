use tokio::sync::{mpsc, oneshot};
use tonic::Request;
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

use crate::actors::chat_service::{
    WriteDmRequest, WriteDmResponse, chat_service_client::ChatServiceClient,
};

// use crate::actors::persistance_actor::chat_service::{
//     WriteDmRequest, WriteDmResponse, chat_service_client::ChatServiceClient,
// };

#[derive(Debug)]
pub enum PersistenceMessage {
    PersistDirectMessage {
        sender_id: i32,
        receiver_id: i32,
        message_content: String,
        respond_to: Option<oneshot::Sender<Result<(), String>>>,
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
                    respond_to,
                } => {
                    let result = self
                        .handle_persist_direct_message(sender_id, receiver_id, message_content)
                        .await;

                    // Send response back if requested
                    if let Some(responder) = respond_to {
                        let _ = responder.send(result);
                    }
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
    ) -> Result<(), String> {
        // Make the gRPC call with retry logic
        match self
            .write_dm_with_retry(sender_id, receiver_id, message_content, 3)
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
}
