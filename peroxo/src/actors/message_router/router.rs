use super::messages::RouterMessage;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::persistance_actor::PersistenceService;
use crate::actors::room_actor::RoomMessage;
use crate::chat::ChatMessage;
use crate::tenant::TenantUserId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

pub struct MessageRouter {
    pub receiver: mpsc::UnboundedReceiver<RouterMessage>,
    pub users: HashMap<TenantUserId, mpsc::Sender<ChatMessage>>,
    pub online_users: Vec<TenantUserId>,
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    pub persistence: Option<Arc<PersistenceService>>,
    pub rooms: HashMap<String, mpsc::UnboundedSender<RoomMessage>>,
}

impl MessageRouter {
    pub fn new(
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        persistence: Arc<PersistenceService>,
    ) -> (Self, mpsc::UnboundedSender<RouterMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let router = Self {
            receiver,
            users: HashMap::new(),
            online_users: Vec::new(),
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence: Some(persistence),
            rooms: HashMap::new(),
        };

        (router, sender)
    }

    pub async fn run(mut self) {
        info!("Message router started");

        while let Some(message) = self.receiver.recv().await {
            match message {
                RouterMessage::RegisterUser {
                    tenant_user_id,
                    sender,
                    respond_to,
                } => {
                    self.handle_register_user(tenant_user_id, sender, respond_to)
                        .await;
                }
                RouterMessage::UnregisterUser { tenant_user_id } => {
                    self.handle_unregister_user(tenant_user_id).await;
                }
                RouterMessage::SendDirectMessage {
                    conversation_id,
                    from,
                    to,
                    content,
                    message_id,
                    #[allow(unused_variables)]
                    respond_to,
                } => {
                    self.handle_direct_message(
                        conversation_id,
                        from,
                        to,
                        content,
                        message_id,
                        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
                        respond_to,
                    )
                    .await;
                }
                RouterMessage::GetOnlineUsers { respond_to } => {
                    let _ = respond_to.send(self.online_users.clone());
                }
                #[cfg(any(feature = "mongo_db", feature = "persistence"))]
                RouterMessage::GetPaginatedMessages {
                    project_id,
                    message_id,
                    conversation_id,
                    respond_to,
                } => {
                    self.handle_get_paginated_chat_history(project_id,message_id, conversation_id, respond_to)
                        .await;
                }
                RouterMessage::JoinRoom {
                    tenant_user_id,
                    room_id,
                    sender,
                    respond_to,
                } => {
                    self.handle_join_room(tenant_user_id, room_id, sender, respond_to)
                        .await;
                }
                RouterMessage::LeaveRoom {
                    tenant_user_id,
                    room_id,
                } => {
                    self.handle_leave_room(tenant_user_id, room_id).await;
                }
                RouterMessage::SendRoomMessage {
                    room_id,
                    from,
                    content,
                    message_id,
                    respond_to,
                } => {
                    self.handle_room_message(room_id, from, content, message_id, respond_to)
                        .await;
                }
                RouterMessage::GetRoomMembers {
                    room_id,
                    respond_to,
                } => {
                    self.handle_get_room_members(room_id, respond_to).await;
                }
                #[cfg(feature = "persistence")]
                RouterMessage::SyncMessages {
                    project_id,
                    conversation_id,
                    message_id,
                    respond_to,
                } => {
                    self.handle_sync_messages(project_id,conversation_id, message_id, respond_to)
                        .await;
                }
            }
        }

        info!("Message router stopped");
    }
}
