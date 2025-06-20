use crate::actors::message_router::RouterMessage;
use crate::chat::ChatMessage;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, warn};

pub struct UserSession {
    username: String,
    socket: WebSocket,
    router_sender: mpsc::UnboundedSender<RouterMessage>,
    session_receiver: mpsc::Receiver<ChatMessage>,
}

impl UserSession {
    pub async fn new(
        username: String,
        socket: WebSocket,
        router_sender: mpsc::UnboundedSender<RouterMessage>,
    ) -> Result<Self, String> {
        const CHANNEL_BUFFER_SIZE: usize = 100;
        let (session_sender, session_receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        // Register with the message router
        let (respond_to, response) = oneshot::channel();
        let register_msg = RouterMessage::RegisterUser {
            username: username.clone(),
            sender: session_sender,
            respond_to,
        };

        if router_sender.send(register_msg).is_err() {
            return Err("Failed to communicate with message router".to_string());
        }

        match response.await {
            Ok(Ok(())) => {
                debug!("User {} registered successfully", username);
            }
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {
                return Err("Router response channel closed".to_string());
            }
        }

        Ok(Self {
            username,
            socket,
            router_sender,
            session_receiver,
        })
    }

    pub async fn run(self) {
        let (mut ws_sender, mut ws_receiver) = self.socket.split();
        let username = self.username.clone();
        let router_sender = self.router_sender.clone();
        let mut session_receiver = self.session_receiver;

        // Task to handle outgoing messages (from session to WebSocket)
        let username_clone = username.clone();
        let mut send_task = tokio::spawn(async move {
            while let Some(message) = session_receiver.recv().await {
                match serde_json::to_string(&message) {
                    Ok(json) => {
                        if ws_sender.send(Message::Text(json.into())).await.is_err() {
                            debug!(
                                "WebSocket send failed for user {}, likely disconnected",
                                username_clone
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to serialize message for {}: {}", username_clone, e);
                    }
                }
            }
        });

        // Task to handle incoming messages (from WebSocket to router)
        let username_clone = username.clone();
        let router_sender_clone = router_sender.clone();
        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(Message::Text(text))) = ws_receiver.next().await {
                match serde_json::from_str::<ChatMessage>(&text) {
                    Ok(ChatMessage::DirectMessage { from, to, content }) => {
                        // Validate sender
                        if from != username_clone {
                            warn!(
                                "User {} attempted to spoof message from {}",
                                username_clone, from
                            );
                            continue;
                        }

                        let router_msg = RouterMessage::SendDirectMessage { from, to, content };

                        if router_sender_clone.send(router_msg).is_err() {
                            error!("Failed to send message to router");
                            break;
                        }
                    }
                    Ok(_) => {
                        // Ignore other message types from clients for now
                    }
                    Err(e) => {
                        warn!("Failed to parse message from {}: {}", username_clone, e);
                    }
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = &mut send_task => {
                debug!("Send task completed for user {}", username);
                recv_task.abort();
            }
            _ = &mut recv_task => {
                debug!("Receive task completed for user {}", username);
                send_task.abort();
            }
        }

        // Unregister from router
        let unregister_msg = RouterMessage::UnregisterUser {
            username: username.clone(),
        };
        let _ = router_sender.send(unregister_msg);

        debug!("User session ended for {}", username);
    }
}
