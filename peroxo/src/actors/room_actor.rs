use crate::chat::{ChatMessage, MessageAckResponse};
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::{actors::persistance_actor::PersistenceMessage, chat::PaginatedMessagesResponse};

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::MessageStatus;

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use tracing::error;

use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug)]
pub enum RoomMessage {
    AddMember {
        user_id: i32,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    RemoveMember {
        user_id: i32,
    },
    SendMessage {
        from: i32,
        content: String,
        message_id: Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    },
    GetMembers {
        respond_to: oneshot::Sender<Vec<i32>>,
    },
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    GetPaginatedMessages {
        message_id: Option<Uuid>,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}

pub struct RoomActor {
    room_id: String,
    receiver: mpsc::UnboundedReceiver<RoomMessage>,
    members: Arc<Mutex<HashMap<i32, mpsc::Sender<ChatMessage>>>>,
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    persistence_sender: Option<mpsc::UnboundedSender<PersistenceMessage>>,
}

impl RoomActor {
    pub fn new(
        room_id: String,
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        persistence_sender: mpsc::UnboundedSender<PersistenceMessage>,
    ) -> (Self, mpsc::UnboundedSender<RoomMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let actor = Self {
            room_id,
            receiver,
            members: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence_sender: Some(persistence_sender),
        };

        (actor, sender)
    }

    pub async fn run(mut self) {
        info!("Room actor started for room: {}", self.room_id);

        {
            let members = Arc::clone(&self.members);
            let room_id = self.room_id.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                info!("Started cleanup task for room {}", room_id);
                loop {
                    interval.tick().await;
                    let mut members_lock = members.lock().await;
                    let before = members_lock.len();
                    members_lock.retain(|_, sender| !sender.is_closed());
                    let after = members_lock.len();
                    if before != after {
                        debug!("Cleaned up {} stale users from {}", before - after, room_id);
                    }
                }
            });
        }

        while let Some(message) = self.receiver.recv().await {
            match message {
                RoomMessage::AddMember {
                    user_id,
                    sender,
                    respond_to,
                } => {
                    self.handle_add_member(user_id, sender, respond_to).await;
                }
                RoomMessage::RemoveMember { user_id } => {
                    self.handle_remove_member(user_id).await;
                }
                RoomMessage::SendMessage {
                    from,
                    content,
                    message_id,
                    #[allow(unused_variables)]
                    respond_to,
                } => {
                    self.handle_send_message(
                        from,
                        content,
                        message_id,
                        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
                        respond_to,
                    )
                    .await;
                }
                RoomMessage::GetMembers { respond_to } => {
                    let members: Vec<i32> = self.members.lock().await.keys().copied().collect();
                    let _ = respond_to.send(members);
                }
                #[cfg(any(feature = "mongo_db", feature = "persistence"))]
                RoomMessage::GetPaginatedMessages {
                    message_id,
                    respond_to,
                } => {
                    self.handle_get_paginated_messages(message_id, respond_to)
                        .await;
                }
            }
        }

        info!("Room actor stopped for room: {}", self.room_id);
    }

    async fn handle_add_member(
        &mut self,
        user_id: i32,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        if self.members.lock().await.contains_key(&user_id) {
            let _ = respond_to.send(Err("User already in room".to_string()));
            return;
        }

        self.members.lock().await.insert(user_id, sender);
        debug!("User {} added to room {}", user_id, self.room_id);

        let _ = respond_to.send(Ok(()));
    }

    async fn handle_remove_member(&mut self, user_id: i32) {
        if self.members.lock().await.remove(&user_id).is_some() {
            debug!("User {} removed from room {}", user_id, self.room_id);
        }
    }

    async fn handle_send_message(
        &self,
        from: i32,
        content: String,
        message_id: Uuid,
        #[cfg(any(feature = "mongo_db", feature = "persistence"))] respond_to: Option<
            oneshot::Sender<MessageAckResponse>,
        >,
    ) {
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        {
            if let Some(persistence_sender) = &self.persistence_sender {
                let (persist_respond_to, persist_response) = oneshot::channel();

                let persist_msg = PersistenceMessage::PersistRoomMessage {
                    room_id: self.room_id.clone(),
                    sender_id: from,
                    message_content: content.clone(),
                    message_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    respond_to: persist_respond_to,
                };

                if let Err(e) = persistence_sender.send(persist_msg) {
                    error!("Failed to send message to persistence actor: {}", e);
                    if let Some(responder) = respond_to {
                        let _ = responder.send(MessageAckResponse {
                            message_id,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            status: MessageStatus::Failed(format!("Persistence error: {}", e)),
                        });
                    }
                    return;
                }

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

        // Broadcast to all members except sender
        let message = ChatMessage::RoomMessage {
            room_id: self.room_id.clone(),
            from,
            content,
            message_id,
        };

        let members = self.members.lock().await;
        for (&member_id, sender) in members.iter() {
            match sender.try_send(message.clone()) {
                Ok(_) => debug!("Message sent to member {} in {}", member_id, self.room_id),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    debug!("Member {} queue full in {}", member_id, self.room_id);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    debug!("Member {} channel closed in {}", member_id, self.room_id);
                }
            }
        }
    }

    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    async fn handle_get_paginated_messages(
        &self,
        message_id: Option<Uuid>,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    ) {
        if let Some(persistence_sender) = &self.persistence_sender {
            let (persist_respond_to, persist_response) = oneshot::channel();
            let persist_msg = PersistenceMessage::GetPaginatedMessages {
                message_id,
                conversation_id: self.room_id.clone(),
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
