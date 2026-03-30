use crate::chat::{ChatMessage, MessageAckResponse};
use crate::tenant::TenantUserId;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::{actors::persistance_actor::PersistenceMessage, chat::PaginatedMessagesResponse};

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::MessageStatus;

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use tracing::error;

use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug)]
pub enum RoomMessage {
    AddMember {
        tenant_user_id: TenantUserId,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    RemoveMember {
        tenant_user_id: TenantUserId,
    },
    SendMessage {
        from: TenantUserId,
        content: String,
        message_id: Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    },
    GetMembers {
        respond_to: oneshot::Sender<Vec<TenantUserId>>,
    },
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    GetPaginatedMessages {
        project_id: String,
        message_id: Option<Uuid>,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    },
}

pub struct RoomActor {
    room_id: String,
    receiver: mpsc::UnboundedReceiver<RoomMessage>,
    members: HashMap<TenantUserId, mpsc::Sender<ChatMessage>>,
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
            members: HashMap::new(),
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence_sender: Some(persistence_sender),
        };

        (actor, sender)
    }

    pub async fn run(mut self) {
        info!("Room actor started for room: {}", self.room_id);

        let mut cleanup_interval =
            tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
            tokio::select! {
                message = self.receiver.recv() => {
                    match message {
                        Some(msg) => self.handle_message(msg).await,
                        None => break,
                    }
                }
                _ = cleanup_interval.tick() => {
                    let before = self.members.len();
                    self.members.retain(|_, sender| !sender.is_closed());
                    let after = self.members.len();
                    if before != after {
                        debug!(
                            "Cleaned up {} stale users from {}",
                            before - after,
                            self.room_id
                        );
                    }
                }
            }
        }

        info!("Room actor stopped for room: {}", self.room_id);
    }

    async fn handle_message(&mut self, message: RoomMessage) {
        match message {
            RoomMessage::AddMember {
                tenant_user_id,
                sender,
                respond_to,
            } => {
                self.handle_add_member(tenant_user_id, sender, respond_to);
            }
            RoomMessage::RemoveMember { tenant_user_id } => {
                self.handle_remove_member(tenant_user_id);
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
                );
            }
            RoomMessage::GetMembers { respond_to } => {
                let members: Vec<TenantUserId> =
                    self.members.keys().cloned().collect();
                let _ = respond_to.send(members);
            }
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            RoomMessage::GetPaginatedMessages {
                project_id,
                message_id,
                respond_to,
            } => {
                self.handle_get_paginated_messages(project_id, message_id, respond_to);
            }
        }
    }

    fn handle_add_member(
        &mut self,
        tenant_user_id: TenantUserId,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        if self.members.contains_key(&tenant_user_id) {
            let _ = respond_to.send(Err("User already in room".to_string()));
            return;
        }

        self.members.insert(tenant_user_id.clone(), sender);
        debug!("User {} added to room {}", tenant_user_id, self.room_id);

        let _ = respond_to.send(Ok(()));
    }

    fn handle_remove_member(&mut self, tenant_user_id: TenantUserId) {
        if self.members.remove(&tenant_user_id).is_some() {
            debug!("User {} removed from room {}", tenant_user_id, self.room_id);
        }
    }

    fn handle_send_message(
        &self,
        from: TenantUserId,
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
                    sender_id: from.clone(),
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

        let message = ChatMessage::RoomMessage {
            room_id: self.room_id.clone(),
            from,
            content,
            message_id,
        };

        for (member_id, sender) in self.members.iter() {
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
    fn handle_get_paginated_messages(
        &self,
        project_id: String,
        message_id: Option<Uuid>,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    ) {
        if let Some(persistence_sender) = &self.persistence_sender {
            let (persist_respond_to, persist_response) = oneshot::channel();
            let persist_msg = PersistenceMessage::GetPaginatedMessages {
                project_id,
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
