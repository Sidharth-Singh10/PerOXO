use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

use super::router::MessageRouter;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::room_actor::RoomActor;
use crate::actors::room_actor::RoomMessage;
use crate::chat::{ChatMessage, MessageAckResponse, MessageStatus};

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::PaginatedMessagesResponse;

use crate::tenant::TenantUserId;

impl MessageRouter {
    pub async fn handle_register_user(
        &mut self,
        tenant_user_id: TenantUserId,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        if self.users.contains_key(&tenant_user_id) {
            let _ = respond_to.send(Err("User already online".to_string()));
            return;
        }
        // clone?????
        self.users.insert(tenant_user_id.clone(), sender);
        self.online_users.push(tenant_user_id.clone());

        debug!("User {} registered successfully", tenant_user_id);

        let _ = respond_to.send(Ok(()));
    }
    // must be a better way
    pub async fn handle_unregister_user(&mut self, tenant_user_id: TenantUserId) {
        if self.users.remove(&tenant_user_id).is_some() {
            self.online_users.retain(|u| u != &tenant_user_id);
        }
    }

    pub async fn handle_direct_message(
        &self,
        conversation_id: String,
        from: TenantUserId,
        to: TenantUserId,
        content: String,
        message_id: uuid::Uuid,
        #[cfg(any(feature = "mongo_db", feature = "persistence"))] respond_to: Option<
            oneshot::Sender<MessageAckResponse>,
        >,
    ) {
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        {
            if let Some(persistence) = &self.persistence {
                let persistence = persistence.clone();
                let from_clone = from.clone();
                let to_clone = to.clone();
                let content_clone = content.clone();
                let timestamp = chrono::Utc::now().timestamp_millis();

                if let Some(responder) = respond_to {
                    tokio::spawn(async move {
                        let result = persistence
                            .handle_persist_direct_message(
                                conversation_id,
                                from_clone,
                                to_clone,
                                content_clone,
                                message_id,
                                timestamp,
                            )
                            .await;

                        crate::metrics::Metrics::websocket_message_persisted();

                        match result {
                            Ok(()) => {
                                let _ = responder.send(MessageAckResponse {
                                    message_id,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    status: MessageStatus::Persisted,
                                });
                            }
                            Err(e) => {
                                let _ = responder.send(MessageAckResponse {
                                    message_id,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    status: MessageStatus::Failed(e),
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
                content,
                server_message_id: message_id,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };

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

    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    pub async fn handle_get_paginated_chat_history(
        &self,
        project_id: String,
        message_id: Option<uuid::Uuid>,
        conversation_id: String,
        respond_to: oneshot::Sender<Result<PaginatedMessagesResponse, String>>,
    ) {
        if let Some(persistence) = &self.persistence {
            let persistence = persistence.clone();

            tokio::spawn(async move {
                let result = persistence
                    .handle_get_paginated_messages(project_id, message_id, conversation_id)
                    .await;
                let _ = respond_to.send(result);
            });
        }
    }

    pub async fn handle_join_room(
        &mut self,
        tenant_user_id: TenantUserId,
        room_id: String,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        let room_sender = if let Some(sender) = self.rooms.get(&room_id) {
            sender.clone()
        } else {
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            let (room_actor, room_sender) = RoomActor::new(
                room_id.clone(),
                self.persistence.as_ref().unwrap().clone(),
            );

            #[cfg(not(any(feature = "mongo_db", feature = "persistence")))]
            let (room_actor, room_sender) = {
                use crate::actors::room_actor::RoomActor;
                RoomActor::new(room_id.clone())
            };

            tokio::spawn(room_actor.run());
            self.rooms.insert(room_id.clone(), room_sender.clone());
            info!("Created new room actor for room {}", room_id);
            room_sender
        };

        let (room_respond_to, room_response) = oneshot::channel();
        let room_msg = RoomMessage::AddMember {
            tenant_user_id,
            sender,
            respond_to: room_respond_to,
        };

        if room_sender.send(room_msg).is_err() {
            let _ = respond_to.send(Err("Failed to communicate with room".to_string()));
            return;
        }

        tokio::spawn(async move {
            match room_response.await {
                Ok(result) => {
                    let _ = respond_to.send(result);
                }
                Err(_) => {
                    let _ = respond_to.send(Err("Room response timeout".to_string()));
                }
            }
        });
    }

    pub async fn handle_leave_room(&mut self, tenant_user_id: TenantUserId, room_id: String) {
        if let Some(room_sender) = self.rooms.get(&room_id) {
            let room_msg = RoomMessage::RemoveMember { tenant_user_id };
            let _ = room_sender.send(room_msg);
        }
    }

    pub async fn handle_room_message(
        &self,
        room_id: String,
        from: TenantUserId,
        content: String,
        message_id: uuid::Uuid,
        respond_to: Option<oneshot::Sender<MessageAckResponse>>,
    ) {
        if let Some(room_sender) = self.rooms.get(&room_id) {
            let room_msg = RoomMessage::SendMessage {
                from,
                content,
                message_id,
                respond_to,
            };
            if room_sender.send(room_msg).is_err() {
                error!("Failed to send message to room {}", room_id);
            }
        } else {
            debug!("Room {} not found", room_id);
            if let Some(responder) = respond_to {
                let _ = responder.send(MessageAckResponse {
                    message_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    status: MessageStatus::Failed("Room not found".to_string()),
                });
            }
        }
    }

    pub async fn handle_get_room_members(
        &self,
        room_id: String,
        respond_to: oneshot::Sender<Option<Vec<TenantUserId>>>,
    ) {
        if let Some(room_sender) = self.rooms.get(&room_id) {
            let (room_respond_to, room_response) = oneshot::channel();
            let room_msg = RoomMessage::GetMembers {
                respond_to: room_respond_to,
            };

            if room_sender.send(room_msg).is_err() {
                let _ = respond_to.send(None);
                return;
            }

            tokio::spawn(async move {
                match room_response.await {
                    Ok(members) => {
                        let _ = respond_to.send(Some(members));
                    }
                    Err(_) => {
                        let _ = respond_to.send(None);
                    }
                }
            });
        } else {
            let _ = respond_to.send(None);
        }
    }

    #[cfg(feature = "persistence")]
    pub async fn handle_sync_messages(
        &self,
        project_id: String,
        conversation_id: String,
        message_id: uuid::Uuid,
        respond_to: oneshot::Sender<Result<Vec<crate::chat::ResponseDirectMessage>, String>>,
    ) {
        if let Some(persistence) = &self.persistence {
            let persistence = persistence.clone();

            tokio::spawn(async move {
                let result = persistence
                    .handle_sync_messages(project_id, conversation_id, message_id)
                    .await;
                let _ = respond_to.send(result);
            });
        } else {
            let _ = respond_to.send(Err("Persistence not available".to_string()));
        }
    }
}
