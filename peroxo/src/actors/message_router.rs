use crate::chat::{ChatMessage, PresenceStatus};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

#[derive(Debug)]
pub enum RouterMessage {
    RegisterUser {
        username: String,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    UnregisterUser {
        username: String,
    },
    SendDirectMessage {
        from: String,
        to: String,
        content: String,
    },
    GetOnlineUsers {
        respond_to: oneshot::Sender<Vec<String>>,
    },
}

pub struct MessageRouter {
    receiver: mpsc::UnboundedReceiver<RouterMessage>,
    users: HashMap<String, mpsc::Sender<ChatMessage>>,
    online_users: Vec<String>,
}

impl MessageRouter {
    pub fn new() -> (Self, mpsc::UnboundedSender<RouterMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let router = Self {
            receiver,
            users: HashMap::new(),
            online_users: Vec::new(),
        };

        (router, sender)
    }

    pub async fn run(mut self) {
        info!("Message router started");

        while let Some(message) = self.receiver.recv().await {
            match message {
                RouterMessage::RegisterUser {
                    username,
                    sender,
                    respond_to,
                } => {
                    self.handle_register_user(username, sender, respond_to)
                        .await;
                }
                RouterMessage::UnregisterUser { username } => {
                    self.handle_unregister_user(username).await;
                }
                RouterMessage::SendDirectMessage { from, to, content } => {
                    self.handle_direct_message(from, to, content).await;
                }
                RouterMessage::GetOnlineUsers { respond_to } => {
                    let _ = respond_to.send(self.online_users.clone());
                }
            }
        }

        info!("Message router stopped");
    }

    async fn handle_register_user(
        &mut self,
        username: String,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        if self.users.contains_key(&username) {
            let _ = respond_to.send(Err("User already online".to_string()));
            return;
        }

        self.users.insert(username.clone(), sender);
        self.online_users.push(username.clone());

        // Broadcast presence update to all users
        self.broadcast_presence_update(username, PresenceStatus::Online)
            .await;

        let _ = respond_to.send(Ok(()));
    }

    async fn handle_unregister_user(&mut self, username: String) {
        if self.users.remove(&username).is_some() {
            self.online_users.retain(|u| u != &username);

            // Broadcast presence update to all users
            self.broadcast_presence_update(username, PresenceStatus::Offline)
                .await;
        }
    }

    async fn handle_direct_message(&self, from: String, to: String, content: String) {
        if let Some(recipient_sender) = self.users.get(&to) {
            let to_clone = to.clone();
            let message = ChatMessage::DirectMessage { from, to, content };

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

    async fn broadcast_presence_update(&self, username: String, status: PresenceStatus) {
        let message = ChatMessage::Presence {
            user: username,
            status,
        };

        for sender in self.users.values() {
            // Use try_send for presence updates to avoid blocking
            let _ = sender.try_send(message.clone());
        }
    }
}
