use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

use super::router::MessageRouter;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::room_actor::RoomActor;
use crate::actors::room_actor::RoomMessage;
use crate::chat::{ChatMessage, MessageAckResponse, MessageStatus};
use std::time::Duration;

const ROOM_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Room registry keys are namespaced by tenant so that two projects using
/// the same room name can never share a RoomActor (cross-tenant broadcast
/// leak). The RoomActor itself keeps the raw room_id so persistence stays
/// tenant-scoped via the sender's project_id.
fn ns_room(project_id: &str, room_id: &str) -> String {
    format!("{}:{room_id}", project_id)
}

#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::chat::PaginatedMessagesResponse;

use crate::tenant::TenantUserId;

impl MessageRouter {
    fn remove_user(&mut self, tenant_user_id: &TenantUserId) {
        if self.users.remove(tenant_user_id).is_some() {
            self.online_users.retain(|u| u != tenant_user_id);
        }
    }

    pub(crate) fn remove_stale_sessions(&mut self) {
        let before = self.users.len();
        if before == 0 {
            return;
        }

        self.users.retain(|_, sender| !sender.is_closed());

        if self.users.len() != before {
            let alive: std::collections::HashSet<&TenantUserId> = self.users.keys().collect();
            self.online_users.retain(|u| alive.contains(u));
            info!(
                "Removed {} stale sessions whose channels were closed",
                before - self.users.len()
            );
        }
    }

    pub async fn handle_register_user(
        &mut self,
        tenant_user_id: TenantUserId,
        sender: mpsc::Sender<ChatMessage>,
        respond_to: oneshot::Sender<Result<(), String>>,
    ) {
        if let Some(existing) = self.users.get(&tenant_user_id) {
            if existing.is_closed() {
                // The previous session died without unregistering; replace it
                // instead of blocking reconnects forever with "User already online".
                self.remove_user(&tenant_user_id);
            } else {
                let _ = respond_to.send(Err("User already online".to_string()));
                return;
            }
        }
        // clone?????
        self.users.insert(tenant_user_id.clone(), sender);
        self.online_users.push(tenant_user_id.clone());

        debug!("User {} registered successfully", tenant_user_id);

        let _ = respond_to.send(Ok(()));
    }
    // must be a better way
    pub async fn handle_unregister_user(&mut self, tenant_user_id: TenantUserId) {
        self.remove_user(&tenant_user_id);
    }

    pub async fn handle_direct_message(
        &mut self,
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
        let (from_clone, to_clone, content_clone, timestamp) = (
            from.clone(),
            to.clone(),
            content.clone(),
            chrono::Utc::now().timestamp_millis(),
        );

        let delivery_outcome: Option<&'static str> = match self.users.get(&to).cloned() {
            Some(recipient_sender) => {
                let message = ChatMessage::DirectMessage {
                    from,
                    content,
                    server_message_id: message_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };

                match recipient_sender.try_send(message) {
                    Ok(()) => {
                        debug!("Message sent successfully to {}", to);
                        None
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        debug!("Recipient {} message queue is full, dropping message", to);
                        Some("queue_full")
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        debug!("Recipient {} channel is closed", to);
                        Some("channel_closed")
                    }
                }
            }
            None => {
                debug!("User {} not found or offline", to);
                Some("offline")
            }
        };

        if let Some(reason) = delivery_outcome {
            crate::metrics::Metrics::websocket_message_dropped(reason);
        }

        if delivery_outcome == Some("channel_closed") {
            self.remove_user(&to);
        }

        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        {
            if let Some(persistence) = &self.persistence {
                let persistence = persistence.clone();

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

                        let status = match result {
                            Ok(()) => match delivery_outcome {
                                None => MessageStatus::Persisted,
                                Some(reason) => MessageStatus::NotDelivered(format!(
                                    "persisted but not delivered to recipient ({reason})"
                                )),
                            },
                            Err(e) => MessageStatus::Failed(e),
                        };

                        let _ = responder.send(MessageAckResponse {
                            message_id,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            status,
                        });
                    });
                }
            }
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
        let room_key = ns_room(&tenant_user_id.project_id, &room_id);

        // If the previous actor for this room died (receiver dropped) before
        // the periodic sweep removed it, drop the stale handle and recreate.
        if let Some(room_sender) = self.rooms.get(&room_key) {
            if room_sender.is_closed() {
                self.rooms.remove(&room_key);
            }
        }

        let room_sender = if let Some(sender) = self.rooms.get(&room_key) {
            sender.clone()
        } else {
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            let (room_actor, room_sender) =
                RoomActor::new(room_id.clone(), self.persistence.as_ref().unwrap().clone());

            #[cfg(not(any(feature = "mongo_db", feature = "persistence")))]
            let (room_actor, room_sender) = {
                use crate::actors::room_actor::RoomActor;
                RoomActor::new(room_id.clone())
            };

            tokio::spawn(room_actor.run());
            self.rooms.insert(room_key.clone(), room_sender.clone());
            info!("Created new room actor for room {}", room_id);
            room_sender
        };

        let (room_respond_to, room_response) = oneshot::channel();
        let room_msg = RoomMessage::AddMember {
            tenant_user_id,
            sender,
            respond_to: room_respond_to,
        };

        if room_sender.send(room_msg).await.is_err() {
            let _ = respond_to.send(Err("Failed to communicate with room".to_string()));
            return;
        }

        tokio::spawn(async move {
            match tokio::time::timeout(ROOM_REPLY_TIMEOUT, room_response).await {
                Ok(Ok(result)) => {
                    let _ = respond_to.send(result);
                }
                Ok(Err(_)) => {
                    let _ = respond_to.send(Err("Room response channel closed".to_string()));
                }
                Err(_) => {
                    let _ = respond_to.send(Err("Room response timeout".to_string()));
                }
            }
        });
    }

    pub async fn handle_leave_room(&mut self, tenant_user_id: TenantUserId, room_id: String) {
        let room_key = ns_room(&tenant_user_id.project_id, &room_id);
        if let Some(room_sender) = self.rooms.get(&room_key) {
            let room_msg = RoomMessage::RemoveMember { tenant_user_id };
            let _ = room_sender.send(room_msg).await;
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
        let room_key = ns_room(&from.project_id, &room_id);
        if let Some(room_sender) = self.rooms.get(&room_key) {
            let room_msg = RoomMessage::SendMessage {
                from,
                content,
                message_id,
                respond_to,
            };
            match room_sender.send(room_msg).await {
                Ok(()) => {}
                Err(mpsc::error::SendError(returned)) => {
                    error!("Failed to send message to room {}", room_id);
                    let respond_to = match returned {
                        RoomMessage::SendMessage { respond_to, .. } => respond_to,
                        _ => None,
                    };
                    if let Some(responder) = respond_to {
                        let _ = responder.send(MessageAckResponse {
                            message_id,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            status: MessageStatus::Failed("Room unavailable".to_string()),
                        });
                    }
                }
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

            if room_sender.send(room_msg).await.is_err() {
                let _ = respond_to.send(None);
                return;
            }

            tokio::spawn(async move {
                match tokio::time::timeout(ROOM_REPLY_TIMEOUT, room_response).await {
                    Ok(Ok(members)) => {
                        let _ = respond_to.send(Some(members));
                    }
                    Ok(Err(_)) | Err(_) => {
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
