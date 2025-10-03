use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use super::router::MessageRouter;
use crate::chat::ChatMessage;

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::{MessageAckResponse, MessageStatus, PaginatedMessagesResponse};

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::persistance_actor::PersistenceMessage;

impl MessageRouter {
    pub async fn handle_register_user(
        &mut self,
        user_id: i32,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        if self.users.contains_key(&user_id) {
            let _ = respond_to.send(Err("User already online".to_string()));
            return;
        }

        self.users.insert(user_id, sender);
        self.online_users.push(user_id);

        debug!("User {} registered successfully", user_id);

        // Broadcast presence update to all users
        // self.broadcast_presence_update(user_id, PresenceStatus::Online)
        //     .await;

        let _ = respond_to.send(Ok(()));
    }

    pub async fn handle_unregister_user(&mut self, user_id: i32) {
        if self.users.remove(&user_id).is_some() {
            self.online_users.retain(|u| u != &user_id);

            // Broadcast presence update to all users
            // self.broadcast_presence_update(user_id, PresenceStatus::Offline)
            //     .await;
        }
    }

    pub async fn handle_direct_message(
        &self,
        from: i32,
        to: i32,
        content: String,
        message_id: uuid::Uuid,
        #[cfg(any(feature = "mongo_db", feature = "persistence"))] respond_to: Option<
            oneshot::Sender<MessageAckResponse>,
        >,
    ) {
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        {
            if let Some(persistence_sender) = &self.persistence_sender {
                let (persist_respond_to, persist_response) = oneshot::channel();

                let persist_msg = PersistenceMessage::PersistDirectMessage {
                    sender_id: from,
                    receiver_id: to,
                    message_content: content.clone(),
                    message_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    respond_to: persist_respond_to,
                };

                if let Err(e) = persistence_sender.send(persist_msg) {
                    tracing::error!("Failed to send message to persistence actor: {}", e);
                    if let Some(responder) = respond_to {
                        let _ = responder.send(MessageAckResponse {
                            message_id,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            status: MessageStatus::Failed(format!("Persistence error: {}", e)),
                        });
                    }
                    return;
                }

                // Handle persistence response in background -- look into this later
                if let Some(responder) = respond_to {
                    tokio::spawn(async move {
                        match persist_response.await {
                            Ok(Ok(())) => {
                                let _ = responder.send(MessageAckResponse {
                                    message_id,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    status: MessageStatus::Persisted,
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = responder.send(MessageAckResponse {
                                    message_id,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    status: MessageStatus::Failed(e),
                                });
                            }
                            Err(_) => {
                                let _ = responder.send(MessageAckResponse {
                                    message_id,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    status: MessageStatus::Failed(
                                        "Persistence timeout".to_string(),
                                    ),
                                });
                            }
                        }
                    });
                }
            }
        }

        if let Some(recipient_sender) = self.users.get(&to) {
            let to_clone = to;
            let message = ChatMessage::DirectMessage {
                from,
                to,
                content,
                message_id,
            };

            // Use try_send to avoid blocking if the recipient's channel is full
            match recipient_sender.try_send(message) {
                Ok(()) => {
                    debug!("Message sent successfully to {}", to_clone);
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    debug!(
                        "Recipient {} message queue is full, dropping message",
                        to_clone
                    );
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    debug!("Recipient {} channel is closed", to_clone);
                }
            }
        } else {
            debug!("User {} not found or offline", to);
        }
    }

    // async fn broadcast_presence_update(&self, user_id: i32, status: PresenceStatus) {
    //     let message = ChatMessage::Presence {
    //         user: user_id,
    //         status,
    //     };

    //     for sender in self.users.values() {
    //         // Use try_send for presence updates to avoid blocking
    //         let _ = sender.try_send(message.clone());
    //     }
    // }

    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    pub async fn handle_get_paginated_chat_history(
        &self,
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    ) {
        if let Some(persistence_sender) = &self.persistence_sender {
            let (persist_respond_to, persist_response) = oneshot::channel();
            let persist_msg = PersistenceMessage::GetPaginatedMessages {
                message_id,
                conversation_id,
                respond_to: persist_respond_to,
            };

            if persistence_sender.send(persist_msg).is_err() {
                let _ = respond_to.send(Err("Failed to communicate with persistence".to_string()));
                return;
            }

            tokio::spawn(async move {
                match persist_response.await {
                    Ok(result) => {
                        let _ = respond_to.send(result);
                    }
                    Err(_) => {
                        let _ = respond_to.send(Err("Persistence timeout".to_string()));
                    }
                }
            });
        }
    }
}
