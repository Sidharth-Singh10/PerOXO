use crate::actors::{message_router::RouterMessage, user_session::handlers};
use crate::chat::ChatMessage;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};

pub struct UserSession {
    user_id: i32,
    socket: WebSocket,
    router_sender: mpsc::UnboundedSender<RouterMessage>,
    session_receiver: mpsc::Receiver<ChatMessage>,
    session_sender: mpsc::Sender<ChatMessage>,
}

impl UserSession {
    pub async fn new(
        user_id: i32,
        socket: WebSocket,
        router_sender: mpsc::UnboundedSender<RouterMessage>,
    ) -> Result<Self, String> {
        const CHANNEL_BUFFER_SIZE: usize = 100;
        let (session_sender, session_receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        // Register with the message router
        let (respond_to, response) = oneshot::channel();
        let register_msg = RouterMessage::RegisterUser {
            user_id,
            sender: session_sender.clone(),
            respond_to,
        };

        if router_sender.send(register_msg).is_err() {
            return Err("Failed to communicate with message router".to_string());
        }

        match response.await {
            Ok(Ok(())) => {
                debug!("User {} registered successfully", user_id);
            }
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {
                return Err("Router response channel closed".to_string());
            }
        }

        Ok(Self {
            user_id,
            socket,
            router_sender,
            session_receiver,
            session_sender,
        })
    }

    pub async fn run(self) {
        let (mut ws_sender, mut ws_receiver) = self.socket.split();
        let user_id = self.user_id;
        let router_sender = self.router_sender.clone();
        let session_sender_for_rooms = self.session_sender.clone();
        let mut session_receiver = self.session_receiver;

        let (ack_sender, mut ack_receiver) = mpsc::channel::<ChatMessage>(100);
        // Task to handle outgoing messages (from session to WebSocket)
        let user_id_clone = user_id;
        let mut send_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Handle regular messages from session_receiver
                    message = session_receiver.recv() => {
                        match message {
                            Some(msg) => {
                                match serde_json::to_string(&msg) {
                                    Ok(json) => {
                                        if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                            debug!(
                                                "WebSocket send failed for user {}, likely disconnected",
                                                user_id_clone
                                            );
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to serialize message for {}: {}", user_id_clone, e);
                                    }
                                }
                            }
                            None => break, // Channel closed
                        }
                    }
                    // Handle acknowledgment messages
                    ack_message = ack_receiver.recv() => {
                        match ack_message {
                            Some(msg) => {
                                match serde_json::to_string(&msg) {
                                    Ok(json) => {
                                        if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                            debug!(
                                                "WebSocket send failed for user {}, likely disconnected",
                                                user_id_clone
                                            );
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to serialize ack message for {}: {}", user_id_clone, e);
                                    }
                                }
                            }
                            None => break, // Channel closed
                        }
                    }
                }
            }
        });

        // Task to handle incoming messages (from WebSocket to router)
        let user_id_clone = user_id;
        let router_sender_clone = router_sender.clone();

        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(Message::Text(text))) = ws_receiver.next().await {
                match serde_json::from_str::<ChatMessage>(&text) {
                    Ok(ChatMessage::DirectMessage {
                        from,
                        to,
                        content,
                        message_id: _,
                    }) => {
                        if let Err(e) = handlers::handle_direct_message(
                            user_id_clone,
                            from,
                            to,
                            content,
                            &router_sender_clone,
                            &ack_sender,
                        )
                        .await
                        {
                            error!("Failed to handle direct message: {}", e);
                        }
                    }
                    #[cfg(feature = "persistence")]
                    Ok(ChatMessage::GetPaginatedMessages {
                        message_id,
                        conversation_id,
                    }) => {
                        let (respond_to, response) = oneshot::channel();
                        let router_msg = RouterMessage::GetPaginatedMessages {
                            message_id,
                            conversation_id,
                            respond_to,
                        };

                        if router_sender_clone.send(router_msg).is_err() {
                            error!("Failed to send chat history request to router");
                        } else {
                            let ack_sender_clone = ack_sender.clone();
                            tokio::spawn(async move {
                                match response.await {
                                    Ok(Ok(paginated_response)) => {
                                        let response_msg = ChatMessage::ChatHistoryResponse {
                                            messages: paginated_response.messages,
                                            has_more: paginated_response.has_more,
                                            next_cursor: paginated_response.next_cursor.and_then(
                                                |cursor| uuid::Uuid::parse_str(&cursor).ok(),
                                            ),
                                        };
                                        let _ = ack_sender_clone.send(response_msg).await;
                                    }
                                    Ok(Err(e)) => {
                                        error!("Failed to get chat history: {}", e);
                                    }
                                    Err(_) => {
                                        error!("Chat history request timeout");
                                    }
                                }
                            });
                        }
                    }

                    Ok(ChatMessage::RoomMessage {
                        room_id,
                        from,
                        content,
                        message_id,
                    }) => {
                        if let Err(e) = handlers::handle_room_message(
                            user_id_clone,
                            room_id,
                            from,
                            content,
                            message_id,
                            &router_sender_clone,
                            &ack_sender,
                        )
                        .await
                        {
                            error!("Failed to handle room message: {}", e);
                        }
                    }
                    Ok(ChatMessage::JoinRoom { room_id }) => {
                        let (respond_to, _) = oneshot::channel();
                        let router_msg = RouterMessage::JoinRoom {
                            user_id: user_id_clone,
                            room_id,
                            sender: session_sender_for_rooms.clone(),
                            respond_to,
                        };

                        if router_sender_clone.send(router_msg).is_err() {
                            error!("Failed to send join room request to router");
                        }
                    }

                    #[cfg(feature = "persistence")]
                    Ok(ChatMessage::SyncMessages {
                        conversation_id,
                        message_id,
                    }) => {
                        let (respond_to, response) = oneshot::channel();
                        let router_msg = RouterMessage::SyncMessages {
                            conversation_id,
                            message_id,
                            respond_to,
                        };

                        if router_sender_clone.send(router_msg).is_err() {
                            error!("Failed to send sync messages request to router");
                        } else {
                            let ack_sender_clone = ack_sender.clone();
                            tokio::spawn(async move {
                                match response.await {
                                    Ok(Ok(messages)) => {
                                        let response_msg =
                                            ChatMessage::SyncMessagesResponse { messages };
                                        let _ = ack_sender_clone.send(response_msg).await;
                                    }
                                    Ok(Err(e)) => {
                                        error!("Failed to sync messages: {}", e);
                                    }
                                    Err(_) => {
                                        error!("Sync messages request timeout");
                                    }
                                }
                            });
                        }
                    }
                    Ok(_) => {
                        // Ignore other message types from clients for now
                    }
                    Err(e) => {
                        error!("Failed to parse message from {}: {}", user_id_clone, e);
                    }
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = &mut send_task => {
                debug!("Send task completed for user {}", user_id);
                recv_task.abort();
            }
            _ = &mut recv_task => {
                debug!("Receive task completed for user {}", user_id);
                send_task.abort();
            }
        }

        // Unregister from router
        let unregister_msg = RouterMessage::UnregisterUser { user_id };
        let _ = router_sender.send(unregister_msg);

        debug!("User session ended for {}", user_id);
    }
}
