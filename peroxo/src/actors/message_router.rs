use crate::{
    actors::persistance_actor::{PaginatedMessagesResponse, PersistenceMessage},
    chat::{ChatMessage, MessageAckResponse, MessageStatus, PresenceStatus},
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

#[derive(Debug)]
pub enum RouterMessage {
    RegisterUser {
        user_id: i32,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    UnregisterUser {
        user_id: i32,
    },
    SendDirectMessage {
        from: i32,
        to: i32,
        content: String,
        message_id: uuid::Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    },
    GetOnlineUsers {
        respond_to: oneshot::Sender<Vec<i32>>,
    },
    GetChatHistory {
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}

pub struct MessageRouter {
    receiver: mpsc::UnboundedReceiver<RouterMessage>,
    users: HashMap<i32, mpsc::Sender<ChatMessage>>,
    online_users: Vec<i32>,
    persistence_sender: Option<mpsc::UnboundedSender<PersistenceMessage>>,
}

impl MessageRouter {
    pub fn new(
        persistence_sender: mpsc::UnboundedSender<PersistenceMessage>,
    ) -> (Self, mpsc::UnboundedSender<RouterMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let router = Self {
            receiver,
            users: HashMap::new(),
            online_users: Vec::new(),
            persistence_sender: Some(persistence_sender),
        };

        (router, sender)
    }

    pub async fn run(mut self) {
        info!("Message router started");

        while let Some(message) = self.receiver.recv().await {
            match message {
                RouterMessage::RegisterUser {
                    user_id,
                    sender,
                    respond_to,
                } => {
                    self.handle_register_user(user_id, sender, respond_to).await;
                }
                RouterMessage::UnregisterUser { user_id } => {
                    self.handle_unregister_user(user_id).await;
                }
                RouterMessage::SendDirectMessage {
                    from,
                    to,
                    content,
                    message_id,
                    respond_to,
                } => {
                    self.handle_direct_message(from, to, content, message_id, respond_to)
                        .await;
                }
                RouterMessage::GetOnlineUsers { respond_to } => {
                    let _ = respond_to.send(self.online_users.clone());
                }
                RouterMessage::GetChatHistory {
                    message_id,
                    conversation_id,
                    respond_to,
                } => {
                    self.handle_get_chat_history(message_id, conversation_id, respond_to)
                        .await;
                }
            }
        }

        info!("Message router stopped");
    }

    async fn handle_register_user(
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
        self.broadcast_presence_update(user_id, PresenceStatus::Online)
            .await;

        let _ = respond_to.send(Ok(()));
    }

    async fn handle_unregister_user(&mut self, user_id: i32) {
        if self.users.remove(&user_id).is_some() {
            self.online_users.retain(|u| u != &user_id);

            // Broadcast presence update to all users
            self.broadcast_presence_update(user_id, PresenceStatus::Offline)
                .await;
        }
    }

    async fn handle_direct_message(
        &self,
        from: i32,
        to: i32,
        content: String,
        message_id: uuid::Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    ) {
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
                                status: MessageStatus::Failed("Persistence timeout".to_string()),
                            });
                        }
                    }
                });
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

    async fn broadcast_presence_update(&self, user_id: i32, status: PresenceStatus) {
        let message = ChatMessage::Presence {
            user: user_id,
            status,
        };

        for sender in self.users.values() {
            // Use try_send for presence updates to avoid blocking
            let _ = sender.try_send(message.clone());
        }
    }

    async fn handle_get_chat_history(
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
